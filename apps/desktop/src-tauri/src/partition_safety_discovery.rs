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
    BackupEvidenceState, BackupVerificationError, BackupVerificationReceipt, ExternalPowerState,
    PartitionSafetyEvidence, PendingRestartState,
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

$backupReceipt = $null
$backupReceiptAclSafe = $false
$receiptPath = Join-Path $env:ProgramData 'ClarityDisk\backup-verification.v1.json'
if (Test-Path -LiteralPath $receiptPath) {
  try {
    $acl = Get-Acl -LiteralPath $receiptPath -ErrorAction Stop
    $ownerSid = ([Security.Principal.NTAccount]$acl.Owner).Translate([Security.Principal.SecurityIdentifier]).Value
    $ownerSafe = @('S-1-5-18', 'S-1-5-32-544') -contains $ownerSid
    $unsafeWriter = @($acl.Access | Where-Object {
      $identitySid = try { $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value } catch { '' }
      $_.AccessControlType -eq 'Allow' -and
      @('S-1-1-0', 'S-1-5-11', 'S-1-5-32-545') -contains $identitySid -and
      ($_.FileSystemRights.ToString() -match '(Write|Modify|FullControl)')
    }).Count -gt 0
    $backupReceiptAclSafe = $ownerSafe -and -not $unsafeWriter
    if ($backupReceiptAclSafe) {
      $backupReceipt = Get-Content -LiteralPath $receiptPath -Raw -Encoding UTF8 -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    } else {
      $warnings.Add('备份恢复凭据可被普通用户修改，独立备份检查保持阻塞。')
    }
  } catch {
    $warnings.Add('无法验证独立备份恢复凭据，备份检查保持阻塞。')
  }
}

[ordered]@{
  externalPowerState = $externalPowerState
  pendingRestartState = $pendingRestartState
  backupReceipt = $backupReceipt
  backupReceiptAclSafe = $backupReceiptAclSafe
  warnings = @($warnings)
} | ConvertTo-Json -Depth 4 -Compress
";

/// Discovers non-mutating system evidence for a partition safety assessment.
///
/// A fixed administrator-protected provider receipt is accepted only after it
/// proves an out-of-place restore on a different physical disk.
///
/// # Errors
///
/// Returns an error when the platform is unsupported, trusted Windows
/// `PowerShell` cannot be located or launched, or provider JSON is invalid.
pub fn discover_partition_safety_evidence(
    source_disk_id: &str,
) -> Result<PartitionSafetyEvidence, PartitionSafetyDiscoveryError> {
    #[cfg(not(windows))]
    {
        let _ = source_disk_id;
        Err(PartitionSafetyDiscoveryError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    {
        discover_windows_partition_safety_evidence(source_disk_id)
    }
}

#[cfg(windows)]
fn discover_windows_partition_safety_evidence(
    source_disk_id: &str,
) -> Result<PartitionSafetyEvidence, PartitionSafetyDiscoveryError> {
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
    Ok(convert_evidence(raw, source_disk_id, current_unix_ms()))
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

fn convert_evidence(
    mut raw: PowerShellSafetyEvidence,
    source_disk_id: &str,
    now_unix_ms: u64,
) -> PartitionSafetyEvidence {
    let backup_evidence_state = match raw.backup_receipt {
        None => BackupEvidenceState::Unavailable,
        Some(_) if !raw.backup_receipt_acl_safe => BackupEvidenceState::Unknown,
        Some(receipt) => match receipt.verify_for_disk(source_disk_id, now_unix_ms) {
            Ok(()) => BackupEvidenceState::Verified,
            Err(BackupVerificationError::Expired) => BackupEvidenceState::Stale,
            Err(error) => {
                raw.warnings
                    .push(format!("独立备份恢复凭据未通过验证：{error}"));
                BackupEvidenceState::Unknown
            }
        },
    };
    PartitionSafetyEvidence {
        captured_at_unix_ms: now_unix_ms,
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
        backup_evidence_state,
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
    backup_receipt: Option<BackupVerificationReceipt>,
    #[serde(default)]
    backup_receipt_acl_safe: bool,
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
        let evidence = convert_evidence(
            PowerShellSafetyEvidence {
                external_power_state: "connected".to_owned(),
                pending_restart_state: "clear".to_owned(),
                backup_receipt: None,
                backup_receipt_acl_safe: false,
                warnings: vec![],
            },
            "disk-source",
            500,
        );
        assert_eq!(evidence.external_power_state, ExternalPowerState::Connected);
        assert_eq!(evidence.pending_restart_state, PendingRestartState::Clear);
        assert_eq!(
            evidence.backup_evidence_state,
            BackupEvidenceState::Unavailable
        );
    }

    #[test]
    fn maps_unrecognized_provider_values_to_unknown() {
        let evidence = convert_evidence(
            PowerShellSafetyEvidence {
                external_power_state: "future-value".to_owned(),
                pending_restart_state: "future-value".to_owned(),
                backup_receipt: None,
                backup_receipt_acl_safe: false,
                warnings: vec!["partial".to_owned()],
            },
            "disk-source",
            500,
        );
        assert_eq!(evidence.external_power_state, ExternalPowerState::Unknown);
        assert_eq!(evidence.pending_restart_state, PendingRestartState::Unknown);
        assert_eq!(evidence.discovery_warnings, vec!["partial"]);
    }

    #[test]
    fn only_accepts_acl_protected_matching_restore_receipt() {
        let receipt = BackupVerificationReceipt {
            schema_version: 1,
            source_disk_id: "disk-source".to_owned(),
            destination_disk_id: "disk-backup".to_owned(),
            recovery_point_id: "point-1".to_owned(),
            manifest_sha256: "a".repeat(64),
            backup_completed_at_unix_ms: 100,
            restore_verified_at_unix_ms: 200,
            expires_at_unix_ms: 1_000,
            restore_digest_verified: true,
        };
        let evidence = convert_evidence(
            PowerShellSafetyEvidence {
                external_power_state: "connected".to_owned(),
                pending_restart_state: "clear".to_owned(),
                backup_receipt: Some(receipt.clone()),
                backup_receipt_acl_safe: true,
                warnings: vec![],
            },
            "disk-source",
            500,
        );
        assert_eq!(
            evidence.backup_evidence_state,
            BackupEvidenceState::Verified
        );

        let unsafe_evidence = convert_evidence(
            PowerShellSafetyEvidence {
                external_power_state: "connected".to_owned(),
                pending_restart_state: "clear".to_owned(),
                backup_receipt: Some(receipt),
                backup_receipt_acl_safe: false,
                warnings: vec![],
            },
            "disk-source",
            500,
        );
        assert_eq!(
            unsafe_evidence.backup_evidence_state,
            BackupEvidenceState::Unknown
        );
    }
}
