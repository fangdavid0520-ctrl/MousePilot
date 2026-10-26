use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::icon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayMessage {
    Open,
    Refresh,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    last_key: Option<(Option<u8>, bool, u8)>,
    last_tooltip: String,
}

impl Tray {
    pub fn create(
        ctx: &egui::Context,
        level: Option<u8>,
        charging: bool,
        low: u8,
    ) -> Result<(Self, Receiver<TrayMessage>), String> {
        let menu = Menu::new();
        let open = MenuItem::new("打开控制台", true, None);
        let refresh = MenuItem::new("立即刷新电量", true, None);
        let quit = MenuItem::new("退出 MousePilot", true, None);
        let separator = PredefinedMenuItem::separator();

        menu.append(&open).map_err(|error| error.to_string())?;
        menu.append(&refresh).map_err(|error| error.to_string())?;
        menu.append(&separator).map_err(|error| error.to_string())?;
        menu.append(&quit).map_err(|error| error.to_string())?;

        let open_id = open.id().clone();
        let refresh_id = refresh.id().clone();
        let quit_id = quit.id().clone();

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("MousePilot · 正在读取鼠标状态")
            .with_icon(icon::tray_level(level, charging, low))
            .build()
            .map_err(|error| error.to_string())?;

        let (sender, receiver) = unbounded();
        install_handlers(
            ctx.clone(),
            sender,
            open_id.clone(),
            refresh_id.clone(),
            quit_id.clone(),
        );

        Ok((
            Self {
                icon: tray,
                last_key: Some((level, charging, low)),
                last_tooltip: "MousePilot · 正在读取鼠标状态".to_owned(),
            },
            receiver,
        ))
    }

    pub fn update(&mut self, level: Option<u8>, charging: bool, low: u8, tooltip: &str) {
        let key = (level, charging, low);
        if self.last_key != Some(key) {
            let _ = self
                .icon
                .set_icon(Some(icon::tray_level(level, charging, low)));
            self.last_key = Some(key);
        }
        if self.last_tooltip != tooltip {
            let _ = self.icon.set_tooltip(Some(tooltip));
            self.last_tooltip = tooltip.to_owned();
        }
    }
}

fn install_handlers(
    ctx: egui::Context,
    sender: Sender<TrayMessage>,
    open_id: MenuId,
    refresh_id: MenuId,
    quit_id: MenuId,
) {
    let sender = Arc::new(sender);
    let icon_sender = Arc::clone(&sender);
    let icon_ctx = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        let message = match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => Some(TrayMessage::Open),
            _ => None,
        };
        if let Some(message) = message {
            let _ = icon_sender.send(message);
            icon_ctx.request_repaint();
        }
    }));

    let menu_sender = sender;
    let menu_ctx = ctx;
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let message = if event.id == open_id {
            Some(TrayMessage::Open)
        } else if event.id == refresh_id {
            Some(TrayMessage::Refresh)
        } else if event.id == quit_id {
            Some(TrayMessage::Quit)
        } else {
            None
        };
        if let Some(message) = message {
            let _ = menu_sender.send(message);
            menu_ctx.request_repaint();
        }
    }));
}
