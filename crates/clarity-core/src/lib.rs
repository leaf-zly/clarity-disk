//! Platform-independent domain models and safety rules for Clarity Disk.

mod dashboard;

pub use dashboard::{
    CleanupSummary, DashboardError, DashboardSnapshot, DiskCategory, DiskCategoryKind, DiskHealth,
    DiskSummary, Suggestion, SuggestionRisk,
};
