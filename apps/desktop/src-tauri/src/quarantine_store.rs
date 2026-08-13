//! Persistence for the latest preview-only quarantine index.

use std::path::PathBuf;
use std::sync::Mutex;

use clarity_core::QuarantineIndex;

use crate::state_store::{StateStoreError, load_json, state_path, write_json};

/// Thread-safe store for a single latest quarantine index preview.
pub(crate) struct QuarantineStore {
    index: Mutex<Option<QuarantineIndex>>,
    path: PathBuf,
}

impl Default for QuarantineStore {
    fn default() -> Self {
        Self::with_path(state_path("quarantine-index.json"))
    }
}

impl QuarantineStore {
    fn with_path(path: PathBuf) -> Self {
        Self {
            index: Mutex::new(load_json(&path)),
            path,
        }
    }

    /// Replaces the latest preview index using a recoverable file swap.
    ///
    /// # Errors
    ///
    /// Returns an error when local state cannot be safely persisted.
    pub(crate) fn save(&self, index: QuarantineIndex) -> Result<(), StateStoreError> {
        write_json(&self.path, &index)?;
        *self.index.lock().expect("quarantine index state poisoned") = Some(index);
        Ok(())
    }

    /// Returns the latest locally persisted preview index, if any.
    pub(crate) fn get(&self) -> Option<QuarantineIndex> {
        self.index
            .lock()
            .expect("quarantine index state poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clarity_core::QuarantineIndex;

    use super::QuarantineStore;

    #[test]
    fn quarantine_index_round_trips_and_corruption_is_ignored() {
        let root = std::env::temp_dir().join(format!(
            "clarity-disk-quarantine-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("quarantine-index.json");
        let store = QuarantineStore::with_path(path.clone());
        let index = QuarantineIndex {
            index_id: "index-1".to_owned(),
            plan_id: "plan-1".to_owned(),
            created_at_unix_ms: 1,
            entries: vec![],
            files_moved: false,
            total_bytes: 0,
        };
        store.save(index.clone()).expect("index should persist");
        assert_eq!(QuarantineStore::with_path(path.clone()).get(), Some(index));
        fs::write(&path, b"not-json").expect("corrupt fixture should write");
        assert!(QuarantineStore::with_path(path).get().is_none());
        let _ = fs::remove_dir_all(root);
    }
}
