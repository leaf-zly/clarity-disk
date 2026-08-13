//! Cross-platform cleanup discovery, plans, audit events, and quarantine previews.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::dashboard::SuggestionRisk;

/// Lifecycle state for a read-only cleanup scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanStatus {
    /// No scan has been started.
    Idle,
    /// The platform adapter is preparing its allow-listed roots.
    Discovering,
    /// Candidate roots are being measured.
    Scanning,
    /// The scan completed and the result can be previewed.
    Completed,
    /// The scan was cancelled at a safe boundary.
    Cancelled,
    /// The scan failed before producing a trustworthy result.
    Failed,
}

/// Progress and provenance for a read-only scan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// Unique identifier for this scan result.
    pub scan_id: String,
    /// Current lifecycle state.
    pub status: ScanStatus,
    /// Number of filesystem entries inspected.
    pub scanned_items: u64,
    /// Number of entries skipped because they were inaccessible or unsafe.
    pub skipped_items: u64,
    /// Human-readable phase description.
    pub message: String,
    /// Stable source-volume hint captured during discovery.
    pub source_volume_id: Option<String>,
}

/// Recovery approach associated with a cleanup rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryStrategy {
    /// The owning application or Windows can regenerate the content.
    Regenerate,
    /// A future executor should stage the content in Clarity Disk quarantine.
    Quarantine,
    /// Windows manages the content and its supported maintenance workflow.
    WindowsManaged,
    /// The content has no guaranteed automated recovery path.
    None,
}

/// Availability of a versioned cleanup rule in the latest scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupRuleAvailability {
    /// At least one non-empty allow-listed root was measured.
    Available,
    /// Allow-listed roots were readable but contained no reclaimable bytes.
    Empty,
    /// The rule could not be evaluated because its environment or access was unavailable.
    Unavailable,
}

/// User-visible status for every evaluated cleanup rule, including empty rules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRuleStatus {
    /// Versioned rule identifier.
    pub rule_id: String,
    /// Concise display title.
    pub title: String,
    /// Result of evaluating the allow-listed roots.
    pub availability: CleanupRuleAvailability,
    /// Safe explanation for empty, skipped, or inaccessible output.
    pub reason: Option<String>,
    /// Whether a future execution workflow would need elevation.
    pub requires_admin: bool,
    /// Risk level assigned by the rule definition.
    pub risk: SuggestionRisk,
}

/// A user-visible cleanup candidate found by a versioned rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCandidate {
    /// Stable rule-scoped identifier for UI selection.
    pub id: String,
    /// Versioned rule identifier that produced this candidate.
    pub rule_id: String,
    /// Explicit rule version used to invalidate stale plans after rule changes.
    pub rule_version: String,
    /// Concise display title.
    pub title: String,
    /// Explanation of why the content is considered reclaimable.
    pub description: String,
    /// Allow-listed path discovered by the platform adapter.
    pub path: String,
    /// Human-readable evidence explaining why the candidate was produced.
    pub evidence: Vec<String>,
    /// Bytes that could be reclaimed if this candidate is later approved.
    pub bytes: u64,
    /// Number of files and directories represented by the candidate.
    pub item_count: u64,
    /// Risk classification controlling default selection and confirmation.
    pub risk: SuggestionRisk,
    /// Whether the content can normally be regenerated or staged for recovery.
    pub recoverable: bool,
    /// Whether a future execution workflow would require elevation.
    pub requires_admin: bool,
    /// Recovery behavior a future executor must follow.
    pub recovery_strategy: RecoveryStrategy,
    /// Whether this snapshot may be included in a quarantine index preview.
    pub quarantine_eligible: bool,
    /// Whether the preview selects this candidate by default.
    pub default_selected: bool,
    /// SHA-256 digest of the discovered file metadata, not file contents.
    pub metadata_digest: String,
    /// Wall-clock time when this candidate was observed, in Unix milliseconds.
    pub observed_at_unix_ms: Option<u64>,
}

/// Complete read-only cleanup preview returned to the UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPreview {
    /// Scan state and completeness information.
    pub scan: ScanProgress,
    /// Candidates grouped by cleanup rule.
    pub candidates: Vec<CleanupCandidate>,
    /// Status for every enabled rule, even when it produced no candidate.
    pub rule_statuses: Vec<CleanupRuleStatus>,
    /// Sum of all candidate sizes, independent of UI selection.
    pub total_reclaimable_bytes: u64,
}

/// Selection sent by the UI to create a review-only cleanup plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareCleanupPlanRequest {
    /// Completed scan whose candidate identities are being selected.
    pub scan_id: String,
    /// Candidate IDs selected from that exact scan.
    pub candidate_ids: Vec<String>,
}

