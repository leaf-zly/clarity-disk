//! Versioned user settings and conservative automatic-maintenance scheduling.

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SETTINGS_SCHEMA_VERSION: u16 = 1;
const MAX_IGNORED_ROOTS: usize = 32;
const MIN_IDLE_SECONDS: u32 = 5 * 60;

/// Persisted, privacy-first application preferences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Version of the serialized settings contract.
    pub schema_version: u16,
    /// Visual appearance preference.
    pub theme: ThemePreference,
    /// Display language preference.
    pub language: LanguagePreference,
    /// Local diagnostic detail; file names and contents are never included.
    pub log_level: LogLevel,
    /// Whether local privacy-safe crash diagnostics may be retained.
    pub retain_crash_diagnostics: bool,
    /// Whether desktop notifications may be shown.
    pub notifications_enabled: bool,
    /// Whether the application should start with Windows.
    pub launch_at_login: bool,
    /// Reviewed quarantine retention tier in days.
    pub quarantine_retention_days: u16,
    /// Reviewed quarantine capacity tier in bytes.
    pub quarantine_max_bytes: u64,
    /// Read-only automatic maintenance cadence.
    pub automatic_maintenance: AutomaticMaintenanceSchedule,
    /// User-selected scan roots to omit from ordinary read-only scans.
    pub ignored_scan_roots: Vec<String>,
    /// Whether update metadata may be checked from the official repository.
    pub update_checks_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            theme: ThemePreference::System,
            language: LanguagePreference::SimplifiedChinese,
            log_level: LogLevel::Standard,
            retain_crash_diagnostics: true,
            notifications_enabled: true,
            launch_at_login: false,
            quarantine_retention_days: 30,
            quarantine_max_bytes: 10 * 1024 * 1024 * 1024,
            automatic_maintenance: AutomaticMaintenanceSchedule::Disabled,
            ignored_scan_roots: Vec::new(),
            update_checks_enabled: true,
        }
    }
}

impl AppSettings {
    /// Validates persisted values before they can replace current settings.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown schema versions, unsupported quarantine
    /// tiers, or ambiguous and excessive ignored roots.
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(SettingsError::UnsupportedSchema);
        }
        if ![7, 15, 30].contains(&self.quarantine_retention_days) {
            return Err(SettingsError::UnsupportedRetention);
        }
        let gib = 1024_u64 * 1024 * 1024;
        if ![gib, 5 * gib, 10 * gib, 20 * gib].contains(&self.quarantine_max_bytes) {
            return Err(SettingsError::UnsupportedCapacity);
        }
        if self.ignored_scan_roots.len() > MAX_IGNORED_ROOTS {
            return Err(SettingsError::TooManyIgnoredRoots);
        }
        for root in &self.ignored_scan_roots {
            let trimmed = root.trim();
            let drive_absolute = trimmed.len() >= 3
                && trimmed.as_bytes()[1] == b':'
                && matches!(trimmed.as_bytes()[2], b'\\' | b'/');
            let unc_absolute = trimmed.starts_with("\\\\");
            if trimmed.is_empty() || trimmed.contains('\0') || (!drive_absolute && !unc_absolute) {
                return Err(SettingsError::InvalidIgnoredRoot);
            }
        }
        Ok(())
    }
}

/// Supported appearance choices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    /// Follow the Windows application theme.
    #[default]
    System,
    /// Always use the light palette.
    Light,
    /// Always use the dark palette.
    Dark,
}

/// Supported interface languages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LanguagePreference {
    /// Simplified Chinese.
    #[default]
    SimplifiedChinese,
    /// English.
    English,
}

/// Privacy-safe local logging verbosity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    /// Only terminal errors and safety decisions.
    Minimal,
    /// Normal lifecycle and safety events.
    #[default]
    Standard,
    /// Additional performance timings without file names or contents.
    Diagnostic,
}

/// Supported read-only automatic-maintenance cadence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutomaticMaintenanceSchedule {
    /// No background maintenance evaluation.
    #[default]
    Disabled,
    /// Evaluate after seven days.
    Weekly,
    /// Evaluate after thirty days.
    Monthly,
}

/// Fresh machine context used to determine whether a scheduled scan may start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutomaticMaintenanceContext {
    /// Current Unix timestamp in milliseconds.
    pub now_unix_ms: u64,
    /// Last successful automatic scan, if any.
    pub last_run_at_unix_ms: Option<u64>,
    /// User idle time in seconds.
    pub idle_seconds: u32,
    /// Whether the machine is on stable external power.
    pub stable_power: bool,
    /// Whether Windows Update is actively applying work.
    pub system_update_active: bool,
    /// Whether a known backup job is active.
    pub backup_active: bool,
}

