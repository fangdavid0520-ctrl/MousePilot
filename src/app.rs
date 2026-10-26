use std::f32::consts::{PI, TAU};
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Response, RichText,
    Sense, Stroke, StrokeKind,
};

use crate::autostart;
use crate::engine::{Engine, Event};
use crate::icon;
use crate::model::Device;
use crate::settings::Settings;
use crate::theme;
use crate::tray::{Tray, TrayMessage};

pub struct MousePilotApp {
    engine: Engine,
    events: Receiver<Event>,
    tray_events: Receiver<TrayMessage>,
    tray: Option<Tray>,
    devices: Vec<Device>,
    selected_id: Option<String>,
    scanning: bool,
    last_scan: Option<Instant>,
    dpi_draft: u32,
    dpi_device_id: Option<String>,
    dpi_dirty: bool,
    dpi_busy: bool,
    settings: Settings,
    startup_enabled: bool,
    toast: Option<Toast>,
    preview: Option<egui::TextureHandle>,
    preview_key: Option<(Option<u8>, bool, u8)>,
    quitting: bool,
}

struct Toast {
    text: String,
    error: bool,
    at: Instant,
}

impl MousePilotApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start_hidden: bool) -> Self {
        theme::install(&cc.egui_ctx);

        let settings = Settings::load();
        let startup_enabled = autostart::is_enabled();
        let (engine, events) = Engine::spawn(settings.refresh_secs);
        cc.egui_ctx
            .request_repaint_after(Duration::from_millis(120));

        let (tray, tray_events, tray_error) =
            match Tray::create(&cc.egui_ctx, None, false, settings.low_battery) {
                Ok((tray, receiver)) => (Some(tray), receiver, None),
                Err(error) => {
                    let (sender, receiver) = unbounded();
                    drop(sender);
                    (None, receiver, Some(error))
                }
            };

        if start_hidden && tray.is_none() {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }

