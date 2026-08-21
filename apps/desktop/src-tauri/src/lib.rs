//! Tauri command adapter for the Clarity Disk desktop application.

use clarity_core::{CleanupSummary, DashboardSnapshot, DiskHealth, Suggestion, SuggestionRisk};
use std::sync::OnceLock;
use std::time::Instant;

mod audit_store;
mod automatic_maintenance;
mod cleanup_adapters;
mod cleanup_executor;
mod cleanup_scan;
mod cleanup_workflow;
mod diagnostics;
mod disk_discovery;
mod disk_health_discovery;
#[cfg(windows)]
mod native_disk_health_discovery;
mod native_maintenance_discovery;
#[cfg(windows)]
mod native_partition_discovery;
#[cfg(windows)]
mod native_partition_safety_discovery;
#[cfg(windows)]
mod native_storage_wmi_discovery;
mod partition_discovery;
mod partition_safety_discovery;
mod privileged_workflow;
mod quarantine_store;
mod settings_store;
mod space_scan;
mod startup_behavior;
mod state_store;
mod windows_process;

static SPACE_SCANS: OnceLock<space_scan::SpaceScanManager> = OnceLock::new();
static CLEANUP_WORKFLOW: OnceLock<cleanup_workflow::CleanupWorkflow> = OnceLock::new();
static PRIVILEGED_WORKFLOW: OnceLock<privileged_workflow::PrivilegedWorkflow> = OnceLock::new();
static SETTINGS: OnceLock<settings_store::SettingsStore> = OnceLock::new();
static AUTOMATIC_MAINTENANCE: OnceLock<automatic_maintenance::AutomaticMaintenanceCoordinator> =
    OnceLock::new();

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

/// Runs blocking Windows discovery or filesystem work outside Tauri's command
/// dispatch thread so navigation and repainting remain responsive.
async fn run_blocking<T, F>(operation_name: &'static str, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("{operation_name} 后台任务异常终止：{error}"))?
}

/// Returns the current dashboard snapshot.
///
/// Disk capacity comes from read-only platform discovery. Cleanup categories,
/// device health, and recommendations remain preview data until their dedicated
/// scanners are implemented.
#[tauri::command]
async fn get_dashboard_snapshot() -> Result<DashboardSnapshot, String> {
    run_blocking("磁盘概览发现", build_dashboard_snapshot).await
}

fn build_dashboard_snapshot() -> Result<DashboardSnapshot, String> {
    let started = Instant::now();
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

    let snapshot = DashboardSnapshot {
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
    };
    diagnostics::record_performance("dashboardDiscovery", started.elapsed());
    Ok(snapshot)
}

/// Produces a read-only cleanup preview from versioned allow-listed rules.
#[tauri::command]
async fn scan_cleanup_preview() -> Result<clarity_core::CleanupPreview, String> {
    run_blocking("清理预览扫描", scan_cleanup_preview_blocking).await
}

fn scan_cleanup_preview_blocking() -> Result<clarity_core::CleanupPreview, String> {
    let started = Instant::now();
    let preview = CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .scan()?;
    diagnostics::record_performance("cleanupPreview", started.elapsed());
    Ok(preview)
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

/// Restores a bounded batch using backend-owned entry identities only.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn restore_quarantine_batch(
    request: clarity_core::RestoreQuarantineBatchRequest,
) -> Result<clarity_core::QuarantineRestoreBatchReport, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .restore_batch(&request)
}

/// Restores one entry to an enumerated backend-resolved user folder.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn restore_quarantine_entry_to(
    request: clarity_core::RestoreQuarantineToRequest,
) -> Result<clarity_core::QuarantineRestoreResult, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .restore_to(&request)
}

/// Prepares a short-lived challenge for permanent quarantine deletion.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn prepare_quarantine_deletion(
    request: clarity_core::PrepareQuarantineDeletionRequest,
) -> Result<clarity_core::QuarantineDeletionChallenge, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .prepare_deletion(&request)
}

/// Consumes a one-time challenge and deletes only revalidated quarantine entries.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn execute_quarantine_deletion(
    request: clarity_core::ExecuteQuarantineDeletionRequest,
) -> Result<clarity_core::QuarantineDeletionReport, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .execute_deletion(&request)
}

/// Updates quarantine retention and capacity from reviewed fixed tiers.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn update_quarantine_policy(
    request: clarity_core::UpdateQuarantinePolicyRequest,
) -> Result<clarity_core::QuarantineIndex, String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .update_policy(request)
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

/// Returns a fresh read-only physical disk and partition topology.
#[tauri::command]
async fn get_partition_topology() -> Result<clarity_core::PartitionTopology, String> {
    run_blocking("分区拓扑发现", discover_partition_topology).await
}

fn discover_partition_topology() -> Result<clarity_core::PartitionTopology, String> {
    let started = Instant::now();
    let topology =
        partition_discovery::discover_partition_topology().map_err(|error| error.to_string())?;
    diagnostics::record_performance("partitionDiscovery", started.elapsed());
    Ok(topology)
}

