//! Durable persistence for quarantine previews and restricted recovery records.

use std::path::PathBuf;
use std::sync::Mutex;

use clarity_core::{QuarantineEntry, QuarantineEntryStatus, QuarantineIndex, QuarantinePolicy};

use crate::state_store::{StateStoreError, load_json, state_path, write_json};

/// Thread-safe store that preserves real recovery records across preview refreshes.
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
    /// Loads one index path and reconciles interrupted staging records.
    pub(crate) fn with_path(path: PathBuf) -> Self {
        let mut index: Option<QuarantineIndex> = load_json(&path);
        if let Some(index) = index.as_mut()
            && reconcile_transactions(index)
        {
            let _ = write_json(&path, index);
        }
        Self {
            index: Mutex::new(index),
            path,
        }
    }

    /// Replaces the latest preview index using a recoverable file swap.
    ///
    /// # Errors
    ///
    /// Returns an error when local state cannot be safely persisted.
    pub(crate) fn save(&self, mut index: QuarantineIndex) -> Result<(), StateStoreError> {
        let mut guard = self.index.lock().expect("quarantine index state poisoned");
        if let Some(existing) = guard.as_ref() {
            index.policy = existing.policy;
            // Rebuilding a preview must never discard recovery records created
            // by an earlier confirmed execution.
            let retained: Vec<_> = existing
                .entries
                .iter()
                .filter(|entry| {
                    entry.status != clarity_core::QuarantineEntryStatus::PreviewOnly
                        && !index
                            .entries
                            .iter()
                            .any(|candidate| candidate.entry_id == entry.entry_id)
                })
                .cloned()
                .collect();
            index.entries.extend(retained);
        }
        recompute(&mut index);
        write_json(&self.path, &index)?;
        *guard = Some(index);
        Ok(())
    }

    /// Appends a durable entry before or after a quarantine movement boundary.
    ///
    /// # Errors
    ///
    /// Returns an error when the updated index cannot be safely persisted.
    pub(crate) fn upsert_entry(
        &self,
        plan_id: &str,
        entry: QuarantineEntry,
    ) -> Result<QuarantineIndex, StateStoreError> {
        let mut guard = self.index.lock().expect("quarantine index state poisoned");
        let index = guard.get_or_insert_with(|| QuarantineIndex {
            index_id: format!("quarantine-{plan_id}"),
            plan_id: plan_id.to_owned(),
            created_at_unix_ms: entry.moved_at_unix_ms.unwrap_or_default(),
            entries: Vec::new(),
            files_moved: false,
            total_bytes: 0,
            policy: QuarantinePolicy::default(),
        });
        plan_id.clone_into(&mut index.plan_id);
        index
            .entries
            .retain(|existing| existing.entry_id != entry.entry_id);
        index.entries.push(entry);
        recompute(index);
        write_json(&self.path, index)?;
        Ok(index.clone())
    }

    /// Replaces one existing entry by identity after restore evaluation.
    ///
    /// # Errors
    ///
    /// Returns an error when the entry is unknown or persistence fails.
    pub(crate) fn replace_entry(
        &self,
        entry: QuarantineEntry,
    ) -> Result<QuarantineIndex, QuarantineStoreError> {
        let mut guard = self.index.lock().expect("quarantine index state poisoned");
        let index = guard.as_mut().ok_or(QuarantineStoreError::EntryNotFound)?;
        let existing = index
            .entries
            .iter_mut()
            .find(|existing| existing.entry_id == entry.entry_id)
            .ok_or(QuarantineStoreError::EntryNotFound)?;
        *existing = entry;
        recompute(index);
        write_json(&self.path, index).map_err(QuarantineStoreError::State)?;
        Ok(index.clone())
    }

    /// Returns the latest locally persisted quarantine index, if any.
    pub(crate) fn get(&self) -> Option<QuarantineIndex> {
        let mut guard = self.index.lock().expect("quarantine index state poisoned");
        if let Some(index) = guard.as_mut()
            && mark_expired(index, unix_ms())
        {
            let _ = write_json(&self.path, index);
        }
        guard.clone()
    }

    /// Applies a validated fixed-tier policy and updates expiry markers.
    ///
    /// # Errors
    ///
    /// Returns an error if the latest index cannot be persisted.
    pub(crate) fn update_policy(
        &self,
        policy: QuarantinePolicy,
    ) -> Result<QuarantineIndex, StateStoreError> {
        let mut guard = self.index.lock().expect("quarantine index state poisoned");
        let index = guard.get_or_insert_with(|| QuarantineIndex {
            index_id: "quarantine-policy".to_owned(),
            plan_id: String::new(),
            created_at_unix_ms: unix_ms(),
            entries: Vec::new(),
            files_moved: false,
            total_bytes: 0,
            policy,
        });
        index.policy = policy;
        for entry in &mut index.entries {
            entry.expires_at_unix_ms = entry
                .moved_at_unix_ms
                .map(|moved| moved.saturating_add(u64::from(policy.retention_days) * 86_400_000));
            if entry.status == QuarantineEntryStatus::Expired
                && entry
                    .expires_at_unix_ms
                    .is_some_and(|expires| expires > unix_ms())
            {
                entry.status = QuarantineEntryStatus::Staged;
            }
        }
        mark_expired(index, unix_ms());
        recompute(index);
        write_json(&self.path, index)?;
        Ok(index.clone())
    }
}

