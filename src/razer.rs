use std::thread;
use std::time::Duration;

use crate::hid::{self, HidCollection};

const REPORT_SIZE: usize = 90;
const FEATURE_SIZE: usize = REPORT_SIZE + 1;
const STATUS_BUSY: u8 = 0x01;
const STATUS_SUCCESS: u8 = 0x02;
const STATUS_NOT_SUPPORTED: u8 = 0x05;
const TX_CANDIDATES: [u8; 3] = [0x1f, 0x3f, 0xff];

#[derive(Clone, Debug)]
pub struct RazerProbe {
    pub path: String,
    pub transaction_id: u8,
    pub varstore: u8,
    pub battery: Option<u8>,
    pub charging: bool,
    pub dpi: Option<u32>,
    pub supports_dpi: bool,
}

#[derive(Debug)]
enum RazerError {
    Io(String),
    Unsupported,
    Rejected(u8),
    Mismatch,
}

impl RazerError {
    fn message(&self) -> String {
        match self {
            Self::Io(message) => message.clone(),
            Self::Unsupported => "鼠标固件不支持该命令".to_owned(),
            Self::Rejected(status) => format!("鼠标拒绝了命令（状态 0x{status:02X}）"),
            Self::Mismatch => "鼠标响应与请求不匹配".to_owned(),
        }
    }
}

pub fn probe(collections: &[HidCollection]) -> Option<RazerProbe> {
    let mut candidates: Vec<&HidCollection> = collections
        .iter()
        .filter(|collection| collection.is_razer() && collection.feature_len == FEATURE_SIZE as u16)
        .collect();
    candidates.sort_by_key(|collection| collection.path.clone());

    for candidate in candidates {
        let Some(handle) = hid::open(&candidate.path, true) else {
            continue;
        };

        for transaction_id in TX_CANDIDATES {
            let battery = read_battery(&handle, transaction_id).ok().flatten();
            let mut dpi = None;
            let mut varstore = 1u8;

            for storage in [1u8, 0u8] {
                if let Ok(value) = read_dpi(&handle, transaction_id, storage) {
                    dpi = Some(value);
                    varstore = storage;
                    break;
                }
            }

            if battery.is_none() && dpi.is_none() {
                continue;
            }

            let charging = read_charging(&handle, transaction_id).unwrap_or(false);
            return Some(RazerProbe {
                path: candidate.path.clone(),
                transaction_id,
                varstore,
                battery,
                charging,
                dpi,
                supports_dpi: true,
            });
        }
    }
    None
}

pub fn set_dpi(
    control_path: &str,
    transaction_id: u8,
    varstore: u8,
    dpi: u32,
) -> Result<u32, String> {
    let handle = hid::open(control_path, true)
        .ok_or_else(|| "无法打开雷蛇控制接口，请确认鼠标已连接".to_owned())?;

    let dpi = dpi.clamp(100, 45_000);
    let mut used_varstore = varstore;
    let mut args = [
        varstore,
        ((dpi >> 8) & 0xff) as u8,
        (dpi & 0xff) as u8,
        ((dpi >> 8) & 0xff) as u8,
        (dpi & 0xff) as u8,
        0,
        0,
    ];

    match transact(&handle, transaction_id, 0x04, 0x05, 0x07, &args) {
        Ok(_) => {}
        Err(first_error) if varstore != 0 => {
            args[0] = 0;
            transact(&handle, transaction_id, 0x04, 0x05, 0x07, &args)
                .map_err(|_| first_error.message())?;
            used_varstore = 0;
        }
        Err(error) => return Err(error.message()),
    }

    thread::sleep(Duration::from_millis(8));
    Ok(read_dpi(&handle, transaction_id, used_varstore).unwrap_or(dpi))
}

fn read_battery(handle: &hid::HidHandle, transaction_id: u8) -> Result<Option<u8>, RazerError> {
    match transact(handle, transaction_id, 0x07, 0x80, 0x02, &[0, 0]) {
        Ok(response) => {
            let raw = response[8 + 1] as u16;
            Ok(Some(((raw * 100 + 127) / 255) as u8))
        }
        Err(RazerError::Unsupported) => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_charging(handle: &hid::HidHandle, transaction_id: u8) -> Result<bool, RazerError> {
    let response = transact(handle, transaction_id, 0x07, 0x84, 0x02, &[0, 0])?;
    Ok(response[8 + 1] != 0)
}

fn read_dpi(handle: &hid::HidHandle, transaction_id: u8, varstore: u8) -> Result<u32, RazerError> {
    let response = transact(
        handle,
        transaction_id,
        0x04,
        0x85,
        0x07,
        &[varstore, 0, 0, 0, 0, 0, 0],
    )?;
    let x = u16::from_be_bytes([response[8 + 1], response[8 + 2]]) as u32;
    let y = u16::from_be_bytes([response[8 + 3], response[8 + 4]]) as u32;
    let value = if x > 0 && x <= 45_000 {
        x
    } else if y > 0 && y <= 45_000 {
        y
    } else {
        return Err(RazerError::Mismatch);
    };
    Ok(value)
}

fn transact(
    handle: &hid::HidHandle,
    transaction_id: u8,
    command_class: u8,
    command_id: u8,
    data_size: u8,
    arguments: &[u8],
) -> Result<[u8; REPORT_SIZE], RazerError> {
    let request = build_report(
        transaction_id,
        command_class,
        command_id,
        data_size,
        arguments,
    );

    let mut buffer = [0u8; FEATURE_SIZE];
    buffer[1..].copy_from_slice(&request);

    let mut last_error = RazerError::Mismatch;
    for attempt in 0..3 {
        handle.send_feature(&buffer).map_err(RazerError::Io)?;
        thread::sleep(if attempt == 0 {
            Duration::from_millis(2)
        } else {
            Duration::from_millis(14)
        });

        let response = handle
            .get_feature(FEATURE_SIZE, 0)
            .map_err(RazerError::Io)?;
        if response.len() < FEATURE_SIZE {
            last_error = RazerError::Mismatch;
            continue;
        }

        let mut report = [0u8; REPORT_SIZE];
        report.copy_from_slice(&response[1..FEATURE_SIZE]);

        if report[6] != command_class || report[7] != command_id {
            last_error = RazerError::Mismatch;
            continue;
        }

        return match report[0] {
            STATUS_SUCCESS => Ok(report),
            // A busy device has not filled the reply payload yet. Retrying is
            // important for battery and DPI reads: treating zeroed busy replies
            // as data produces a false 0% battery and an unknown DPI.
            STATUS_BUSY => {
                last_error = RazerError::Rejected(STATUS_BUSY);
                continue;
            }
            STATUS_NOT_SUPPORTED => Err(RazerError::Unsupported),
            status => Err(RazerError::Rejected(status)),
        };
    }
    Err(last_error)
}

fn build_report(
    transaction_id: u8,
    command_class: u8,
    command_id: u8,
    data_size: u8,
    arguments: &[u8],
) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];
    report[1] = transaction_id;
    report[5] = data_size;
    report[6] = command_class;
    report[7] = command_id;
    let length = arguments.len().min(80);
    report[8..8 + length].copy_from_slice(&arguments[..length]);

    let mut crc = 0u8;
    for byte in &report[2..88] {
        crc ^= byte;
    }
    report[88] = crc;
    report
}
