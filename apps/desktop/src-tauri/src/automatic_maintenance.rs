//! Read-only automatic maintenance coordinator and Windows protection discovery.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_core::{
    AppSettings, AutomaticMaintenanceContext, AutomaticMaintenanceDecision, CleanupPreview,
};
use serde::{Deserialize, Serialize};

use crate::state_store::{load_json, state_path, write_json};

const MAINTENANCE_CONTEXT_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8

if (-not ('ClarityDiskIdleTime' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ClarityDiskIdleTime {
  [StructLayout(LayoutKind.Sequential)] public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
  [DllImport("user32.dll")] static extern bool GetLastInputInfo(ref LASTINPUTINFO info);
  public static uint Seconds() {
    var info = new LASTINPUTINFO(); info.cbSize = (uint)Marshal.SizeOf(info);
    uint now = unchecked((uint)Environment.TickCount);
    return GetLastInputInfo(ref info) ? unchecked(now - info.dwTime) / 1000 : 0;
  }
}
'@
}

$stablePower = $false
try {
  $batteries = @(Get-CimInstance -ClassName Win32_Battery -ErrorAction Stop)
  if ($batteries.Count -eq 0) { $stablePower = $true }
  else {
    $connected = @(2, 3, 6, 7, 8, 9, 11)
    $stablePower = @($batteries | Where-Object { $connected -notcontains [int]$_.BatteryStatus }).Count -eq 0
  }
} catch { $stablePower = $false }

$updateActive = $false
try {
  $updateActive = $null -ne (Get-Process -Name 'TiWorker','TrustedInstaller' -ErrorAction SilentlyContinue | Select-Object -First 1)
} catch { $updateActive = $true }

$backupActive = $false
try {
  $backupActive = $null -ne (Get-Process -Name 'wbengine','sdclt' -ErrorAction SilentlyContinue | Select-Object -First 1)
} catch { $backupActive = $true }

[ordered]@{
  idleSeconds = [ClarityDiskIdleTime]::Seconds()
  stablePower = $stablePower
  systemUpdateActive = $updateActive
  backupActive = $backupActive
} | ConvertTo-Json -Compress
"#;

/// Result of one scheduler tick; automatic maintenance never deletes content.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomaticMaintenanceRunReport {
    /// Conservative scheduler decision.
    pub decision: AutomaticMaintenanceDecision,
    /// Read-only cleanup preview when a due scan completed.
    pub preview: Option<CleanupPreview>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutomaticMaintenanceState {
    last_run_at_unix_ms: Option<u64>,
}

/// Serializes scheduler ticks and persists the last successful run.
pub(crate) struct AutomaticMaintenanceCoordinator {
    state: Mutex<AutomaticMaintenanceState>,
    path: PathBuf,
}

impl Default for AutomaticMaintenanceCoordinator {
    fn default() -> Self {
        let path = state_path("automatic-maintenance.v1.json");
        Self {
            state: Mutex::new(load_json(&path)),
            path,
        }
    }
}

impl AutomaticMaintenanceCoordinator {
    /// Evaluates fresh Windows protections and runs at most one read-only scan.
    ///
    /// # Errors
    ///
    /// Returns an error when protection discovery, scanning, or durable state
    /// persistence fails. Unknown protection state never starts a scan.
    pub(crate) fn run_if_due(
        &self,
        settings: &AppSettings,
        scan: impl FnOnce() -> Result<CleanupPreview, String>,
    ) -> Result<AutomaticMaintenanceRunReport, String> {
        let discovered = discover_context()?;
        let now_unix_ms = unix_ms();
        let mut state = self
            .state
            .lock()
            .expect("automatic maintenance state poisoned");
        let decision = AutomaticMaintenanceDecision::evaluate(
            settings.automatic_maintenance,
            AutomaticMaintenanceContext {
                now_unix_ms,
                last_run_at_unix_ms: state.last_run_at_unix_ms,
                idle_seconds: discovered.idle_seconds,
                stable_power: discovered.stable_power,
                system_update_active: discovered.system_update_active,
                backup_active: discovered.backup_active,
            },
        );
        if !decision.should_run {
            return Ok(AutomaticMaintenanceRunReport {
                decision,
                preview: None,
            });
        }
        let preview = scan()?;
        state.last_run_at_unix_ms = Some(now_unix_ms);
        write_json(&self.path, &*state).map_err(|error| error.to_string())?;
        Ok(AutomaticMaintenanceRunReport {
            decision,
            preview: Some(preview),
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveredMaintenanceContext {
    idle_seconds: u32,
    stable_power: bool,
    system_update_active: bool,
    backup_active: bool,
}

fn discover_context() -> Result<DiscoveredMaintenanceContext, String> {
    #[cfg(not(windows))]
    {
        Err("automatic maintenance protection discovery is supported only on Windows".to_owned())
    }
    #[cfg(windows)]
    {
        let system_root = std::env::var_os("SystemRoot")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Windows SystemRoot is unavailable".to_owned())?;
        let powershell =
            PathBuf::from(system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let output = Command::new(powershell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                MAINTENANCE_CONTEXT_SCRIPT,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| {
                format!("could not launch maintenance protection discovery: {error}")
            })?;
        if !output.status.success() {
            return Err("automatic maintenance protections could not be established".to_owned());
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("invalid maintenance protection response: {error}"))
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::AutomaticMaintenanceState;

    #[test]
    fn state_defaults_without_prior_run() {
        assert_eq!(
            AutomaticMaintenanceState::default().last_run_at_unix_ms,
            None
        );
    }
}
