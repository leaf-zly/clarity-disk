//! Restricted cleanup execution with allow-listed adapters and recoverable transfers.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_core::{
    CleanupCandidate, CleanupExecutionItemResult, CleanupExecutionItemStatus, QuarantineEntry,
    QuarantineEntryStatus, QuarantineRestoreResult, QuarantineTransferKind,
};
use sha2::{Digest, Sha256};

use crate::cleanup_adapters::{AdapterTarget, adapter_for, allowed_roots, roots_match};
use crate::quarantine_store::QuarantineStore;

/// Returns whether a candidate belongs to a reviewed quarantine adapter.
pub(crate) fn is_quarantine_executable(candidate: &CleanupCandidate) -> bool {
    adapter_for(&candidate.rule_id).is_some_and(|adapter| adapter.quarantine)
        && candidate.quarantine_eligible
        && !candidate.requires_admin
}

/// Returns whether a candidate is the official Windows Recycle Bin operation.
pub(crate) fn is_recycle_bin_executable(candidate: &CleanupCandidate) -> bool {
    adapter_for(&candidate.rule_id)
        .is_some_and(|adapter| adapter.target == AdapterTarget::WindowsRecycleBin)
        && !candidate.requires_admin
}

/// Stages every reviewed root of one freshly revalidated candidate.
///
/// Execution roots must match the current backend adapter exactly. The display
/// path is never parsed, and arbitrary UI-authored paths are never accepted.
///
/// # Errors
///
/// Returns an error for unsupported rules, changed roots, unsafe paths, capacity
/// exhaustion, or durable-state failures.
pub(crate) fn stage_candidate(
    plan_id: &str,
    candidate: &CleanupCandidate,
    store: &QuarantineStore,
) -> Result<CleanupExecutionItemResult, CleanupExecutorError> {
    if !roots_match(&candidate.rule_id, &candidate.execution_roots) {
        return Err(CleanupExecutorError::AllowListChanged);
    }
    let quarantine_root = quarantine_root()?;
    stage_candidate_at(plan_id, candidate, store, &quarantine_root, false)
}

