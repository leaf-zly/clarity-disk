//! Restricted user-level quarantine executor for allow-listed low-risk candidates.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_core::{
    CleanupCandidate, CleanupExecutionItemResult, CleanupExecutionItemStatus, QuarantineEntry,
    QuarantineEntryStatus, QuarantineRestoreResult, RecoveryStrategy,
};
use sha2::{Digest, Sha256};

use crate::quarantine_store::QuarantineStore;

/// Versioned rule IDs supported by the first restricted executor.
const EXECUTABLE_RULES: [&str; 1] = ["user-temp.v1"];

/// Returns whether a candidate is supported by the user-level executor.
pub(crate) fn is_executable(candidate: &CleanupCandidate) -> bool {
    EXECUTABLE_RULES.contains(&candidate.rule_id.as_str())
        && candidate.quarantine_eligible
        && !candidate.requires_admin
        && candidate.recovery_strategy == RecoveryStrategy::Quarantine
}

/// Stages direct children of one allow-listed candidate in application quarantine.
///
/// The caller must provide a candidate copied from a freshly revalidated scan.
/// This function never follows symbolic links or reparse points, never accepts a
/// UI-authored path, and persists a `Staging` recovery entry before each rename.
///
/// # Errors
///
/// Returns an error if the candidate is unsupported, its root is not a safe
/// directory, or recovery metadata cannot be persisted.
pub(crate) fn stage_candidate(
    plan_id: &str,
    candidate: &CleanupCandidate,
    store: &QuarantineStore,
) -> Result<CleanupExecutionItemResult, CleanupExecutorError> {
    let quarantine_root = quarantine_root()?;
    stage_candidate_at(plan_id, candidate, store, &quarantine_root)
}

fn stage_candidate_at(
    plan_id: &str,
    candidate: &CleanupCandidate,
    store: &QuarantineStore,
    quarantine_root: &Path,
) -> Result<CleanupExecutionItemResult, CleanupExecutorError> {
    if !is_executable(candidate) {
        return Err(CleanupExecutorError::UnsupportedRule(
            candidate.rule_id.clone(),
        ));
    }
    let root = PathBuf::from(&candidate.path);
    validate_root(&root)?;
    if !is_clean_absolute(quarantine_root) {
        return Err(CleanupExecutorError::UnsafeQuarantinePath);
    }
    let destination_root = quarantine_root.join(plan_id).join(&candidate.id);
    fs::create_dir_all(&destination_root).map_err(CleanupExecutorError::CreateQuarantine)?;
    if !path_chain_is_safe(&destination_root).map_err(CleanupExecutorError::ReadPathMetadata)? {
        return Err(CleanupExecutorError::UnsafeQuarantinePath);
    }
    let mut staged_bytes = 0_u64;
    let mut staged_items = 0_u64;
    let mut skipped_items = 0_u64;

    for item in fs::read_dir(&root).map_err(CleanupExecutorError::ReadRoot)? {
        let Ok(item) = item else {
            skipped_items = skipped_items.saturating_add(1);
            continue;
        };
        let original_path = item.path();
        if original_path.starts_with(quarantine_root) {
            skipped_items = skipped_items.saturating_add(1);
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&original_path) else {
            skipped_items = skipped_items.saturating_add(1);
            continue;
        };
        if is_unsafe_indirection(&metadata)
            || !original_path.starts_with(&root)
            || !tree_is_safe(&original_path, &metadata)
        {
            skipped_items = skipped_items.saturating_add(1);
            continue;
        }
        let entry_id = entry_identity(plan_id, candidate, &original_path);
        let quarantine_path = destination_root.join(&entry_id);
        if quarantine_path.exists() {
            skipped_items = skipped_items.saturating_add(1);
            continue;
        }
        let bytes = item_size(&original_path, &metadata);
        let now = unix_ms();
        let mut entry = QuarantineEntry {
            entry_id,
            candidate_id: candidate.id.clone(),
            rule_id: candidate.rule_id.clone(),
            original_path: original_path.to_string_lossy().into_owned(),
            quarantine_path: Some(quarantine_path.to_string_lossy().into_owned()),
            bytes,
            metadata_digest: candidate.metadata_digest.clone(),
            status: QuarantineEntryStatus::Staging,
            moved_at_unix_ms: Some(now),
            restored_at_unix_ms: None,
        };
        store.upsert_entry(plan_id, entry.clone())?;
        if fs::rename(&original_path, &quarantine_path).is_ok() {
            entry.status = QuarantineEntryStatus::Staged;
            store.upsert_entry(plan_id, entry)?;
            staged_bytes = staged_bytes.saturating_add(bytes);
            staged_items = staged_items.saturating_add(1);
        } else {
            entry.status = QuarantineEntryStatus::PreviewOnly;
            store.upsert_entry(plan_id, entry)?;
            skipped_items = skipped_items.saturating_add(1);
        }
    }

    let status = if staged_items == 0 {
        CleanupExecutionItemStatus::Skipped
    } else if skipped_items == 0 {
        CleanupExecutionItemStatus::Staged
    } else {
        CleanupExecutionItemStatus::PartiallyStaged
    };
    Ok(CleanupExecutionItemResult {
        candidate_id: candidate.id.clone(),
        status,
        staged_bytes,
        staged_items,
        skipped_items,
        reason: match status {
            CleanupExecutionItemStatus::Staged => "已安全移入隔离区".to_owned(),
            CleanupExecutionItemStatus::PartiallyStaged => {
                "部分项目已隔离；锁定或不安全项目已跳过".to_owned()
            }
            _ => "没有可安全移动的直接子项，未修改文件".to_owned(),
        },
    })
}

