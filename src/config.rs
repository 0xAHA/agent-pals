use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static BASE: OnceLock<PathBuf> = OnceLock::new();

/// Folder holding config.json, profiles/ and themes/.
/// Prefers the exe folder; falls back to the working dir (handy for `cargo run`).
pub fn base_dir() -> &'static Path {
    BASE.get_or_init(|| {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        if let Some(d) = &exe_dir {
            if d.join("profiles").is_dir() {
                return d.clone();
            }
        }
        std::env::current_dir()
            .ok()
            .filter(|d| d.join("profiles").is_dir())
            .or(exe_dir)
            .unwrap_or_else(|| PathBuf::from("."))
    })
}

pub fn log(msg: impl AsRef<str>) {
    let path = base_dir().join("agent-pals.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", msg.as_ref());
    }
}

/// Keep the log from growing forever.
pub fn trim_log() {
    let path = base_dir().join("agent-pals.log");
    if std::fs::metadata(&path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(path);
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub scale: u32,
    pub fps: u32,
    pub max_agents: usize,
    /// Logical (unscaled) width of the strip the pals live on.
    pub strip_width: u32,
    pub margin_px: i32,
    pub visible: bool,
    /// "notify" (offer in the tray), "auto" (install when no agent is busy) or "off".
    pub updates: String,
    /// Seconds a pal may sit idle before it walks off (it returns with its agent's next event).
    /// 0 keeps it on stage.
    pub hide_idle_secs: u64,
    /// Where along the taskbar the strip sits: "left", "center" or "right".
    pub position: String,
}

/// Where the strip sits above the taskbar, and so which edge pals line up from and walk off by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    Left,
    Center,
    Right,
}

impl Placement {
    pub fn parse(s: &str) -> Self {
        match s {
            "left" => Placement::Left,
            "center" | "centre" | "middle" => Placement::Center,
            _ => Placement::Right,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: crate::sprites::RANDOM.into(),
            scale: 2,
            fps: 10,
            max_agents: 8,
            strip_width: 160,
            margin_px: 8,
            visible: true,
            updates: "notify".into(),
            hide_idle_secs: 120,
            position: "right".into(),
        }
    }
}

impl Config {
    pub fn load(dir: &Path) -> Self {
        match std::fs::read_to_string(dir.join("config.json")) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                log(format!("config.json invalid, using defaults: {e}"));
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, dir: &Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            if let Err(e) = std::fs::write(dir.join("config.json"), s) {
                log(format!("config save failed: {e}"));
            }
        }
    }
}