/// Immutable, execution-free cleanup plan generated from one completed scan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    /// Stable identifier for this plan instance.
    pub plan_id: String,
    /// Scan identifier from which the plan was derived.
    pub scan_id: String,
    /// Selected candidates copied from the completed preview.
    pub candidates: Vec<CleanupCandidate>,
    /// SHA-256 digest of the canonical plan payload.
    pub plan_digest: String,
    /// Explicitly false until a separately reviewed executor is implemented.
    pub execution_authorized: bool,
    /// Creation time in Unix milliseconds.
    pub created_at_unix_ms: u64,
    /// Hard expiry after which revalidation must fail.
    pub expires_at_unix_ms: u64,
    /// Volume hint from the originating scan.
    pub source_volume_id: Option<String>,
}

/// Lifecycle events retained in the local privacy-preserving audit log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditEventKind {
    /// A read-only scan completed.
    ScanCompleted,
    /// A non-authorizing plan was generated.
    PlanCreated,
    /// A plan request or revalidation was rejected.
    PlanRejected,
    /// A quarantine index preview was created without moving files.
    QuarantineIndexCreated,
}

/// Minimal, privacy-preserving audit record for cleanup safety decisions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    /// Event identifier generated by the application workflow.
    pub event_id: String,
    /// Event type.
    pub kind: AuditEventKind,
    /// Related scan, plan, or index identifier.
    pub subject_id: String,
    /// Event time in Unix milliseconds.
    pub occurred_at_unix_ms: u64,
    /// Optional safe-to-display reason; never contains file contents or paths.
    pub reason: Option<String>,
    /// Number of candidates involved in the event.
    pub candidate_count: u64,
    /// Aggregate candidate bytes involved in the event.
    pub total_bytes: u64,
    /// Versioned rules involved; paths are deliberately excluded.
    pub rule_ids: Vec<String>,
}

/// State of an entry in the read-only quarantine index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuarantineEntryStatus {
    /// Metadata is indexed for review; no filesystem move occurred.
    PreviewOnly,
}

/// Candidate metadata retained for a possible future quarantine operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineEntry {
    /// Candidate identity copied from a validated cleanup plan.
    pub candidate_id: String,
    /// Versioned cleanup rule responsible for the candidate.
    pub rule_id: String,
    /// Original allow-listed path, used only for local preview and future revalidation.
    pub original_path: String,
    /// Expected bytes from the immutable plan snapshot.
    pub bytes: u64,
    /// Metadata digest that a future executor must revalidate.
    pub metadata_digest: String,
    /// Current preview-only state.
    pub status: QuarantineEntryStatus,
}

/// Persisted read-only quarantine index derived from a validated plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineIndex {
    /// Unique index identifier.
    pub index_id: String,
    /// Plan from which all entries were copied.
    pub plan_id: String,
    /// Creation time in Unix milliseconds.
    pub created_at_unix_ms: u64,
    /// Eligible entries included in the preview.
    pub entries: Vec<QuarantineEntry>,
    /// Always false in plan two; no file move API exists.
    pub files_moved: bool,
    /// Aggregate bytes represented by indexed entries.
    pub total_bytes: u64,
}

const PLAN_TTL_MS: u64 = 10 * 60 * 1000;

impl CleanupPlan {
    /// Builds a deterministic, non-authorizing plan from default-selected items.
    ///
    /// # Errors
    ///
    /// Returns an error if the preview is incomplete, has no defaults, or its
    /// candidate data cannot be serialized for hashing.
    pub fn from_preview(preview: &CleanupPreview) -> Result<Self, CleanupPlanError> {
        let request = PrepareCleanupPlanRequest {
            scan_id: preview.scan.scan_id.clone(),
            candidate_ids: preview
                .candidates
                .iter()
                .filter(|candidate| candidate.default_selected)
                .map(|candidate| candidate.id.clone())
                .collect(),
        };
        Self::from_selection(preview, &request)
    }

