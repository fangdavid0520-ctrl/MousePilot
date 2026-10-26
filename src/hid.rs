use std::ffi::c_void;
use std::hash::{Hash, Hasher};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO,
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetFeature, HidD_GetHidGuid,
    HidD_GetInputReport, HidD_GetManufacturerString, HidD_GetPreparsedData, HidD_GetProductString,
    HidD_GetSerialNumberString, HidD_SetFeature, HidP_Feature, HidP_GetCaps, HidP_GetUsageValue,
    HidP_GetValueCaps, HidP_Input, HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS,
    HIDP_VALUE_CAPS,
};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

const BATTERY_USAGE_PAGE: u16 = 0x0006;
const BATTERY_STRENGTH_USAGE: u16 = 0x0020;

#[derive(Clone, Debug)]
pub struct HidCollection {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: String,
    pub product: String,
    pub serial: String,
    pub usage_page: u16,
    pub usage: u16,
    pub feature_len: u16,
}

impl HidCollection {
    pub fn is_razer(&self) -> bool {
        self.vendor_id == 0x1532
    }

    pub fn is_mouse(&self) -> bool {
        self.usage_page == 0x0001 && self.usage == 0x0002
    }

    pub fn display_name(&self) -> String {
        let product = self.product.trim();
        if !product.is_empty() {
            return product.to_owned();
        }
        let manufacturer = self.manufacturer.trim();
        if !manufacturer.is_empty() {
            return format!("{manufacturer} HID 设备");
        }
        format!("HID 设备 ({:04X}:{:04X})", self.vendor_id, self.product_id)
    }

    pub fn group_key(&self) -> String {
        let serial = self.serial.trim();
        if !serial.is_empty() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            serial.to_lowercase().hash(&mut hasher);
            return format!(
                "{:04x}:{:04x}:{:016x}",
                self.vendor_id,
                self.product_id,
                hasher.finish()
            );
        }
        let product = self.product.trim().to_lowercase();
        if !product.is_empty() {
            return format!("{:04x}:{:04x}:{}", self.vendor_id, self.product_id, product);
        }
        format!(
            "{:04x}:{:04x}:{}",
            self.vendor_id,
            self.product_id,
            self.path.to_lowercase()
        )
    }
}

pub struct HidHandle {
    handle: HANDLE,
}

impl Drop for HidHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

impl HidHandle {
    pub fn send_feature(&self, report: &[u8]) -> Result<(), String> {
        if report.is_empty() {
            return Err("Feature report 为空".to_owned());
        }
        let ok = unsafe {
            HidD_SetFeature(
                self.handle,
                report.as_ptr() as *const c_void,
                report.len() as u32,
            )
        };
        if ok == 0 {
            Err(format!("HidD_SetFeature 失败（错误码 {}）", unsafe {
                GetLastError()
            }))
        } else {
            Ok(())
        }
    }

    pub fn get_feature(&self, length: usize, report_id: u8) -> Result<Vec<u8>, String> {
        if length == 0 {
            return Err("Feature report 长度无效".to_owned());
        }
        let mut buffer = vec![0u8; length];
        buffer[0] = report_id;
        let ok = unsafe {
            HidD_GetFeature(
                self.handle,
                buffer.as_mut_ptr() as *mut c_void,
                length as u32,
            )
        };
        if ok == 0 {
            Err(format!("HidD_GetFeature 失败（错误码 {}）", unsafe {
                GetLastError()
            }))
        } else {
            Ok(buffer)
        }
    }

    fn get_input(&self, length: usize, report_id: u8) -> Result<Vec<u8>, String> {
        if length == 0 {
            return Err("Input report 长度无效".to_owned());
        }
        let mut buffer = vec![0u8; length];
        buffer[0] = report_id;
        let ok = unsafe {
            HidD_GetInputReport(
                self.handle,
                buffer.as_mut_ptr() as *mut c_void,
                length as u32,
            )
        };
        if ok == 0 {
            Err("HidD_GetInputReport 失败".to_owned())
        } else {
            Ok(buffer)
        }
    }
}

