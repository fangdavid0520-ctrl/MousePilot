use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadSource {
    Razer,
    StandardHid,
}

impl ReadSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Razer => "RAZER HID",
            Self::StandardHid => "HID BATTERY",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::Razer => "雷蛇协议",
            Self::StandardHid => "标准 HID",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: String,
    pub battery: Option<u8>,
    pub charging: bool,
    pub dpi: Option<u32>,
    pub supports_dpi: bool,
    pub source: ReadSource,
    pub connection: String,
    pub note: Option<String>,
    /// Internal Razer control collection. Empty for standard HID devices.
    pub control_path: String,
    pub razer_tx: u8,
    pub razer_varstore: u8,
}

impl Device {
    pub fn battery_text(&self) -> String {
        self.battery
            .map(|b| format!("{b}%"))
            .unwrap_or_else(|| "--".to_owned())
    }

    pub fn is_low(&self, threshold: u8) -> bool {
        matches!(self.battery, Some(b) if b <= threshold && !self.charging)
    }

    pub fn status_text(&self, threshold: u8) -> &'static str {
        match self.battery {
            Some(_) if self.charging => "正在充电",
            Some(b) if b <= 10 => "电量告急",
            Some(b) if b <= threshold => "建议充电",
            Some(_) => "电量充足",
            None => "未报告电量",
        }
    }

    pub fn dpi_max(&self) -> u32 {
        let current = self.dpi.unwrap_or(0);
        if current > 30_000 {
            45_000
        } else {
            30_000
        }
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.battery_text())
    }
}
