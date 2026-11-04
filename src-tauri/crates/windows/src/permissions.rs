use anyhow::{bail, Result};
use windows::core::{w, PCWSTR};
use windows::Devices::Enumeration::{DeviceAccessInformation, DeviceAccessStatus, DeviceClass};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn consent(key: HKEY, path: &str) -> Option<String> {
    let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let mut buffer = [0u16; 32];
    let mut bytes = (buffer.len() * 2) as u32;
    let result = unsafe {
        RegGetValueW(
            key,
            PCWSTR(path.as_ptr()),
            w!("Value"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    result.ok().ok()?;
    Some(String::from_utf16_lossy(
        &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
    ))
}

pub fn microphone() -> &'static str {
    let path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
    let global = consent(HKEY_LOCAL_MACHINE, path);
    let desktop_path = format!(r"{path}\NonPackaged");
    if global.as_deref() == Some("Deny")
        || consent(HKEY_LOCAL_MACHINE, &desktop_path).as_deref() == Some("Deny")
    {
        return "restricted";
    }
    let desktop = consent(HKEY_CURRENT_USER, &desktop_path);
    if desktop.as_deref() == Some("Deny") {
        return "denied";
    }
    if desktop.as_deref() == Some("Allow") || global.as_deref() == Some("Allow") {
        return "granted";
    }
    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
    let access = DeviceAccessInformation::CreateFromDeviceClass(DeviceClass::AudioCapture)
        .and_then(|info| info.CurrentStatus());
    if initialized {
        unsafe { RoUninitialize() };
    }
    match access {
        Ok(DeviceAccessStatus::Allowed) => "granted",
        Ok(DeviceAccessStatus::DeniedByUser) => "denied",
        Ok(DeviceAccessStatus::DeniedBySystem) => "restricted",
        _ => "not_determined",
    }
}

pub fn open_settings(kind: &str) -> Result<()> {
    let uri = match kind {
        "microphone" => w!("ms-settings:privacy-microphone"),
        "screen" => w!("ms-settings:sound"),
        _ => bail!("Unknown permission: {kind}"),
    };
    open_uri(uri)
}

fn open_uri(uri: PCWSTR) -> Result<()> {
    let result = unsafe { ShellExecuteW(None, w!("open"), uri, None, None, SW_SHOWNORMAL) };
    if result.0 as usize <= 32 {
        bail!(
            "Windows Settings could not be opened: {}",
            result.0 as usize
        );
    }
    Ok(())
}

pub fn open_path(path: &std::path::Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    open_uri(PCWSTR(path.as_ptr()))
}

pub fn display_name() -> Option<String> {
    use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;
    if unsafe {
        GetUserNameExW(
            NameDisplay,
            Some(windows::core::PWSTR(buffer.as_mut_ptr())),
            &mut size,
        )
    } {
        let name = String::from_utf16_lossy(&buffer[..size as usize])
            .trim_end_matches('\0')
            .to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    std::env::var("USERNAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
}
