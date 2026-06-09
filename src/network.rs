use std::sync::mpsc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc as tmpc;
use tokio::time::{sleep, Duration};

use crate::discovery;
use crate::protocol::{read_msg, write_msg, Message};

pub enum Mode {
    Server(u16),
    Client(String),
}

/// Items that arrive from the remote peer and are displayed in the GUI.
#[derive(Debug, Clone)]
pub enum SharedItem {
    Text(String),
    File { name: String, data: Vec<u8> },
}

/// Main network loop.  Runs inside a dedicated tokio runtime on a background thread.
pub async fn run(
    mode: Mode,
    to_gui: mpsc::Sender<SharedItem>,
    from_gui: mpsc::Receiver<Message>,
    status: mpsc::Sender<String>,
) {
    // Bridge the blocking std Receiver into an async tokio channel so the
    // write-half of every connection can await on it.
    let (bridge_tx, mut bridge_rx) = tmpc::unbounded_channel::<Message>();
    std::thread::spawn(move || {
        while let Ok(msg) = from_gui.recv() {
            if bridge_tx.send(msg).is_err() {
                break;
            }
        }
    });

    let s = |msg: String| { let _ = status.send(msg); };

    match mode {
        Mode::Server(port) => {
            match TcpListener::bind(format!("0.0.0.0:{port}")).await {
                Err(e) => s(format!("Failed to bind port {port}: {e}")),
                Ok(listener) => {
                    s(format!("Listening on port {port}"));
                    discovery::start_advertising(port);
                    loop {
                        match listener.accept().await {
                            Ok((stream, peer)) => {
                                s(format!("Connected: {peer}"));
                                handle_conn(stream, to_gui.clone(), &mut bridge_rx).await;
                                s("Disconnected. Waiting for connection…".into());
                            }
                            Err(e) => s(format!("Accept error: {e}")),
                        }
                    }
                }
            }
        }
        Mode::Client(addr) => loop {
            s(format!("Connecting to {addr}…"));
            match TcpStream::connect(&addr).await {
                Ok(stream) => {
                    s(format!("Connected to {addr}"));
                    handle_conn(stream, to_gui.clone(), &mut bridge_rx).await;
                    s("Disconnected. Retrying in 3 s…".into());
                }
                Err(e) => s(format!("Connection failed: {e}. Retrying in 3 s…")),
            }
            sleep(Duration::from_secs(3)).await;
        },
    }
}

/// Drive a single connection until either side closes it.
async fn handle_conn(
    stream: TcpStream,
    to_gui: mpsc::Sender<SharedItem>,
    from_gui: &mut tmpc::UnboundedReceiver<Message>,
) {
    let (mut rd, mut wr) = stream.into_split();

    // Signal channel: reader task notifies writer loop when the peer disconnects.
    let (disc_tx, mut disc_rx) = tmpc::channel::<()>(1);

    let reader = tokio::spawn(async move {
        loop {
            match read_msg(&mut rd).await {
                Ok(msg) => {
                    let item = match msg {
                        Message::Text(s) => SharedItem::Text(s),
                        Message::File { name, data } => SharedItem::File { name, data },
                        Message::Ping => continue,
                    };
                    if to_gui.send(item).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = disc_tx.send(()).await;
                    break;
                }
            }
        }
    });

    // Writer loop: pull messages queued by the GUI and send them to the peer.
    loop {
        tokio::select! {
            msg = from_gui.recv() => {
                match msg {
                    Some(m) => {
                        if write_msg(&mut wr, &m).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = disc_rx.recv() => break,
        }
    }

    reader.abort();
}