    /// Builds a non-authorizing plan from candidate IDs in one completed preview.
    ///
    /// Candidate snapshots are always copied from `preview`; callers cannot
    /// inject a path, digest, risk, or rule version through this request.
    ///
    /// # Errors
    ///
    /// Returns an error for incomplete or mismatched scans, empty or duplicate
    /// selections, unknown IDs, or serialization failures.
    pub fn from_selection(
        preview: &CleanupPreview,
        request: &PrepareCleanupPlanRequest,
    ) -> Result<Self, CleanupPlanError> {
        if preview.scan.status != ScanStatus::Completed {
            return Err(CleanupPlanError::PreviewNotCompleted {
                status: preview.scan.status,
            });
        }
        if preview.scan.scan_id != request.scan_id {
            return Err(CleanupPlanError::ScanIdChanged);
        }
        if request.candidate_ids.is_empty() {
            return Err(CleanupPlanError::NoSelectedCandidates);
        }
        let selected: HashSet<_> = request.candidate_ids.iter().collect();
        if selected.len() != request.candidate_ids.len() {
            return Err(CleanupPlanError::DuplicateCandidate);
        }
        let candidates: Vec<_> = request
            .candidate_ids
            .iter()
            .map(|id| {
                preview
                    .candidates
                    .iter()
                    .find(|candidate| candidate.id == *id)
                    .cloned()
                    .ok_or_else(|| CleanupPlanError::UnknownCandidate(id.clone()))
            })
            .collect::<Result<_, _>>()?;
        let plan_digest = hash_plan(&preview.scan.scan_id, &candidates)?;
        let created_at_unix_ms = current_unix_ms();
        Ok(Self {
            plan_id: format!("plan-{}-{created_at_unix_ms}", preview.scan.scan_id),
            scan_id: preview.scan.scan_id.clone(),
            candidates,
            plan_digest,
            execution_authorized: false,
            created_at_unix_ms,
            expires_at_unix_ms: created_at_unix_ms.saturating_add(PLAN_TTL_MS),
            source_volume_id: preview.scan.source_volume_id.clone(),
        })
    }

    /// Revalidates this plan against a fresh read-only preview.
    ///
    /// # Errors
    ///
    /// Returns an error when the preview is incomplete, expired, belongs to a
    /// different scan or volume, or selected candidate metadata has changed.
    pub fn validate_against(&self, preview: &CleanupPreview) -> Result<(), CleanupPlanError> {
        self.validate_against_at(preview, current_unix_ms())
    }

    /// Revalidates using a supplied clock, making expiry behavior testable.
    ///
    /// # Errors
    ///
    /// Returns an error under the same stale or changed conditions as
    /// [`Self::validate_against`], evaluated at `now_unix_ms`.
    pub fn validate_against_at(
        &self,
        preview: &CleanupPreview,
        now_unix_ms: u64,
    ) -> Result<(), CleanupPlanError> {
        if preview.scan.status != ScanStatus::Completed {
            return Err(CleanupPlanError::PreviewNotCompleted {
                status: preview.scan.status,
            });
        }
        if self.scan_id != preview.scan.scan_id {
            return Err(CleanupPlanError::ScanIdChanged);
        }
        if now_unix_ms > self.expires_at_unix_ms {
            return Err(CleanupPlanError::PlanExpired);
        }
        if self.source_volume_id != preview.scan.source_volume_id {
            return Err(CleanupPlanError::VolumeChanged);
        }
        let request = PrepareCleanupPlanRequest {
            scan_id: preview.scan.scan_id.clone(),
            candidate_ids: self
                .candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect(),
        };
        let current = Self::from_selection(preview, &request)?;
        if current.plan_digest != self.plan_digest || current.candidates != self.candidates {
            return Err(CleanupPlanError::CandidateChanged);
        }
        Ok(())
    }
}

impl QuarantineIndex {
    /// Builds a preview-only quarantine index from eligible plan candidates.
    ///
    /// # Errors
    ///
    /// Returns an error if no selected candidate permits quarantine or byte
    /// aggregation overflows.
    pub fn from_plan(plan: &CleanupPlan) -> Result<Self, QuarantineError> {
        let entries: Vec<_> = plan
            .candidates
            .iter()
            .filter(|candidate| candidate.quarantine_eligible)
            .map(|candidate| QuarantineEntry {
                candidate_id: candidate.id.clone(),
                rule_id: candidate.rule_id.clone(),
                original_path: candidate.path.clone(),
                bytes: candidate.bytes,
                metadata_digest: candidate.metadata_digest.clone(),
                status: QuarantineEntryStatus::PreviewOnly,
            })
            .collect();
        if entries.is_empty() {
            return Err(QuarantineError::NoEligibleCandidates);
        }
        let total_bytes = entries
            .iter()
            .try_fold(0_u64, |total, entry| total.checked_add(entry.bytes))
            .ok_or(QuarantineError::BytesOverflow)?;
        let created_at_unix_ms = current_unix_ms();
        Ok(Self {
            index_id: format!("quarantine-{}-{created_at_unix_ms}", plan.plan_id),
            plan_id: plan.plan_id.clone(),
            created_at_unix_ms,
            entries,
            files_moved: false,
            total_bytes,
        })
    }
}

