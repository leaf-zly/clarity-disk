//! Privacy-preserving local audit persistence for cleanup workflows.

use std::cmp::Reverse;
use std::path::PathBuf;
use std::sync::Mutex;

use clarity_core::AuditEvent;

use crate::state_store::{StateStoreError, load_json, state_path, write_json};

const MAX_AUDIT_EVENTS: usize = 200;

/// Thread-safe bounded store for local cleanup audit events.
pub(crate) struct AuditStore {
    events: Mutex<Vec<AuditEvent>>,
    path: PathBuf,
}

impl Default for AuditStore {
    fn default() -> Self {
        Self::with_path(state_path("audit-events.json"))
    }
}

impl AuditStore {
    fn with_path(path: PathBuf) -> Self {
        let mut events: Vec<AuditEvent> = load_json(&path);
        normalize(&mut events);
        Self {
            events: Mutex::new(events),
            path,
        }
    }

    /// Records one event and persists at most 200 newest entries.
    ///
    /// # Errors
    ///
    /// Returns an error when local state cannot be safely replaced.
    pub(crate) fn record(&self, event: AuditEvent) -> Result<(), StateStoreError> {
        let mut events = self.events.lock().expect("cleanup audit state poisoned");
        events.retain(|existing| existing.event_id != event.event_id);
        events.push(event);
        normalize(&mut events);
        write_json(&self.path, &*events)
    }

    /// Returns newest events first without exposing mutable store state.
    pub(crate) fn events(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .expect("cleanup audit state poisoned")
            .clone()
    }

    /// Clears all cleanup audit events using the same recoverable file swap.
    ///
    /// # Errors
    ///
    /// Returns an error when the empty audit document cannot be persisted.
    pub(crate) fn clear(&self) -> Result<(), StateStoreError> {
        let mut events = self.events.lock().expect("cleanup audit state poisoned");
        write_json(&self.path, &Vec::<AuditEvent>::new())?;
        events.clear();
        Ok(())
    }
}

fn normalize(events: &mut Vec<AuditEvent>) {
    events.sort_by_key(|event| Reverse(event.occurred_at_unix_ms));
    events.truncate(MAX_AUDIT_EVENTS);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clarity_core::{AuditEvent, AuditEventKind};

    use super::{AuditStore, MAX_AUDIT_EVENTS};

    #[test]
    fn audit_is_bounded_newest_first_and_corruption_is_ignored() {
        let root =
            std::env::temp_dir().join(format!("clarity-disk-audit-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("audit-events.json");
        let store = AuditStore::with_path(path.clone());
        for index in 0..=MAX_AUDIT_EVENTS {
            store
                .record(AuditEvent {
                    event_id: format!("event-{index}"),
                    kind: AuditEventKind::ScanCompleted,
                    subject_id: format!("scan-{index}"),
                    occurred_at_unix_ms: index as u64,
                    reason: None,
                    candidate_count: 0,
                    total_bytes: 0,
                    rule_ids: vec![],
                })
                .expect("audit event should persist");
        }
        assert_eq!(store.events().len(), MAX_AUDIT_EVENTS);
        assert_eq!(
            store.events()[0].event_id,
            format!("event-{MAX_AUDIT_EVENTS}")
        );
        fs::write(&path, b"not-json").expect("corrupt fixture should write");
        assert!(AuditStore::with_path(path).events().is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn audit_can_be_cleared_durably() {
        let root = std::env::temp_dir().join(format!(
            "clarity-disk-audit-clear-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("audit-events.json");
        let store = AuditStore::with_path(path.clone());
        store
            .record(AuditEvent {
                event_id: "event".to_owned(),
                kind: AuditEventKind::ScanCompleted,
                subject_id: "scan".to_owned(),
                occurred_at_unix_ms: 1,
                reason: None,
                candidate_count: 0,
                total_bytes: 0,
                rule_ids: vec![],
            })
            .expect("event should persist");
        store.clear().expect("audit should clear");
        assert!(AuditStore::with_path(path).events().is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