        Self {
            engine,
            events,
            tray_events,
            tray,
            devices: Vec::new(),
            selected_id: None,
            scanning: true,
            last_scan: None,
            dpi_draft: 800,
            dpi_device_id: None,
            dpi_dirty: false,
            dpi_busy: false,
            settings: settings.clone(),
            startup_enabled,
            toast: tray_error.map(|error| Toast {
                text: format!("托盘初始化失败：{error}"),
                error: true,
                at: Instant::now(),
            }),
            preview: None,
            preview_key: None,
            quitting: false,
        }
    }

    fn selected(&self) -> Option<&Device> {
        let id = self.selected_id.as_deref()?;
        self.devices.iter().find(|device| device.id.as_str() == id)
    }

    fn selected_owned(&self) -> Option<Device> {
        self.selected().cloned()
    }

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            at: Instant::now(),
        });
    }

    fn poll_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::ScanStarted => {
                    self.scanning = true;
                }
                Event::ScanFinished(devices) => {
                    self.scanning = false;
                    self.last_scan = Some(Instant::now());
                    let had_devices = !self.devices.is_empty();
                    self.devices = devices;
                    self.reconcile_selection();
                    if !had_devices && self.devices.is_empty() {
                        self.notify("未发现支持电量读取的鼠标，请确认接收器已连接", false);
                    }
                }
                Event::DpiApplied { id, dpi } => {
                    self.dpi_busy = false;
                    self.dpi_dirty = false;
                    if let Some(device) = self.devices.iter_mut().find(|device| device.id == id) {
                        device.dpi = Some(dpi);
                    }
                    if self.selected_id.as_deref() == Some(id.as_str()) {
                        self.dpi_draft = dpi;
                    }
                    self.notify(format!("DPI 已切换为 {dpi}"), false);
                    self.engine.refresh();
                }
                Event::DpiFailed { id, message } => {
                    self.dpi_busy = false;
                    if self.selected_id.as_deref() == Some(id.as_str()) {
                        self.notify(format!("DPI 设置失败：{message}"), true);
                    }
                }
            }
        }

        while let Ok(message) = self.tray_events.try_recv() {
            match message {
                TrayMessage::Open => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayMessage::Refresh => {
                    self.scanning = true;
                    self.engine.refresh();
                }
                TrayMessage::Quit => {
                    self.quitting = true;
                }
            }
        }
    }

    fn reconcile_selection(&mut self) {
        let current_valid = self
            .selected_id
            .as_ref()
            .is_some_and(|id| self.devices.iter().any(|device| &device.id == id));

        let saved_id = self.settings.last_device_id.clone();
        let saved_valid =
            !saved_id.is_empty() && self.devices.iter().any(|device| device.id == saved_id);

        let next_id = if current_valid {
            self.selected_id.clone()
        } else if saved_valid {
            Some(saved_id)
        } else {
            self.devices
                .iter()
                .find(|device| device.battery.is_some())
                .or_else(|| self.devices.first())
                .map(|device| device.id.clone())
        };

        if self.selected_id != next_id {
            self.selected_id = next_id.clone();
            self.dpi_device_id = None;
            self.dpi_dirty = false;
            if let Some(id) = next_id {
                self.settings.last_device_id = id;
                self.settings.save();
            }
        }

        let Some(device) = self.selected().cloned() else {
            return;
        };

        if self.dpi_device_id.as_deref() != Some(device.id.as_str()) {
            self.dpi_device_id = Some(device.id.clone());
            self.dpi_draft = device.dpi.unwrap_or(800).clamp(100, 45_000);
            self.dpi_dirty = false;
        } else if !self.dpi_dirty {
            if let Some(dpi) = device.dpi {
                self.dpi_draft = dpi;
            }
        }
    }

    fn refresh_tray(&mut self) {
        let selected = self.selected_owned();
        let level = selected.as_ref().and_then(|device| device.battery);
        let charging = selected
            .as_ref()
            .map(|device| device.charging)
            .unwrap_or(false);
        let low = self.settings.low_battery;

        let tooltip = match &selected {
            Some(device) => {
                let charging = if device.charging { " · 充电中" } else { "" };
                format!(
                    "MousePilot · {} · {}{}",
                    device.name,
                    device.battery_text(),
                    charging
                )
            }
            None if self.scanning => "MousePilot · 正在读取鼠标状态".to_owned(),
            None => "MousePilot · 未发现受支持的鼠标".to_owned(),
        };

        if let Some(tray) = &mut self.tray {
            tray.update(level, charging, low, &tooltip);
        }
    }

    fn update_preview(&mut self, ctx: &egui::Context) {
        let selected = self.selected_owned();
        let level = selected.as_ref().and_then(|device| device.battery);
        let charging = selected
            .as_ref()
            .map(|device| device.charging)
            .unwrap_or(false);
        let low = self.settings.low_battery;
        let key = (level, charging, low);
        if self.preview_key != Some(key) {
            let image = icon::preview_level(level, charging, low);
            self.preview =
                Some(ctx.load_texture("tray-preview", image, egui::TextureOptions::NEAREST));
            self.preview_key = Some(key);
        }
    }

    fn ago_text(&self) -> String {
        let Some(last) = self.last_scan else {
            return "等待首次读取".to_owned();
        };
        let seconds = last.elapsed().as_secs();
        match seconds {
            0..=4 => "刚刚更新".to_owned(),
            5..=59 => format!("{seconds} 秒前更新"),
            _ => format!("{} 分钟前更新", seconds / 60),
        }
    }
}

impl eframe::App for MousePilotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events(ctx);

        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if close_requested && !self.quitting && self.settings.minimize_on_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            return;
        }

        if self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.refresh_tray();
        self.update_preview(ctx);

        let selected = self.selected_owned();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(26, 18)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.header(ui, ctx);
                        ui.add_space(22.0);

                        ui.horizontal_top(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(340.0);
                                self.battery_card(ui, selected.as_ref());
                                ui.add_space(14.0);
                                self.devices_card(ui);
                            });

                            ui.add_space(16.0);

                            ui.vertical(|ui| {
                                self.dpi_card(ui, selected.as_ref());
                                ui.add_space(14.0);
                                self.settings_card(ui);
                            });
                        });
                    });
            });

        self.draw_toast(ctx);
        ctx.request_repaint_after(if self.scanning {
            Duration::from_millis(200)
        } else {
            Duration::from_secs(1)
        });
    }
}

