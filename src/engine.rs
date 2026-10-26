use std::collections::BTreeMap;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::hid::{self, HidCollection};
use crate::model::{Device, ReadSource};
use crate::razer;

#[derive(Debug)]
pub enum Command {
    Refresh,
    SetInterval(u64),
    SetDpi {
        id: String,
        control_path: String,
        transaction_id: u8,
        varstore: u8,
        dpi: u32,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum Event {
    ScanStarted,
    ScanFinished(Vec<Device>),
    DpiApplied { id: String, dpi: u32 },
    DpiFailed { id: String, message: String },
}

pub struct Engine {
    commands: Sender<Command>,
}

impl Engine {
    pub fn spawn(refresh_secs: u64) -> (Self, Receiver<Event>) {
        let (commands, command_rx) = unbounded();
        let (events, event_rx) = unbounded();
        let _ = thread::Builder::new()
            .name("mousepilot-devices".to_owned())
            .spawn(move || run(command_rx, events, refresh_secs));
        (Self { commands }, event_rx)
    }

    pub fn refresh(&self) {
        let _ = self.commands.send(Command::Refresh);
    }

    pub fn set_interval(&self, seconds: u64) {
        let _ = self.commands.send(Command::SetInterval(seconds));
    }

    pub fn set_dpi(&self, device: &Device, dpi: u32) {
        let _ = self.commands.send(Command::SetDpi {
            id: device.id.clone(),
            control_path: device.control_path.clone(),
            transaction_id: device.razer_tx,
            varstore: device.razer_varstore,
            dpi,
        });
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
    }
}

fn run(commands: Receiver<Command>, events: Sender<Event>, initial_interval: u64) {
    let mut interval = Duration::from_secs(initial_interval.clamp(15, 1800));
    let mut next_scan = Instant::now();

    loop {
        let wait = next_scan.saturating_duration_since(Instant::now());
        let should_scan = match commands.recv_timeout(wait) {
            Ok(Command::Refresh) => true,
            Ok(Command::SetInterval(seconds)) => {
                interval = Duration::from_secs(seconds.clamp(15, 1800));
                true
            }
            Ok(Command::SetDpi {
                id,
                control_path,
                transaction_id,
                varstore,
                dpi,
            }) => {
                match razer::set_dpi(&control_path, transaction_id, varstore, dpi) {
                    Ok(actual) => {
                        let _ = events.send(Event::DpiApplied { id, dpi: actual });
                    }
                    Err(message) => {
                        let _ = events.send(Event::DpiFailed { id, message });
                    }
                }
                true
            }
            Ok(Command::Shutdown) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => true,
        };

        if !should_scan {
            continue;
        }

        let _ = events.send(Event::ScanStarted);
        let devices = scan();
        let _ = events.send(Event::ScanFinished(devices));
        next_scan = Instant::now() + interval;
    }
}

fn scan() -> Vec<Device> {
    let collections = hid::enumerate();
    if collections.is_empty() {
        return Vec::new();
    }

    let mut razer_groups: BTreeMap<String, Vec<HidCollection>> = BTreeMap::new();
    let mut other_groups: BTreeMap<String, Vec<HidCollection>> = BTreeMap::new();

    for collection in collections {
        let key = collection.group_key();
        if collection.is_razer() {
            razer_groups.entry(key).or_default().push(collection);
        } else {
            other_groups.entry(key).or_default().push(collection);
        }
    }

    let mut devices = Vec::new();

    for (key, group) in razer_groups {
        let Some(probe) = razer::probe(&group) else {
            continue;
        };
        let name = group_name(&group, "雷蛇鼠标");
        let serial = group
            .iter()
            .find_map(|item| (!item.serial.trim().is_empty()).then(|| item.serial.clone()))
            .unwrap_or_default();
        let product_id = group.first().map(|item| item.product_id).unwrap_or(0);
        let connection = if probe.battery.is_some() {
            "2.4G 无线".to_owned()
        } else {
            "USB 有线".to_owned()
        };
        let note = if probe.battery.is_none() {
            Some("固件未报告电量；DPI 控制仍然可用".to_owned())
        } else {
            None
        };

        devices.push(Device {
            id: format!("razer:{key}"),
            name,
            vendor_id: 0x1532,
            product_id,
            serial,
            battery: probe.battery,
            charging: probe.charging,
            dpi: probe.dpi,
            supports_dpi: probe.supports_dpi,
            source: ReadSource::Razer,
            connection,
            note,
            control_path: probe.path,
            razer_tx: probe.transaction_id,
            razer_varstore: probe.varstore,
        });
    }

    for (key, group) in other_groups {
        if !group.iter().any(|item| item.is_mouse()) {
            continue;
        }

        let battery = group
            .iter()
            .find_map(|collection| hid::read_standard_battery(collection));

        let Some(level) = battery else {
            continue;
        };

        devices.push(Device {
            id: format!("hid:{key}"),
            name: group_name(&group, "2.4G 无线鼠标"),
            vendor_id: group.first().map(|item| item.vendor_id).unwrap_or(0),
            product_id: group.first().map(|item| item.product_id).unwrap_or(0),
            serial: group
                .iter()
                .find_map(|item| (!item.serial.trim().is_empty()).then(|| item.serial.clone()))
                .unwrap_or_default(),
            battery: Some(level),
            charging: false,
            dpi: None,
            supports_dpi: false,
            source: ReadSource::StandardHid,
            connection: "2.4G 无线".to_owned(),
            note: Some("标准 HID 电量接口；该设备未提供 DPI 控制协议".to_owned()),
            control_path: String::new(),
            razer_tx: 0,
            razer_varstore: 1,
        });
    }

    devices.sort_by(|a, b| {
        let a_razer = a.source == ReadSource::Razer;
        let b_razer = b.source == ReadSource::Razer;
        b_razer
            .cmp(&a_razer)
            .then_with(|| b.battery.is_some().cmp(&a.battery.is_some()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    devices
}

fn group_name(group: &[HidCollection], fallback: &str) -> String {
    group
        .iter()
        .map(HidCollection::display_name)
        .find(|name| {
            let lower = name.to_lowercase();
            !name.trim().is_empty()
                && !lower.contains("usb receiver")
                && !lower.contains("wireless receiver")
        })
        .or_else(|| {
            group
                .iter()
                .map(HidCollection::display_name)
                .find(|name| !name.trim().is_empty())
        })
        .unwrap_or_else(|| fallback.to_owned())
}
