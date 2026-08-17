//! Fixed current-user launch-at-login integration.

use std::path::Path;

const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE_NAME: &str = "ClarityDisk";

/// Applies a fixed HKCU startup preference for the current executable.
///
/// # Errors
///
/// Returns an error when the platform, current executable, or fixed native
/// registry operation is unavailable. No shell process or caller-controlled
/// registry path is used.
pub(crate) fn apply_launch_at_login(enabled: bool) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("Windows 登录启动功能仅支持 Windows".to_owned())
    }
    #[cfg(windows)]
    {
        apply_windows_launch_at_login(enabled)
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn apply_windows_launch_at_login(enabled: bool) -> Result<(), String> {
    use std::ptr;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
    };

    let executable =
        std::env::current_exe().map_err(|error| format!("无法读取当前程序路径：{error}"))?;
    let command = enabled.then(|| startup_command_value(&executable));
    let byte_length = command
        .as_ref()
        .map(|value| {
            value
                .len()
                .checked_mul(size_of::<u16>())
                .and_then(|length| u32::try_from(length).ok())
                .ok_or_else(|| "Windows 登录启动命令长度无效".to_owned())
        })
        .transpose()?;
    let key_path = wide_null(RUN_KEY_PATH);
    let value_name = wide_null(RUN_VALUE_NAME);
    let mut key: HKEY = ptr::null_mut();
    let open_status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key_path.as_ptr(),
            0,
            ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            ptr::null(),
            &raw mut key,
            ptr::null_mut(),
        )
    };
    if open_status != ERROR_SUCCESS {
        return Err(registry_error(
            "无法打开 Windows 登录启动注册表项",
            open_status,
        ));
    }

    // The handle is always closed before interpreting the operation result.
    let operation_status = if let (Some(command), Some(byte_length)) = (command, byte_length) {
        unsafe {
            RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                byte_length,
            )
        }
    } else {
        unsafe { RegDeleteValueW(key, value_name.as_ptr()) }
    };
    let close_status = unsafe { RegCloseKey(key) };

    if operation_status != ERROR_SUCCESS && !(operation_status == ERROR_FILE_NOT_FOUND && !enabled)
    {
        return Err(registry_error(
            "Windows 拒绝更新登录启动设置",
            operation_status,
        ));
    }
    if close_status != ERROR_SUCCESS {
        return Err(registry_error(
            "Windows 登录启动注册表项无法安全关闭",
            close_status,
        ));
    }
    Ok(())
}

fn startup_command_value(executable: &Path) -> Vec<u16> {
    // Quoting the complete executable path is required by the Run-key contract
    // and prevents spaces from being interpreted as an argument boundary.
    format!("\"{}\"", executable.to_string_lossy())
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn registry_error(context: &str, code: u32) -> String {
    format!("{context}（Windows 错误代码 {code}）")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::startup_command_value;

    #[test]
    fn startup_command_quotes_paths_with_spaces() {
        let encoded =
            startup_command_value(Path::new(r"C:\Program Files\Clarity Disk\Clarity Disk.exe"));
        let decoded = String::from_utf16(&encoded[..encoded.len() - 1])
            .expect("startup command should remain valid UTF-16");
        assert_eq!(
            decoded,
            r#""C:\Program Files\Clarity Disk\Clarity Disk.exe""#
        );
        assert_eq!(encoded.last(), Some(&0));
    }
}