impl MousePilotApp {
    fn header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(width, 44.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), Sense::hover());
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(12), theme::ACCENT);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "M",
                    FontId::proportional(22.0),
                    theme::ACCENT_DARK,
                );

                ui.add_space(11.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(210.0, 44.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("MOUSEPILOT")
                                .size(17.0)
                                .strong()
                                .color(theme::TEXT),
                        );
                        ui.add_space(1.0);
                        ui.label(
                            RichText::new("2.4G 电量 · DPI 控制台")
                                .size(11.0)
                                .color(theme::MUTED),
                        );
                    },
                );

                let target = (ui.max_rect().right() - 300.0).max(ui.cursor().left());
                ui.add_space(target - ui.cursor().left());

                let status = if self.scanning {
                    "读取中"
                } else if self.devices.is_empty() {
                    "未发现设备"
                } else {
                    "已连接"
                };
                let color = if self.scanning {
                    theme::AMBER
                } else if self.devices.is_empty() {
                    theme::MUTED
                } else {
                    theme::ACCENT
                };
                theme::pill(ui, status, color);
                ui.label(
                    RichText::new(self.ago_text())
                        .size(11.0)
                        .color(theme::FAINT),
                );
                if ghost_button(ui, "刷新").clicked() {
                    self.scanning = true;
                    self.engine.refresh();
                }
                if ghost_button(ui, "最小化").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            },
        );
    }

    fn battery_card(&mut self, ui: &mut egui::Ui, device: Option<&Device>) {
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            let row_width = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(row_width, 22.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.label(
                        RichText::new("当前设备")
                            .size(12.0)
                            .strong()
                            .color(theme::MUTED),
                    );
                    let target = (ui.max_rect().right() - 92.0).max(ui.cursor().left());
                    ui.add_space(target - ui.cursor().left());
                    if let Some(device) = device {
                        theme::pill(ui, device.source.label(), theme::FAINT);
                    }
                },
            );

            ui.add_space(8.0);

            let level = device.and_then(|item| item.battery);
            let charging = device.map(|item| item.charging).unwrap_or(false);
            let color = theme::level_color(level, charging, self.settings.low_battery);

            ui.vertical_centered(|ui| {
                let response = battery_dial(ui, level, charging, color);
                if let Some(device) = device {
                    let mut tip = format!(
                        "{}\n{} · {}\n{:04X}:{:04X}",
                        device.name,
                        device.battery_text(),
                        device.connection,
                        device.vendor_id,
                        device.product_id
                    );
                    if !device.serial.trim().is_empty() {
                        tip.push_str(&format!("\nSN {}", device.serial));
                    }
                    let _ = response.on_hover_text(tip);
                }
                ui.add_space(4.0);
                let name = device
                    .map(|item| item.name.as_str())
                    .unwrap_or("未检测到鼠标");
                ui.label(RichText::new(name).size(18.0).strong().color(theme::TEXT));
                ui.add_space(2.0);

                if let Some(device) = device {
                    theme::pill(ui, device.status_text(self.settings.low_battery), color);
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(&device.connection)
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                } else {
                    ui.label(
                        RichText::new(if self.scanning {
                            "正在扫描 HID 设备…"
                        } else {
                            "插入接收器或移动鼠标以唤醒"
                        })
                        .size(12.0)
                        .color(theme::MUTED),
                    );
                }
            });

            if let Some(device) = device {
                if device.is_low(self.settings.low_battery) {
                    ui.add_space(10.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(0xFF, 0x6E, 0x6E, 20))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("电量低于提醒阈值，建议尽快充电")
                                    .size(11.0)
                                    .color(theme::RED),
                            );
                        });
                }

                if let Some(note) = &device.note {
                    ui.add_space(8.0);
                    ui.label(RichText::new(note).size(10.5).color(theme::FAINT));
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            row_with_right(
                ui,
                32.0,
                92.0,
                |ui| {
                    ui.label(
                        RichText::new("通知区域图标实时显示此电量")
                            .size(10.5)
                            .color(theme::FAINT),
                    );
                },
                |ui| {
                    if ghost_button(ui, "重新读取").clicked() {
                        self.scanning = true;
                        self.engine.refresh();
                    }
                },
            );
        });
    }

    fn devices_card(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            row_with_right(
                ui,
                24.0,
                42.0,
                |ui| {
                    ui.label(RichText::new("已连接设备").size(15.0).strong());
                },
                |ui| {
                    ui.label(
                        RichText::new(format!("{} 台", self.devices.len()))
                            .size(11.0)
                            .color(theme::FAINT),
                    );
                },
            );
            ui.add_space(10.0);

            if self.devices.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("未发现支持电量读取的鼠标")
                        .size(13.0)
                        .color(theme::MUTED),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new("雷蛇 2.4G 设备需要插入接收器并唤醒鼠标")
                        .size(11.0)
                        .color(theme::FAINT),
                );
                ui.add_space(12.0);
                if primary_button(ui, "重新扫描", !self.scanning, 130.0).clicked() {
                    self.scanning = true;
                    self.engine.refresh();
                }
            } else {
                let devices = self.devices.clone();
                for device in devices {
                    let selected = self.selected_id.as_deref() == Some(device.id.as_str());
                    if device_row(ui, &device, selected, self.settings.low_battery) {
                        self.selected_id = Some(device.id.clone());
                        self.settings.last_device_id = device.id.clone();
                        self.settings.save();
                        self.dpi_device_id = None;
                        self.dpi_dirty = false;
                        self.reconcile_selection();
                    }
                    ui.add_space(4.0);
                }
            }
        });
    }

    fn dpi_card(&mut self, ui: &mut egui::Ui, device: Option<&Device>) {
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            row_with_right(
                ui,
                24.0,
                96.0,
                |ui| {
                    ui.label(RichText::new("DPI 灵敏度").size(15.0).strong());
                },
                |ui| match device {
                    Some(device) if device.supports_dpi => {
                        theme::pill(ui, "RAZER HID", theme::CYAN)
                    }
                    Some(_) => theme::pill(ui, "只读设备", theme::FAINT),
                    None => theme::pill(ui, "等待设备", theme::FAINT),
                },
            );
            ui.add_space(6.0);

            let Some(device) = device else {
                ui.add_space(18.0);
                ui.label(
                    RichText::new("连接雷蛇鼠标后即可读取并调整 DPI")
                        .size(13.0)
                        .color(theme::MUTED),
                );
                ui.label(
                    RichText::new("雷蛇 2.4G 接收器与有线模式均使用同一套 HID 协议")
                        .size(11.0)
                        .color(theme::FAINT),
                );
                ui.add_space(18.0);
                return;
            };

            if !device.supports_dpi {
                ui.add_space(16.0);
                ui.label(
                    RichText::new("该设备仅通过标准 HID 上报电量")
                        .size(14.0)
                        .color(theme::TEXT),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new("厂商未公开 DPI 控制通道，因此这里保持只读。")
                        .size(11.0)
                        .color(theme::MUTED),
                );
                ui.add_space(16.0);
                return;
            }

            row_with_right(
                ui,
                50.0,
                92.0,
                |ui| {
                    ui.label(
                        RichText::new(format!("{:>4}", self.dpi_draft))
                            .font(FontId::monospace(38.0))
                            .color(theme::ACCENT),
                    );
                    ui.add_space(3.0);
                    ui.label(RichText::new("DPI").size(13.0).color(theme::MUTED));
                },
                |ui| {
                    let current = device
                        .dpi
                        .map(|dpi| format!("当前 {dpi}"))
                        .unwrap_or_else(|| "当前值未知".to_owned());
                    ui.label(RichText::new(current).size(11.0).color(theme::FAINT));
                },
            );

            ui.add_space(6.0);

            let max = device.dpi_max();
            ui.spacing_mut().slider_width = (ui.available_width() - 12.0).max(120.0);
            let slider = egui::Slider::new(&mut self.dpi_draft, 100..=max)
                .show_value(false)
                .step_by(50.0);
            if ui.add(slider).changed() {
                self.dpi_dirty = true;
            }

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                for preset in [400u32, 800, 1600, 3200, 6400] {
                    if chip(ui, &preset.to_string(), self.dpi_draft == preset).clicked() {
                        self.dpi_draft = preset;
                        self.dpi_dirty = true;
                    }
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let enabled = !self.dpi_busy && !device.control_path.is_empty();
                let label = if self.dpi_busy {
                    "正在写入…"
                } else {
                    "应用 DPI"
                };
                if primary_button(ui, label, enabled, 150.0).clicked() {
                    self.dpi_busy = true;
                    self.engine
                        .set_dpi(device, self.dpi_draft.clamp(100, 45_000));
                }
                if ghost_button(ui, "恢复设备值").clicked() {
                    if let Some(dpi) = device.dpi {
                        self.dpi_draft = dpi;
                        self.dpi_dirty = false;
                    }
                }
                if self.dpi_busy {
                    ui.spinner();
                }
            });

            ui.add_space(6.0);
            ui.label(
                RichText::new("写入鼠标板载配置并立即生效。若失败，请先退出 Razer Synapse。")
                    .size(10.5)
                    .color(theme::FAINT),
            );
        });
    }

    fn settings_card(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("偏好设置").size(15.0).strong());
            ui.add_space(10.0);

            ui.label(
                RichText::new("电量刷新")
                    .size(11.0)
                    .strong()
                    .color(theme::MUTED),
            );
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                for (label, seconds) in [
                    ("30 秒", 30u64),
                    ("1 分钟", 60),
                    ("2 分钟", 120),
                    ("5 分钟", 300),
                ] {
                    if chip(ui, label, self.settings.refresh_secs == seconds).clicked() {
                        self.settings.refresh_secs = seconds;
                        self.settings.save();
                        self.engine.set_interval(seconds);
                    }
                }
            });

            ui.add_space(10.0);
            row_with_right(
                ui,
                22.0,
                62.0,
                |ui| {
                    ui.label(
                        RichText::new("低电量提醒")
                            .size(11.0)
                            .strong()
                            .color(theme::MUTED),
                    );
                },
                |ui| {
                    ui.label(
                        RichText::new(format!("≤ {}%", self.settings.low_battery))
                            .size(11.0)
                            .color(theme::AMBER),
                    );
                },
            );
            ui.spacing_mut().slider_width = (ui.available_width() - 12.0).max(100.0);
            let slider = egui::Slider::new(&mut self.settings.low_battery, 5..=50)
                .show_value(false)
                .step_by(1.0);
            if ui.add(slider).changed() {
                self.settings.sanitize();
                self.settings.save();
            }

            ui.add_space(8.0);
            separators(ui);
            ui.add_space(8.0);

            let minimize = toggle_row(
                ui,
                "关闭窗口时最小化到任务栏",
                &mut self.settings.minimize_on_close,
            );
            if minimize {
                self.settings.save();
            }

            let mut startup = self.startup_enabled;
            if toggle_row(ui, "开机自动启动", &mut startup) {
                match autostart::set_enabled(startup) {
                    Ok(()) => {
                        self.startup_enabled = startup;
                        self.notify(
                            if startup {
                                "已加入开机启动"
                            } else {
                                "已取消开机启动"
                            },
                            false,
                        );
                    }
                    Err(error) => self.notify(error, true),
                }
            }

            ui.add_space(8.0);
            separators(ui);
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if let Some(preview) = &self.preview {
                    ui.image(egui::load::SizedTexture::new(
                        preview.id(),
                        egui::vec2(48.0, 48.0),
                    ));
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("通知区域图标").size(11.0).color(theme::TEXT));
                    ui.label(
                        RichText::new("图标数字 = 当前鼠标电量")
                            .size(10.0)
                            .color(theme::FAINT),
                    );
                });
            });

            ui.add_space(8.0);
            ui.label(
                RichText::new("所有数据仅在本机通过 HID 读取，不经过网络。")
                    .size(10.0)
                    .color(theme::FAINT),
            );
        });
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else {
            return;
        };
        if toast.at.elapsed() >= Duration::from_secs(4) {
            self.toast = None;
            return;
        }

        let color = if toast.error {
            theme::RED
        } else {
            theme::ACCENT
        };
        egui::Area::new(egui::Id::new("mousepilot-toast"))
            .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -22.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x1A, 0x22, 0x2B))
                    .stroke(Stroke::new(1.0_f32, color))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(7.0, 7.0), Sense::hover());
                            ui.painter().circle_filled(rect.center(), 3.5, color);
                            ui.label(RichText::new(&toast.text).size(12.0).color(theme::TEXT));
                        });
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn battery_dial(ui: &mut egui::Ui, level: Option<u8>, charging: bool, color: Color32) -> Response {
    let size = 160.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    let radius = 62.0;
    let ring = Stroke::new(9.0_f32, Color32::from_rgb(0x22, 0x2B, 0x36));

    painter.circle_stroke(center, radius, ring);

    if level.is_some() {
        let fraction = level.unwrap_or(0) as f32 / 100.0;
        if fraction >= 0.995 {
            painter.circle_stroke(center, radius, Stroke::new(9.0_f32, color));
        } else if fraction > 0.01 {
            let start = -PI / 2.0;
            let end = start + fraction * TAU;
            let points: Vec<Pos2> = (0..=80)
                .map(|index| {
                    let angle = start + (end - start) * index as f32 / 80.0;
                    center + egui::vec2(angle.cos(), angle.sin()) * radius
                })
                .collect();
            if let Some(last) = points.last() {
                painter.circle_filled(*last, 4.5, color);
            }
            painter.add(egui::Shape::line(points, Stroke::new(9.0_f32, color)));
        }
    }

    let text = match level {
        Some(value) => format!("{value}%"),
        None => "--".to_owned(),
    };
    painter.text(
        center + egui::vec2(0.0, -4.0),
        Align2::CENTER_CENTER,
        text,
        FontId::monospace(if level == Some(100) { 34.0 } else { 40.0 }),
        theme::TEXT,
    );
    painter.text(
        center + egui::vec2(0.0, 30.0),
        Align2::CENTER_CENTER,
        if charging { "充电中" } else { "电量" },
        FontId::proportional(10.5),
        if charging { theme::CYAN } else { theme::FAINT },
    );

    response
}

