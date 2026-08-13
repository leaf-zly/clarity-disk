//! Cross-platform read-only space scanning models.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Lifecycle state of a space scan task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpaceScanStatus {
    /// The task has not started.
    Idle,
    /// The scanner is traversing the requested root.
    Scanning,
    /// The task stopped at a safe cancellation boundary.
    Cancelled,
    /// The scanner completed the requested traversal.
    Completed,
    /// The scanner failed before producing a trustworthy result.
    Failed,
}

/// User-selected scope and resource limits for a read-only scan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceScanRequest {
    /// Existing directory to inspect.
    pub root_path: String,
    /// Maximum directory depth; adapters clamp this to a safe upper bound.
    pub max_depth: u8,
    /// Maximum number of filesystem entries to inspect.
    pub max_entries: u64,
}

/// A stable task identifier returned when a scan starts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceScanStart {
    /// Identifier used to poll, cancel, and audit the task.
    pub scan_id: String,
}

/// Progress information shared while the scanner is running.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceScanProgress {
    /// Task identifier.
    pub scan_id: String,
    /// Current lifecycle state.
    pub status: SpaceScanStatus,
    /// Number of entries inspected so far.
    pub scanned_items: u64,
    /// Number of entries skipped for safety, limits, or access errors.
    pub skipped_items: u64,
    /// Sum of inspected file sizes.
    pub bytes_scanned: u64,
    /// Most recent path being inspected, if available.
    pub current_path: Option<String>,
    /// User-facing phase or failure message.
    pub message: String,
}

/// A large file or directory reported by the scanner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceScanEntry {
    /// Absolute path observed during the read-only scan.
    pub path: String,
    /// Aggregated file size in bytes.
    pub bytes: u64,
    /// Number of files represented by this entry.
    pub item_count: u64,
    /// Whether the entry represents a file or directory.
    pub kind: String,
}

/// File extension or category aggregate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceScanTypeStat {
    /// Lowercase extension or `[无扩展名]`.
    pub file_type: String,
    /// Total bytes represented by this category.
    pub bytes: u64,
    /// Number of files represented by this category.
    pub item_count: u64,
}

/// Pollable result for a space scan task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceScanSnapshot {
    /// Current progress and status.
    pub progress: SpaceScanProgress,
    /// Largest observed files and directories, limited by the adapter.
    pub largest_entries: Vec<SpaceScanEntry>,
    /// File type aggregates sorted by descending size.
    pub file_types: Vec<SpaceScanTypeStat>,
}

/// Validation and runtime errors for space scanning.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SpaceScanError {
    /// The root path was not an existing directory.
    #[error("space scan root is not an accessible directory: {0}")]
    InvalidRoot(String),
    /// A request exceeded supported resource limits.
    #[error("space scan request exceeds supported limits")]
    InvalidLimits,
    /// The task identifier did not exist.
    #[error("space scan task was not found: {0}")]
    TaskNotFound(String),
    /// The platform adapter failed while reading metadata.
    #[error("space scan failed at {path}: {message}")]
    Read { path: String, message: String },
}
