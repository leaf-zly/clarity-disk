//! Read-only system volume discovery used by the dashboard command.

use std::path::Path;

use clarity_core::{
    DashboardError, DiskCategory, DiskCategoryKind, DiskMetadata, DiskSummary, VolumeHealthStatus,
};
use sysinfo::Disks;

/// Discovers every mounted logical volume using read-only operating system
/// counters. No directory traversal, deletion, mounting, or partition change
/// occurs in this function.
pub fn discover_disks() -> Result<Vec<DiskSummary>, DiscoveryError> {
    let disks = Disks::new_with_refreshed_list();
    let system_root = system_root();
    let mut discovered = disks
        .list()
        .iter()
        .map(|disk| {
            let mount_point = disk.mount_point().to_string_lossy().into_owned();
            let id = volume_id(&mount_point);
            let total_bytes = disk.total_space();
            let used_bytes = total_bytes.saturating_sub(disk.available_space());
            let is_system_volume = paths_refer_to_same_volume(
                Path::new(&mount_point),
                Path::new(&system_root),
            );
            let is_read_only = disk.is_read_only();
            let health_status = if is_read_only {
                VolumeHealthStatus::ReadOnly
            } else {
                VolumeHealthStatus::Healthy
            };
            let health_note = is_read_only.then(|| "卷当前报告为只读".to_owned());
            let volume_name = disk.name().to_string_lossy();
            let label = if volume_name.trim().is_empty() {
                format!("本地磁盘 ({id})")
            } else {
                format!("{} · 本地磁盘 ({id})", volume_name.trim())
            };
            let categories = (used_bytes > 0).then(|| DiskCategory {
                // A detailed category scan is the next product slice. Until
                // then, keep occupied bytes visibly unclassified.
                kind: DiskCategoryKind::System,
                label: "待详细扫描".to_owned(),
                bytes: used_bytes,
            });

            DiskSummary::try_new(
                id,
                label,
                total_bytes,
                used_bytes,
                categories.into_iter().collect(),
            )
            .map(|summary| {
                summary.with_metadata(DiskMetadata {
                    mount_point,
                    file_system: disk.file_system().to_string_lossy().into_owned(),
                    device_type: disk.kind().to_string(),
                    is_system_volume,
                    is_removable: disk.is_removable(),
                    is_read_only,
                    health_status,
                    health_note,
                })
            })
            .map_err(DiscoveryError::InvalidCapacity)
        })
        .collect::<Result<Vec<_>, _>>()?;

    discovered.sort_by(|left, right| {
        right
            .metadata
            .is_system_volume
            .cmp(&left.metadata.is_system_volume)
            .then_with(|| left.id.cmp(&right.id))
    });

    if discovered.is_empty() {
        return Err(DiscoveryError::NoVolumesFound);
    }

    Ok(discovered)
}

fn system_root() -> String {
    std::env::var_os("SystemDrive").map_or_else(
        || r"C:\".to_owned(),
        |drive| format!("{}\\", drive.to_string_lossy()),
    )
}

fn volume_id(mount_point: &str) -> String {
    mount_point.trim_end_matches(['\\', '/']).to_owned()
}

/// Errors produced while reading the primary volume.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// No mounted logical volumes were returned by the operating system.
    #[error("no mounted volumes were found")]
    NoVolumesFound,
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
