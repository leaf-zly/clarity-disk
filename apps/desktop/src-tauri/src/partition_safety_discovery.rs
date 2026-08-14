//! Read-only Windows evidence for partition-operation safety planning.
//!
//! The adapter runs one compile-time script from the trusted in-box
//! Windows `PowerShell` path. It accepts no caller input and only reads power and
//! pending-restart evidence.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use clarity_core::{
    BackupEvidenceState, ExternalPowerState, PartitionSafetyEvidence, PendingRestartState,
};
use serde::Deserialize;

// No value from a caller is interpolated. Registry and CIM reads are fixed so
// this adapter cannot become a general shell or arbitrary registry surface.
const SAFETY_EVIDENCE_SCRIPT: &str = r"
$ErrorActionPreference = 'Stop'
$utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$warnings = [System.Collections.Generic.List[string]]::new()

$externalPowerState = 'unknown'
try {
  $batteries = @(Get-CimInstance -ClassName Win32_Battery -ErrorAction Stop)
  if ($batteries.Count -eq 0) {
    $externalPowerState = 'desktopNoBattery'
  } else {
    $connectedStates = @(2, 3, 6, 7, 8, 9, 11)
    $allConnected = $true
    foreach ($battery in $batteries) {
      if ($connectedStates -notcontains [int]$battery.BatteryStatus) {
        $allConnected = $false
      }
    }
    $externalPowerState = if ($allConnected) { 'connected' } else { 'onBattery' }
  }
} catch {
  $warnings.Add('无法完整读取系统供电状态，稳定供电检查保持阻塞。')
}

$pendingRestartState = 'unknown'
try {
  $componentServicing = Test-Path -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending' -ErrorAction Stop
  $windowsUpdate = Test-Path -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired' -ErrorAction Stop
  $sessionManager = Get-Item -LiteralPath 'Registry::HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager' -ErrorAction Stop
  $fileRename = $null -ne $sessionManager.GetValue('PendingFileRenameOperations', $null)
  $pendingRestartState = if ($componentServicing -or $windowsUpdate -or $fileRename) { 'present' } else { 'clear' }
} catch {
  $warnings.Add('无法完整读取 Windows 待重启标记，重启检查保持阻塞。')
}

[ordered]@{
  externalPowerState = $externalPowerState
  pendingRestartState = $pendingRestartState
  warnings = @($warnings)
} | ConvertTo-Json -Depth 4 -Compress
";

/// Discovers non-mutating system evidence for a partition safety assessment.
///
/// Backup evidence deliberately remains unavailable until an independent
/// provider can verify recoverability on another physical device.
///
/// # Errors
///
/// Returns an error when the platform is unsupported, trusted Windows
/// `PowerShell` cannot be located or launched, or provider JSON is invalid.
pub fn discover_partition_safety_evidence()
-> Result<PartitionSafetyEvidence, PartitionSafetyDiscoveryError> {
    #[cfg(not(windows))]
    {
        Err(PartitionSafetyDiscoveryError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    {
        discover_windows_partition_safety_evidence()
    }
}

#[cfg(windows)]
fn discover_windows_partition_safety_evidence()
-> Result<PartitionSafetyEvidence, PartitionSafetyDiscoveryError> {
    let output = Command::new(powershell_path()?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SAFETY_EVIDENCE_SCRIPT,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(PartitionSafetyDiscoveryError::Launch)?;

    if !output.status.success() {
        return Err(PartitionSafetyDiscoveryError::ProviderFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let raw: PowerShellSafetyEvidence = serde_json::from_slice(&output.stdout)
        .map_err(PartitionSafetyDiscoveryError::InvalidProviderResponse)?;
    Ok(convert_evidence(raw))
}

#[cfg(windows)]
fn powershell_path() -> Result<PathBuf, PartitionSafetyDiscoveryError> {
    let system_root = std::env::var_os("SystemRoot")
        .filter(|value| !value.is_empty())
        .ok_or(PartitionSafetyDiscoveryError::SystemRootUnavailable)?;
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !executable.is_file() {
        return Err(PartitionSafetyDiscoveryError::PowerShellUnavailable(
            executable,
        ));
    }
    Ok(executable)
}

fn convert_evidence(raw: PowerShellSafetyEvidence) -> PartitionSafetyEvidence {
    PartitionSafetyEvidence {
        captured_at_unix_ms: current_unix_ms(),
        external_power_state: match raw.external_power_state.as_str() {
            "connected" => ExternalPowerState::Connected,
            "desktopNoBattery" => ExternalPowerState::DesktopNoBattery,
            "onBattery" => ExternalPowerState::OnBattery,
            _ => ExternalPowerState::Unknown,
        },
        pending_restart_state: match raw.pending_restart_state.as_str() {
            "clear" => PendingRestartState::Clear,
            "present" => PendingRestartState::Present,
            _ => PendingRestartState::Unknown,
        },
        // User confirmation is not proof of recoverability. A future backup
        // provider must verify an independent restore before this can pass.
        backup_evidence_state: BackupEvidenceState::Unavailable,
        discovery_warnings: raw.warnings,
    }
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellSafetyEvidence {
    external_power_state: String,
    pending_restart_state: String,
    #[serde(default)]
    warnings: Vec<String>,
}

/// Errors produced by the read-only Windows safety-evidence adapter.
#[derive(Debug, thiserror::Error)]
pub enum PartitionSafetyDiscoveryError {
    /// Safety evidence is available only on Windows.
    #[cfg(not(windows))]
    #[error("partition safety evidence is supported only on Windows")]
    UnsupportedPlatform,
    /// Windows system root was unavailable.
    #[error("Windows SystemRoot is unavailable")]
    SystemRootUnavailable,
    /// The trusted in-box Windows `PowerShell` executable was not present.
    #[error("Windows PowerShell is unavailable at {0}")]
    PowerShellUnavailable(PathBuf),
    /// The fixed evidence query could not be launched.
    #[error("failed to launch the read-only partition safety provider: {0}")]
    Launch(#[source] std::io::Error),
    /// The fixed provider query failed.
    #[error("read-only partition safety provider failed: {0}")]
    ProviderFailed(String),
    /// Provider output did not match the adapter contract.
    #[error("invalid partition safety provider response: {0}")]
    InvalidProviderResponse(#[source] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_known_power_and_restart_evidence() {
        let evidence = convert_evidence(PowerShellSafetyEvidence {
            external_power_state: "connected".to_owned(),
            pending_restart_state: "clear".to_owned(),
            warnings: vec![],
        });
        assert_eq!(evidence.external_power_state, ExternalPowerState::Connected);
        assert_eq!(evidence.pending_restart_state, PendingRestartState::Clear);
        assert_eq!(
            evidence.backup_evidence_state,
            BackupEvidenceState::Unavailable
        );
    }

    #[test]
    fn maps_unrecognized_provider_values_to_unknown() {
        let evidence = convert_evidence(PowerShellSafetyEvidence {
            external_power_state: "future-value".to_owned(),
            pending_restart_state: "future-value".to_owned(),
            warnings: vec!["partial".to_owned()],
        });
        assert_eq!(evidence.external_power_state, ExternalPowerState::Unknown);
        assert_eq!(evidence.pending_restart_state, PendingRestartState::Unknown);
        assert_eq!(evidence.discovery_warnings, vec!["partial"]);
    }
}
