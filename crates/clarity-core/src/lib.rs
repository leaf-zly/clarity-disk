//! Platform-independent domain models and safety rules for Clarity Disk.

mod cleanup;
mod dashboard;
mod space_scan;

pub use cleanup::{
    AuditEvent, AuditEventKind, CLEANUP_CONFIRMATION_PHRASE, CleanupCandidate, CleanupError,
    CleanupExecutionChallenge, CleanupExecutionItemResult, CleanupExecutionItemStatus,
    CleanupExecutionReport, CleanupPlan, CleanupPlanError, CleanupPreview, CleanupRuleAvailability,
    CleanupRuleStatus, ExecuteCleanupRequest, PrepareCleanupExecutionRequest,
    PrepareCleanupPlanRequest, QuarantineEntry, QuarantineEntryStatus, QuarantineError,
    QuarantineIndex, QuarantineRestoreResult, RecoveryStrategy, RestoreQuarantineRequest,
    ScanProgress, ScanStatus,
};
pub use dashboard::{
    CleanupSummary, DashboardError, DashboardSnapshot, DiskCategory, DiskCategoryKind, DiskHealth,
    DiskMetadata, DiskSummary, Suggestion, SuggestionRisk, VolumeHealthStatus,
};
pub use space_scan::{
    SpaceScanEntry, SpaceScanError, SpaceScanHistoryEntry, SpaceScanProgress, SpaceScanRequest,
    SpaceScanSnapshot, SpaceScanStart, SpaceScanStatus, SpaceScanTypeStat,
};
