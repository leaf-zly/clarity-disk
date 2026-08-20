//! Fixed Windows system-maintenance adapters.

use clarity_privileged_protocol::MaintenanceOperation;
use thiserror::Error;

/// Executes one enumerated maintenance operation and verifies its exit status.
pub(crate) fn execute(operation: MaintenanceOperation) -> Result<String, MaintenanceError> {
    #[cfg(windows)]
    {
        execute_windows(operation)
    }
    #[cfg(not(windows))]
    {
        let _ = operation;
        Err(MaintenanceError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
fn execute_windows(operation: MaintenanceOperation) -> Result<String, MaintenanceError> {
    match operation {
        MaintenanceOperation::SetHibernation { enabled } => set_hibernation(enabled),
        MaintenanceOperation::ResetWindowsUpdateDownloadCache => reset_update_cache(),
        MaintenanceOperation::CreateSystemRestorePoint => create_restore_point(),
    }
}

#[cfg(windows)]
fn set_hibernation(enabled: bool) -> Result<String, MaintenanceError> {
    let executable = trusted_system32_executable("powercfg.exe")?;
    let argument = if enabled { "on" } else { "off" };
    run_checked(&executable, &["/hibernate", argument], "powercfg")?;
    Ok(if enabled {
        "休眠功能已启用，并已由 Windows powercfg 验证完成。"
    } else {
        "休眠功能已关闭，hiberfil.sys 由 Windows 管理释放。"
    }
    .to_owned())
}

#[cfg(windows)]
fn create_restore_point() -> Result<String, MaintenanceError> {
    const SCRIPT: &str = "$ErrorActionPreference='Stop'; Checkpoint-Computer -Description 'Clarity Disk maintenance' -RestorePointType 'MODIFY_SETTINGS'";
    let powershell = trusted_powershell()?;
    run_checked(
        &powershell,
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ],
        "restore point provider",
    )?;
    Ok("Windows 系统还原点已创建。".to_owned())
}

#[cfg(windows)]
fn reset_update_cache() -> Result<String, MaintenanceError> {
    const STOP_SCRIPT: &str = "$ErrorActionPreference='Stop'; $items=@(Get-Service -Name wuauserv,bits -ErrorAction Stop); $items | Where-Object Status -ne 'Stopped' | Stop-Service -Force -ErrorAction Stop; $items | ForEach-Object { $_.WaitForStatus('Stopped',[TimeSpan]::FromSeconds(30)) }";
    const START_SCRIPT: &str = "$ErrorActionPreference='Stop'; Get-Service -Name bits,wuauserv -ErrorAction Stop | Where-Object Status -ne 'Running' | Start-Service -ErrorAction Stop";

    let powershell = trusted_powershell()?;
    run_checked(
        &powershell,
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            STOP_SCRIPT,
        ],
        "Windows Update service stop",
    )?;

    // Service restart is attempted even when cache preflight or deletion fails.
    let cleanup_result = clear_fixed_update_download_root();
    let restart_result = run_checked(
        &powershell,
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            START_SCRIPT,
        ],
        "Windows Update service restart",
    );
    cleanup_result?;
    restart_result?;
    Ok("Windows 更新下载缓存已维护，相关服务已恢复运行。".to_owned())
}