fn device_row(ui: &mut egui::Ui, device: &Device, selected: bool, low: u8) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 62.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();

    let fill = if selected {
        Color32::from_rgb(0x19, 0x21, 0x2A)
    } else if response.hovered() {
        Color32::from_rgb(0x16, 0x1D, 0x26)
    } else {
        Color32::from_rgb(0x13, 0x19, 0x21)
    };
    painter.rect_filled(rect, CornerRadius::same(12), fill);
    if selected {
        painter.rect_stroke(
            rect,
            CornerRadius::same(12),
            Stroke::new(
                1.0_f32,
                Color32::from_rgba_unmultiplied(0xC4, 0xF0, 0x58, 140),
            ),
            StrokeKind::Inside,
        );
    }

    let color = theme::level_color(device.battery, device.charging, low);
    let bar = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(0.0, 16.0),
        egui::vec2(3.0, 30.0),
    );
    painter.rect_filled(bar, CornerRadius::same(2), color);

    painter.text(
        rect.left_top() + egui::vec2(16.0, 12.0),
        Align2::LEFT_TOP,
        &device.name,
        FontId::proportional(13.0),
        theme::TEXT,
    );
    painter.text(
        rect.left_top() + egui::vec2(16.0, 34.0),
        Align2::LEFT_TOP,
        format!("{} · {}", device.source.short(), device.connection),
        FontId::proportional(10.5),
        theme::FAINT,
    );
    painter.text(
        rect.right_center() - egui::vec2(14.0, 0.0),
        Align2::RIGHT_CENTER,
        device.battery_text(),
        FontId::monospace(17.0),
        if device.battery.is_some() {
            color
        } else {
            theme::FAINT
        },
    );

    response.clicked()
}