fn stage_candidate_at(
    plan_id: &str,
    candidate: &CleanupCandidate,
    store: &QuarantineStore,
    quarantine_root: &Path,
    force_copy: bool,
) -> Result<CleanupExecutionItemResult, CleanupExecutorError> {
    if !is_quarantine_executable(candidate) {
        return Err(CleanupExecutorError::UnsupportedRule(
            candidate.rule_id.clone(),
        ));
    }
    let adapter = adapter_for(&candidate.rule_id)
        .ok_or_else(|| CleanupExecutorError::UnsupportedRule(candidate.rule_id.clone()))?;
    if !is_clean_absolute(quarantine_root) {
        return Err(CleanupExecutorError::UnsafeQuarantinePath);
    }
    fs::create_dir_all(quarantine_root).map_err(CleanupExecutorError::CreateQuarantine)?;
    if !path_chain_is_safe(quarantine_root).map_err(CleanupExecutorError::ReadPathMetadata)? {
        return Err(CleanupExecutorError::UnsafeQuarantinePath);
    }

    let roots: Vec<PathBuf> = candidate
        .execution_roots
        .iter()
        .map(PathBuf::from)
        .collect();
    if roots.is_empty() {
        return Err(CleanupExecutorError::AllowListChanged);
    }
    let mut items = Vec::new();
    for root in roots {
        match adapter.target {
            AdapterTarget::RootContents => {
                validate_directory_root(&root)?;
                for item in fs::read_dir(&root).map_err(CleanupExecutorError::ReadRoot)? {
                    if let Ok(item) = item {
                        items.push((root.clone(), item.path()));
                    }
                }
            }
            AdapterTarget::ExactItems => {
                if root.exists() {
                    let parent = root.parent().ok_or(CleanupExecutorError::UnsafeRoot)?;
                    validate_directory_root(parent)?;
                    items.push((parent.to_path_buf(), root));
                }
            }
            AdapterTarget::WindowsRecycleBin | AdapterTarget::ReadOnly => {
                return Err(CleanupExecutorError::UnsupportedRule(
                    candidate.rule_id.clone(),
                ));
            }
        }
    }

    let destination_root = quarantine_root.join(plan_id).join(&candidate.id);
    fs::create_dir_all(&destination_root).map_err(CleanupExecutorError::CreateQuarantine)?;
    let mut staged_bytes = 0_u64;
    let mut staged_items = 0_u64;
    let mut skipped_items = 0_u64;
    for (allowed_parent, original_path) in items {
        let result = stage_item(
            plan_id,
            candidate,
            &allowed_parent,
            &original_path,
            &destination_root,
            quarantine_root,
            store,
            force_copy,
        );
        match result {
            Ok(bytes) => {
                staged_bytes = staged_bytes.saturating_add(bytes);
                staged_items = staged_items.saturating_add(1);
            }
            Err(CleanupExecutorError::CapacityExceeded) => {
                skipped_items = skipped_items.saturating_add(1);
            }
            Err(_) => skipped_items = skipped_items.saturating_add(1),
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
                "部分项目已隔离；锁定、不安全或超出容量的项目已跳过".to_owned()
            }
            _ => "没有可安全移动的项目，未修改文件".to_owned(),
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn stage_item(
    plan_id: &str,
    candidate: &CleanupCandidate,
    allowed_parent: &Path,
    original_path: &Path,
    destination_root: &Path,
    quarantine_root: &Path,
    store: &QuarantineStore,
    force_copy: bool,
) -> Result<u64, CleanupExecutorError> {
    let metadata =
        fs::symlink_metadata(original_path).map_err(CleanupExecutorError::ReadRootMetadata)?;
    if original_path.parent() != Some(allowed_parent)
        || original_path.starts_with(quarantine_root)
        || is_unsafe_indirection(&metadata)
        || !tree_is_safe(original_path, &metadata)
    {
        return Err(CleanupExecutorError::UnsafeRoot);
    }
    let bytes = item_size(original_path, &metadata);
    let current = store.get();
    let policy = current.as_ref().map_or_default(|index| index.policy);
    let used = current.as_ref().map_or(0, |index| index.total_bytes);
    if used.saturating_add(bytes) > policy.max_bytes {
        return Err(CleanupExecutorError::CapacityExceeded);
    }
    let entry_id = entry_identity(plan_id, candidate, original_path);
    let quarantine_path = destination_root.join(&entry_id);
    let transaction_path = destination_root.join(format!(".{entry_id}.copying"));
    if quarantine_path.exists() || transaction_path.exists() {
        return Err(CleanupExecutorError::UnsafeQuarantinePath);
    }
    let now = unix_ms();
    let expires_at = now.saturating_add(u64::from(policy.retention_days) * 86_400_000);
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
        expires_at_unix_ms: Some(expires_at),
        transfer_kind: QuarantineTransferKind::Rename,
        integrity_digest: None,
        transaction_path: None,
    };
    store.upsert_entry(plan_id, entry.clone())?;

    if !force_copy {
        match fs::rename(original_path, &quarantine_path) {
            Ok(()) => {
                entry.status = QuarantineEntryStatus::Staged;
                store.upsert_entry(plan_id, entry)?;
                return Ok(bytes);
            }
            Err(error) if !is_cross_device(&error) => {
                entry.status = QuarantineEntryStatus::PreviewOnly;
                store.upsert_entry(plan_id, entry)?;
                return Err(CleanupExecutorError::StageMove(error));
            }
            Err(_) => {}
        }
    }

    // Cross-volume staging never removes the source until the copied tree is
    // verified and atomically promoted inside the application-owned directory.
    entry.status = QuarantineEntryStatus::Copying;
    entry.transfer_kind = QuarantineTransferKind::VerifiedCopy;
    entry.transaction_path = Some(transaction_path.to_string_lossy().into_owned());
    store.upsert_entry(plan_id, entry.clone())?;
    copy_tree(original_path, &transaction_path, &metadata)?;
    let source_digest = integrity_digest(original_path)?;
    let copied_digest = integrity_digest(&transaction_path)?;
    if source_digest != copied_digest {
        return Err(CleanupExecutorError::IntegrityMismatch);
    }
    fs::rename(&transaction_path, &quarantine_path).map_err(CleanupExecutorError::StageMove)?;
    entry.status = QuarantineEntryStatus::CopyVerified;
    entry.integrity_digest = Some(source_digest);
    entry.transaction_path = None;
    store.upsert_entry(plan_id, entry.clone())?;
    remove_tree(original_path, &metadata).map_err(CleanupExecutorError::RemoveSource)?;
    entry.status = QuarantineEntryStatus::Staged;
    store.upsert_entry(plan_id, entry)?;
    Ok(bytes)
}

/// Restores one backend-owned quarantine entry without overwriting conflicts.
///
/// # Errors
///
/// Returns an error for invalid state, adapter drift, unsafe paths, integrity
/// mismatch, or filesystem failures.
pub(crate) fn restore_entry(
    entry: &QuarantineEntry,
) -> Result<(QuarantineEntry, QuarantineRestoreResult), CleanupExecutorError> {
    let quarantine_root = quarantine_root()?;
    restore_entry_at(
        entry,
        &quarantine_root,
        &allowed_roots(&entry.rule_id),
        false,
    )
}

fn restore_entry_at(
    entry: &QuarantineEntry,
    quarantine_root: &Path,
    allowed_roots: &[PathBuf],
    force_copy: bool,
) -> Result<(QuarantineEntry, QuarantineRestoreResult), CleanupExecutorError> {
    if !matches!(
        entry.status,
        QuarantineEntryStatus::Staged
            | QuarantineEntryStatus::Expired
            | QuarantineEntryStatus::RestoreConflict
            | QuarantineEntryStatus::CopyVerified
            | QuarantineEntryStatus::Restoring
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
    let metadata =
        fs::symlink_metadata(&quarantine).map_err(CleanupExecutorError::ReadQuarantineMetadata)?;
    if !tree_is_safe(&quarantine, &metadata) {
        return Err(CleanupExecutorError::UnsafeQuarantinePath);
    }
    let destination_allowed = is_clean_absolute(&original)
        && original.parent().is_some_and(|parent| {
            allowed_roots.iter().any(|root| {
                if root.is_dir() {
                    parent == root
                } else {
                    &original == root
                }
            })
        });
    if !destination_allowed || original.exists() {
        let mut updated = entry.clone();
        updated.status = QuarantineEntryStatus::RestoreConflict;
        return Ok((
            updated,
            QuarantineRestoreResult {
                entry_id: entry.entry_id.clone(),
                status: QuarantineEntryStatus::RestoreConflict,
                reason: "原位置不再允许或已有同名项目，未覆盖任何文件".to_owned(),
            },
        ));
    }
    let parent = original
        .parent()
        .ok_or(CleanupExecutorError::UnsafeRestoreDestination)?;
    fs::create_dir_all(parent).map_err(CleanupExecutorError::CreateRestoreParent)?;
    if !path_chain_is_safe(parent).map_err(CleanupExecutorError::ReadPathMetadata)? {
        return Err(CleanupExecutorError::UnsafeRestoreDestination);
    }
    let mut updated = entry.clone();
    updated.status = QuarantineEntryStatus::Restoring;
    if !force_copy {
        match fs::rename(&quarantine, &original) {
            Ok(()) => return Ok(restored(updated, entry)),
            Err(error) if !is_cross_device(&error) => {
                return Err(CleanupExecutorError::RestoreMove(error));
            }
            Err(_) => {}
        }
    }
    let transaction = parent.join(format!(".clarity-restore-{}", entry.entry_id));
    if transaction.exists() {
        return Err(CleanupExecutorError::UnsafeRestoreDestination);
    }
    copy_tree(&quarantine, &transaction, &metadata)?;
    if integrity_digest(&quarantine)? != integrity_digest(&transaction)? {
        return Err(CleanupExecutorError::IntegrityMismatch);
    }
    fs::rename(&transaction, &original).map_err(CleanupExecutorError::RestoreMove)?;
    remove_tree(&quarantine, &metadata).map_err(CleanupExecutorError::RemoveSource)?;
    Ok(restored(updated, entry))
}

fn restored(
    mut updated: QuarantineEntry,
    original: &QuarantineEntry,
) -> (QuarantineEntry, QuarantineRestoreResult) {
    updated.status = QuarantineEntryStatus::Restored;
    updated.restored_at_unix_ms = Some(unix_ms());
    (
        updated,
        QuarantineRestoreResult {
            entry_id: original.entry_id.clone(),
            status: QuarantineEntryStatus::Restored,
            reason: "已恢复到原位置".to_owned(),
        },
    )
}

/// Permanently empties Windows Recycle Bin through the documented Shell API.
///
/// The volume is resolved by the fixed adapter and cannot come from UI input.
///
/// # Errors
///
/// Returns an error when the platform is unsupported, the fixed volume is
/// unavailable, or the Shell API reports failure.
pub(crate) fn empty_windows_recycle_bin() -> Result<(), CleanupExecutorError> {
    empty_windows_recycle_bin_impl()
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn empty_windows_recycle_bin_impl() -> Result<(), CleanupExecutorError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW,
    };
    let root = std::env::var_os("SystemDrive")
        .map(PathBuf::from)
        .ok_or(CleanupExecutorError::SystemDriveUnavailable)?;
    let mut wide: Vec<u16> = root.as_os_str().encode_wide().collect();
    wide.push(0);
    // SAFETY: `wide` is NUL-terminated and lives for the duration of the call;
    // HWND is null by design because Clarity Disk owns the confirmation UI.
    let result = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            wide.as_ptr(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    if result < 0 {
        return Err(CleanupExecutorError::RecycleBinApi(result));
    }
    Ok(())
}

#[cfg(not(windows))]
fn empty_windows_recycle_bin_impl() -> Result<(), CleanupExecutorError> {
    Err(CleanupExecutorError::UnsupportedPlatform)
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    metadata: &fs::Metadata,
) -> Result<(), CleanupExecutorError> {
    if is_unsafe_indirection(metadata) {
        return Err(CleanupExecutorError::UnsafeRoot);
    }
    if metadata.is_file() {
        fs::copy(source, destination).map_err(CleanupExecutorError::CopyTransfer)?;
        return Ok(());
    }
    fs::create_dir(destination).map_err(CleanupExecutorError::CopyTransfer)?;
    for item in fs::read_dir(source).map_err(CleanupExecutorError::ReadRoot)? {
        let item = item.map_err(CleanupExecutorError::ReadRoot)?;
        let child = item.path();
        let child_metadata =
            fs::symlink_metadata(&child).map_err(CleanupExecutorError::ReadRootMetadata)?;
        copy_tree(&child, &destination.join(item.file_name()), &child_metadata)?;
    }
    Ok(())
}

fn integrity_digest(path: &Path) -> Result<String, CleanupExecutorError> {
    let metadata = fs::symlink_metadata(path).map_err(CleanupExecutorError::ReadRootMetadata)?;
    let mut hasher = Sha256::new();
    digest_tree(path, &metadata, &mut hasher)?;
    Ok(format_digest(hasher.finalize()))
}

fn digest_tree(
    path: &Path,
    metadata: &fs::Metadata,
    hasher: &mut Sha256,
) -> Result<(), CleanupExecutorError> {
    if is_unsafe_indirection(metadata) {
        return Err(CleanupExecutorError::UnsafeRoot);
    }
    hasher.update(metadata.len().to_le_bytes());
    if metadata.is_file() {
        let bytes = fs::read(path).map_err(CleanupExecutorError::CopyTransfer)?;
        hasher.update(bytes);
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(path)
        .map_err(CleanupExecutorError::ReadRoot)?
        .collect::<Result<_, _>>()
        .map_err(CleanupExecutorError::ReadRoot)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        hasher.update(entry.file_name().to_string_lossy().as_bytes());
        let child = entry.path();
        let child_metadata =
            fs::symlink_metadata(&child).map_err(CleanupExecutorError::ReadRootMetadata)?;
        digest_tree(&child, &child_metadata, hasher)?;
    }
    Ok(())
}

fn remove_tree(path: &Path, metadata: &fs::Metadata) -> std::io::Result<()> {
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn validate_directory_root(root: &Path) -> Result<(), CleanupExecutorError> {
    if !is_clean_absolute(root)
        || !path_chain_is_safe(root).map_err(CleanupExecutorError::ReadPathMetadata)?
    {
        return Err(CleanupExecutorError::UnsafeRoot);
    }
    let metadata = fs::symlink_metadata(root).map_err(CleanupExecutorError::ReadRootMetadata)?;
    if !metadata.is_dir() || is_unsafe_indirection(&metadata) {
        return Err(CleanupExecutorError::UnsafeRoot);
    }
    Ok(())
}

fn path_chain_is_safe(path: &Path) -> Result<bool, std::io::Error> {
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

fn entry_identity(plan_id: &str, candidate: &CleanupCandidate, path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plan_id.as_bytes());
    hasher.update(candidate.id.as_bytes());
    hasher.update(path.to_string_lossy().as_bytes());
    format_digest(&hasher.finalize()[..12])
}

fn format_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn item_size(path: &Path, metadata: &fs::Metadata) -> u64 {
    if metadata.is_file() {
        return metadata.len();
    }
    fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|item| {
            fs::symlink_metadata(item.path())
                .ok()
                .map(|metadata| (item.path(), metadata))
        })
        .filter(|(_, metadata)| !is_unsafe_indirection(metadata))
        .map(|(path, metadata)| item_size(&path, &metadata))
        .fold(0, u64::saturating_add)
}

fn tree_is_safe(path: &Path, metadata: &fs::Metadata) -> bool {
    if is_unsafe_indirection(metadata) {
        return false;
    }
    if !metadata.is_dir() {
        return true;
    }
    fs::read_dir(path).ok().is_some_and(|mut entries| {
        entries.all(|item| {
            item.ok().is_some_and(|item| {
                let child = item.path();
                fs::symlink_metadata(&child)
                    .ok()
                    .is_some_and(|metadata| tree_is_safe(&child, &metadata))
            })
        })
    })
}

fn is_cross_device(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::CrossesDevices || error.raw_os_error() == Some(17)
}

fn is_unsafe_indirection(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || is_platform_reparse_point(metadata)
}

#[cfg(windows)]
fn is_platform_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0400 != 0
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

/// Failures from restricted cleanup and recovery operations.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CleanupExecutorError {
    #[error("cleanup rule is not executable: {0}")]
    UnsupportedRule(String),
    #[error("cleanup adapter roots changed; run a fresh scan")]
    AllowListChanged,
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
    #[error("cleanup item could not be moved: {0}")]
    StageMove(std::io::Error),
    #[error("quarantine capacity limit would be exceeded")]
    CapacityExceeded,
    #[error("cross-volume transfer failed: {0}")]
    CopyTransfer(std::io::Error),
    #[error("cross-volume transfer integrity verification failed")]
    IntegrityMismatch,
    #[error("verified source could not be removed: {0}")]
    RemoveSource(std::io::Error),
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
    #[error("SystemDrive is unavailable")]
    SystemDriveUnavailable,
    #[error("Windows Recycle Bin API failed with HRESULT {0}")]
    RecycleBinApi(i32),
    #[error("Windows Recycle Bin cleanup is only supported on Windows")]
    UnsupportedPlatform,
}

#[cfg(test)]
mod tests {
    use super::{restore_entry_at, stage_candidate_at};
    use crate::quarantine_store::QuarantineStore;
    use clarity_core::{CleanupCandidate, QuarantineEntryStatus, RecoveryStrategy, SuggestionRisk};
    use std::fs;

    fn candidate(root: &Path) -> CleanupCandidate {
        CleanupCandidate {
            id: "user-temp.v1".to_owned(),
            rule_id: "user-temp.v1".to_owned(),
            rule_version: "1".to_owned(),
            title: "用户临时文件".to_owned(),
            description: "测试".to_owned(),
            path: root.to_string_lossy().into_owned(),
            execution_roots: vec![root.to_string_lossy().into_owned()],
            evidence: vec![],
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

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "clarity-disk-executor-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn verified_copy_stages_and_restores_without_overwrite() {
        let root = test_root("verified-copy");
        let source = root.join("source");
        let quarantine = root.join("quarantine");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("one.tmp"), b"data").unwrap();
        let store = QuarantineStore::with_path(root.join("state/index.json"));
        let result =
            stage_candidate_at("plan-1", &candidate(&source), &store, &quarantine, true).unwrap();
        assert_eq!(result.staged_items, 1);
        let entry = store
            .get()
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.status == QuarantineEntryStatus::Staged)
            .unwrap();
        assert_eq!(
            entry.transfer_kind,
            clarity_core::QuarantineTransferKind::VerifiedCopy
        );
        let (updated, result) =
            restore_entry_at(&entry, &quarantine, std::slice::from_ref(&source), true).unwrap();
        assert_eq!(updated.status, QuarantineEntryStatus::Restored);
        assert_eq!(result.status, QuarantineEntryStatus::Restored);
        assert_eq!(fs::read(source.join("one.tmp")).unwrap(), b"data");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restore_conflict_never_overwrites() {
        let root = test_root("conflict");
        let source = root.join("source");
        let quarantine = root.join("quarantine");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("one.tmp"), b"old").unwrap();
        let store = QuarantineStore::with_path(root.join("state/index.json"));
        stage_candidate_at("plan-1", &candidate(&source), &store, &quarantine, false).unwrap();
        let entry = store.get().unwrap().entries[0].clone();
        fs::write(source.join("one.tmp"), b"new").unwrap();
        let (_, result) =
            restore_entry_at(&entry, &quarantine, std::slice::from_ref(&source), false).unwrap();
        assert_eq!(result.status, QuarantineEntryStatus::RestoreConflict);
        assert_eq!(fs::read(source.join("one.tmp")).unwrap(), b"new");
        let _ = fs::remove_dir_all(root);
    }

    use std::path::{Path, PathBuf};
}
