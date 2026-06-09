use eframe::egui;
use std::sync::mpsc;

use crate::config::{Config, ConfigMode};
use crate::network::SharedItem;
use crate::protocol::Message;

// ── Window geometry ──────────────────────────────────────────────────────────
pub const MINI_W: f32 = 60.0;
pub const MINI_H: f32 = 60.0;
const FULL_W: f32 = 350.0;
const FULL_H: f32 = 520.0;

// ── Timing ───────────────────────────────────────────────────────────────────
const HOVER_TO_EXPAND_SECS: f32 = 0.2;
const LEAVE_TO_HIDE_SECS: f32 = 0.05;
const EXPAND_SECS: f32 = 0.25;
const COLLAPSE_SECS: f32 = 0.18;

// ── State machine ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// First-launch configuration wizard.
    Setup,
    /// Transparent 60×60 hotspot — visually nothing.
    Hidden,
    /// Visible 60×60 drop-target icon.
    Mini,
    /// Growing from MINI to FULL (anim_t: 0 → 1).
    Expanding,
    /// Full 350×520 panel.
    Full,
    /// Shrinking back to corner (anim_t: 1 → 0), then Hidden.
    Collapsing,
}

// ── App ──────────────────────────────────────────────────────────────────────
pub struct SShareApp {
    // ── Main app state ──────────────────────────────────────────────────────
    items: Vec<Item>,
    send_tx: mpsc::Sender<Message>,
    recv_rx: mpsc::Receiver<SharedItem>,
    status_rx: mpsc::Receiver<String>,
    status: String,
    clipboard: Option<arboard::Clipboard>,

    phase: Phase,
    anim_t: f32,
    hover_timer: f32,
    leave_timer: f32,
    monitor_h: f32,
    initialized: bool,
    drop_grace: f32,

    // ── Setup wizard state ──────────────────────────────────────────────────
    net_setup_tx: Option<mpsc::Sender<Config>>,
    setup_mode: ConfigMode,
    setup_port: String,
    setup_address: String,
    setup_login_startup: bool,
    discovered_servers: Vec<String>,
    discovery_rx: Option<mpsc::Receiver<String>>,
}

#[derive(Clone)]
enum Item {
    Text(String),
    File { name: String, data: Vec<u8> },
}

impl SShareApp {
    pub fn new(
        cc: &eframe::CreationContext,
        send_tx: mpsc::Sender<Message>,
        recv_rx: mpsc::Receiver<SharedItem>,
        status_rx: mpsc::Receiver<String>,
        net_setup_tx: Option<mpsc::Sender<Config>>,
    ) -> Self {
        load_japanese_font(&cc.egui_ctx);

        let phase = if net_setup_tx.is_some() {
            Phase::Setup
        } else {
            Phase::Hidden
        };

        let (discovery_rx, disc_tx) = {
            let (tx, rx) = mpsc::channel();
            (Some(rx), Some(tx))
        };
        if phase == Phase::Setup {
            if let Some(tx) = disc_tx {
                crate::discovery::start_browsing(tx);
            }
        }

        Self {
            items: Vec::new(),
            send_tx,
            recv_rx,
            status_rx,
            status: "Starting…".into(),
            clipboard: arboard::Clipboard::new().ok(),
            phase,
            anim_t: 0.0,
            hover_timer: 0.0,
            leave_timer: 0.0,
            monitor_h: 900.0,
            initialized: false,
            drop_grace: 0.0,
            net_setup_tx,
            setup_mode: ConfigMode::Server,
            setup_port: "7878".into(),
            setup_address: String::new(),
            setup_login_startup: false,
            discovered_servers: Vec::new(),
            discovery_rx: if phase == Phase::Setup {
                discovery_rx
            } else {
                None
            },
        }
    }

    // ── Channel polling ───────────────────────────────────────────────────
    fn poll(&mut self) {
        while let Ok(item) = self.recv_rx.try_recv() {
            self.items.push(match item {
                SharedItem::Text(s) => Item::Text(s),
                SharedItem::File { name, data } => Item::File { name, data },
            });
        }
        while let Ok(s) = self.status_rx.try_recv() {
            self.status = s;
        }
        if let Some(rx) = &self.discovery_rx {
            while let Ok(addr) = rx.try_recv() {
                if !self.discovered_servers.contains(&addr) {
                    self.discovered_servers.push(addr);
                }
            }
        }
    }

