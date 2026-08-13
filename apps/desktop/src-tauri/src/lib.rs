//! Tauri command adapter for the Clarity Disk desktop application.

use clarity_core::{CleanupSummary, DashboardSnapshot, DiskHealth, Suggestion, SuggestionRisk};

mod cleanup_scan;
mod disk_discovery;

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

/// Returns the current dashboard snapshot.
///
/// Disk capacity comes from read-only platform discovery. Cleanup categories,
/// device health, and recommendations remain preview data until their dedicated
/// scanners are implemented.
#[tauri::command]
fn get_dashboard_snapshot() -> Result<DashboardSnapshot, String> {
    let disks = disk_discovery::discover_disks().map_err(|error| error.to_string())?;
    let disk = disks
        .iter()
        .find(|volume| volume.metadata.is_system_volume)
        .cloned()
        .ok_or_else(|| "Windows system volume was not found".to_owned())?;
    let health = DiskHealth {
        status: match disk.metadata.health_status {
            clarity_core::VolumeHealthStatus::Healthy => "良好".to_owned(),
            clarity_core::VolumeHealthStatus::ReadOnly => "只读".to_owned(),
            clarity_core::VolumeHealthStatus::Warning => "需注意".to_owned(),
        },
        device_type: disk.metadata.device_type.clone(),
        temperature_celsius: None,
        has_warning: disk.metadata.health_status != clarity_core::VolumeHealthStatus::Healthy,
    };

    Ok(DashboardSnapshot {
        disk,
        disks,
        health,
        cleanup: CleanupSummary {
            reclaimable_bytes: GIB + 860 * MIB,
            category_count: 3,
        },
        suggestions: vec![
            Suggestion {
                id: "hibernation".to_owned(),
                title: "休眠文件占用较大".to_owned(),
                description: "若不使用休眠，可释放 12.74 GB".to_owned(),
                risk: SuggestionRisk::ConfirmationRequired,
                reclaimable_bytes: 12 * GIB + 758 * MIB,
            },
            Suggestion {
                id: "browser-cache".to_owned(),
                title: "浏览器缓存".to_owned(),
                description: "Chrome 与 Edge 共 454 MB".to_owned(),
                risk: SuggestionRisk::Safe,
                reclaimable_bytes: 454 * MIB,
            },
            Suggestion {
                id: "recycle-bin".to_owned(),
                title: "回收站".to_owned(),
                description: "3,700 个文件，共 389 MB".to_owned(),
                risk: SuggestionRisk::Review,
                reclaimable_bytes: 389 * MIB,
            },
        ],
    })
}

/// Produces a read-only browser-cache cleanup preview.
#[tauri::command]
fn scan_cleanup_preview() -> Result<clarity_core::CleanupPreview, String> {
    cleanup_scan::scan_cleanup_preview().map_err(|error| error.to_string())
}

/// Starts the desktop runtime and registers the minimal command surface.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or the desktop event loop terminates
/// with a fatal runtime error.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_dashboard_snapshot,
            scan_cleanup_preview
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Clarity Disk");
}
