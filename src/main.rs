#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod engine;
mod hid;
mod icon;
mod model;
mod razer;
mod settings;
mod theme;
mod tray;

use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

fn main() {
    let start_hidden = std::env::args().any(|argument| argument == "--tray");
    if let Err(error) = run(start_hidden) {
        show_error(&format!("MousePilot 无法启动：\n{error}"));
    }
}

fn run(start_hidden: bool) -> eframe::Result<()> {
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([900.0, 600.0])
        .with_min_inner_size([780.0, 540.0])
        .with_position([70.0, 40.0])
        .with_icon(icon::egui_level(None, false, 20))
        .with_visible(!start_hidden)
        .with_app_id("MousePilot");

    let options = eframe::NativeOptions {
        viewport,
        centered: false,
        vsync: true,
        ..Default::default()
    };

    eframe::run_native(
        "MousePilot · 雷蛇电量与 DPI",
        options,
        Box::new(move |cc| Ok(Box::new(app::MousePilotApp::new(cc, start_hidden)))),
    )
}

fn show_error(message: &str) {
    let title: Vec<u16> = "MousePilot"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let body: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut::<std::ffi::c_void>(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