/// Restores one backend-owned quarantine entry without overwriting conflicts.
///
/// # Errors
///
/// Returns an error for non-staged entries, missing backend paths, unsafe
/// destinations, or filesystem failures.
pub(crate) fn restore_entry(
    entry: &QuarantineEntry,
) -> Result<(QuarantineEntry, QuarantineRestoreResult), CleanupExecutorError> {
    let quarantine_root = quarantine_root()?;
    let allowed_roots = temp_paths();
    restore_entry_at(entry, &quarantine_root, &allowed_roots)
}

fn restore_entry_at(
    entry: &QuarantineEntry,
    quarantine_root: &Path,
    allowed_roots: &[PathBuf],
) -> Result<(QuarantineEntry, QuarantineRestoreResult), CleanupExecutorError> {
    if !matches!(
        entry.status,
        QuarantineEntryStatus::Staged | QuarantineEntryStatus::RestoreConflict
    ) {
        return Err(CleanupExecutorError::EntryNotStaged);
    }
    let original = PathBuf::from(&entry.original_path);
    let quarantine = entry
        .quarantine_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or(CleanupExecutorError::MissingQuarantinePath)?;
    if !is_clean_absolute(quarantine_root)
        || !is_clean_absolute(&quarantine)
        || !quarantine.starts_with(quarantine_root)
        || !path_chain_is_safe(&quarantine).map_err(CleanupExecutorError::ReadQuarantineMetadata)?
    {
        return Err(CleanupExecutorError::UnsafeQuarantinePath);
    }
    let quarantine_metadata =
        fs::symlink_metadata(&quarantine).map_err(CleanupExecutorError::ReadQuarantineMetadata)?;
    if !tree_is_safe(&quarantine, &quarantine_metadata) {
        return Err(CleanupExecutorError::UnsafeQuarantinePath);
    }
    let destination_allowed = original.parent().is_some_and(|parent| {
        is_clean_absolute(&original)
            && allowed_roots
                .iter()
                .any(|root| is_clean_absolute(root) && parent == root)
    });
    if !destination_allowed {
        return Err(CleanupExecutorError::UnsafeRestoreDestination);
    }
    let original_parent = original
        .parent()
        .ok_or(CleanupExecutorError::UnsafeRestoreDestination)?;
    if !path_chain_is_safe(original_parent).map_err(CleanupExecutorError::ReadPathMetadata)? {
        return Err(CleanupExecutorError::UnsafeRestoreDestination);
    }
    let mut updated = entry.clone();
    if original.exists() {
        updated.status = QuarantineEntryStatus::RestoreConflict;
        return Ok((
            updated,
            QuarantineRestoreResult {
                entry_id: entry.entry_id.clone(),
                status: QuarantineEntryStatus::RestoreConflict,
                reason: "原位置已有同名项目，未覆盖任何文件".to_owned(),
            },
        ));
    }
    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent).map_err(CleanupExecutorError::CreateRestoreParent)?;
    }
    fs::rename(&quarantine, &original).map_err(CleanupExecutorError::RestoreMove)?;
    updated.status = QuarantineEntryStatus::Restored;
    updated.restored_at_unix_ms = Some(unix_ms());
    Ok((
        updated,
        QuarantineRestoreResult {
            entry_id: entry.entry_id.clone(),
            status: QuarantineEntryStatus::Restored,
            reason: "已恢复到原位置".to_owned(),
        },
    ))
}

