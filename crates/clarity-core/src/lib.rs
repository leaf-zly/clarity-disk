//! Platform-independent domain models and safety rules for Clarity Disk.

mod backup;
mod cleanup;
mod dashboard;
mod health;
mod partition;
mod partition_safety;
mod settings;
mod space_scan;

pub use backup::{BackupVerificationError, BackupVerificationReceipt};
pub use cleanup::{
    AuditEvent, AuditEventKind, CLEANUP_CONFIRMATION_PHRASE, CleanupCandidate, CleanupError,
    CleanupExecutionChallenge, CleanupExecutionItemResult, CleanupExecutionItemStatus,
    CleanupExecutionMode, CleanupExecutionReport, CleanupPlan, CleanupPlanError, CleanupPreview,
    CleanupRuleAvailability, CleanupRuleStatus, ExecuteCleanupRequest,
    ExecuteQuarantineDeletionRequest, PrepareCleanupExecutionRequest, PrepareCleanupPlanRequest,
    PrepareQuarantineDeletionRequest, QUARANTINE_DELETE_CONFIRMATION_PHRASE,
    QuarantineDeletionChallenge, QuarantineDeletionReport, QuarantineDeletionResult,
    QuarantineEntry, QuarantineEntryStatus, QuarantineError, QuarantineIndex, QuarantinePolicy,
    QuarantinePolicyError, QuarantineRestoreBatchReport, QuarantineRestoreDestination,
    QuarantineRestoreResult, QuarantineTransferKind, RECYCLE_BIN_CONFIRMATION_PHRASE,
    RecoveryStrategy, RestoreQuarantineBatchRequest, RestoreQuarantineRequest,
    RestoreQuarantineToRequest, ScanProgress, ScanStatus, UpdateQuarantinePolicyRequest,
};
pub use dashboard::{
    CleanupSummary, DashboardError, DashboardSnapshot, DiskCategory, DiskCategoryKind, DiskHealth,
    DiskMetadata, DiskSummary, Suggestion, SuggestionRisk, VolumeHealthStatus,
};
pub use health::{
    DiskEncryptionSummary, DiskHealthSignal, DiskHealthSignalCode, DiskHealthSnapshot,
    DiskHealthStatus, DiskHealthSummary, HealthDataCompleteness, HealthModelError,
    HealthSignalSeverity, IdentityMappingConfidence, PhysicalDiskHealth, PhysicalDiskHealthInput,
    ProviderHealthStatus, SmartHealthStatus,
};
pub use partition::{
    DiskLayoutKind, EncryptionState, MediaErrorState, MergeBlocker, MergeBlockerCode, MergeCheck,
    MergeCheckCode, MergePreview, MergePreviewRequest, MergeRiskLevel, PartitionDescriptor,
    PartitionExecutionIdentity, PartitionKind, PartitionOperationalState, PartitionPreviewError,
    PartitionTopology, PhysicalDisk, SimulatedPartition, SimulatedPartitionLayout, SnapshotState,
    TopologyHealth,
};
pub use partition_safety::{
    BackupEvidenceState, ExternalPowerState, ImmutablePartitionPlan, PartitionOperationKind,
    PartitionRecoveryEvent, PartitionRecoveryJournal, PartitionRecoveryState,
    PartitionSafetyAssessment, PartitionSafetyBlocker, PartitionSafetyBlockerCode,
    PartitionSafetyCheck, PartitionSafetyCheckCode, PartitionSafetyError, PartitionSafetyEvidence,
    PartitionSafetyStatus, PendingRestartState,
};
pub use settings::{
    AppSettings, AutomaticMaintenanceContext, AutomaticMaintenanceDecision,
    AutomaticMaintenanceReason, AutomaticMaintenanceSchedule, LanguagePreference, LogLevel,
    SettingsError, ThemePreference,
};
pub use space_scan::{
    SpaceScanEntry, SpaceScanError, SpaceScanHistoryEntry, SpaceScanProgress, SpaceScanRequest,
    SpaceScanSnapshot, SpaceScanStart, SpaceScanStatus, SpaceScanTypeStat,
};