fn recompute(index: &mut QuarantineIndex) {
    index.files_moved = index.entries.iter().any(|entry| {
        matches!(
            entry.status,
            QuarantineEntryStatus::Staging
                | QuarantineEntryStatus::Copying
                | QuarantineEntryStatus::CopyVerified
                | QuarantineEntryStatus::Staged
                | QuarantineEntryStatus::Expired
                | QuarantineEntryStatus::Restoring
                | QuarantineEntryStatus::RestoreConflict
        )
    });
    index.total_bytes = index
        .entries
        .iter()
        .filter(|entry| {
            !matches!(
                entry.status,
                QuarantineEntryStatus::PreviewOnly | QuarantineEntryStatus::Restored
            )
        })
        .map(|entry| entry.bytes)
        .fold(0_u64, u64::saturating_add);
}

fn reconcile_transactions(index: &mut QuarantineIndex) -> bool {
    let mut changed = false;
    for entry in &mut index.entries {
        if !matches!(
            entry.status,
            QuarantineEntryStatus::Staging
                | QuarantineEntryStatus::Copying
                | QuarantineEntryStatus::CopyVerified
                | QuarantineEntryStatus::Restoring
        ) {
            continue;
        }
        let original_exists = PathBuf::from(&entry.original_path).exists();
        let quarantine_exists = entry
            .quarantine_path
            .as_deref()
            .is_some_and(|path| PathBuf::from(path).exists());
        entry.status = match (original_exists, quarantine_exists) {
            (false, true) => QuarantineEntryStatus::Staged,
            // Both copies are present only after external interference or an
            // incomplete non-atomic copy; retain the record for manual review.
            // A verified copy and its source are both preserved for explicit
            // user review; startup recovery never deletes the source.
            (true, true) => QuarantineEntryStatus::CopyVerified,
            (true, false) => QuarantineEntryStatus::PreviewOnly,
            (false, false) => QuarantineEntryStatus::RestoreConflict,
        };
        changed = true;
    }
    if changed {
        recompute(index);
    }
    changed
}

fn mark_expired(index: &mut QuarantineIndex, now_unix_ms: u64) -> bool {
    let mut changed = false;
    for entry in &mut index.entries {
        if entry.status == QuarantineEntryStatus::Staged
            && entry
                .expires_at_unix_ms
                .is_some_and(|expires| expires <= now_unix_ms)
        {
            // Expiry is only a label. Permanent deletion remains unavailable.
            entry.status = QuarantineEntryStatus::Expired;
            changed = true;
        }
    }
    changed
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum QuarantineStoreError {
    #[error("quarantine entry was not found")]
    EntryNotFound,
    #[error(transparent)]
    State(#[from] StateStoreError),
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clarity_core::{QuarantineEntry, QuarantineEntryStatus, QuarantineIndex, QuarantinePolicy};

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
            policy: QuarantinePolicy::default(),
        };
        store.save(index.clone()).expect("index should persist");
        assert_eq!(QuarantineStore::with_path(path.clone()).get(), Some(index));
        fs::write(&path, b"not-json").expect("corrupt fixture should write");
        assert!(QuarantineStore::with_path(path).get().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_reconciles_a_durable_staging_record() {
        let root = std::env::temp_dir().join(format!(
            "clarity-disk-quarantine-reconcile-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let quarantine_path = root.join("quarantine/item");
        fs::create_dir_all(quarantine_path.parent().expect("parent should exist"))
            .expect("quarantine root should be created");
        fs::write(&quarantine_path, b"data").expect("quarantine fixture should be written");
        let state_path = root.join("state/index.json");
        let store = QuarantineStore::with_path(state_path.clone());
        store
            .upsert_entry(
                "plan-1",
                QuarantineEntry {
                    entry_id: "entry-1".to_owned(),
                    candidate_id: "user-temp.v1".to_owned(),
                    rule_id: "user-temp.v1".to_owned(),
                    original_path: root.join("source/item").to_string_lossy().into_owned(),
                    quarantine_path: Some(quarantine_path.to_string_lossy().into_owned()),
                    bytes: 4,
                    metadata_digest: "digest".to_owned(),
                    status: QuarantineEntryStatus::Staging,
                    moved_at_unix_ms: Some(1),
                    restored_at_unix_ms: None,
                    expires_at_unix_ms: Some(u64::MAX),
                    transfer_kind: clarity_core::QuarantineTransferKind::Rename,
                    integrity_digest: None,
                    transaction_path: None,
                },
            )
            .expect("staging record should persist");

        let reconciled = QuarantineStore::with_path(state_path)
            .get()
            .expect("index should exist");
        assert_eq!(reconciled.entries[0].status, QuarantineEntryStatus::Staged);
        assert!(reconciled.files_moved);
        let _ = fs::remove_dir_all(root);
    }
}
