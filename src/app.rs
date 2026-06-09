use eframe::egui;
use std::sync::mpsc;

use crate::network::SharedItem;
use crate::protocol::Message;

// ── Layout constants (pub so main.rs can use them for initial window size) ──
pub const TAB_W: f32 = 28.0;
pub const TAB_H: f32 = 80.0;
const FULL_W: f32 = 340.0;
const FULL_H: f32 = 520.0;

/// How many seconds after the cursor leaves before collapsing.
const LEAVE_DELAY: f32 = 0.8;
/// Animation speed (fraction of the range per second).
const ANIM_SPEED: f32 = 8.0;

pub struct SShareApp {
    items: Vec<Item>,
    send_tx: mpsc::Sender<Message>,
    recv_rx: mpsc::Receiver<SharedItem>,
    status_rx: mpsc::Receiver<String>,
    status: String,
    clipboard: Option<arboard::Clipboard>,
    /// 0.0 = fully collapsed (tab only), 1.0 = fully expanded.
    anim_t: f32,
    /// Seconds since the cursor last left the window.
    leave_timer: f32,
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
            anim_t: 0.0,
            leave_timer: 0.0,
        }
    }

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

    /// Drive the show/hide animation and update the OS window geometry.
    fn update_animation(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let cursor_in = ctx.input(|i| i.pointer.has_pointer());

        if cursor_in {
            self.leave_timer = 0.0;
            self.anim_t = (self.anim_t + dt * ANIM_SPEED).min(1.0);
        } else {
            self.leave_timer += dt;
            if self.leave_timer >= LEAVE_DELAY {
                self.anim_t = (self.anim_t - dt * ANIM_SPEED).max(0.0);
            }
        }

        // Keep repainting while animating or while the collapse delay is ticking.
        let animating = self.anim_t > 0.001 && self.anim_t < 0.999;
        let waiting = !cursor_in && self.leave_timer < LEAVE_DELAY && self.anim_t > 0.001;
        if animating || waiting {
            ctx.request_repaint();
        } else {
            // Still poll channels while idle.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Compute current geometry with an ease-out curve.
        let t = ease_out_cubic(self.anim_t);
        let w = TAB_W + (FULL_W - TAB_W) * t;
        let h = TAB_H + (FULL_H - TAB_H) * t;

        // Anchor to bottom-left of the monitor.
        let monitor_h = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1440.0, 900.0))
            .y;

        let pos = egui::pos2(0.0, monitor_h - h);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
    }
}

impl eframe::App for SShareApp {
    // Transparent clear colour lets the egui fill colour be the actual background.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        self.handle_input(ctx);
        self.update_animation(ctx);

        let t = ease_out_cubic(self.anim_t);
        let cur_w = TAB_W + (FULL_W - TAB_W) * t;

        // ── Panel background (rounded left side only when expanded) ──────────
        let bg = egui::Color32::from_rgb(30, 30, 35);
        let panel_frame = egui::Frame::none()
            .fill(bg)
            .rounding(egui::Rounding {
                nw: 0.0,
                sw: 0.0,
                ne: if t > 0.05 { 10.0 } else { 0.0 },
                se: if t > 0.05 { 10.0 } else { 0.0 },
            })
            .inner_margin(if cur_w > TAB_W + 20.0 {
                egui::Margin::symmetric(8.0, 6.0)
            } else {
                egui::Margin::symmetric(0.0, 0.0)
            });

        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ctx, |ui| {
                // ── Collapsed tab view ─────────────────────────────────────
                if cur_w < TAB_W + 20.0 {
                    ui.vertical_centered(|ui| {
                        ui.add_space(18.0);
                        ui.label(
                            egui::RichText::new("≡")
                                .size(18.0)
                                .color(egui::Color32::from_gray(200)),
                        );
                        ui.add_space(6.0);
                        // Connection status dot
                        let dot_color = if self.status.starts_with("Connected") {
                            egui::Color32::from_rgb(50, 210, 100)
                        } else {
                            egui::Color32::from_rgb(220, 180, 40)
                        };
                        ui.colored_label(dot_color, "●");
                    });
                    return;
                }

                // ── Expanded view ──────────────────────────────────────────

                // Fade in content after the window is wide enough.
                let content_alpha = ((cur_w - TAB_W - 20.0) / 60.0).clamp(0.0, 1.0);
                let fade = |base: egui::Color32| -> egui::Color32 {
                    egui::Color32::from_rgba_unmultiplied(
                        base.r(), base.g(), base.b(),
                        (base.a() as f32 * content_alpha) as u8,
                    )
                };

                // Status bar
                ui.horizontal(|ui| {
                    let connected = self.status.starts_with("Connected");
                    let dot_color = if connected {
                        egui::Color32::from_rgb(50, 210, 100)
                    } else {
                        egui::Color32::from_rgb(220, 180, 40)
                    };
                    ui.colored_label(fade(dot_color), "●");
                    ui.label(
                        egui::RichText::new(&self.status)
                            .size(11.0)
                            .color(fade(egui::Color32::from_gray(190))),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Close button
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new("✕")
                                    .size(11.0)
                                    .color(fade(egui::Color32::from_gray(160))),
                            ).frame(false))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui
                            .add(egui::Button::new(
                                egui::RichText::new("Clear")
                                    .size(11.0)
                                    .color(fade(egui::Color32::from_gray(160))),
                            ).frame(false))
                            .clicked()
                        {
                            self.items.clear();
                        }
                    });
                });

                ui.add(egui::Separator::default());

                // Items list
                let list_height = ui.available_height() - 64.0;
                egui::ScrollArea::vertical()
                    .max_height(list_height)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let mut remove_idx: Option<usize> = None;
                        for (i, item) in self.items.iter().enumerate() {
                            ui.horizontal(|ui| {
                                match item {
                                    Item::Text(t) => {
                                        ui.label(
                                            egui::RichText::new(format!("📝 {}", truncate(t, 46)))
                                                .size(12.0)
                                                .color(fade(egui::Color32::from_gray(220))),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button("✕").clicked() {
                                                    remove_idx = Some(i);
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
                                                "📎 {}  ({})",
                                                name,
                                                fmt_size(data.len())
                                            ))
                                            .size(12.0)
                                            .color(fade(egui::Color32::from_gray(220))),
                                        );
                                        let data = data.clone();
                                        let name = name.clone();
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button("✕").clicked() {
                                                    remove_idx = Some(i);
                                                }
                                                if ui.small_button("Save").clicked() {
                                                    save_file(&name, &data);
                                                }
                                            },
                                        );
                                    }
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
                                        .color(fade(egui::Color32::from_gray(110))),
                                );
                            });
                        }

                        if let Some(i) = remove_idx {
                            self.items.remove(i);
                        }
                    });

                // Drop zone
                ui.add(egui::Separator::default());
                let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Sense::hover(),
                );
                let drop_bg = if hovering {
                    fade(egui::Color32::from_rgba_premultiplied(30, 100, 230, 55))
                } else {
                    fade(egui::Color32::from_rgba_premultiplied(80, 80, 80, 25))
                };
                let border_col = if hovering {
                    fade(egui::Color32::from_rgb(80, 160, 255))
                } else {
                    fade(egui::Color32::from_gray(80))
                };
                ui.painter()
                    .rect(rect, 6.0, drop_bg, egui::Stroke::new(1.5, border_col));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if hovering {
                        "Drop to share →"
                    } else {
                        "Drop files here  •  Ctrl+V to paste"
                    },
                    egui::FontId::proportional(12.0),
                    fade(egui::Color32::from_gray(140)),
                );
            });
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
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
        "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
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
