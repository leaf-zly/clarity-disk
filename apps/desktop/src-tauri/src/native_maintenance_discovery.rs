//! Native Windows evidence used by the automatic-maintenance scheduler.
//!
//! The provider intentionally uses only read-only Win32 APIs. A failure to
//! establish any protection signal is returned to the caller so the scheduler
//! can fail closed instead of guessing that maintenance is safe.

#[cfg(windows)]
mod windows {
    #![allow(unsafe_code)]

    use std::io;

    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
            SystemInformation::GetTickCount,
        },
        UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
    };

    /// Protection evidence required before an unattended cleanup preview.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct MaintenanceContext {
        /// Seconds since the last keyboard or mouse input.
        pub(crate) idle_seconds: u32,
        /// Whether Windows reports stable external power.
        pub(crate) stable_power: bool,
        /// Whether an update worker is currently active.
        pub(crate) system_update_active: bool,
        /// Whether a backup worker is currently active.
        pub(crate) backup_active: bool,
    }

    /// Discovers scheduler protection signals without launching a shell.
    pub(crate) fn discover() -> Result<MaintenanceContext, io::Error> {
        Ok(MaintenanceContext {
            idle_seconds: idle_seconds()?,
            stable_power: stable_power()?,
            system_update_active: process_running(&["tiworker.exe", "trustedinstaller.exe"])?,
            backup_active: process_running(&["wbengine.exe", "sdclt.exe"])?,
        })
    }

    fn idle_seconds() -> Result<u32, io::Error> {
        let mut input = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        let success = unsafe { GetLastInputInfo(&raw mut input) != 0 };
        if !success {
            return Err(io::Error::last_os_error());
        }
        // Both values are 32-bit tick counters and intentionally wrap. The
        // subtraction therefore remains correct across the wrap boundary.
        Ok(unsafe { GetTickCount().wrapping_sub(input.dwTime) } / 1_000)
    }

    fn stable_power() -> Result<bool, io::Error> {
        let mut status = SYSTEM_POWER_STATUS::default();
        let success = unsafe { GetSystemPowerStatus(&raw mut status) != 0 };
        if !success {
            return Err(io::Error::last_os_error());
        }
        // ACLineStatus=1 covers desktops and laptops on external power. A
        // critical battery flag remains blocked even if the AC signal races
        // with a transient power transition.
        Ok(status.ACLineStatus == 1 && status.BatteryFlag != 4)
    }

    fn process_running(targets: &[&str]) -> Result<bool, io::Error> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let result = process_running_in_snapshot(snapshot, targets);
        unsafe { CloseHandle(snapshot) };
        result
    }

    fn process_running_in_snapshot(
        snapshot: windows_sys::Win32::Foundation::HANDLE,
        targets: &[&str],
    ) -> Result<bool, io::Error> {
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..PROCESSENTRY32W::default()
        };
        let first = unsafe { Process32FirstW(snapshot, &raw mut entry) != 0 };
        if !first {
            return Err(io::Error::last_os_error());
        }
        loop {
            let length = entry
                .szExeFile
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..length]).to_ascii_lowercase();
            if targets.iter().any(|target| *target == name) {
                return Ok(true);
            }
            let next = unsafe { Process32NextW(snapshot, &raw mut entry) != 0 };
            if !next {
                // ERROR_NO_MORE_FILES is the normal end condition. Any other
                // error is unknown evidence and must block maintenance.
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    return Ok(false);
                }
                return Err(error);
            }
        }
    }
}

#[cfg(windows)]
pub(crate) use windows::{MaintenanceContext, discover};

#[cfg(not(windows))]
mod unsupported {
    use std::io;

    /// Protection evidence required before an unattended cleanup preview.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct MaintenanceContext {
        pub(crate) idle_seconds: u32,
        pub(crate) stable_power: bool,
        pub(crate) system_update_active: bool,
        pub(crate) backup_active: bool,
    }

    pub(crate) fn discover() -> Result<MaintenanceContext, io::Error> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "automatic maintenance protection discovery is supported only on Windows",
        ))
    }
}

#[cfg(not(windows))]
pub(crate) use unsupported::{MaintenanceContext, discover};