    // ── Drop / paste handling ─────────────────────────────────────────────
    fn handle_input(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Paste(text) = ev {
                    if !text.is_empty() {
                        let _ = self.send_tx.send(Message::Text(text.clone()));
                        self.items.push(Item::Text(text.clone()));
                    }
                }
            }
            for f in &i.raw.dropped_files {
                if let Some(path) = &f.path {
                    if let Ok(data) = std::fs::read(path) {
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        let _ = self.send_tx.send(Message::File {
                            name: name.clone(),
                            data: data.clone(),
                        });
                        self.items.push(Item::File { name, data });
                    }
                }
            }
        });
    }

    // ── Setup UI ─────────────────────────────────────────────────────────
    /// Returns true when the user clicks "確定" with valid settings.
    fn show_setup_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut confirm = false;

        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("SShare セットアップ")
                    .size(20.0)
                    .strong(),
            );
        });
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(10.0);

        // ── Mode buttons ────────────────────────────────────────────────
        ui.label(egui::RichText::new("起動モードを選択").size(13.0));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let server_selected = self.setup_mode == ConfigMode::Server;
            let client_selected = self.setup_mode == ConfigMode::Client;

            let server_btn = egui::Button::new(
                egui::RichText::new("  サーバー  ").size(14.0),
            )
            .fill(if server_selected {
                egui::Color32::from_rgb(50, 130, 230)
            } else {
                egui::Color32::from_gray(55)
            });
            if ui.add(server_btn).clicked() {
                self.setup_mode = ConfigMode::Server;
            }

            let client_btn = egui::Button::new(
                egui::RichText::new("  クライアント  ").size(14.0),
            )
            .fill(if client_selected {
                egui::Color32::from_rgb(50, 130, 230)
            } else {
                egui::Color32::from_gray(55)
            });
            if ui.add(client_btn).clicked() {
                self.setup_mode = ConfigMode::Client;
            }
        });
        ui.add_space(14.0);

        match self.setup_mode {
            ConfigMode::Server => {
                // ── Server settings ──────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label("ポート番号:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.setup_port)
                            .desired_width(80.0)
                            .hint_text("7878"),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "クライアントからの接続を待ちます。\n\
                         同じLANのクライアントへは自動検出されます。\n\
                         SSH経由: ssh -L <port>:localhost:<port> user@host",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_gray(140)),
                );
            }
            ConfigMode::Client => {
                // ── Connection target ────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label("接続先:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.setup_address)
                            .desired_width(200.0)
                            .hint_text("192.168.x.x:7878 または localhost:7878"),
                    );
                });
                ui.add_space(8.0);

                // ── Discovered servers ───────────────────────────────────
                if !self.discovered_servers.is_empty() {
                    ui.label(
                        egui::RichText::new("検出されたサーバー:")
                            .size(12.0)
                            .color(egui::Color32::from_gray(180)),
                    );
                    let servers = self.discovered_servers.clone();
                    for addr in &servers {
                        let btn = egui::Button::new(
                            egui::RichText::new(format!("  {}  ", addr)).size(12.0),
                        )
                        .fill(egui::Color32::from_gray(45));
                        if ui.add(btn).clicked() {
                            self.setup_address = addr.clone();
                        }
                    }
                    ui.add_space(4.0);
                } else {
                    ui.label(
                        egui::RichText::new("サーバーを検索中…  (同じLANのみ自動検出)")
                            .size(11.0)
                            .color(egui::Color32::from_gray(120)),
                    );
                    ui.add_space(4.0);
                }

                ui.label(
                    egui::RichText::new(
                        "SSH経由の場合: ssh -L 7878:localhost:7878 user@host\n\
                         その後、接続先に localhost:7878 を入力",
                    )
                    .size(11.0)
                    .color(egui::Color32::from_gray(140)),
                );
            }
        }

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);

        // ── Login startup ────────────────────────────────────────────────
        ui.checkbox(
            &mut self.setup_login_startup,
            "ログイン時に自動起動",
        );
        ui.add_space(12.0);

        // ── Confirm button ───────────────────────────────────────────────
        ui.vertical_centered(|ui| {
            let ready = match self.setup_mode {
                ConfigMode::Server => self.setup_port.parse::<u16>().is_ok(),
                ConfigMode::Client => !self.setup_address.trim().is_empty(),
            };
            let btn = egui::Button::new(
                egui::RichText::new("  確定  ").size(15.0).strong(),
            )
            .fill(if ready {
                egui::Color32::from_rgb(50, 180, 90)
            } else {
                egui::Color32::from_gray(55)
            });
            if ui.add_enabled(ready, btn).clicked() {
                confirm = true;
            }
        });

        confirm
    }

    fn confirm_setup(&mut self, ctx: &egui::Context) {
        let cfg = Config {
            mode: self.setup_mode.clone(),
            port: self.setup_port.parse().unwrap_or(7878),
            server_address: self.setup_address.trim().to_string(),
            login_startup: self.setup_login_startup,
        };
        cfg.save();
        cfg.apply_login_startup();

        if let Some(tx) = self.net_setup_tx.take() {
            // First launch: unblock the waiting network thread.
            let _ = tx.send(cfg);
        } else {
            // Re-setup: drop old channels and start a fresh network thread.
            self.restart_network(&Config {
                mode: self.setup_mode.clone(),
                port: self.setup_port.parse().unwrap_or(7878),
                server_address: self.setup_address.trim().to_string(),
                login_startup: self.setup_login_startup,
            });
        }
        self.discovery_rx = None;

        self.phase = Phase::Hidden;
        self.initialized = false;

        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::viewport::WindowLevel::AlwaysOnTop,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            MINI_W, MINI_H,
        )));
    }

    fn begin_setup(&mut self, ctx: &egui::Context) {
        // Pre-fill fields from saved config so user sees current values.
        if let Some(cfg) = Config::load() {
            self.setup_mode = cfg.mode;
            self.setup_port = cfg.port.to_string();
            self.setup_address = cfg.server_address;
            self.setup_login_startup = cfg.login_startup;
        }

        let (disc_tx, disc_rx) = mpsc::channel();
        crate::discovery::start_browsing(disc_tx);
        self.discovery_rx = Some(disc_rx);
        self.discovered_servers.clear();

        self.phase = Phase::Setup;
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::viewport::WindowLevel::Normal,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(480.0, 440.0)));
    }

    fn restart_network(&mut self, cfg: &Config) {
        let (to_gui_tx, to_gui_rx) = mpsc::channel::<crate::network::SharedItem>();
        let (from_gui_tx, from_gui_rx) = mpsc::channel::<crate::protocol::Message>();
        let (status_tx, status_rx) = mpsc::channel::<String>();

        let mode = match cfg.mode {
            ConfigMode::Server => crate::network::Mode::Server(cfg.port),
            ConfigMode::Client => crate::network::Mode::Client(cfg.server_address.clone()),
        };

        std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(crate::network::run(mode, to_gui_tx, from_gui_rx, status_tx));
        });

        // Replacing send_tx drops the old Sender → old from_gui channel closes → old thread exits.
        self.send_tx = from_gui_tx;
        self.recv_rx = to_gui_rx;
        self.status_rx = status_rx;
        self.status = "Reconnecting…".into();
    }

    // ── State machine + viewport commands ────────────────────────────────
    fn update_phase(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let cursor_in = ctx.input(|i| i.pointer.has_pointer());

        let prev_monitor_h = self.monitor_h;
        if let Some(sz) = ctx.input(|i| i.viewport().monitor_size) {
            self.monitor_h = sz.y;
        }
        let monitor_changed = (self.monitor_h - prev_monitor_h).abs() > 0.5;

        let prev_phase = self.phase;

        let file_just_dropped = ctx.input(|i| !i.raw.dropped_files.is_empty());
        if file_just_dropped {
            self.drop_grace = 0.6;
        } else {
            self.drop_grace = (self.drop_grace - dt).max(0.0);
        }

        let file_dragging = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let present = cursor_in || file_dragging || self.drop_grace > 0.0;

        match self.phase {
            Phase::Hidden => {
                if present {
                    self.phase = Phase::Mini;
                    self.hover_timer = 0.0;
                }
            }
            Phase::Mini => {
                if present {
                    self.hover_timer += dt;
                    if file_dragging || self.hover_timer >= HOVER_TO_EXPAND_SECS {
                        self.phase = Phase::Expanding;
                    }
                } else {
                    self.phase = Phase::Hidden;
                    self.hover_timer = 0.0;
                }
            }
            Phase::Full => {
                if present {
                    self.leave_timer = 0.0;
                } else {
                    self.leave_timer += dt;
                    if self.leave_timer >= LEAVE_TO_HIDE_SECS {
                        self.phase = Phase::Collapsing;
                    }
                }
            }
            Phase::Expanding | Phase::Collapsing | Phase::Setup => {}
        }

        match self.phase {
            Phase::Expanding => {
                self.anim_t = (self.anim_t + dt / EXPAND_SECS).min(1.0);
                ctx.request_repaint();
                if self.anim_t >= 1.0 {
                    self.phase = Phase::Full;
                    self.leave_timer = 0.0;
                }
            }
            Phase::Collapsing => {
                self.anim_t = (self.anim_t - dt / COLLAPSE_SECS).max(0.0);
                ctx.request_repaint();
                if self.anim_t <= 0.0 {
                    self.phase = Phase::Hidden;
                }
            }
            _ => {
                ctx.request_repaint_after(std::time::Duration::from_millis(80));
            }
        }

        let animating = matches!(self.phase, Phase::Expanding | Phase::Collapsing);
        let needs_update = !self.initialized || self.phase != prev_phase || animating || monitor_changed;
        if needs_update {
            let (w, h) = self.window_size();
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(self.window_pos()));
            self.initialized = true;
        }
        // Transparent windows are click-through by default; opt back in for the hidden hotspot.
        if prev_phase != Phase::Hidden && self.phase == Phase::Hidden {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
        }
        if !self.initialized {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
        }
    }

    fn window_size(&self) -> (f32, f32) {
        // Height grows/shrinks with animation; window bottom stays pinned to monitor_h.
        // This keeps the window fully on-screen so the WM never overrides the position.
        let h = match self.phase {
            Phase::Hidden | Phase::Mini => MINI_H,
            Phase::Expanding => lerp(MINI_H, FULL_H, ease_out_cubic(self.anim_t)),
            Phase::Full => FULL_H,
            Phase::Collapsing => lerp(MINI_H, FULL_H, ease_in_cubic(self.anim_t)),
            Phase::Setup => MINI_H,
        };
        let w = if self.phase == Phase::Hidden { MINI_W } else { FULL_W };
        (w, h)
    }

    fn window_pos(&self) -> egui::Pos2 {
        // Always pin the bottom edge of the window to the bottom of the monitor.
        let (_, h) = self.window_size();
        egui::pos2(0.0, self.monitor_h - h)
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────────
impl eframe::App for SShareApp {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        match self.phase {
            Phase::Setup => {
                let c = egui::Color32::from_rgb(30, 30, 38);
                [
                    c.r() as f32 / 255.0,
                    c.g() as f32 / 255.0,
                    c.b() as f32 / 255.0,
                    1.0,
                ]
            }
            Phase::Hidden => [0.0; 4],
            _ => {
                let c = egui::Color32::from_rgb(28, 28, 34);
                [
                    c.r() as f32 / 255.0,
                    c.g() as f32 / 255.0,
                    c.b() as f32 / 255.0,
                    1.0,
                ]
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        // ── Setup wizard ─────────────────────────────────────────────────────
        if self.phase == Phase::Setup {
            if let Some(sz) = ctx.input(|i| i.viewport().monitor_size) {
                self.monitor_h = sz.y;
            }
            let mut confirmed = false;
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(30, 30, 38))
                        .inner_margin(egui::Margin::symmetric(24.0, 12.0)),
                )
                .show(ctx, |ui| {
                    confirmed = self.show_setup_ui(ui);
                });
            if confirmed {
                self.confirm_setup(ctx);
            }
            return;
        }

        // ── Main app ─────────────────────────────────────────────────────────
        self.handle_input(ctx);
        self.update_phase(ctx);
        let mut open_settings = false;

        match self.phase {
            Phase::Setup => unreachable!(),

            Phase::Hidden => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
                    .show(ctx, |_| {});
            }

            Phase::Mini | Phase::Expanding | Phase::Full | Phase::Collapsing => {
                let hovering_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
                let hover_progress =
                    (self.hover_timer / HOVER_TO_EXPAND_SECS).clamp(0.0, 1.0);
                let dot_color = if self.status.starts_with("Connected") {
                    egui::Color32::from_rgb(50, 210, 100)
                } else {
                    egui::Color32::from_rgb(220, 180, 40)
                };

                let frame = egui::Frame::none()
                    .fill(egui::Color32::from_rgb(28, 28, 34))
                    .rounding(egui::Rounding {
                        nw: 0.0,
                        sw: 0.0,
                        ne: 10.0,
                        se: 0.0,
                    })
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0));

                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    let strip_rect = egui::Rect::from_min_size(
                        ui.min_rect().min,
                        egui::vec2(ui.available_width(), MINI_H - 12.0),
                    );
                    let center = strip_rect.center();
                    let radius = 20.0;

                    ui.painter().circle_stroke(
                        center,
                        radius,
                        egui::Stroke::new(2.0, egui::Color32::from_gray(55)),
                    );
                    let arc_alpha = (200.0 * hover_progress) as u8;
                    let arc_color =
                        egui::Color32::from_rgba_premultiplied(80, 160, 255, arc_alpha);
                    let segs = 32usize;
                    for seg in 0..(segs as f32 * hover_progress) as usize {
                        let a0 = std::f32::consts::TAU * seg as f32 / segs as f32
                            - std::f32::consts::FRAC_PI_2;
                        let a1 = std::f32::consts::TAU * (seg + 1) as f32 / segs as f32
                            - std::f32::consts::FRAC_PI_2;
                        ui.painter().line_segment(
                            [
                                center + egui::vec2(a0.cos(), a0.sin()) * radius,
                                center + egui::vec2(a1.cos(), a1.sin()) * radius,
                            ],
                            egui::Stroke::new(2.5, arc_color),
                        );
                    }
                    ui.painter().text(
                        center - egui::vec2(0.0, 3.0),
                        egui::Align2::CENTER_CENTER,
                        if hovering_file { "📥" } else { "⬆" },
                        egui::FontId::proportional(20.0),
                        egui::Color32::from_gray(210),
                    );
                    ui.painter().circle_filled(
                        center + egui::vec2(12.0, 10.0),
                        4.0,
                        dot_color,
                    );
                    ui.allocate_rect(strip_rect, egui::Sense::hover());

                    ui.separator();

                    // ── Button row (above status) ──────────────────────────
                    ui.horizontal(|ui| {
                        if close_btn(ui) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if text_btn(ui, "Clear") {
                                self.items.clear();
                            }
                            if icon_btn(ui, "⚙", "設定") {
                                open_settings = true;
                            }
                        });
                    });

                    // ── Status row ─────────────────────────────────────────
                    ui.horizontal(|ui| {
                        ui.colored_label(dot_color, "●");
                        ui.label(
                            egui::RichText::new(&self.status)
                                .size(11.0)
                                .color(egui::Color32::from_gray(180)),
                        );
                    });
                    ui.separator();

                    let list_h = ui.available_height() - 62.0;
                    egui::ScrollArea::vertical()
                        .max_height(list_h)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            let mut remove: Option<usize> = None;
                            for (i, item) in self.items.iter().enumerate() {
                                ui.horizontal(|ui| match item {
                                    Item::Text(t) => {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "📝 {}",
                                                truncate(t, 46)
                                            ))
                                            .size(12.0)
                                            .color(egui::Color32::from_gray(220)),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button("✕").clicked() {
                                                    remove = Some(i);
                                                }
                                                if ui.small_button("Copy").clicked() {
                                                    if let Some(cb) = &mut self.clipboard {
                                                        let _ = cb.set_text(t.clone());
                                                    }
                                                }
                                            },
                                        );
                                    }
                                    Item::File { name, data } => {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "📎 {} ({})",
                                                name,
                                                fmt_size(data.len())
                                            ))
                                            .size(12.0)
                                            .color(egui::Color32::from_gray(220)),
                                        );
                                        let (name, data) = (name.clone(), data.clone());
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button("✕").clicked() {
                                                    remove = Some(i);
                                                }
                                                if ui.small_button("Save").clicked() {
                                                    save_file(&name, &data);
                                                }
                                            },
                                        );
                                    }
                                });
                                ui.separator();
                            }
                            if self.items.is_empty() {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(16.0);
                                    ui.label(
                                        egui::RichText::new("No shared items yet.")
                                            .size(12.0)
                                            .color(egui::Color32::from_gray(100)),
                                    );
                                });
                            }
                            if let Some(i) = remove {
                                self.items.remove(i);
                            }
                        });

                    ui.separator();
                    let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ui.available_height()),
                        egui::Sense::hover(),
                    );
                    let bg = if hovering {
                        egui::Color32::from_rgba_premultiplied(30, 100, 230, 55)
                    } else {
                        egui::Color32::from_rgba_premultiplied(80, 80, 80, 25)
                    };
                    let border = if hovering {
                        egui::Color32::from_rgb(80, 160, 255)
                    } else {
                        egui::Color32::from_gray(75)
                    };
                    ui.painter()
                        .rect(rect, 6.0, bg, egui::Stroke::new(1.5, border));
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if hovering {
                            "Drop to share →"
                        } else {
                            "Drop files  •  Ctrl+V to paste"
                        },
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_gray(130),
                    );
                });
            }
        }

        if open_settings {
            self.begin_setup(ctx);
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Close button: compact rect so the highlight wraps the × character correctly.
fn close_btn(ui: &mut egui::Ui) -> bool {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(16.0, 20.0), egui::Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, 5.0, egui::Color32::from_gray(58));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "×",
        egui::FontId::proportional(14.0),
        if hovered {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_gray(155)
        },
    );
    resp.on_hover_text("終了").clicked()
}

