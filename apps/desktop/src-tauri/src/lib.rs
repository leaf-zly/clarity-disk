//! Tauri command adapter for the Clarity Disk desktop application.

use clarity_core::{CleanupSummary, DashboardSnapshot, DiskHealth, Suggestion, SuggestionRisk};
use std::sync::OnceLock;

mod audit_store;
mod cleanup_executor;
mod cleanup_scan;
mod cleanup_workflow;
mod disk_discovery;
mod quarantine_store;
mod space_scan;
mod state_store;

static SPACE_SCANS: OnceLock<space_scan::SpaceScanManager> = OnceLock::new();
static CLEANUP_WORKFLOW: OnceLock<cleanup_workflow::CleanupWorkflow> = OnceLock::new();

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

/// Produces a read-only cleanup preview from versioned allow-listed rules.
#[tauri::command]
fn scan_cleanup_preview() -> Result<clarity_core::CleanupPreview, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .scan()
}

/// Creates an immutable plan from IDs in the latest preview without authorizing execution.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn prepare_cleanup_plan(
    request: clarity_core::PrepareCleanupPlanRequest,
) -> Result<clarity_core::CleanupPlan, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .prepare_plan(&request)
}

/// Creates a persisted quarantine index preview without moving any file.
#[tauri::command]
fn prepare_quarantine_index(plan_id: &str) -> Result<clarity_core::QuarantineIndex, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .prepare_quarantine(plan_id)
}

/// Returns the latest preview-only quarantine index, if present.
#[tauri::command]
fn get_quarantine_index() -> Option<clarity_core::QuarantineIndex> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .quarantine_index()
}

/// Returns newest privacy-preserving cleanup audit events.
#[tauri::command]
fn get_audit_events() -> Vec<clarity_core::AuditEvent> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .audit_events()
}

/// Issues a short-lived one-time confirmation after a fresh read-only validation.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn prepare_cleanup_execution(
    request: clarity_core::PrepareCleanupExecutionRequest,
) -> Result<clarity_core::CleanupExecutionChallenge, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .prepare_execution(&request)
}

/// Consumes a one-time confirmation and runs the restricted quarantine executor.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn execute_cleanup(
    request: clarity_core::ExecuteCleanupRequest,
) -> Result<clarity_core::CleanupExecutionReport, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .execute(&request)
}

/// Restores one backend-indexed item without overwriting an existing path.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn restore_quarantine_entry(
    request: clarity_core::RestoreQuarantineRequest,
) -> Result<clarity_core::QuarantineRestoreResult, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .restore(&request)
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
fn get_space_scan(scan_id: &str) -> Result<clarity_core::SpaceScanSnapshot, String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .snapshot(scan_id)
        .map_err(|error| error.to_string())
}

/// Requests cooperative cancellation of a space scan task.
#[tauri::command]
fn cancel_space_scan(scan_id: &str) -> Result<(), String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .cancel(scan_id)
        .map_err(|error| error.to_string())
}

/// Pauses a running space scan at a cooperative boundary.
#[tauri::command]
fn pause_space_scan(scan_id: &str) -> Result<(), String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .pause(scan_id)
        .map_err(|error| error.to_string())
}

/// Resumes a cooperatively paused space scan.
#[tauri::command]
fn resume_space_scan(scan_id: &str) -> Result<(), String> {
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .resume(scan_id)
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
            prepare_quarantine_index,
            get_quarantine_index,
            get_audit_events,
            prepare_cleanup_execution,
            execute_cleanup,
            restore_quarantine_entry,
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
