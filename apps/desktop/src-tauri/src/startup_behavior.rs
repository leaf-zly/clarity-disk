//! Fixed current-user launch-at-login integration.

use std::path::PathBuf;
use std::process::{Command, Stdio};

const STARTUP_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$runKey = 'Registry::HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run'
if ($args[0] -eq 'enable') {
  $quoted = '"' + $args[1] + '"'
  New-Item -Path $runKey -Force | Out-Null
  Set-ItemProperty -LiteralPath $runKey -Name 'ClarityDisk' -Value $quoted -Type String
} elseif ($args[0] -eq 'disable') {
  Remove-ItemProperty -LiteralPath $runKey -Name 'ClarityDisk' -ErrorAction SilentlyContinue
} else { throw 'unsupported launch-at-login state' }
"#;

/// Applies a fixed HKCU startup preference for the current executable.
///
/// # Errors
///
/// Returns an error when the platform, trusted PowerShell path, current
/// executable, or fixed registry operation is unavailable.
pub(crate) fn apply_launch_at_login(enabled: bool) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("launch at login is supported only on Windows".to_owned())
    }
    #[cfg(windows)]
    {
        let system_root = std::env::var_os("SystemRoot")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Windows SystemRoot is unavailable".to_owned())?;
        let powershell =
            PathBuf::from(system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        if !powershell.is_file() {
            return Err("trusted Windows PowerShell is unavailable".to_owned());
        }
        let executable = std::env::current_exe()
            .map_err(|error| format!("current application path is unavailable: {error}"))?;
        let state = if enabled { "enable" } else { "disable" };
        let status = Command::new(powershell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                STARTUP_SCRIPT,
                state,
            ])
            .arg(executable)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("launch-at-login integration could not start: {error}"))?;
        if !status.success() {
            return Err("Windows rejected the fixed launch-at-login update".to_owned());
        }
        Ok(())
    }
}
