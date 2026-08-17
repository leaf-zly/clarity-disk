//! Privacy-safe local crash diagnostics and lightweight performance baselines.

use std::cmp::Reverse;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::state_store::{load_json, state_path, write_json};

const MAX_CRASH_REPORTS: usize = 20;
const MAX_METRICS: usize = 100;
static RETAIN_CRASH_DIAGNOSTICS: AtomicBool = AtomicBool::new(true);
static PERFORMANCE_METRICS: OnceLock<Mutex<Vec<PerformanceMetric>>> = OnceLock::new();

/// One privacy-safe crash marker without panic payload, paths, or file data.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CrashReport {
    /// Stable report identity.
    report_id: String,
    /// Application version that terminated unexpectedly.
    app_version: String,
    /// Unix timestamp in milliseconds.
    occurred_at_unix_ms: u64,
    /// Internal Rust source file name without its directory.
    source_file: Option<String>,
    /// Internal Rust source line, when available.
    source_line: Option<u32>,
    /// Fixed classification; panic payloads are deliberately excluded.
    classification: String,
}

/// One measured command latency compared with a reviewed baseline.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PerformanceMetric {
    /// Fixed operation name selected by Rust.
    operation: String,
    /// Measured wall time.
    duration_ms: u64,
    /// Reviewed responsiveness threshold.
    baseline_ms: u64,
    /// Whether the measured duration stayed within its threshold.
    within_baseline: bool,
    /// Measurement timestamp.
    measured_at_unix_ms: u64,
}

/// Combined local diagnostics snapshot displayed in settings.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticsSnapshot {
    /// Newest privacy-safe crash markers.
    crash_reports: Vec<CrashReport>,
    /// Newest in-process latency measurements.
    performance_metrics: Vec<PerformanceMetric>,
    /// Whether future crash markers are retained.
    retention_enabled: bool,
}

/// Installs a panic hook that never persists panic messages or user paths.
pub(crate) fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        if !RETAIN_CRASH_DIAGNOSTICS.load(Ordering::Acquire) {
            return;
        }
        let now = unix_ms();
        let location = info.location();
        let report = CrashReport {
            report_id: format!("crash-{}-{now}", std::process::id()),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            occurred_at_unix_ms: now,
            source_file: location.and_then(|value| {
                PathBuf::from(value.file())
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            }),
            source_line: location.map(std::panic::Location::line),
            classification: "unexpectedPanic".to_owned(),
        };
        let path = state_path("crash-diagnostics.v1.json");
        let mut reports: Vec<CrashReport> = load_json(&path);
        reports.push(report);
        reports.sort_by_key(|value| Reverse(value.occurred_at_unix_ms));
        reports.truncate(MAX_CRASH_REPORTS);
        let _ = write_json(&path, &reports);
    }));
}

/// Enables or disables future local crash marker retention.
pub(crate) fn set_crash_retention(enabled: bool) {
    RETAIN_CRASH_DIAGNOSTICS.store(enabled, Ordering::Release);
}

/// Records one fixed operation latency in memory.
pub(crate) fn record_performance(operation: &'static str, elapsed: Duration) {
    let baseline_ms = match operation {
        "dashboardDiscovery" => 1_500,
        "cleanupPreview" => 30_000,
        "partitionDiscovery" => 5_000,
        _ => return,
    };
    let duration_ms = elapsed.as_millis().try_into().unwrap_or(u64::MAX);
    let mut metrics = PERFORMANCE_METRICS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("performance metrics state poisoned");
    metrics.push(PerformanceMetric {
        operation: operation.to_owned(),
        duration_ms,
        baseline_ms,
        within_baseline: duration_ms <= baseline_ms,
        measured_at_unix_ms: unix_ms(),
    });
    metrics.sort_by_key(|value| Reverse(value.measured_at_unix_ms));
    metrics.truncate(MAX_METRICS);
}

/// Returns newest local diagnostics without file contents or user paths.
pub(crate) fn snapshot() -> DiagnosticsSnapshot {
    let mut reports: Vec<CrashReport> = load_json(&state_path("crash-diagnostics.v1.json"));
    reports.sort_by_key(|value| Reverse(value.occurred_at_unix_ms));
    reports.truncate(MAX_CRASH_REPORTS);
    let performance_metrics = PERFORMANCE_METRICS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("performance metrics state poisoned")
        .clone();
    DiagnosticsSnapshot {
        crash_reports: reports,
        performance_metrics,
        retention_enabled: RETAIN_CRASH_DIAGNOSTICS.load(Ordering::Acquire),
    }
}

/// Clears crash markers without changing audit or recovery records.
pub(crate) fn clear_crash_reports() -> Result<(), String> {
    write_json(
        &state_path("crash-diagnostics.v1.json"),
        &Vec::<CrashReport>::new(),
    )
    .map_err(|error| error.to_string())
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{record_performance, snapshot};

    #[test]
    fn performance_metrics_use_fixed_names_and_thresholds() {
        record_performance("dashboardDiscovery", Duration::from_millis(10));
        record_performance("callerSuppliedName", Duration::from_millis(10));
        let snapshot = snapshot();
        assert!(
            snapshot.performance_metrics.iter().any(|metric| {
                metric.operation == "dashboardDiscovery" && metric.within_baseline
            })
        );
        assert!(
            !snapshot
                .performance_metrics
                .iter()
                .any(|metric| metric.operation == "callerSuppliedName")
        );
    }
}
