use eframe::egui;
use std::sync::mpsc;

use crate::network::SharedItem;
use crate::protocol::Message;

// ── Window geometry ──────────────────────────────────────────────────────────
pub const MINI_W: f32 = 60.0;
pub const MINI_H: f32 = 60.0;
const FULL_W: f32 = 350.0;
const FULL_H: f32 = 520.0;

// ── Timing ───────────────────────────────────────────────────────────────────
const HOVER_TO_EXPAND_SECS: f32 = 0.2;  // dwell on Mini before auto-expanding
const LEAVE_TO_HIDE_SECS: f32  = 0.05;  // idle after cursor leaves Full
const EXPAND_SECS: f32         = 0.25;  // expand animation duration
const COLLAPSE_SECS: f32       = 0.18;  // collapse animation duration

// ── State machine ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// Transparent 60×60 hotspot at corner — visually nothing.
    Hidden,
    /// Visible 60×60 drop-target icon. Drag-and-drop works here.
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
    items: Vec<Item>,
    send_tx: mpsc::Sender<Message>,
    recv_rx: mpsc::Receiver<SharedItem>,
    status_rx: mpsc::Receiver<String>,
    status: String,
    clipboard: Option<arboard::Clipboard>,

    phase: Phase,
    anim_t: f32,       // 0.0 = MINI size, 1.0 = FULL size
    hover_timer: f32,  // seconds cursor has rested on Mini
    leave_timer: f32,  // seconds cursor has been outside Full
    monitor_h: f32,    // cached monitor height (logical px)
    initialized: bool, // false until first OuterPosition is sent
    drop_grace: f32,   // seconds remaining in post-drop grace period
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
    ) -> Self {
        load_japanese_font(&cc.egui_ctx);
        Self {
            items: Vec::new(),
            send_tx,
            recv_rx,
            status_rx,
            status: "Starting…".into(),
            clipboard: arboard::Clipboard::new().ok(),
            phase: Phase::Hidden,
            anim_t: 0.0,
            hover_timer: 0.0,
            leave_timer: 0.0,
            monitor_h: 900.0,
            initialized: false,
            drop_grace: 0.0,
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
    }

    // ── Drop / paste handling ─────────────────────────────────────────────
    fn handle_input(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Ctrl+V / Cmd+V  (only useful in Full)
            for ev in &i.events {
                if let egui::Event::Paste(text) = ev {
                    if !text.is_empty() {
                        let _ = self.send_tx.send(Message::Text(text.clone()));
                        self.items.push(Item::Text(text.clone()));
                    }
                }
            }
            // File drag-and-drop (works in Mini too)
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

    // ── State machine + viewport commands ────────────────────────────────
    fn update_phase(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let cursor_in = ctx.input(|i| i.pointer.has_pointer());
        let clicked = ctx.input(|i| i.pointer.primary_clicked());

        if let Some(sz) = ctx.input(|i| i.viewport().monitor_size) {
            self.monitor_h = sz.y;
        }

        let prev_phase = self.phase;

        // After a drop, macOS takes a moment to transition from drag-session
        // tracking back to normal pointer tracking.  Keep a grace timer so the
        // window doesn't collapse in that gap.
        let file_just_dropped = ctx.input(|i| !i.raw.dropped_files.is_empty());
        if file_just_dropped {
            self.drop_grace = 0.6;
        } else {
            self.drop_grace = (self.drop_grace - dt).max(0.0);
        }

        // During a drag, macOS replaces normal pointer tracking with a drag
        // session so has_pointer() can be false even when the cursor is over
        // the window. Treat hovered_files / drop grace as "cursor present".
        let file_dragging = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let present = cursor_in || file_dragging || self.drop_grace > 0.0;

        // ── Step 1: input-driven transitions ──────────────────────────────
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
                    // File drag → expand immediately without dwell wait.
                    if file_dragging || self.hover_timer >= HOVER_TO_EXPAND_SECS || clicked {
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
            // Expanding / Collapsing complete via animation below.
            Phase::Expanding | Phase::Collapsing => {}
        }

        // ── Step 2: advance anim_t, request repaint every animation frame ──
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

        // ── Viewport commands ─────────────────────────────────────────────
        // KEY INSIGHT (same as macOS Dock): never resize during animation.
        // The window is always FULL_W×FULL_H when visible; only OuterPosition
        // (Y) changes each frame.  Position changes are composited by the GPU
        // and are smooth even at high frame rates.  InnerSize changes go
        // through the window server and cause the observed jank.
        let animating = matches!(self.phase, Phase::Expanding | Phase::Collapsing);

        // Resize only when crossing the Hidden boundary (once per show/hide).
        let size_changed = !self.initialized
            || (prev_phase == Phase::Hidden) != (self.phase == Phase::Hidden);
        if size_changed {
            let (w, h) = self.window_size();
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        }

        // Position every frame while animating; once on state transitions.
        if !self.initialized || self.phase != prev_phase || animating {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                self.window_pos(),
            ));
            self.initialized = true;
        }
    }

    /// Window size: small transparent hotspot when Hidden, full otherwise.
    fn window_size(&self) -> (f32, f32) {
        if self.phase == Phase::Hidden {
            (MINI_W, MINI_H)
        } else {
            (FULL_W, FULL_H)
        }
    }

    /// Window Y-only animation — no width change ever during animation.
    fn window_pos(&self) -> egui::Pos2 {
        // Collapsed Y: window top sits at (monitor_h − MINI_H) so only the
        // top MINI_H pixels are on-screen (the "mini strip").
        // Expanded Y: window top at (monitor_h − FULL_H), fully on-screen.
        let y_collapsed = self.monitor_h - MINI_H;
        let y_expanded  = self.monitor_h - FULL_H;

        let y = match self.phase {
            Phase::Hidden => self.monitor_h - MINI_H,
            Phase::Mini   => y_collapsed,
            Phase::Expanding => {
                let t = ease_out_cubic(self.anim_t);
                lerp(y_collapsed, y_expanded, t)
            }
            Phase::Full => y_expanded,
            Phase::Collapsing => {
                let t = ease_in_cubic(self.anim_t); // 1 → 0
                lerp(y_collapsed, y_expanded, t)
            }
        };
        egui::pos2(0.0, y)
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────────
impl eframe::App for SShareApp {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        // Hidden: fully transparent window (invisible hotspot).
        if self.phase == Phase::Hidden {
            [0.0; 4]
        } else {
            let c = egui::Color32::from_rgb(28, 28, 34);
            [
                c.r() as f32 / 255.0,
                c.g() as f32 / 255.0,
                c.b() as f32 / 255.0,
                1.0,
            ]
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        self.handle_input(ctx);
        self.update_phase(ctx);

        match self.phase {
            // ── Hidden: near-transparent hotspot ──────────────────────────
            // On macOS a fully transparent window may not receive pointer
            // events, so fill with 1/255 alpha — visually invisible but
            // keeps the window hittable.
            Phase::Hidden => {
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 1)),
                    )
                    .show(ctx, |_| {});
            }

            // ── Mini / Expanding / Full / Collapsing ───────────────────────
            // All share one 350×520 window.  In Mini the window is positioned
            // so only the top MINI_H px are on-screen — exactly like the Dock.
            // Animation slides the Y position; no resize occurs mid-animation.
            Phase::Mini | Phase::Expanding | Phase::Full | Phase::Collapsing => {
                let hovering_file = ctx.input(|i| !i.raw.hovered_files.is_empty());
                let hover_progress = (self.hover_timer / HOVER_TO_EXPAND_SECS).clamp(0.0, 1.0);
                let dot_color = if self.status.starts_with("Connected") {
                    egui::Color32::from_rgb(50, 210, 100)
                } else {
                    egui::Color32::from_rgb(220, 180, 40)
                };

                let frame = egui::Frame::none()
                    .fill(egui::Color32::from_rgb(28, 28, 34))
                    .rounding(egui::Rounding { nw: 0.0, sw: 0.0, ne: 10.0, se: 0.0 })
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0));

                egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    // ── Top strip (always visible in Mini) ─────────────────
                    // Height = MINI_H.  Shows icon + progress ring + status dot.
                    let strip_rect = egui::Rect::from_min_size(
                        ui.min_rect().min,
                        egui::vec2(ui.available_width(), MINI_H - 12.0),
                    );
                    let center = strip_rect.center();
                    let radius = 20.0;

                    // Background ring
                    ui.painter().circle_stroke(
                        center, radius,
                        egui::Stroke::new(2.0, egui::Color32::from_gray(55)),
                    );
                    // Progress arc (fill as hover_timer advances toward expand)
                    let arc_alpha = (200.0 * hover_progress) as u8;
                    let arc_color = egui::Color32::from_rgba_premultiplied(80, 160, 255, arc_alpha);
                    let segs = 32usize;
                    for seg in 0..(segs as f32 * hover_progress) as usize {
                        let a0 = std::f32::consts::TAU * seg as f32 / segs as f32
                            - std::f32::consts::FRAC_PI_2;
                        let a1 = std::f32::consts::TAU * (seg + 1) as f32 / segs as f32
                            - std::f32::consts::FRAC_PI_2;
                        ui.painter().line_segment(
                            [center + egui::vec2(a0.cos(), a0.sin()) * radius,
                             center + egui::vec2(a1.cos(), a1.sin()) * radius],
                            egui::Stroke::new(2.5, arc_color),
                        );
                    }
                    // Icon
                    ui.painter().text(
                        center - egui::vec2(0.0, 3.0),
                        egui::Align2::CENTER_CENTER,
                        if hovering_file { "📥" } else { "⬆" },
                        egui::FontId::proportional(20.0),
                        egui::Color32::from_gray(210),
                    );
                    // Status dot
                    ui.painter().circle_filled(
                        center + egui::vec2(12.0, 10.0), 4.0, dot_color,
                    );

                    // Allocate space so egui knows we used the strip.
                    ui.allocate_rect(strip_rect, egui::Sense::hover());

                    // ── Below-strip content (only visible once window rises) ─
                    ui.separator();

                    // Header bar
                    ui.horizontal(|ui| {
                        ui.colored_label(dot_color, "●");
                        ui.label(
                            egui::RichText::new(&self.status)
                                .size(11.0)
                                .color(egui::Color32::from_gray(180)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(
                                egui::RichText::new("✕").size(11.0)
                                    .color(egui::Color32::from_gray(150)),
                            ).frame(false)).clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            if ui.add(egui::Button::new(
                                egui::RichText::new("Clear").size(11.0)
                                    .color(egui::Color32::from_gray(150)),
                            ).frame(false)).clicked() {
                                self.items.clear();
                            }
                        });
                    });
                    ui.separator();

                    // Items list
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
                                            egui::RichText::new(format!("📝 {}", truncate(t, 46)))
                                                .size(12.0).color(egui::Color32::from_gray(220)),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button("✕").clicked() { remove = Some(i); }
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
                                            egui::RichText::new(format!("📎 {} ({})", name, fmt_size(data.len())))
                                                .size(12.0).color(egui::Color32::from_gray(220)),
                                        );
                                        let (name, data) = (name.clone(), data.clone());
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button("✕").clicked() { remove = Some(i); }
                                                if ui.small_button("Save").clicked() { save_file(&name, &data); }
                                            },
                                        );
                                    }
                                });
                                ui.separator();
                            }
                            if self.items.is_empty() {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(16.0);
                                    ui.label(egui::RichText::new("No shared items yet.")
                                        .size(12.0).color(egui::Color32::from_gray(100)));
                                });
                            }
                            if let Some(i) = remove { self.items.remove(i); }
                        });

                    // Drop zone
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
                    ui.painter().rect(rect, 6.0, bg, egui::Stroke::new(1.5, border));
                    ui.painter().text(
                        rect.center(), egui::Align2::CENTER_CENTER,
                        if hovering { "Drop to share →" } else { "Drop files  •  Ctrl+V to paste" },
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_gray(130),
                    );
                });
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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
            fonts.families.entry(family).or_default().push("jp".to_owned());
        }
        ctx.set_fonts(fonts);
    }
}