fn ghost_button(ui: &mut egui::Ui, label: &str) -> Response {
    let width = 24.0 + label.chars().count() as f32 * 13.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 32.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    let hovered = response.hovered();
    painter.rect_filled(
        rect,
        CornerRadius::same(10),
        if hovered {
            Color32::from_rgb(0x1B, 0x24, 0x2E)
        } else {
            Color32::TRANSPARENT
        },
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(
            1.0_f32,
            if hovered {
                theme::BORDER
            } else {
                Color32::TRANSPARENT
            },
        ),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.5),
        if hovered { theme::TEXT } else { theme::MUTED },
    );
    response
}

fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 38.0), Sense::click());
    let response = if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    let painter = ui.painter();
    let hovered = enabled && response.hovered();
    painter.rect_filled(
        rect,
        CornerRadius::same(11),
        if !enabled {
            Color32::from_rgb(0x25, 0x2E, 0x38)
        } else if hovered {
            Color32::from_rgb(0xD6, 0xFA, 0x78)
        } else {
            theme::ACCENT
        },
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.5),
        if enabled {
            theme::ACCENT_DARK
        } else {
            theme::FAINT
        },
    );
    response
}

fn chip(ui: &mut egui::Ui, label: &str, selected: bool) -> Response {
    let width = 20.0 + label.chars().count() as f32 * 11.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    let hovered = response.hovered();
    let fill = if selected {
        theme::ACCENT
    } else if hovered {
        Color32::from_rgb(0x1C, 0x25, 0x2F)
    } else {
        Color32::from_rgb(0x15, 0x1C, 0x24)
    };
    painter.rect_filled(rect, CornerRadius::same(9), fill);
    if !selected {
        painter.rect_stroke(
            rect,
            CornerRadius::same(9),
            Stroke::new(1.0_f32, theme::BORDER),
            StrokeKind::Inside,
        );
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.5),
        if selected {
            theme::ACCENT_DARK
        } else {
            theme::MUTED
        },
    );
    response
}

