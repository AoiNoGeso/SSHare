use std::sync::mpsc;

pub fn start_browsing(tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let _ = browse_inner(tx);
    });
}

fn browse_inner(tx: mpsc::Sender<String>) -> anyhow::Result<()> {
    let mdns = mdns_sd::ServiceDaemon::new()?;
    let recv = mdns.browse("_sshare._tcp.local.")?;
    while let Ok(event) = recv.recv() {
        if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
            let port = info.get_port();
            for addr in info.get_addresses() {
                if addr.is_ipv4() {
                    if tx.send(format!("{addr}:{port}")).is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn start_advertising(port: u16) {
    std::thread::spawn(move || {
        let _ = advertise_inner(port);
    });
}

fn advertise_inner(port: u16) -> anyhow::Result<()> {
    let mdns = mdns_sd::ServiceDaemon::new()?;
    let ip = local_ipv4().ok_or_else(|| anyhow::anyhow!("no local IPv4"))?;
    let hostname = get_hostname();
    let ip_str = ip.to_string();

    let info = mdns_sd::ServiceInfo::new(
        "_sshare._tcp.local.",
        "SShare",
        &format!("{hostname}.local."),
        ip_str.as_str(),
        port,
        None::<std::collections::HashMap<String, String>>,
    )?;
    mdns.register(info)?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

fn local_ipv4() -> Option<std::net::Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v4) => Some(v4),
        _ => None,
    }
}

fn get_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "sshare".to_string())
}