/// Returns a fresh, privacy-preserving, read-only physical-disk health snapshot.
#[tauri::command]
async fn get_disk_health_snapshot() -> Result<clarity_core::DiskHealthSnapshot, String> {
    run_blocking("磁盘健康发现", || {
        disk_health_discovery::discover_disk_health().map_err(|error| error.to_string())
    })
    .await
}

/// Re-discovers disk state and evaluates a non-authorizing merge preview.
#[tauri::command]
async fn preview_partition_merge(
    request: clarity_core::MergePreviewRequest,
) -> Result<clarity_core::MergePreview, String> {
    run_blocking("分区合并预演", move || {
        preview_partition_merge_blocking(&request)
    })
    .await
}

fn preview_partition_merge_blocking(
    request: &clarity_core::MergePreviewRequest,
) -> Result<clarity_core::MergePreview, String> {
    let topology =
        partition_discovery::discover_partition_topology().map_err(|error| error.to_string())?;
    topology
        .preview_merge(request)
        .map_err(|error| error.to_string())
}

/// Re-discovers topology and system evidence to build a non-authorizing safety plan.
#[tauri::command]
async fn assess_partition_merge_safety(
    request: clarity_core::MergePreviewRequest,
) -> Result<clarity_core::PartitionSafetyAssessment, String> {
    run_blocking("分区安全评估", move || {
        build_partition_safety_assessment(&request)
    })
    .await
}

fn build_partition_safety_assessment(
    request: &clarity_core::MergePreviewRequest,
) -> Result<clarity_core::PartitionSafetyAssessment, String> {
    let topology =
        partition_discovery::discover_partition_topology().map_err(|error| error.to_string())?;
    let preview = topology
        .preview_merge(request)
        .map_err(|error| error.to_string())?;
    let evidence = partition_safety_discovery::discover_partition_safety_evidence(&preview.disk_id)
        .map_err(|error| error.to_string())?;
    let now_unix_ms = evidence.captured_at_unix_ms;
    clarity_core::PartitionSafetyAssessment::try_new(&preview, evidence, now_unix_ms)
        .map_err(|error| error.to_string())
}

/// Returns the installed one-shot administrator broker capability handshake.
#[tauri::command]
fn get_privileged_capabilities() -> clarity_privileged_protocol::PrivilegedCapabilities {
    privileged_workflow::PrivilegedWorkflow::capabilities()
}

/// Issues a one-time challenge for one fixed privileged maintenance adapter.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn prepare_maintenance_execution(
    request: privileged_workflow::PrepareMaintenanceRequest,
) -> Result<privileged_workflow::PrivilegedExecutionChallenge, String> {
    PRIVILEGED_WORKFLOW
        .get_or_init(privileged_workflow::PrivilegedWorkflow::default)
        .prepare_maintenance(request.operation)
}

/// Re-discovers partition evidence and prepares execution only when all gates pass.
#[tauri::command]
async fn prepare_partition_execution(
    request: clarity_core::MergePreviewRequest,
) -> Result<privileged_workflow::PartitionExecutionPreparation, String> {
    run_blocking("分区执行准备", move || {
        prepare_partition_execution_blocking(&request)
    })
    .await
}

fn prepare_partition_execution_blocking(
    request: &clarity_core::MergePreviewRequest,
) -> Result<privileged_workflow::PartitionExecutionPreparation, String> {
    let assessment = build_partition_safety_assessment(request)?;
    PRIVILEGED_WORKFLOW
        .get_or_init(privileged_workflow::PrivilegedWorkflow::default)
        .prepare_partition(assessment)
}

/// Consumes a one-time challenge and crosses the Windows UAC boundary.
#[tauri::command]
async fn execute_privileged_operation(
    request: privileged_workflow::ExecutePrivilegedRequest,
) -> Result<clarity_privileged_protocol::PrivilegedExecutionReport, String> {
    run_blocking("管理员操作执行", move || {
        PRIVILEGED_WORKFLOW
            .get_or_init(privileged_workflow::PrivilegedWorkflow::default)
            .execute(&request)
    })
    .await
}

/// Returns newest privacy-preserving privileged terminal events.
#[tauri::command]
fn get_privileged_audit_events() -> Vec<privileged_workflow::PrivilegedAuditEvent> {
    PRIVILEGED_WORKFLOW
        .get_or_init(privileged_workflow::PrivilegedWorkflow::default)
        .audit_events()
}

/// Returns validated, versioned local application settings.
#[tauri::command]
fn get_app_settings() -> clarity_core::AppSettings {
    SETTINGS
        .get_or_init(settings_store::SettingsStore::default)
        .get()
}

/// Persists validated settings and synchronizes the reviewed quarantine policy.
#[tauri::command]
async fn update_app_settings(
    settings: clarity_core::AppSettings,
) -> Result<clarity_core::AppSettings, String> {
    run_blocking("设置保存", move || {
        update_app_settings_blocking(settings)
    })
    .await
}

