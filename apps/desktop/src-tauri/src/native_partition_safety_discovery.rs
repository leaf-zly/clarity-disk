//! Native Windows power and pending-restart evidence for partition safety.
//!
//! The provider deliberately does not read or validate backup receipts. When
//! an administrator-protected receipt exists, discovery returns an error so
//! the compatibility provider can perform its ACL and restore-proof checks.

#![allow(unsafe_code)]

use std::path::PathBuf;

use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS},
    System::{
        Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
        Registry::{
            HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
        },
    },
};

/// Native safety evidence that has no backup-receipt payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeSafetyEvidence {
    /// Normalized external-power state expected by the core model.
    pub(crate) external_power_state: String,
    /// Normalized pending-restart state expected by the core model.
    pub(crate) pending_restart_state: String,
    /// Non-fatal provider limitations.
    pub(crate) warnings: Vec<String>,
}

/// Discovers power and restart evidence without starting a shell process.
///
/// # Errors
///
/// Returns an error when the backup receipt must be checked by the restricted
/// compatibility provider or when the native provider cannot establish that
/// the receipt path is absent.
pub(crate) fn discover() -> Result<NativeSafetyEvidence, std::io::Error> {
    ensure_backup_receipt_is_absent()?;
    let mut warnings = Vec::new();
    let external_power_state = if let Ok(value) = external_power_state() {
        value
    } else {
        warnings.push("无法读取系统供电状态，稳定供电检查保持阻塞。".to_owned());
        "unknown".to_owned()
    };
    let pending_restart_state = if let Ok(value) = pending_restart_state() {
        value
    } else {
        warnings.push("无法读取 Windows 待重启标记，重启检查保持阻塞。".to_owned());
        "unknown".to_owned()
    };
    Ok(NativeSafetyEvidence {
        external_power_state,
        pending_restart_state,
        warnings,
    })
}

fn ensure_backup_receipt_is_absent() -> Result<(), std::io::Error> {
    let program_data = std::env::var_os("ProgramData").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "ProgramData is unavailable")
    })?;
    let receipt_path = PathBuf::from(program_data)
        .join("ClarityDisk")
        .join("backup-verification.v1.json");
    match std::fs::metadata(receipt_path) {
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "backup receipt requires ACL-aware compatibility validation",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn external_power_state() -> Result<String, std::io::Error> {
    let mut status = SYSTEM_POWER_STATUS::default();
    let success = unsafe { GetSystemPowerStatus(&raw mut status) != 0 };
    if !success {
        return Err(std::io::Error::last_os_error());
    }
    let state = match (status.ACLineStatus, status.BatteryFlag) {
        (1, 128) => "desktopNoBattery",
        (1, _) => "connected",
        (0, _) => "onBattery",
        _ => "unknown",
    };
    Ok(state.to_owned())
}

fn pending_restart_state() -> Result<String, std::io::Error> {
    let mut unknown = false;
    let servicing = key_exists(
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending",
        &mut unknown,
    );
    let update = key_exists(
        r"SOFTWARE\Microsoft\Windows\WindowsUpdate\Auto Update\RebootRequired",
        &mut unknown,
    );
    let session_value = value_exists(
        r"SYSTEM\CurrentControlSet\Control\Session Manager",
        "PendingFileRenameOperations",
        &mut unknown,
    );
    if servicing || update || session_value {
        Ok("present".to_owned())
    } else if unknown {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "one or more restart registry probes were denied",
        ))
    } else {
        Ok("clear".to_owned())
    }
}

fn key_exists(path: &str, unknown: &mut bool) -> bool {
    let path = wide(path);
    let mut key: HKEY = std::ptr::null_mut();
    let result =
        unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_READ, &raw mut key) };
    if result == ERROR_SUCCESS {
        unsafe { RegCloseKey(key) };
        true
    } else if result == ERROR_FILE_NOT_FOUND {
        false
    } else {
        *unknown = true;
        false
    }
}

fn value_exists(path: &str, value: &str, unknown: &mut bool) -> bool {
    let path = wide(path);
    let value = wide(value);
    let mut key: HKEY = std::ptr::null_mut();
    let open_result =
        unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_READ, &raw mut key) };
    if open_result == ERROR_FILE_NOT_FOUND {
        return false;
    }
    if open_result != ERROR_SUCCESS {
        *unknown = true;
        return false;
    }
    let mut value_type = 0_u32;
    let mut byte_length = 0_u32;
    let result = unsafe {
        RegQueryValueExW(
            key,
            value.as_ptr(),
            std::ptr::null(),
            &raw mut value_type,
            std::ptr::null_mut(),
            &raw mut byte_length,
        )
    };
    unsafe { RegCloseKey(key) };
    match result {
        ERROR_SUCCESS | ERROR_MORE_DATA => true,
        ERROR_FILE_NOT_FOUND => false,
        _ => {
            *unknown = true;
            false
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