#[cfg(windows)]
fn clear_fixed_update_download_root() -> Result<(), MaintenanceError> {
    use std::fs;

    let root = windows_directory()?
        .join("SoftwareDistribution")
        .join("Download");
    if !root.is_dir() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(&root).map_err(MaintenanceError::ReadCache)?;
    if is_reparse(&metadata) || !tree_safe(&root, &metadata)? {
        return Err(MaintenanceError::UnsafeCacheTree);
    }
    for entry in fs::read_dir(&root).map_err(MaintenanceError::ReadCache)? {
        let entry = entry.map_err(MaintenanceError::ReadCache)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(MaintenanceError::ReadCache)?;
        if metadata.is_dir() {
            fs::remove_dir_all(path).map_err(MaintenanceError::ClearCache)?;
        } else {
            fs::remove_file(path).map_err(MaintenanceError::ClearCache)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn tree_safe(
    path: &std::path::Path,
    metadata: &std::fs::Metadata,
) -> Result<bool, MaintenanceError> {
    use std::fs;

    if is_reparse(metadata) {
        return Ok(false);
    }
    if !metadata.is_dir() {
        return Ok(true);
    }
    for entry in fs::read_dir(path).map_err(MaintenanceError::ReadCache)? {
        let entry = entry.map_err(MaintenanceError::ReadCache)?;
        let child = entry.path();
        let metadata = fs::symlink_metadata(&child).map_err(MaintenanceError::ReadCache)?;
        if !tree_safe(&child, &metadata)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(windows)]
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0400 != 0
}

#[cfg(windows)]
fn trusted_powershell() -> Result<std::path::PathBuf, MaintenanceError> {
    let executable = windows_directory()?
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    validate_executable(executable)
}

#[cfg(windows)]
fn trusted_system32_executable(name: &str) -> Result<std::path::PathBuf, MaintenanceError> {
    debug_assert!(matches!(name, "powercfg.exe"));
    validate_executable(windows_directory()?.join("System32").join(name))
}

#[cfg(windows)]
fn validate_executable(path: std::path::PathBuf) -> Result<std::path::PathBuf, MaintenanceError> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(MaintenanceError::TrustedExecutableUnavailable(path))
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_directory() -> Result<std::path::PathBuf, MaintenanceError> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;

    let mut buffer = vec![0_u16; 32_768];
    // SAFETY: buffer is writable for its declared capacity and Windows returns
    // the copied UTF-16 length without the terminating NUL.
    let length = unsafe {
        GetWindowsDirectoryW(
            buffer.as_mut_ptr(),
            u32::try_from(buffer.len()).unwrap_or(u32::MAX),
        )
    };
    let length = usize::try_from(length).unwrap_or(usize::MAX);
    if length == 0 || length >= buffer.len() {
        return Err(MaintenanceError::WindowsDirectoryUnavailable);
    }
    buffer.truncate(length);
    let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer));
    if !path.is_absolute() {
        return Err(MaintenanceError::WindowsDirectoryUnavailable);
    }
    Ok(path)
}

#[cfg(windows)]
fn run_checked(
    executable: &std::path::Path,
    arguments: &[&str],
    adapter: &'static str,
) -> Result<(), MaintenanceError> {
    use std::process::{Command, Stdio};

    let output = crate::windows_process::hide_console_window(&mut Command::new(executable))
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|source| MaintenanceError::Launch { adapter, source })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(MaintenanceError::ProviderFailed { adapter, detail });
    }
    Ok(())
}

/// Failures from fixed privileged maintenance adapters.
#[derive(Debug, Error)]
pub(crate) enum MaintenanceError {
    #[cfg(not(windows))]
    #[error("privileged maintenance is supported only on Windows")]
    UnsupportedPlatform,
    #[cfg(windows)]
    #[error("trusted Windows directory could not be resolved")]
    WindowsDirectoryUnavailable,
    #[cfg(windows)]
    #[error("trusted Windows executable is unavailable: {0}")]
    TrustedExecutableUnavailable(std::path::PathBuf),
    #[cfg(windows)]
    #[error("{adapter} could not be launched: {source}")]
    Launch {
        adapter: &'static str,
        source: std::io::Error,
    },
    #[cfg(windows)]
    #[error("{adapter} failed: {detail}")]
    ProviderFailed {
        adapter: &'static str,
        detail: String,
    },
    #[cfg(windows)]
    #[error("Windows Update cache metadata could not be read: {0}")]
    ReadCache(std::io::Error),
    #[cfg(windows)]
    #[error("Windows Update cache contains a reparse point; maintenance stopped")]
    UnsafeCacheTree,
    #[cfg(windows)]
    #[error("Windows Update cache could not be cleared completely: {0}")]
    ClearCache(std::io::Error),
}