fn toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.5).color(theme::TEXT));
        let target = (ui.max_rect().right() - 42.0).max(ui.cursor().left());
        ui.add_space(target - ui.cursor().left());
        let (rect, response) = ui.allocate_exact_size(egui::vec2(42.0, 23.0), Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let painter = ui.painter();
        let on = *value;
        painter.rect_filled(
            rect,
            CornerRadius::same(12),
            if on {
                theme::ACCENT
            } else {
                Color32::from_rgb(0x2A, 0x35, 0x41)
            },
        );
        let knob_x = if on {
            rect.right() - 12.0
        } else {
            rect.left() + 12.0
        };
        painter.circle_filled(
            Pos2::new(knob_x, rect.center().y),
            7.5,
            if on { theme::ACCENT_DARK } else { theme::MUTED },
        );
        if response.clicked() {
            *value = !*value;
            changed = true;
        }
    });
    changed
}

fn row_with_right(
    ui: &mut egui::Ui,
    height: f32,
    right_width: f32,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui),
) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        Layout::left_to_right(Align::Center),
        |ui| {
            left(ui);
            let target = (ui.max_rect().right() - right_width).max(ui.cursor().left());
            ui.add_space(target - ui.cursor().left());
            right(ui);
        },
    );
}

fn separators(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::ZERO,
        Color32::from_rgb(0x22, 0x2B, 0x35),
    );
}