fn validate_root(root: &Path) -> Result<(), CleanupExecutorError> {
    if !is_clean_absolute(root) {
        return Err(CleanupExecutorError::UnsafeRoot);
    }
    if !path_chain_is_safe(root).map_err(CleanupExecutorError::ReadPathMetadata)? {
        return Err(CleanupExecutorError::UnsafeRoot);
    }
    let metadata = fs::symlink_metadata(root).map_err(CleanupExecutorError::ReadRootMetadata)?;
    if !metadata.is_dir() || is_unsafe_indirection(&metadata) {
        return Err(CleanupExecutorError::UnsafeRoot);
    }
    Ok(())
}

fn path_chain_is_safe(path: &Path) -> Result<bool, std::io::Error> {
    // Reject reparse points in every existing ancestor, not only at the leaf;
    // otherwise an allow-listed textual path could resolve outside its root.
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if is_unsafe_indirection(&metadata) => return Ok(false),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}

fn is_clean_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
}

fn quarantine_root() -> Result<PathBuf, CleanupExecutorError> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("ClarityDisk/quarantine"))
        .ok_or(CleanupExecutorError::LocalAppDataUnavailable)
}

fn temp_paths() -> Vec<PathBuf> {
    std::env::var_os("TEMP")
        .or_else(|| std::env::var_os("TMP"))
        .map(PathBuf::from)
        .into_iter()
        .collect()
}

fn entry_identity(plan_id: &str, candidate: &CleanupCandidate, path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plan_id.as_bytes());
    hasher.update(candidate.id.as_bytes());
    hasher.update(path.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    digest[..12].iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write;
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
        output
    })
}

fn item_size(path: &Path, metadata: &fs::Metadata) -> u64 {
    if metadata.is_file() {
        return metadata.len();
    }
    directory_size(path)
}

fn directory_size(root: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|item| item.path())
        .filter_map(|path| {
            fs::symlink_metadata(&path)
                .ok()
                .map(|metadata| (path, metadata))
        })
        .filter(|(_, metadata)| !is_unsafe_indirection(metadata))
        .map(|(path, metadata)| item_size(&path, &metadata))
        .fold(0_u64, u64::saturating_add)
}

fn tree_is_safe(path: &Path, metadata: &fs::Metadata) -> bool {
    if is_unsafe_indirection(metadata) {
        return false;
    }
    if !metadata.is_dir() {
        return true;
    }
    let Ok(mut entries) = fs::read_dir(path) else {
        return false;
    };
    entries.all(|item| {
        let Ok(item) = item else {
            return false;
        };
        let child = item.path();
        fs::symlink_metadata(&child)
            .ok()
            .is_some_and(|metadata| tree_is_safe(&child, &metadata))
    })
}

fn is_unsafe_indirection(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || is_platform_reparse_point(metadata)
}

#[cfg(windows)]
fn is_platform_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_platform_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
}

