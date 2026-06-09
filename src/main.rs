use clap::Parser;
use std::sync::mpsc;
use std::thread;

mod app;
mod network;
mod protocol;

/// Share files and clipboard text between SSH-connected machines in real time.
#[derive(Parser)]
#[command(name = "sshare", version)]
struct Args {
    /// Port to listen on (server mode, default)
    #[arg(long, short, default_value = "7878")]
    port: u16,

    /// Remote address to connect to, e.g. 10.0.0.5:7878  (client mode)
    #[arg(long, short)]
    connect: Option<String>,
}

fn main() -> eframe::Result {
    let args = Args::parse();

    let (to_gui_tx, to_gui_rx) = mpsc::channel::<network::SharedItem>();
    let (from_gui_tx, from_gui_rx) = mpsc::channel::<protocol::Message>();
    let (status_tx, status_rx) = mpsc::channel::<String>();

    let mode = match args.connect {
        Some(addr) => network::Mode::Client(addr),
        None => network::Mode::Server(args.port),
    };

    thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
            .block_on(network::run(mode, to_gui_tx, from_gui_rx, status_tx));
    });

    eframe::run_native(
        "SShare",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("SShare")
                .with_inner_size([app::MINI_W, app::MINI_H])
                .with_min_inner_size([app::MINI_W, app::MINI_H])
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top(),
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(app::SShareApp::new(
                cc,
                from_gui_tx,
                to_gui_rx,
                status_rx,
            )))
        }),
    )
}