fn hash_plan(scan_id: &str, candidates: &[CleanupCandidate]) -> Result<String, CleanupPlanError> {
    let encoded = serde_json::to_vec(&PlanDigestPayload {
        scan_id,
        candidates,
    })
    .map_err(|error| CleanupPlanError::Serialization(error.to_string()))?;
    let digest = Sha256::digest(encoded);
    Ok(digest.iter().fold(
        String::with_capacity(digest.len() * 2),
        |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        },
    ))
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanDigestPayload<'a> {
    scan_id: &'a str,
    candidates: &'a [CleanupCandidate],
}

impl CleanupPreview {
    /// Creates a validated completed preview from scan output and rule statuses.
    ///
    /// # Errors
    ///
    /// Returns an error when the scan is not complete or candidate bytes overflow.
    pub fn completed(
        scan: ScanProgress,
        candidates: Vec<CleanupCandidate>,
        rule_statuses: Vec<CleanupRuleStatus>,
    ) -> Result<Self, CleanupError> {
        if scan.status != ScanStatus::Completed {
            return Err(CleanupError::PreviewRequiresCompletedScan {
                status: scan.status,
            });
        }
        let total_reclaimable_bytes = candidates
            .iter()
            .try_fold(0_u64, |total, candidate| total.checked_add(candidate.bytes))
            .ok_or(CleanupError::CandidateBytesOverflow)?;
        Ok(Self {
            scan,
            candidates,
            rule_statuses,
            total_reclaimable_bytes,
        })
    }
}

/// Validation errors for cleanup previews.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CleanupError {
    /// A preview must never look complete when the scan was cancelled or failed.
    #[error("cleanup preview requires a completed scan, got {status:?}")]
    PreviewRequiresCompletedScan {
        /// Actual scan state.
        status: ScanStatus,
    },
    /// Candidate sizes exceeded the representable byte range.
    #[error("cleanup candidate bytes overflowed while aggregating")]
    CandidateBytesOverflow,
}

/// Errors raised while creating a non-authorizing cleanup plan.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CleanupPlanError {
    /// Plans may only be derived from completed scans.
    #[error("cleanup plan requires a completed scan, got {status:?}")]
    PreviewNotCompleted {
        /// Actual scan state returned by the preview.
        status: ScanStatus,
    },
    /// The request does not belong to the latest scan.
    #[error("cleanup plan scan id changed")]
    ScanIdChanged,
    /// An empty selection cannot produce a useful review artifact.
    #[error("cleanup plan has no selected candidates")]
    NoSelectedCandidates,
    /// Duplicate IDs make the intended selection ambiguous.
    #[error("cleanup plan contains a duplicate candidate")]
    DuplicateCandidate,
    /// A submitted candidate did not originate in the selected preview.
    #[error("cleanup plan references unknown candidate {0}")]
    UnknownCandidate(String),
    /// One or more candidate snapshots differ from the original plan.
    #[error("cleanup plan candidates changed; a fresh plan is required")]
    CandidateChanged,
    /// The plan exceeded its short review lifetime.
    #[error("cleanup plan expired; a fresh scan is required")]
    PlanExpired,
    /// The source volume changed between scan and revalidation.
    #[error("cleanup plan source volume changed")]
    VolumeChanged,
    /// Candidate data could not be encoded for digesting.
    #[error("cleanup plan could not be serialized: {0}")]
    Serialization(String),
}

/// Errors raised while preparing a preview-only quarantine index.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum QuarantineError {
    /// The plan has no candidate approved for quarantine indexing.
    #[error("cleanup plan has no quarantine-eligible candidates")]
    NoEligibleCandidates,
    /// Indexed candidate sizes exceeded the representable range.
    #[error("quarantine index bytes overflowed")]
    BytesOverflow,
}

#[cfg(test)]
mod tests {
    use super::{
        AuditEvent, AuditEventKind, CleanupCandidate, CleanupError, CleanupPlan, CleanupPlanError,
        CleanupPreview, PrepareCleanupPlanRequest, QuarantineIndex, RecoveryStrategy, ScanProgress,
        ScanStatus,
    };
    use crate::dashboard::SuggestionRisk;

