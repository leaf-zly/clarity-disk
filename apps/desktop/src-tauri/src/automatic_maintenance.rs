//! Read-only automatic maintenance coordinator and Windows protection discovery.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_core::{
    AppSettings, AutomaticMaintenanceContext, AutomaticMaintenanceDecision, CleanupPreview,
};
use serde::{Deserialize, Serialize};

use crate::state_store::{load_json, state_path, write_json};

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

#[derive(Clone, Debug)]
struct DiscoveredMaintenanceContext {
    idle_seconds: u32,
    stable_power: bool,
    system_update_active: bool,
    backup_active: bool,
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
        let now_unix_ms = unix_ms();
        let mut state = self
            .state
            .lock()
            .expect("automatic maintenance state poisoned");
        // Cadence is evaluated with permissive machine evidence first. This
        // avoids launching Windows discovery when maintenance is disabled or
        // not due, while real evidence still gates every eligible scan.
        let cadence = AutomaticMaintenanceDecision::evaluate(
            settings.automatic_maintenance,
            AutomaticMaintenanceContext {
                now_unix_ms,
                last_run_at_unix_ms: state.last_run_at_unix_ms,
                idle_seconds: u32::MAX,
                stable_power: true,
                system_update_active: false,
                backup_active: false,
            },
        );
        if !cadence.should_run {
            return Ok(AutomaticMaintenanceRunReport {
                decision: cadence,
                preview: None,
            });
        }

        let discovered = discover_context()?;
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

fn discover_context() -> Result<DiscoveredMaintenanceContext, String> {
    crate::native_maintenance_discovery::discover()
        .map(|context| DiscoveredMaintenanceContext {
            idle_seconds: context.idle_seconds,
            stable_power: context.stable_power,
            system_update_active: context.system_update_active,
            backup_active: context.backup_active,
        })
        .map_err(|error| format!("could not establish maintenance protections: {error}"))
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
    use clarity_core::{AppSettings, AutomaticMaintenanceReason};

    use super::{AutomaticMaintenanceCoordinator, AutomaticMaintenanceState};

    #[test]
    fn state_defaults_without_prior_run() {
        assert_eq!(
            AutomaticMaintenanceState::default().last_run_at_unix_ms,
            None
        );
    }

    #[test]
    fn disabled_schedule_skips_platform_discovery_and_scan() {
        let report = AutomaticMaintenanceCoordinator::default()
            .run_if_due(&AppSettings::default(), || {
                panic!("disabled maintenance must not scan")
            })
            .expect("disabled maintenance should not need Windows discovery");
        assert_eq!(report.decision.reason, AutomaticMaintenanceReason::Disabled);
        assert!(report.preview.is_none());
    }
}
