//! Tauri command adapter for the Clarity Disk desktop application.

use clarity_core::{
    CleanupSummary, DashboardSnapshot, DiskCategory, DiskCategoryKind, DiskHealth, DiskSummary,
    Suggestion, SuggestionRisk,
};

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

/// Returns the current dashboard snapshot.
///
/// The first product slice uses deterministic sample data so the UI and command
/// contract can be reviewed before privileged platform discovery is introduced.
#[tauri::command]
fn get_dashboard_snapshot() -> DashboardSnapshot {
    let disk = DiskSummary::try_new(
        "C:",
        "Windows · 本地磁盘 (C:)",
        200 * GIB + 61 * MIB,
        159 * GIB + 645 * MIB,
        vec![
            DiskCategory {
                kind: DiskCategoryKind::Applications,
                label: "应用".to_owned(),
                bytes: 56 * GIB + 205 * MIB,
            },
            DiskCategory {
                kind: DiskCategoryKind::System,
                label: "系统".to_owned(),
                bytes: 44 * GIB + 819 * MIB,
            },
            DiskCategory {
                kind: DiskCategoryKind::Files,
                label: "文件".to_owned(),
                bytes: 33 * GIB + 410 * MIB,
            },
            DiskCategory {
                kind: DiskCategoryKind::Development,
                label: "开发".to_owned(),
                bytes: 25 * GIB + 205 * MIB,
            },
        ],
    )
    .expect("embedded dashboard fixture must remain valid");

    DashboardSnapshot {
        disk,
        health: DiskHealth {
            status: "良好".to_owned(),
            device_type: "NVMe".to_owned(),
            temperature_celsius: Some(42),
            has_warning: false,
        },
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
    }
}

/// Starts the desktop runtime and registers the minimal command surface.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_dashboard_snapshot])
        .run(tauri::generate_context!())
        .expect("failed to run Clarity Disk");
}
