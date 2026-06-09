use std::sync::mpsc;
use std::thread;

mod app;
mod config;
mod discovery;
mod network;
mod protocol;

fn main() -> eframe::Result {
    let saved = config::Config::load();
    let needs_setup = saved.is_none();

    let (to_gui_tx, to_gui_rx) = mpsc::channel::<network::SharedItem>();
    let (from_gui_tx, from_gui_rx) = mpsc::channel::<protocol::Message>();
    let (status_tx, status_rx) = mpsc::channel::<String>();
    let (net_cfg_tx, net_cfg_rx) = mpsc::channel::<config::Config>();

    // If already configured, start the network thread immediately.
    if let Some(ref cfg) = saved {
        let _ = net_cfg_tx.send(cfg.clone());
    }

    thread::spawn(move || {
        let cfg = match net_cfg_rx.recv() {
            Ok(c) => c,
            Err(_) => return,
        };
        let mode = match cfg.mode {
            config::ConfigMode::Server => network::Mode::Server(cfg.port),
            config::ConfigMode::Client => network::Mode::Client(cfg.server_address),
        };
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(network::run(mode, to_gui_tx, from_gui_rx, status_tx));
    });

    // Give the app the sender only when setup is still needed.
    let net_setup_tx = if needs_setup { Some(net_cfg_tx) } else { None };

    let viewport = if needs_setup {
        egui::ViewportBuilder::default()
            .with_title("SShare セットアップ")
            .with_inner_size([480.0, 440.0])
            .with_resizable(false)
    } else {
        egui::ViewportBuilder::default()
            .with_title("SShare")
            .with_inner_size([app::MINI_W, app::MINI_H])
            .with_min_inner_size([app::MINI_W, app::MINI_H])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
    };

    eframe::run_native(
        "SShare",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(app::SShareApp::new(
                cc,
                from_gui_tx,
                to_gui_rx,
                status_rx,
                net_setup_tx,
            )))
        }),
    )
}
