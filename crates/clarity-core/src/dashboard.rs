use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A point-in-time view of the information presented on the dashboard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    /// Primary system disk capacity and usage.
    pub disk: DiskSummary,
    /// All logical volumes discovered during the same read-only snapshot.
    pub disks: Vec<DiskSummary>,
    /// Current health status reported by the platform health provider.
    pub health: DiskHealth,
    /// Cleanup opportunity that is safe enough to present on the dashboard.
    pub cleanup: CleanupSummary,
    /// Prioritized, user-visible maintenance suggestions.
    pub suggestions: Vec<Suggestion>,
}

/// Capacity and usage information for a logical disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSummary {
    /// Stable volume identifier used only for display in this early implementation.
    pub id: String,
    /// User-facing volume label.
    pub label: String,
    /// Total volume capacity in bytes.
    pub total_bytes: u64,
    /// Used volume capacity in bytes.
    pub used_bytes: u64,
    /// Usage categories shown in the capacity bar.
    pub categories: Vec<DiskCategory>,
    /// Read-only platform metadata for the logical volume.
    pub metadata: DiskMetadata,
}

impl DiskSummary {
    /// Creates a validated disk summary.
    ///
    /// # Errors
    ///
    /// Returns an error when used bytes exceed volume capacity or when category
    /// totals exceed the reported used capacity.
    pub fn try_new(
        id: impl Into<String>,
        label: impl Into<String>,
        total_bytes: u64,
        used_bytes: u64,
        categories: Vec<DiskCategory>,
    ) -> Result<Self, DashboardError> {
        let id = id.into();
        if used_bytes > total_bytes {
            return Err(DashboardError::UsedCapacityExceedsTotal {
                used_bytes,
                total_bytes,
            });
        }

        let categorized_bytes = categories
            .iter()
            .try_fold(0_u64, |total, category| total.checked_add(category.bytes));

        if categorized_bytes.is_none_or(|bytes| bytes > used_bytes) {
            return Err(DashboardError::CategoryCapacityExceedsUsed {
                categorized_bytes: categorized_bytes.unwrap_or(u64::MAX),
                used_bytes,
            });
        }

        Ok(Self {
            id: id.clone(),
            label: label.into(),
            total_bytes,
            used_bytes,
            categories,
            metadata: DiskMetadata::preview(id),
        })
    }

    /// Attaches read-only platform metadata discovered for this volume.
    #[must_use]
    pub fn with_metadata(mut self, metadata: DiskMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Returns the currently available capacity in bytes.
    #[must_use]
    pub const fn available_bytes(&self) -> u64 {
        self.total_bytes - self.used_bytes
    }
}

/// A category contributing to used disk capacity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskCategory {
    /// Stable category identifier used by the UI color system.
    pub kind: DiskCategoryKind,
    /// User-facing category label.
    pub label: String,
    /// Capacity attributed to this category.
    pub bytes: u64,
}

/// Read-only metadata describing a logical volume and its basic health signal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskMetadata {
    /// Normalized mount point used for display and later identity checks.
    pub mount_point: String,
    /// File-system name reported by the operating system.
    pub file_system: String,
    /// Device kind reported by the platform adapter, such as SSD or HDD.
    pub device_type: String,
    /// Whether this volume contains the running Windows installation.
    pub is_system_volume: bool,
    /// Whether the device is removable.
    pub is_removable: bool,
    /// Whether the volume is currently read-only.
    pub is_read_only: bool,
    /// Basic health status based only on read-only discovery signals.
    pub health_status: VolumeHealthStatus,
    /// Additional explanation when the status is not a normal writable volume.
    pub health_note: Option<String>,
}

impl DiskMetadata {
    fn preview(id: String) -> Self {
        Self {
            mount_point: id,
            file_system: String::new(),
            device_type: "Unknown".to_owned(),
            is_system_volume: false,
            is_removable: false,
            is_read_only: false,
            health_status: VolumeHealthStatus::Healthy,
            health_note: None,
        }
    }
}

/// Basic health state available without SMART or vendor-specific queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeHealthStatus {
    /// The volume is online and writable according to the read-only query.
    Healthy,
    /// The volume is online but currently reports read-only access.
    ReadOnly,
    /// The platform returned a condition that requires user attention.
    Warning,
}