    fn candidate(id: &str, bytes: u64) -> CleanupCandidate {
        CleanupCandidate {
            id: id.to_owned(),
            rule_id: "browser-cache.v1".to_owned(),
            rule_version: "1".to_owned(),
            title: "浏览器缓存".to_owned(),
            description: "可重新生成的缓存内容".to_owned(),
            path: "C:\\Users\\demo\\Cache".to_owned(),
            evidence: vec!["允许目录中的元数据快照".to_owned()],
            bytes,
            item_count: 3,
            risk: SuggestionRisk::Safe,
            recoverable: true,
            requires_admin: false,
            recovery_strategy: RecoveryStrategy::Quarantine,
            quarantine_eligible: true,
            default_selected: true,
            metadata_digest: "digest".to_owned(),
            observed_at_unix_ms: Some(1),
        }
    }

    fn completed_scan() -> ScanProgress {
        ScanProgress {
            scan_id: "scan-1".to_owned(),
            status: ScanStatus::Completed,
            scanned_items: 10,
            skipped_items: 1,
            message: "扫描完成".to_owned(),
            source_volume_id: Some("C:".to_owned()),
        }
    }

    fn preview() -> CleanupPreview {
        CleanupPreview::completed(
            completed_scan(),
            vec![candidate("browser", 10), candidate("thumbnail", 20)],
            vec![],
        )
        .expect("completed scan should produce a preview")
    }

    #[test]
    fn aggregates_candidate_bytes() {
        assert_eq!(preview().total_reclaimable_bytes, 30);
    }

    #[test]
    fn rejects_incomplete_scans() {
        let mut scan = completed_scan();
        scan.status = ScanStatus::Cancelled;
        assert_eq!(
            CleanupPreview::completed(scan, vec![], vec![]),
            Err(CleanupError::PreviewRequiresCompletedScan {
                status: ScanStatus::Cancelled,
            })
        );
    }

    #[test]
    fn selection_cannot_inject_unknown_or_duplicate_candidates() {
        let preview = preview();
        let unknown = PrepareCleanupPlanRequest {
            scan_id: "scan-1".to_owned(),
            candidate_ids: vec!["outside".to_owned()],
        };
        assert_eq!(
            CleanupPlan::from_selection(&preview, &unknown),
            Err(CleanupPlanError::UnknownCandidate("outside".to_owned()))
        );
        let duplicate = PrepareCleanupPlanRequest {
            scan_id: "scan-1".to_owned(),
            candidate_ids: vec!["browser".to_owned(), "browser".to_owned()],
        };
        assert_eq!(
            CleanupPlan::from_selection(&preview, &duplicate),
            Err(CleanupPlanError::DuplicateCandidate)
        );
    }

    #[test]
    fn creates_non_authorizing_selected_plan() {
        let preview = preview();
        let request = PrepareCleanupPlanRequest {
            scan_id: "scan-1".to_owned(),
            candidate_ids: vec!["thumbnail".to_owned()],
        };
        let plan = CleanupPlan::from_selection(&preview, &request).expect("selection is valid");
        assert_eq!(plan.candidates.len(), 1);
        assert_eq!(plan.candidates[0].id, "thumbnail");
        assert!(!plan.execution_authorized);
        assert_eq!(plan.plan_digest.len(), 64);
    }

    #[test]
    fn rejects_changed_candidate_snapshot_and_expired_plan() {
        let preview = preview();
        let plan = CleanupPlan::from_preview(&preview).expect("plan should be valid");
        let mut changed = preview.clone();
        changed.candidates[0].metadata_digest = "changed".to_owned();
        assert_eq!(
            plan.validate_against(&changed),
            Err(CleanupPlanError::CandidateChanged)
        );
        assert_eq!(
            plan.validate_against_at(&preview, plan.expires_at_unix_ms + 1),
            Err(CleanupPlanError::PlanExpired)
        );
    }

    #[test]
    fn quarantine_index_never_moves_files() {
        let plan = CleanupPlan::from_preview(&preview()).expect("plan should be valid");
        let index = QuarantineIndex::from_plan(&plan).expect("eligible candidates should index");
        assert!(!index.files_moved);
        assert_eq!(index.entries.len(), 2);
        assert_eq!(index.total_bytes, 30);
    }

    #[test]
    fn audit_event_serializes_without_file_contents() {
        let event = AuditEvent {
            event_id: "event-1".to_owned(),
            kind: AuditEventKind::PlanCreated,
            subject_id: "plan-scan-1".to_owned(),
            occurred_at_unix_ms: 42,
            reason: Some("仅供复核".to_owned()),
            candidate_count: 1,
            total_bytes: 10,
            rule_ids: vec!["browser-cache.v1".to_owned()],
        };
        let json = serde_json::to_string(&event).expect("audit event should serialize");
        assert!(json.contains("planCreated"));
        assert!(!json.contains("C:\\"));
    }
}
