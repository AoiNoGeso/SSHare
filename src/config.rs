use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum ConfigMode {
    #[default]
    Server,
    Client,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub mode: ConfigMode,
    pub port: u16,
    pub server_address: String,
    pub login_startup: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: ConfigMode::Server,
            port: 7878,
            server_address: String::new(),
            login_startup: false,
        }
    }
}

impl Config {
    pub fn load() -> Option<Self> {
        let s = std::fs::read_to_string(config_path()).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }

    pub fn apply_login_startup(&self) {
        let exe = std::env::current_exe().unwrap_or_default();
        set_login_startup(self.login_startup, &exe);
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("SShare")
        .join("config.json")
}

pub fn set_login_startup(enable: bool, exe: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        let agents_dir = match dirs::home_dir() {
            Some(h) => h.join("Library/LaunchAgents"),
            None => return,
        };
        let plist_path = agents_dir.join("com.sshare.app.plist");

        if enable {
            let _ = std::fs::create_dir_all(&agents_dir);
            let content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.sshare.app</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#,
                exe.display()
            );
            if std::fs::write(&plist_path, content).is_ok() {
                let _ = std::process::Command::new("launchctl")
                    .args(["load", plist_path.to_str().unwrap_or("")])
                    .output();
            }
        } else {
            if plist_path.exists() {
                let _ = std::process::Command::new("launchctl")
                    .args(["unload", plist_path.to_str().unwrap_or("")])
                    .output();
                let _ = std::fs::remove_file(&plist_path);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let autostart_dir = match dirs::config_dir() {
            Some(d) => d.join("autostart"),
            None => return,
        };
        let desktop_path = autostart_dir.join("sshare.desktop");

        if enable {
            let _ = std::fs::create_dir_all(&autostart_dir);
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=SShare\nExec={}\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\n",
                exe.display()
            );
            let _ = std::fs::write(desktop_path, content);
        } else {
            let _ = std::fs::remove_file(desktop_path);
        }
    }

    // Suppress unused warning on other platforms
    let _ = enable;
    let _ = exe;
}