/// Conservative result of one automatic-maintenance scheduling evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomaticMaintenanceDecision {
    /// Whether a read-only scan may start now.
    pub should_run: bool,
    /// Stable explanation suitable for a local history record.
    pub reason: AutomaticMaintenanceReason,
    /// Next eligible time when it can be calculated.
    pub next_eligible_at_unix_ms: Option<u64>,
}

impl AutomaticMaintenanceDecision {
    /// Evaluates cadence and machine protections without authorizing deletion.
    pub fn evaluate(
        schedule: AutomaticMaintenanceSchedule,
        context: AutomaticMaintenanceContext,
    ) -> Self {
        let interval_ms = match schedule {
            AutomaticMaintenanceSchedule::Disabled => {
                return Self::blocked(AutomaticMaintenanceReason::Disabled, None);
            }
            AutomaticMaintenanceSchedule::Weekly => 7 * 24 * 60 * 60 * 1_000_u64,
            AutomaticMaintenanceSchedule::Monthly => 30 * 24 * 60 * 60 * 1_000_u64,
        };
        let next = context
            .last_run_at_unix_ms
            .map(|last| last.saturating_add(interval_ms));
        if next.is_some_and(|next| context.now_unix_ms < next) {
            return Self::blocked(AutomaticMaintenanceReason::NotDue, next);
        }
        if context.idle_seconds < MIN_IDLE_SECONDS {
            return Self::blocked(AutomaticMaintenanceReason::UserActive, next);
        }
        if !context.stable_power {
            return Self::blocked(AutomaticMaintenanceReason::PowerUnsafe, next);
        }
        if context.system_update_active {
            return Self::blocked(AutomaticMaintenanceReason::SystemUpdateActive, next);
        }
        if context.backup_active {
            return Self::blocked(AutomaticMaintenanceReason::BackupActive, next);
        }
        Self {
            should_run: true,
            reason: AutomaticMaintenanceReason::Due,
            next_eligible_at_unix_ms: Some(context.now_unix_ms.saturating_add(interval_ms)),
        }
    }

    fn blocked(reason: AutomaticMaintenanceReason, next: Option<u64>) -> Self {
        Self {
            should_run: false,
            reason,
            next_eligible_at_unix_ms: next,
        }
    }
}

/// Stable scheduling outcomes shown in settings and history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutomaticMaintenanceReason {
    /// Scheduling is disabled.
    Disabled,
    /// The configured interval has not elapsed.
    NotDue,
    /// Recent input indicates the user is active.
    UserActive,
    /// Stable power could not be established.
    PowerUnsafe,
    /// Windows Update is active.
    SystemUpdateActive,
    /// A backup job is active.
    BackupActive,
    /// All protections pass and a read-only scan is due.
    Due,
}

/// Validation failures for persisted settings.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SettingsError {
    /// Settings schema is not supported by this application version.
    #[error("settings schema is unsupported")]
    UnsupportedSchema,
    /// Quarantine retention is not a reviewed fixed tier.
    #[error("settings quarantine retention tier is unsupported")]
    UnsupportedRetention,
    /// Quarantine capacity is not a reviewed fixed tier.
    #[error("settings quarantine capacity tier is unsupported")]
    UnsupportedCapacity,
    /// Too many ignored roots were submitted.
    #[error("settings contains too many ignored scan roots")]
    TooManyIgnoredRoots,
    /// An ignored root was empty, relative, or malformed.
    #[error("settings contains an invalid ignored scan root")]
    InvalidIgnoredRoot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_privacy_first_and_valid() {
        let settings = AppSettings::default();
        assert_eq!(settings.validate(), Ok(()));
        assert!(!settings.launch_at_login);
        assert_eq!(
            settings.automatic_maintenance,
            AutomaticMaintenanceSchedule::Disabled
        );
    }

    #[test]
    fn ignored_roots_must_be_absolute_windows_paths() {
        let mut settings = AppSettings::default();
        settings.ignored_scan_roots = vec!["relative".to_owned()];
        assert_eq!(settings.validate(), Err(SettingsError::InvalidIgnoredRoot));
        settings.ignored_scan_roots = vec!["D:\\Projects".to_owned()];
        assert_eq!(settings.validate(), Ok(()));
    }

    #[test]
    fn scheduler_fails_closed_until_every_protection_passes() {
        let context = AutomaticMaintenanceContext {
            now_unix_ms: 1_000_000_000,
            last_run_at_unix_ms: None,
            idle_seconds: 600,
            stable_power: true,
            system_update_active: false,
            backup_active: false,
        };
        assert!(
            AutomaticMaintenanceDecision::evaluate(AutomaticMaintenanceSchedule::Weekly, context)
                .should_run
        );
        assert_eq!(
            AutomaticMaintenanceDecision::evaluate(
                AutomaticMaintenanceSchedule::Weekly,
                AutomaticMaintenanceContext {
                    stable_power: false,
                    ..context
                }
            )
            .reason,
            AutomaticMaintenanceReason::PowerUnsafe
        );
    }
}