/// Icon button (single glyph). Returns true if clicked.
/// Shows a rounded highlight background and white text on hover.
fn icon_btn(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> bool {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, 5.0, egui::Color32::from_gray(58));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(13.0),
        if hovered {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_gray(155)
        },
    );
    resp.on_hover_text(tooltip).clicked()
}

/// Text button. Returns true if clicked.
fn text_btn(ui: &mut egui::Ui, label: &str) -> bool {
    let desired = egui::vec2(
        ui.fonts(|f| f.glyph_width(&egui::FontId::proportional(11.0), 'x')) * label.len() as f32
            + 10.0,
        22.0,
    );
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, 5.0, egui::Color32::from_gray(58));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        if hovered {
            egui::Color32::from_gray(220)
        } else {
            egui::Color32::from_gray(140)
        },
    );
    resp.clicked()
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

fn truncate(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let prefix: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn fmt_size(n: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;
    if n < KB {
        format!("{n} B")
    } else if n < MB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else if n < GB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else {
        format!("{:.2} GB", n as f64 / GB as f64)
    }
}

fn save_file(name: &str, data: &[u8]) {
    let dir = dirs::download_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("SShare");
    if std::fs::create_dir_all(&dir).is_ok() {
        let path = dir.join(name);
        if std::fs::write(&path, data).is_ok() {
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open")
                .args(["-R", path.to_str().unwrap_or("")])
                .spawn();
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open")
                .arg(dir.to_str().unwrap_or(""))
                .spawn();
        }
    }
}

fn load_japanese_font(ctx: &egui::Context) {
    let candidates: &[&str] = &[
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Arial Unicode MS.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/ipaexfont-gothic/ipaexg.ttf",
    ];
    if let Some(data) = candidates.iter().find_map(|p| std::fs::read(p).ok()) {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("jp".to_owned(), egui::FontData::from_owned(data));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("jp".to_owned());
        }
        ctx.set_fonts(fonts);
    }
}
