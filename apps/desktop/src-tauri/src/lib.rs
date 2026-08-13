//! Tauri command adapter for the Clarity Disk desktop application.

use clarity_core::{CleanupSummary, DashboardSnapshot, DiskHealth, Suggestion, SuggestionRisk};
use std::sync::OnceLock;

mod cleanup_scan;
mod disk_discovery;
mod space_scan;

static SPACE_SCANS: OnceLock<space_scan::SpaceScanManager> = OnceLock::new();

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

/// Creates a fresh immutable cleanup plan without authorizing execution.
#[tauri::command]
fn prepare_cleanup_plan() -> Result<clarity_core::CleanupPlan, String> {
    let preview = cleanup_scan::scan_cleanup_preview().map_err(|error| error.to_string())?;
    clarity_core::CleanupPlan::from_preview(&preview).map_err(|error| error.to_string())
}

/// Starts a bounded, read-only directory scan.
#[tauri::command]
fn start_space_scan(
    request: clarity_core::SpaceScanRequest,
) -> Result<clarity_core::SpaceScanStart, String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .start(request)
        .map_err(|error| error.to_string())
}

/// Returns the latest snapshot for a space scan task.
#[tauri::command]
fn get_space_scan(scan_id: String) -> Result<clarity_core::SpaceScanSnapshot, String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .snapshot(&scan_id)
        .map_err(|error| error.to_string())
}

/// Requests cooperative cancellation of a space scan task.
#[tauri::command]
fn cancel_space_scan(scan_id: String) -> Result<(), String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .cancel(&scan_id)
        .map_err(|error| error.to_string())
}

/// Pauses a running space scan at a cooperative boundary.
#[tauri::command]
fn pause_space_scan(scan_id: String) -> Result<(), String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .pause(&scan_id)
        .map_err(|error| error.to_string())
}

/// Resumes a cooperatively paused space scan.
#[tauri::command]
fn resume_space_scan(scan_id: String) -> Result<(), String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .resume(&scan_id)
        .map_err(|error| error.to_string())
}

/// Returns recent terminal space scan summaries.
#[tauri::command]
fn get_space_scan_history() -> Vec<clarity_core::SpaceScanHistoryEntry> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .history()
}

/// Returns a safe default scope for the selected volume.
#[tauri::command]
fn get_default_space_scan_request(
    root_path: String,
) -> Result<clarity_core::SpaceScanRequest, String> {
    space_scan::default_request(root_path).map_err(|error| error.to_string())
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
            scan_cleanup_preview,
            prepare_cleanup_plan,
            start_space_scan,
            get_space_scan,
            cancel_space_scan,
            pause_space_scan,
            resume_space_scan,
            get_space_scan_history,
            get_default_space_scan_request
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Clarity Disk");
}