/// Failures from the restricted quarantine executor.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CleanupExecutorError {
    #[error("cleanup rule is not executable: {0}")]
    UnsupportedRule(String),
    #[error("cleanup candidate root could not be read: {0}")]
    ReadRootMetadata(std::io::Error),
    #[error("cleanup path chain could not be read: {0}")]
    ReadPathMetadata(std::io::Error),
    #[error("cleanup candidate root is unsafe")]
    UnsafeRoot,
    #[error("cleanup candidate root could not be enumerated: {0}")]
    ReadRoot(std::io::Error),
    #[error("quarantine directory could not be created: {0}")]
    CreateQuarantine(std::io::Error),
    #[error("LOCALAPPDATA is unavailable; restricted execution is disabled")]
    LocalAppDataUnavailable,
    #[error("quarantine state could not be persisted: {0}")]
    State(#[from] crate::state_store::StateStoreError),
    #[error("quarantine entry is not staged")]
    EntryNotStaged,
    #[error("quarantine entry has no backend path")]
    MissingQuarantinePath,
    #[error("quarantine entry path is outside the application directory")]
    UnsafeQuarantinePath,
    #[error("quarantine entry metadata could not be read: {0}")]
    ReadQuarantineMetadata(std::io::Error),
    #[error("restore destination is outside the executable allow-list")]
    UnsafeRestoreDestination,
    #[error("restore parent could not be created: {0}")]
    CreateRestoreParent(std::io::Error),
    #[error("quarantine entry could not be restored: {0}")]
    RestoreMove(std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clarity_core::{CleanupCandidate, QuarantineEntryStatus, RecoveryStrategy, SuggestionRisk};

    use super::{CleanupExecutorError, is_executable, restore_entry_at, stage_candidate_at};
    use crate::quarantine_store::QuarantineStore;

    fn candidate(root: &std::path::Path) -> CleanupCandidate {
        CleanupCandidate {
            id: "user-temp.v1".to_owned(),
            rule_id: "user-temp.v1".to_owned(),
            rule_version: "1".to_owned(),
            title: "用户临时文件".to_owned(),
            description: "测试允许目录".to_owned(),
            path: root.to_string_lossy().into_owned(),
            evidence: vec!["测试固定目录".to_owned()],
            bytes: 4,
            item_count: 1,
            risk: SuggestionRisk::Review,
            recoverable: true,
            requires_admin: false,
            recovery_strategy: RecoveryStrategy::Quarantine,
            quarantine_eligible: true,
            default_selected: false,
            metadata_digest: "digest".to_owned(),
            observed_at_unix_ms: Some(1),
        }
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "clarity-disk-executor-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("test root should be created");
        root
    }

    #[test]
    fn stages_and_restores_without_accepting_paths_from_a_request() {
        let root = test_root("round-trip");
        let source = root.join("source");
        let quarantine = root.join("quarantine");
        fs::create_dir_all(&source).expect("source should be created");
        fs::write(source.join("one.tmp"), b"data").expect("fixture should be written");
        let store = QuarantineStore::with_path(root.join("state/index.json"));

        let result = stage_candidate_at("plan-1", &candidate(&source), &store, &quarantine)
            .expect("allow-listed fixture should stage");
        assert_eq!(result.staged_items, 1);
        assert!(!source.join("one.tmp").exists());
        let entry = store
            .get()
            .expect("index should exist")
            .entries
            .into_iter()
            .find(|entry| entry.status == QuarantineEntryStatus::Staged)
            .expect("staged entry should be indexed");
        let (updated, restored) =
            restore_entry_at(&entry, &quarantine, std::slice::from_ref(&source))
                .expect("staged fixture should restore");
        assert_eq!(restored.status, QuarantineEntryStatus::Restored);
        assert_eq!(updated.status, QuarantineEntryStatus::Restored);
        assert_eq!(
            fs::read(source.join("one.tmp")).expect("file restored"),
            b"data"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restore_conflict_never_overwrites_existing_content() {
        let root = test_root("conflict");
        let source = root.join("source");
        let quarantine = root.join("quarantine");
        fs::create_dir_all(&source).expect("source should be created");
        fs::write(source.join("one.tmp"), b"old").expect("fixture should be written");
        let store = QuarantineStore::with_path(root.join("state/index.json"));
        stage_candidate_at("plan-1", &candidate(&source), &store, &quarantine)
            .expect("fixture should stage");
        let entry = store.get().expect("index should exist").entries[0].clone();
        fs::write(source.join("one.tmp"), b"new").expect("conflict should be written");

        let (_, result) = restore_entry_at(&entry, &quarantine, std::slice::from_ref(&source))
            .expect("conflict is a safe terminal result");
        assert_eq!(result.status, QuarantineEntryStatus::RestoreConflict);
        assert_eq!(
            fs::read(source.join("one.tmp")).expect("conflict remains"),
            b"new"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_candidates_without_quarantine_recovery() {
        let root = test_root("unsupported-recovery");
        let mut candidate = candidate(&root);
        candidate.recovery_strategy = RecoveryStrategy::None;

        assert!(!is_executable(&candidate));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restore_rejects_destinations_outside_the_exact_allow_list() {
        let root = test_root("restore-allow-list");
        let source = root.join("source");
        let quarantine = root.join("quarantine");
        fs::create_dir_all(&source).expect("source should be created");
        fs::write(source.join("one.tmp"), b"data").expect("fixture should be written");
        let store = QuarantineStore::with_path(root.join("state/index.json"));
        stage_candidate_at("plan-1", &candidate(&source), &store, &quarantine)
            .expect("fixture should stage");
        let entry = store.get().expect("index should exist").entries[0].clone();
        let unrelated = root.join("other");

        assert!(matches!(
            restore_entry_at(&entry, &quarantine, &[unrelated]),
            Err(CleanupExecutorError::UnsafeRestoreDestination)
        ));
        assert!(!source.join("one.tmp").exists());
        let _ = fs::remove_dir_all(root);
    }
}
