use std::path::PathBuf;

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "MousePilot";

fn executable() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

pub fn is_enabled() -> bool {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ)
    else {
        return false;
    };
    key.get_value::<String, _>(VALUE_NAME)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if enabled {
        let exe = executable().ok_or_else(|| "无法定位程序路径".to_owned())?;
        let command = format!("\"{}\" --tray", exe.display());
        let (key, _) = hkcu
            .create_subkey(RUN_KEY)
            .map_err(|error| format!("无法写入启动项：{error}"))?;
        key.set_value(VALUE_NAME, &command.as_str())
            .map_err(|error| format!("无法写入启动项：{error}"))?;
    } else if let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
        match key.delete_value(VALUE_NAME) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法移除启动项：{error}")),
        }
    }
    Ok(())
}
