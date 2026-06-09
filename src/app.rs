use eframe::egui;
use std::sync::mpsc;

use crate::network::SharedItem;
use crate::protocol::Message;

pub struct SShareApp {
    items: Vec<Item>,
    send_tx: mpsc::Sender<Message>,
    recv_rx: mpsc::Receiver<SharedItem>,
    status_rx: mpsc::Receiver<String>,
    status: String,
    clipboard: Option<arboard::Clipboard>,
}

#[derive(Clone)]
enum Item {
    Text(String),
    File { name: String, data: Vec<u8> },
}

impl SShareApp {
    pub fn new(
        _cc: &eframe::CreationContext,
        send_tx: mpsc::Sender<Message>,
        recv_rx: mpsc::Receiver<SharedItem>,
        status_rx: mpsc::Receiver<String>,
    ) -> Self {
        Self {
            items: Vec::new(),
            send_tx,
            recv_rx,
            status_rx,
            status: "Starting…".into(),
            clipboard: arboard::Clipboard::new().ok(),
        }
    }

    fn poll(&mut self) {
        while let Ok(item) = self.recv_rx.try_recv() {
            self.items.push(match item {
                SharedItem::Text(s) => Item::Text(s),
                SharedItem::File { name, data } => Item::File { name, data },
            });
        }
        // Only keep the most recent status message each frame.
        while let Ok(s) = self.status_rx.try_recv() {
            self.status = s;
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Ctrl+V / Cmd+V  ─  paste text
            for ev in &i.events {
                if let egui::Event::Paste(text) = ev {
                    if !text.is_empty() {
                        let _ = self.send_tx.send(Message::Text(text.clone()));
                        self.items.push(Item::Text(text.clone()));
                    }
                }
            }

            // Drag-and-drop files
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
}

impl eframe::App for SShareApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        self.handle_input(ctx);

        // Poll channels at ~10 Hz without busy-spinning.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::CentralPanel::default().show(ctx, |ui| {
            // ── Status bar ─────────────────────────────────────────────────
            ui.horizontal(|ui| {
                let connected = self.status.starts_with("Connected");
                let dot = if connected {
                    egui::Color32::from_rgb(50, 210, 100)
                } else {
                    egui::Color32::from_rgb(220, 180, 40)
                };
                ui.colored_label(dot, "●");
                ui.label(
                    egui::RichText::new(&self.status)
                        .size(11.0)
                        .color(egui::Color32::from_gray(190)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Clear all").clicked() {
                        self.items.clear();
                    }
                });
            });

            ui.separator();

            // ── Shared items ───────────────────────────────────────────────
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
                                            .size(12.0),
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
                                        .size(12.0),
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
                            ui.add_space(20.0);
                            ui.label(
                                egui::RichText::new("No shared items yet.")
                                    .size(12.0)
                                    .color(egui::Color32::from_gray(120)),
                            );
                        });
                    }

                    if let Some(i) = remove_idx {
                        self.items.remove(i);
                    }
                });

            // ── Drop zone ──────────────────────────────────────────────────
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
            let border_color = if hovering {
                egui::Color32::from_rgb(80, 160, 255)
            } else {
                egui::Color32::from_gray(90)
            };

            ui.painter().rect(
                rect,
                6.0,
                bg,
                egui::Stroke::new(1.5, border_color),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if hovering {
                    "Drop to share →"
                } else {
                    "Drop files here  •  Ctrl+V to paste text"
                },
                egui::FontId::proportional(12.0),
                egui::Color32::from_gray(150),
            );
        });
    }
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