struct DeviceInfoSet(HDEVINFO);

impl Drop for DeviceInfoSet {
    fn drop(&mut self) {
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

struct PreparsedData(isize);

impl Drop for PreparsedData {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                HidD_FreePreparsedData(self.0);
            }
        }
    }
}

pub fn enumerate() -> Vec<HidCollection> {
    let mut result = Vec::new();
    unsafe {
        let mut hid_guid: GUID = zeroed();
        HidD_GetHidGuid(&mut hid_guid);

        let device_info = SetupDiGetClassDevsW(
            &hid_guid,
            null(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        );
        if device_info == INVALID_HANDLE_VALUE as isize {
            return result;
        }
        let device_info = DeviceInfoSet(device_info);

        let mut index = 0u32;
        loop {
            let mut interface_data: SP_DEVICE_INTERFACE_DATA = zeroed();
            interface_data.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            if SetupDiEnumDeviceInterfaces(
                device_info.0,
                null_mut(),
                &hid_guid,
                index,
                &mut interface_data,
            ) == 0
            {
                break;
            }
            index += 1;

            let mut required = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(
                device_info.0,
                &interface_data,
                null_mut(),
                0,
                &mut required,
                null_mut(),
            );
            if required == 0 {
                continue;
            }

            let mut buffer = vec![0u32; (required as usize + 3) / 4];
            let buffer_bytes = buffer.as_mut_ptr() as *mut u8;
            let detail = buffer_bytes as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = if cfg!(target_pointer_width = "64") {
                8
            } else {
                6
            };
            let ok = SetupDiGetDeviceInterfaceDetailW(
                device_info.0,
                &interface_data,
                detail,
                required,
                null_mut(),
                null_mut(),
            );
            if ok == 0 {
                continue;
            }

            let path = read_wide_ptr(buffer_bytes.add(4) as *const u16);
            if path.is_empty() {
                continue;
            }
            if let Some(collection) = describe(&path) {
                result.push(collection);
            }
        }
    }
    result
}

pub fn describe(path: &str) -> Option<HidCollection> {
    let handle = open(path, false)?;
    unsafe {
        let mut attributes: HIDD_ATTRIBUTES = zeroed();
        attributes.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
        if HidD_GetAttributes(handle.handle, &mut attributes) == 0 {
            return None;
        }

        let mut raw_preparsed: isize = 0;
        if HidD_GetPreparsedData(handle.handle, &mut raw_preparsed) == 0 {
            return None;
        }
        let preparsed = PreparsedData(raw_preparsed);
        let mut caps: HIDP_CAPS = zeroed();
        if HidP_GetCaps(preparsed.0, &mut caps) != HIDP_STATUS_SUCCESS {
            return None;
        }

        Some(HidCollection {
            path: path.to_owned(),
            vendor_id: attributes.VendorID,
            product_id: attributes.ProductID,
            manufacturer: read_string(handle.handle, StringKind::Manufacturer),
            product: read_string(handle.handle, StringKind::Product),
            serial: read_string(handle.handle, StringKind::Serial),
            usage_page: caps.UsagePage,
            usage: caps.Usage,
            feature_len: caps.FeatureReportByteLength,
        })
    }
}

pub fn open(path: &str, writable: bool) -> Option<HidHandle> {
    let wide = wide_string(path);
    let access_modes: &[u32] = if writable {
        &[
            windows_sys::Win32::Foundation::GENERIC_READ
                | windows_sys::Win32::Foundation::GENERIC_WRITE,
            windows_sys::Win32::Foundation::GENERIC_WRITE,
            0,
        ]
    } else {
        &[0]
    };

    for access in access_modes {
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                *access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            return Some(HidHandle { handle });
        }
    }
    None
}