/// Supported dashboard disk usage categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiskCategoryKind {
    /// Installed desktop and Store applications.
    Applications,
    /// Windows and system-managed content.
    System,
    /// User-created documents and media.
    Files,
    /// Developer dependencies, build output, and caches.
    Development,
}

/// Current disk health status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealth {
    /// Short, localized status suitable for the dashboard.
    pub status: String,
    /// Device bus or storage technology.
    pub device_type: String,
    /// Temperature in degrees Celsius when available.
    pub temperature_celsius: Option<u16>,
    /// Whether the platform reported an actionable health warning.
    pub has_warning: bool,
}

/// Aggregate cleanup opportunity shown before a detailed scan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSummary {
    /// Estimated bytes that can be reclaimed using low-risk rules.
    pub reclaimable_bytes: u64,
    /// Number of cleanup categories included in the estimate.
    pub category_count: u32,
}

/// A user-visible recommendation produced by the rules engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// Stable identifier used for navigation and analytics.
    pub id: String,
    /// Concise recommendation title.
    pub title: String,
    /// Explanation of impact and expected reclaimed capacity.
    pub description: String,
    /// Risk classification that controls default selection behavior.
    pub risk: SuggestionRisk,
    /// Estimated capacity affected by the recommendation.
    pub reclaimable_bytes: u64,
}

/// Risk classification for cleanup and maintenance recommendations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuggestionRisk {
    /// Regenerable content with no expected loss of user state.
    Safe,
    /// Content that requires a user review before removal.
    Review,
    /// A system behavior or feature changes after applying the suggestion.
    ConfirmationRequired,
}

/// Validation errors for dashboard domain models.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DashboardError {
    /// Used capacity was greater than total disk capacity.
    #[error("used capacity {used_bytes} exceeds total capacity {total_bytes}")]
    UsedCapacityExceedsTotal {
        /// Reported used capacity.
        used_bytes: u64,
        /// Reported total capacity.
        total_bytes: u64,
    },
    /// Categorized capacity was greater than total used capacity.
    #[error("categorized capacity {categorized_bytes} exceeds used capacity {used_bytes}")]
    CategoryCapacityExceedsUsed {
        /// Sum of category capacities.
        categorized_bytes: u64,
        /// Reported used capacity.
        used_bytes: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        DashboardError, DiskCategory, DiskCategoryKind, DiskMetadata, DiskSummary,
        VolumeHealthStatus,
    };

    #[test]
    fn computes_available_capacity_for_valid_summary() {
        let summary = DiskSummary::try_new(
            "C:",
            "Windows",
            200,
            160,
            vec![DiskCategory {
                kind: DiskCategoryKind::System,
                label: "System".to_owned(),
                bytes: 80,
            }],
        )
        .expect("valid capacity should construct");

        assert_eq!(summary.available_bytes(), 40);
    }

    #[test]
    fn rejects_used_capacity_greater_than_total() {
        let error = DiskSummary::try_new("C:", "Windows", 100, 101, vec![])
            .expect_err("invalid capacity must be rejected");

        assert_eq!(
            error,
            DashboardError::UsedCapacityExceedsTotal {
                used_bytes: 101,
                total_bytes: 100,
            }
        );
    }

    #[test]
    fn rejects_category_total_greater_than_used_capacity() {
        let error = DiskSummary::try_new(
            "C:",
            "Windows",
            200,
            100,
            vec![DiskCategory {
                kind: DiskCategoryKind::Applications,
                label: "Applications".to_owned(),
                bytes: 101,
            }],
        )
        .expect_err("category overflow must be rejected");

        assert_eq!(
            error,
            DashboardError::CategoryCapacityExceedsUsed {
                categorized_bytes: 101,
                used_bytes: 100,
            }
        );
    }

    #[test]
    fn attaches_read_only_metadata_without_changing_capacity() {
        let summary = DiskSummary::try_new("D:", "Data", 100, 20, vec![])
            .expect("valid capacity should construct")
            .with_metadata(DiskMetadata {
                mount_point: "D:\\".to_owned(),
                file_system: "NTFS".to_owned(),
                device_type: "SSD".to_owned(),
                is_system_volume: false,
                is_removable: false,
                is_read_only: true,
                health_status: VolumeHealthStatus::ReadOnly,
                health_note: Some("卷当前为只读".to_owned()),
            });

        assert_eq!(summary.available_bytes(), 80);
        assert_eq!(summary.metadata.health_status, VolumeHealthStatus::ReadOnly);
    }
}
