//! Platform-independent domain models and safety rules for Clarity Disk.

mod cleanup;
mod dashboard;

pub use cleanup::{CleanupCandidate, CleanupError, CleanupPreview, ScanProgress, ScanStatus};
pub use dashboard::{
    CleanupSummary, DashboardError, DashboardSnapshot, DiskCategory, DiskCategoryKind, DiskHealth,
    DiskMetadata, DiskSummary, Suggestion, SuggestionRisk, VolumeHealthStatus,
};
