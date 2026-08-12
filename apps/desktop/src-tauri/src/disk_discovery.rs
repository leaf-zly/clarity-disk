//! Read-only system volume discovery used by the dashboard command.

use std::path::Path;

use clarity_core::{DashboardError, DiskCategory, DiskCategoryKind, DiskSummary};
use sysinfo::Disks;

/// Finds the primary Windows volume without changing files, permissions, or
/// partition metadata.
///
/// The implementation intentionally reads only volume capacity counters.
/// Category attribution and cleanup rules remain separate so discovery cannot
/// accidentally become a destructive operation.
pub fn discover_primary_disk() -> Result<DiskSummary, DiscoveryError> {
    let disks = Disks::new_with_refreshed_list();
    let system_root = std::env::var_os("SystemDrive").map_or_else(
        || r"C:\".to_owned(),
        |drive| format!("{}\\", drive.to_string_lossy()),
    );

    let disk = disks
        .list()
        .iter()
        .find(|disk| paths_refer_to_same_volume(disk.mount_point(), Path::new(&system_root)))
        .ok_or(DiscoveryError::SystemVolumeNotFound)?;

    let total_bytes = disk.total_space();
    let used_bytes = total_bytes.saturating_sub(disk.available_space());
    let volume_name = disk.name().to_string_lossy();
    let label = if volume_name.trim().is_empty() {
        "Windows".to_owned()
    } else {
        volume_name.trim().to_owned()
    };
    let drive_id = system_root.trim_end_matches(['\\', '/']).to_owned();

    DiskSummary::try_new(
        drive_id.clone(),
        format!("{label} · 本地磁盘 ({drive_id})"),
        total_bytes,
        used_bytes,
        vec![DiskCategory {
            // A detailed category scan is the next product slice. Until then,
            // represent all occupied capacity as system-managed/uncategorized.
            kind: DiskCategoryKind::System,
            label: "待详细扫描".to_owned(),
            bytes: used_bytes,
        }],
    )
    .map_err(DiscoveryError::InvalidCapacity)
}

/// Errors produced while reading the primary volume.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// No mounted disk matched the Windows system drive.
    #[error("Windows system volume was not found")]
    SystemVolumeNotFound,
    /// Capacity values could not satisfy the domain invariants.
    #[error("invalid volume capacity: {0}")]
    InvalidCapacity(#[from] DashboardError),
}

/// Compares mount paths using Windows' case-insensitive drive semantics while
/// remaining deterministic in unit tests on other targets.
fn paths_refer_to_same_volume(left: &Path, right: &Path) -> bool {
    normalize_mount_path(left) == normalize_mount_path(right)
}

fn normalize_mount_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::paths_refer_to_same_volume;

    #[test]
    fn matches_windows_mount_paths_case_insensitively() {
        assert!(paths_refer_to_same_volume(
            Path::new(r"C:\"),
            Path::new("c:")
        ));
    }

    #[test]
    fn rejects_different_drive_mounts() {
        assert!(!paths_refer_to_same_volume(
            Path::new(r"C:\"),
            Path::new(r"D:\")
        ));
    }
}