/// Reads the standard HID `Battery Strength` usage (06/20) from a collection.
///
/// Both feature reports and the most recent input report are considered. The
/// latter is how many 2.4 GHz receivers expose the mouse battery.
pub fn read_standard_battery(collection: &HidCollection) -> Option<u8> {
    let handle = open(&collection.path, false)?;
    unsafe {
        let mut raw_preparsed: isize = 0;
        if HidD_GetPreparsedData(handle.handle, &mut raw_preparsed) == 0 {
            return None;
        }
        let preparsed = PreparsedData(raw_preparsed);
        let mut caps: HIDP_CAPS = zeroed();
        if HidP_GetCaps(preparsed.0, &mut caps) != HIDP_STATUS_SUCCESS {
            return None;
        }

        for report_type in [HidP_Feature, HidP_Input] {
            let count = if report_type == HidP_Feature {
                caps.NumberFeatureValueCaps
            } else {
                caps.NumberInputValueCaps
            };
            if count == 0 {
                continue;
            }

            let mut value_caps = vec![zeroed::<HIDP_VALUE_CAPS>(); count as usize];
            let mut length = count;
            if HidP_GetValueCaps(
                report_type,
                value_caps.as_mut_ptr(),
                &mut length,
                preparsed.0,
            ) != HIDP_STATUS_SUCCESS
            {
                continue;
            }
            value_caps.truncate(length as usize);

            let mut seen = Vec::<(u8, u16)>::new();
            for value in &value_caps {
                if value.UsagePage != BATTERY_USAGE_PAGE {
                    continue;
                }
                let is_battery = if value.IsRange != 0 {
                    let range = value.Anonymous.Range;
                    range.UsageMin <= BATTERY_STRENGTH_USAGE
                        && range.UsageMax >= BATTERY_STRENGTH_USAGE
                } else {
                    value.Anonymous.NotRange.Usage == BATTERY_STRENGTH_USAGE
                };
                if !is_battery {
                    continue;
                }

                let key = (value.ReportID, value.LinkCollection);
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);

                let report_len = if report_type == HidP_Feature {
                    caps.FeatureReportByteLength
                } else {
                    caps.InputReportByteLength
                } as usize;
                if report_len == 0 {
                    continue;
                }

                let report = if report_type == HidP_Feature {
                    match handle.get_feature(report_len, value.ReportID) {
                        Ok(report) => report,
                        Err(_) => continue,
                    }
                } else {
                    match handle.get_input(report_len, value.ReportID) {
                        Ok(report) => report,
                        Err(_) => continue,
                    }
                };

                let mut battery = 0u32;
                let status = HidP_GetUsageValue(
                    report_type,
                    BATTERY_USAGE_PAGE,
                    value.LinkCollection,
                    BATTERY_STRENGTH_USAGE,
                    &mut battery,
                    preparsed.0,
                    report.as_ptr(),
                    report_len as u32,
                );
                if status == HIDP_STATUS_SUCCESS {
                    return Some(normalize_battery(battery));
                }
            }
        }
    }
    None
}

fn normalize_battery(value: u32) -> u8 {
    if value <= 100 {
        value as u8
    } else if value <= 255 {
        ((value * 100 + 127) / 255) as u8
    } else {
        100
    }
}

enum StringKind {
    Manufacturer,
    Product,
    Serial,
}

fn read_string(handle: HANDLE, kind: StringKind) -> String {
    let mut buffer = [0u16; 256];
    let ok = unsafe {
        match kind {
            StringKind::Manufacturer => HidD_GetManufacturerString(
                handle,
                buffer.as_mut_ptr() as *mut c_void,
                (buffer.len() * 2) as u32,
            ),
            StringKind::Product => HidD_GetProductString(
                handle,
                buffer.as_mut_ptr() as *mut c_void,
                (buffer.len() * 2) as u32,
            ),
            StringKind::Serial => HidD_GetSerialNumberString(
                handle,
                buffer.as_mut_ptr() as *mut c_void,
                (buffer.len() * 2) as u32,
            ),
        }
    };
    if ok == 0 {
        return String::new();
    }
    read_wide_buffer(&buffer)
}

fn read_wide_ptr(pointer: *const u16) -> String {
    unsafe {
        let mut length = 0usize;
        while length < 32_767 && *pointer.add(length) != 0 {
            length += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
    }
}

fn read_wide_buffer(buffer: &[u16]) -> String {
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
        .trim()
        .to_owned()
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
