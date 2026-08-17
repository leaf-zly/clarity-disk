//! Durable, versioned application settings persistence.

use std::path::PathBuf;
use std::sync::Mutex;

use clarity_core::AppSettings;

use crate::state_store::{StateStoreError, load_json, state_path, write_json};

/// Thread-safe store that validates every settings replacement.
pub(crate) struct SettingsStore {
    settings: Mutex<AppSettings>,
    path: PathBuf,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::with_path(state_path("settings.v1.json"))
    }
}

impl SettingsStore {
    fn with_path(path: PathBuf) -> Self {
        let loaded: AppSettings = load_json(&path);
        let settings = if loaded.validate().is_ok() {
            loaded
        } else {
            AppSettings::default()
        };
        Self {
            settings: Mutex::new(settings),
            path,
        }
    }

    /// Returns a detached settings snapshot.
    pub(crate) fn get(&self) -> AppSettings {
        self.settings
            .lock()
            .expect("application settings state poisoned")
            .clone()
    }

    /// Validates and durably replaces the settings document.
    ///
    /// # Errors
    ///
    /// Returns a user-safe validation or persistence error.
    pub(crate) fn replace(&self, settings: AppSettings) -> Result<AppSettings, String> {
        settings.validate().map_err(|error| error.to_string())?;
        write_json(&self.path, &settings).map_err(|error| error.to_string())?;
        *self
            .settings
            .lock()
            .expect("application settings state poisoned") = settings.clone();
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clarity_core::AutomaticMaintenanceSchedule;

    use super::SettingsStore;

    #[test]
    fn settings_round_trip_and_corruption_falls_back_to_safe_defaults() {
        let root =
            std::env::temp_dir().join(format!("clarity-disk-settings-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("settings.json");
        let store = SettingsStore::with_path(path.clone());
        let mut settings = store.get();
        settings.automatic_maintenance = AutomaticMaintenanceSchedule::Weekly;
        store
            .replace(settings.clone())
            .expect("settings should persist");
        assert_eq!(SettingsStore::with_path(path.clone()).get(), settings);

        fs::write(&path, b"not-json").expect("corrupt fixture should write");
        assert_eq!(
            SettingsStore::with_path(path).get().automatic_maintenance,
            AutomaticMaintenanceSchedule::Disabled
        );
        let _ = fs::remove_dir_all(root);
    }
}
