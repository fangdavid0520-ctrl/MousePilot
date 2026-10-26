use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub refresh_secs: u64,
    pub low_battery: u8,
    #[serde(alias = "minimize_to_tray")]
    pub minimize_on_close: bool,
    pub last_device_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_secs: 60,
            low_battery: 20,
            minimize_on_close: true,
            last_device_id: String::new(),
        }
    }
}

impl Settings {
    pub fn sanitize(&mut self) {
        self.refresh_secs = self.refresh_secs.clamp(15, 1800);
        self.low_battery = self.low_battery.clamp(5, 50);
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        let mut parsed: Self = serde_json::from_str(&raw).unwrap_or_default();
        parsed.sanitize();
        parsed
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, raw);
        }
    }

    fn path() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())?;
        Some(base.join("MousePilot").join("settings.json"))
    }
}