fn update_app_settings_blocking(
    settings: clarity_core::AppSettings,
) -> Result<clarity_core::AppSettings, String> {
    settings.validate().map_err(|error| error.to_string())?;
    let store = SETTINGS.get_or_init(settings_store::SettingsStore::default);
    let previous = store.get();
    let startup_changed = previous.launch_at_login != settings.launch_at_login;
    let policy_changed = previous.quarantine_retention_days != settings.quarantine_retention_days
        || previous.quarantine_max_bytes != settings.quarantine_max_bytes;

    // Avoid touching Windows integration when unrelated preferences are saved.
    if startup_changed {
        startup_behavior::apply_launch_at_login(settings.launch_at_login)?;
    }
    if policy_changed
        && let Err(error) = CLEANUP_WORKFLOW
            .get_or_init(cleanup_workflow::CleanupWorkflow::default)
            .update_policy(clarity_core::UpdateQuarantinePolicyRequest {
                retention_days: settings.quarantine_retention_days,
                max_bytes: settings.quarantine_max_bytes,
            })
    {
        if startup_changed {
            let _ = startup_behavior::apply_launch_at_login(previous.launch_at_login);
        }
        return Err(error);
    }

    let updated = match store.replace(settings) {
        Ok(updated) => updated,
        Err(error) => {
            if policy_changed {
                let _ = CLEANUP_WORKFLOW
                    .get_or_init(cleanup_workflow::CleanupWorkflow::default)
                    .update_policy(clarity_core::UpdateQuarantinePolicyRequest {
                        retention_days: previous.quarantine_retention_days,
                        max_bytes: previous.quarantine_max_bytes,
                    });
            }
            if startup_changed {
                let _ = startup_behavior::apply_launch_at_login(previous.launch_at_login);
            }
            return Err(error);
        }
    };
    diagnostics::set_crash_retention(updated.retain_crash_diagnostics);
    if !updated.retain_crash_diagnostics {
        diagnostics::clear_crash_reports()?;
    }
    Ok(updated)
}

/// Evaluates protections and runs only a due read-only cleanup scan.
#[tauri::command]
async fn run_automatic_maintenance()
-> Result<automatic_maintenance::AutomaticMaintenanceRunReport, String> {
    run_blocking("自动维护评估", run_automatic_maintenance_blocking).await
}

fn run_automatic_maintenance_blocking()
-> Result<automatic_maintenance::AutomaticMaintenanceRunReport, String> {
    let settings = SETTINGS
        .get_or_init(settings_store::SettingsStore::default)
        .get();
    AUTOMATIC_MAINTENANCE
        .get_or_init(automatic_maintenance::AutomaticMaintenanceCoordinator::default)
        .run_if_due(&settings, || {
            let started = Instant::now();
            let preview = CLEANUP_WORKFLOW
                .get_or_init(cleanup_workflow::CleanupWorkflow::default)
                .scan()?;
            diagnostics::record_performance("cleanupPreview", started.elapsed());
            Ok(preview)
        })
}

/// Returns local privacy-safe crash markers and performance timings.
#[tauri::command]
fn get_diagnostics_snapshot() -> diagnostics::DiagnosticsSnapshot {
    diagnostics::snapshot()
}

/// Clears privacy-safe crash markers without touching audit or recovery records.
#[tauri::command]
fn clear_crash_diagnostics() -> Result<(), String> {
    diagnostics::clear_crash_reports()
}

/// Clears scan and audit history while preserving quarantine and recovery state.
#[tauri::command]
fn clear_activity_history() -> Result<(), String> {
    CLEANUP_WORKFLOW
        .get_or_init(cleanup_workflow::CleanupWorkflow::default)
        .clear_audit_events()?;
    SPACE_SCANS
        .get_or_init(space_scan::SpaceScanManager::default)
        .clear_history()?;
    PRIVILEGED_WORKFLOW
        .get_or_init(privileged_workflow::PrivilegedWorkflow::default)
        .clear_audit_events()
}

/// Starts the desktop runtime and registers the minimal command surface.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or the desktop event loop terminates
/// with a fatal runtime error.
pub fn run() {
    let settings = SETTINGS
        .get_or_init(settings_store::SettingsStore::default)
        .get();
    diagnostics::set_crash_retention(settings.retain_crash_diagnostics);
    diagnostics::install_panic_hook();
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
            restore_quarantine_batch,
            restore_quarantine_entry_to,
            prepare_quarantine_deletion,
            execute_quarantine_deletion,
            update_quarantine_policy,
            start_space_scan,
            get_space_scan,
            cancel_space_scan,
            pause_space_scan,
            resume_space_scan,
            get_space_scan_history,
            get_default_space_scan_request,
            get_disk_health_snapshot,
            get_partition_topology,
            preview_partition_merge,
            assess_partition_merge_safety,
            get_privileged_capabilities,
            prepare_maintenance_execution,
            prepare_partition_execution,
            execute_privileged_operation,
            get_privileged_audit_events,
            get_app_settings,
            update_app_settings,
            run_automatic_maintenance,
            get_diagnostics_snapshot,
            clear_crash_diagnostics,
            clear_activity_history
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Clarity Disk");
}
