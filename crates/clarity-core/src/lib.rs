//! Platform-independent domain models and safety rules for Clarity Disk.

mod cleanup;
mod dashboard;

pub use cleanup::{
    AuditEvent, AuditEventKind, CleanupCandidate, CleanupError, CleanupPlan, CleanupPlanError,
    CleanupPreview, ScanProgress, ScanStatus,
};
pub use dashboard::{
    CleanupSummary, DashboardError, DashboardSnapshot, DiskCategory, DiskCategoryKind, DiskHealth,
    DiskMetadata, DiskSummary, Suggestion, SuggestionRisk, VolumeHealthStatus,
};
