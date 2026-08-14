//! Non-authorizing partition safety plans and recovery protocol.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::MergePreview;

const PLAN_SCHEMA_VERSION: u16 = 1;
const RECOVERY_SCHEMA_VERSION: u16 = 1;
const PLAN_LIFETIME_MS: u64 = 5 * 60 * 1_000;
const EVIDENCE_MAX_AGE_MS: u64 = 60 * 1_000;

/// Read-only platform evidence used by the partition safety foundation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionSafetyEvidence {
    /// Unix timestamp in milliseconds when platform evidence was captured.
    pub captured_at_unix_ms: u64,
    /// Whether stable external power can be established.
    pub external_power_state: ExternalPowerState,
    /// Whether Windows reports work that requires a restart.
    pub pending_restart_state: PendingRestartState,
    /// Evidence that recoverable user data exists outside the affected disk.
    pub backup_evidence_state: BackupEvidenceState,
    /// Non-fatal platform limitations that must remain visible.
    pub discovery_warnings: Vec<String>,
}

/// External-power conclusions used by high-risk partition preflight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalPowerState {
    /// A battery-backed computer reports external power connected.
    Connected,
    /// No system battery was detected, as expected on most desktops.
    DesktopNoBattery,
    /// The computer is currently discharging a system battery.
    OnBattery,
    /// Power state could not be established.
    Unknown,
}

/// Pending-restart conclusions from fixed Windows registry evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PendingRestartState {
    /// No supported pending-restart marker was found.
    Clear,
    /// One or more supported pending-restart markers were found.
    Present,
    /// Registry evidence could not be read completely.
    Unknown,
}

/// Backup evidence accepted by the safety model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupEvidenceState {
    /// An independent provider verified a current recoverable backup.
    Verified,
    /// A backup exists but is stale for a high-risk partition operation.
    Stale,
    /// No independently verifiable backup evidence is available.
    Unavailable,
    /// The backup provider could not be queried.
    Unknown,
}

/// Immutable, expiring description of one future partition operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImmutablePartitionPlan {
    /// Version of the serialized plan contract.
    pub schema_version: u16,
    /// Digest binding every safety-relevant plan field.
    pub plan_digest: String,
    /// P6 preview digest used as the topology and selection source.
    pub preview_id: String,
    /// Topology capture time copied from the preview.
    pub topology_captured_at_unix_ms: u64,
    /// Platform evidence capture time.
    pub evidence_captured_at_unix_ms: u64,
    /// Plan creation time.
    pub created_at_unix_ms: u64,
    /// Time after which all evidence must be rediscovered.
    pub expires_at_unix_ms: u64,
    /// Only operation represented by the current safety foundation.
    pub operation: PartitionOperationKind,
    /// Stable physical-disk identity.
    pub disk_id: String,
    /// Stable source-partition identity.
    pub source_partition_id: String,
    /// Stable target-partition identity.
    pub target_partition_id: String,
    /// Data bytes a future implementation would need to migrate.
    pub migration_bytes: u64,
    /// Version of the recovery journal state machine.
    pub recovery_schema_version: u16,
}

/// Enumerated future partition operations; arbitrary commands are impossible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionOperationKind {
    /// Migrate the right source and expand the adjacent left target.
    MergeAdjacentDataPartitions,
}

/// Complete plan-seven safety assessment without execution authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionSafetyAssessment {
    /// Deterministic assessment identity.
    pub assessment_id: String,
    /// Immutable plan bound to current preview and platform evidence.
    pub plan: ImmutablePartitionPlan,
    /// Aggregate safety-foundation readiness.
    pub status: PartitionSafetyStatus,
    /// Every evaluated precondition, including passing evidence.
    pub checks: Vec<PartitionSafetyCheck>,
    /// Actionable reasons that prevent readiness.
    pub blockers: Vec<PartitionSafetyBlocker>,
    /// Platform discovery limitations.
    pub discovery_warnings: Vec<String>,
    /// Always false; plan seven cannot authorize a partition operation.
    pub execution_authorized: bool,
    /// Always false; no writer is registered in the command surface.
    pub write_capability_present: bool,
    /// User-visible description of the plan-seven boundary.
    pub disclaimer: String,
}

impl PartitionSafetyAssessment {
    /// Builds a digest-bound safety plan from a fresh P6 preview and evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for an unexpectedly authorizing preview, empty stable
    /// identities, timestamps in the future, or timestamp overflow.
    pub fn try_new(
        preview: &MergePreview,
        evidence: PartitionSafetyEvidence,
        now_unix_ms: u64,
    ) -> Result<Self, PartitionSafetyError> {
        validate_inputs(preview, &evidence, now_unix_ms)?;
        let plan = build_immutable_plan(preview, &evidence, now_unix_ms)?;
        let (checks, blockers) = evaluate_safety_checks(preview, &evidence, now_unix_ms);
        let status = if blockers.is_empty() {
            PartitionSafetyStatus::FoundationReady
        } else {
            PartitionSafetyStatus::Blocked
        };
        let assessment_id = format!("partition-safety:{}", plan.plan_digest);
        Ok(Self {
            assessment_id,
            plan,
            status,
            checks,
            blockers,
            discovery_warnings: evidence.discovery_warnings,
            execution_authorized: false,
            write_capability_present: false,
            disclaimer: "这是计划七安全基础评估，不是执行批准；当前版本没有任何分区写入命令。"
                .to_owned(),
        })
    }
}

fn build_immutable_plan(
    preview: &MergePreview,
    evidence: &PartitionSafetyEvidence,
    now_unix_ms: u64,
) -> Result<ImmutablePartitionPlan, PartitionSafetyError> {
    let expires_at_unix_ms = now_unix_ms
        .checked_add(PLAN_LIFETIME_MS)
        .ok_or(PartitionSafetyError::TimestampOverflow)?;
    let mut plan = ImmutablePartitionPlan {
        schema_version: PLAN_SCHEMA_VERSION,
        plan_digest: String::new(),
        preview_id: preview.preview_id.clone(),
        topology_captured_at_unix_ms: preview.topology_captured_at_unix_ms,
        evidence_captured_at_unix_ms: evidence.captured_at_unix_ms,
        created_at_unix_ms: now_unix_ms,
        expires_at_unix_ms,
        operation: PartitionOperationKind::MergeAdjacentDataPartitions,
        disk_id: preview.disk_id.clone(),
        source_partition_id: preview.source_partition_id.clone(),
        target_partition_id: preview.target_partition_id.clone(),
        migration_bytes: preview.migration_bytes,
        recovery_schema_version: RECOVERY_SCHEMA_VERSION,
    };
    plan.plan_digest = plan_digest(&plan, preview.feasible, evidence);
    Ok(plan)
}

fn evaluate_safety_checks(
    preview: &MergePreview,
    evidence: &PartitionSafetyEvidence,
    now_unix_ms: u64,
) -> (Vec<PartitionSafetyCheck>, Vec<PartitionSafetyBlocker>) {
    let mut checks = Vec::new();
    let mut blockers = Vec::new();
    record_check(
        &mut checks,
        &mut blockers,
        PartitionSafetyCheckCode::PreviewFeasible,
        preview.feasible,
        "只读合并预演已通过",
        PartitionSafetyBlockerCode::PreviewBlocked,
        "当前分区预演仍有阻塞项",
        "返回拓扑页面处理全部阻塞项，再重新生成安全计划。",
    );
    let evidence_fresh = now_unix_ms
        .saturating_sub(preview.topology_captured_at_unix_ms)
        .max(now_unix_ms.saturating_sub(evidence.captured_at_unix_ms))
        <= EVIDENCE_MAX_AGE_MS;
    record_check(
        &mut checks,
        &mut blockers,
        PartitionSafetyCheckCode::EvidenceFresh,
        evidence_fresh,
        "拓扑和系统证据处于新鲜窗口内",
        PartitionSafetyBlockerCode::EvidenceExpired,
        "拓扑或系统证据已过期",
        "重新读取磁盘拓扑和系统安全证据。",
    );
    let stable_power = matches!(
        evidence.external_power_state,
        ExternalPowerState::Connected | ExternalPowerState::DesktopNoBattery
    );
    record_check(
        &mut checks,
        &mut blockers,
        PartitionSafetyCheckCode::StablePower,
        stable_power,
        "已确认外接电源或桌面设备",
        PartitionSafetyBlockerCode::StablePowerUnavailable,
        "无法确认稳定供电",
        "连接外接电源；关键设备应使用可靠 UPS 后重新评估。",
    );
    record_check(
        &mut checks,
        &mut blockers,
        PartitionSafetyCheckCode::NoPendingRestart,
        evidence.pending_restart_state == PendingRestartState::Clear,
        "未发现受支持的 Windows 待重启标记",
        PartitionSafetyBlockerCode::PendingRestartUnsafe,
        "Windows 待重启状态不安全或未知",
        "完成系统重启和更新，再重新读取安全证据。",
    );
    record_check(
        &mut checks,
        &mut blockers,
        PartitionSafetyCheckCode::VerifiedBackup,
        evidence.backup_evidence_state == BackupEvidenceState::Verified,
        "已验证受影响数据存在独立可恢复备份",
        PartitionSafetyBlockerCode::VerifiedBackupMissing,
        "缺少可独立验证的最新备份",
        "先在其他物理设备创建并验证备份；用户勾选不能替代恢复验证。",
    );
    checks.push(PartitionSafetyCheck {
        code: PartitionSafetyCheckCode::RecoveryProtocolPrepared,
        passed: true,
        message: format!("恢复状态机协议 v{RECOVERY_SCHEMA_VERSION} 已绑定计划摘要"),
    });
    checks.push(PartitionSafetyCheck {
        code: PartitionSafetyCheckCode::WriteCapabilityDisabled,
        passed: true,
        message: "计划七没有注册分区写入能力或执行令牌".to_owned(),
    });
    (checks, blockers)
}

/// Aggregate result of the non-authorizing safety foundation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionSafetyStatus {
    /// All modeled preconditions pass, but execution remains unavailable.
    FoundationReady,
    /// One or more preconditions require safe remediation.
    Blocked,
}

/// One evidence-bearing plan-seven precondition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionSafetyCheck {
    /// Stable check identifier.
    pub code: PartitionSafetyCheckCode,
    /// Whether the precondition passed.
    pub passed: bool,
    /// Localized evidence or limitation.
    pub message: String,
}

/// Stable plan-seven precondition identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionSafetyCheckCode {
    /// P6 feasibility must pass on freshly discovered topology.
    PreviewFeasible,
    /// Topology and platform evidence must be recent.
    EvidenceFresh,
    /// Stable external power must be established.
    StablePower,
    /// Windows must not have pending restart work.
    NoPendingRestart,
    /// An independent current backup must be verified.
    VerifiedBackup,
    /// A versioned recovery state machine must be bound to the plan.
    RecoveryProtocolPrepared,
    /// The current command surface must contain no writer.
    WriteCapabilityDisabled,
}

/// One actionable safety-foundation blocker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionSafetyBlocker {
    /// Stable blocker identifier.
    pub code: PartitionSafetyBlockerCode,
    /// Concise reason for the blocked assessment.
    pub message: String,
    /// Safe remediation that never bypasses protection.
    pub recovery_suggestion: String,
}

/// Stable plan-seven blocker identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionSafetyBlockerCode {
    /// P6 preview remains infeasible.
    PreviewBlocked,
    /// Topology or system evidence exceeded the freshness window.
    EvidenceExpired,
    /// Stable external power is absent or unknown.
    StablePowerUnavailable,
    /// Windows reports or may have pending restart work.
    PendingRestartUnsafe,
    /// A current independently verifiable backup is absent.
    VerifiedBackupMissing,
}

/// Versioned recovery journal for a future privileged partition executor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionRecoveryJournal {
    /// Recovery protocol schema.
    pub schema_version: u16,
    /// Immutable plan digest this journal can recover.
    pub plan_digest: String,
    /// Current recovery checkpoint.
    pub state: PartitionRecoveryState,
    /// Last accepted state-machine event.
    pub last_event: Option<PartitionRecoveryEvent>,
    /// Last update timestamp.
    pub updated_at_unix_ms: u64,
    /// True once any partition metadata write could have started.
    pub write_started: bool,
    /// True when automated continuation is prohibited.
    pub manual_recovery_required: bool,
}

impl PartitionRecoveryJournal {
    /// Creates a journal at the pre-write planned checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the plan digest is empty.
    pub fn try_new(
        plan: &ImmutablePartitionPlan,
        now_unix_ms: u64,
    ) -> Result<Self, PartitionSafetyError> {
        if plan.plan_digest.trim().is_empty() {
            return Err(PartitionSafetyError::EmptyPlanDigest);
        }
        Ok(Self {
            schema_version: RECOVERY_SCHEMA_VERSION,
            plan_digest: plan.plan_digest.clone(),
            state: PartitionRecoveryState::Planned,
            last_event: None,
            updated_at_unix_ms: now_unix_ms,
            write_started: false,
            manual_recovery_required: false,
        })
    }

    /// Applies one explicit recovery event and fails closed on illegal order.
    ///
    /// Interruption before a metadata write safely stops the plan. Interruption
    /// after a write may have started always requires manual recovery.
    ///
    /// # Errors
    ///
    /// Returns an error when time moves backwards or the event is invalid for
    /// the current checkpoint.
    pub fn apply(
        &mut self,
        event: PartitionRecoveryEvent,
        now_unix_ms: u64,
    ) -> Result<(), PartitionSafetyError> {
        if now_unix_ms < self.updated_at_unix_ms {
            return Err(PartitionSafetyError::TimestampMovedBackwards);
        }
        let next = next_recovery_state(self.state, event).ok_or(
            PartitionSafetyError::InvalidRecoveryTransition {
                state: self.state,
                event,
            },
        )?;
        self.state = next;
        self.last_event = Some(event);
        self.updated_at_unix_ms = now_unix_ms;
        self.write_started |= matches!(
            next,
            PartitionRecoveryState::MutationStarted
                | PartitionRecoveryState::Verifying
                | PartitionRecoveryState::Completed
                | PartitionRecoveryState::ManualRecoveryRequired
        );
        self.manual_recovery_required = next == PartitionRecoveryState::ManualRecoveryRequired;
        Ok(())
    }
}

/// Recovery checkpoints for a future partition writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionRecoveryState {
    /// Immutable plan exists, with no privileged work started.
    Planned,
    /// Fresh preflight passed immediately before migration.
    PreflightValidated,
    /// Migrated data is staged and independently verified.
    MigrationPrepared,
    /// A partition metadata write may have started.
    MutationStarted,
    /// Metadata mutation completed and postconditions are being verified.
    Verifying,
    /// All postconditions passed.
    Completed,
    /// Work stopped before any partition metadata write.
    SafeStopped,
    /// Automated continuation is unsafe and manual recovery is required.
    ManualRecoveryRequired,
}

/// Explicit events accepted by the recovery state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionRecoveryEvent {
    /// Fresh privileged preflight succeeded.
    PreflightPassed,
    /// Migrated data was verified before metadata mutation.
    MigrationPrepared,
    /// The first partition metadata mutation is about to begin.
    MutationStarted,
    /// Metadata mutation returned and postcondition verification begins.
    MutationCommitted,
    /// Every postcondition passed.
    VerificationPassed,
    /// A postcondition failed.
    VerificationFailed,
    /// Process, system, storage, or power interruption occurred.
    Interrupted,
    /// User or system requested a safe stop before mutation.
    AbortBeforeWrite,
}

/// Validation errors for safety plans and recovery journals.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PartitionSafetyError {
    /// A P6 preview unexpectedly attempted to authorize execution.
    #[error("partition preview unexpectedly authorized execution")]
    PreviewAuthorizedExecution,
    /// A stable disk or partition identity was empty.
    #[error("partition safety identity is empty")]
    EmptyIdentity,
    /// Platform evidence was captured in the future.
    #[error("partition safety evidence timestamp is in the future")]
    EvidenceFromFuture,
    /// Topology was captured in the future.
    #[error("partition topology timestamp is in the future")]
    TopologyFromFuture,
    /// Plan expiration overflowed.
    #[error("partition safety timestamp overflowed")]
    TimestampOverflow,
    /// Recovery journal received an empty plan digest.
    #[error("partition recovery plan digest is empty")]
    EmptyPlanDigest,
    /// Recovery journal time moved backwards.
    #[error("partition recovery timestamp moved backwards")]
    TimestampMovedBackwards,
    /// Event did not follow the versioned recovery state machine.
    #[error("invalid partition recovery transition from {state:?} using {event:?}")]
    InvalidRecoveryTransition {
        /// Current checkpoint.
        state: PartitionRecoveryState,
        /// Rejected event.
        event: PartitionRecoveryEvent,
    },
}

fn validate_inputs(
    preview: &MergePreview,
    evidence: &PartitionSafetyEvidence,
    now_unix_ms: u64,
) -> Result<(), PartitionSafetyError> {
    if preview.execution_authorized {
        return Err(PartitionSafetyError::PreviewAuthorizedExecution);
    }
    if [
        preview.preview_id.as_str(),
        preview.disk_id.as_str(),
        preview.source_partition_id.as_str(),
        preview.target_partition_id.as_str(),
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(PartitionSafetyError::EmptyIdentity);
    }
    if evidence.captured_at_unix_ms > now_unix_ms {
        return Err(PartitionSafetyError::EvidenceFromFuture);
    }
    if preview.topology_captured_at_unix_ms > now_unix_ms {
        return Err(PartitionSafetyError::TopologyFromFuture);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_check(
    checks: &mut Vec<PartitionSafetyCheck>,
    blockers: &mut Vec<PartitionSafetyBlocker>,
    code: PartitionSafetyCheckCode,
    passed: bool,
    success_message: &str,
    blocker_code: PartitionSafetyBlockerCode,
    failure_message: &str,
    recovery_suggestion: &str,
) {
    checks.push(PartitionSafetyCheck {
        code,
        passed,
        message: if passed {
            success_message.to_owned()
        } else {
            failure_message.to_owned()
        },
    });
    if !passed {
        blockers.push(PartitionSafetyBlocker {
            code: blocker_code,
            message: failure_message.to_owned(),
            recovery_suggestion: recovery_suggestion.to_owned(),
        });
    }
}

fn plan_digest(
    plan: &ImmutablePartitionPlan,
    preview_feasible: bool,
    evidence: &PartitionSafetyEvidence,
) -> String {
    let source = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:?}|{:?}|{:?}",
        plan.schema_version,
        plan.preview_id,
        plan.topology_captured_at_unix_ms,
        plan.evidence_captured_at_unix_ms,
        plan.created_at_unix_ms,
        plan.expires_at_unix_ms,
        plan.disk_id,
        plan.source_partition_id,
        plan.target_partition_id,
        plan.migration_bytes,
        evidence.external_power_state,
        evidence.pending_restart_state,
        evidence.backup_evidence_state,
    );
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    hasher.update([u8::from(preview_feasible)]);
    hex_digest(&hasher.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

const fn next_recovery_state(
    state: PartitionRecoveryState,
    event: PartitionRecoveryEvent,
) -> Option<PartitionRecoveryState> {
    use PartitionRecoveryEvent as Event;
    use PartitionRecoveryState as State;
    match (state, event) {
        (State::Planned, Event::PreflightPassed) => Some(State::PreflightValidated),
        (State::PreflightValidated, Event::MigrationPrepared) => Some(State::MigrationPrepared),
        (State::MigrationPrepared, Event::MutationStarted) => Some(State::MutationStarted),
        (State::MutationStarted, Event::MutationCommitted) => Some(State::Verifying),
        (State::Verifying, Event::VerificationPassed) => Some(State::Completed),
        (State::Verifying, Event::VerificationFailed) => Some(State::ManualRecoveryRequired),
        (
            State::Planned | State::PreflightValidated | State::MigrationPrepared,
            Event::Interrupted | Event::AbortBeforeWrite,
        ) => Some(State::SafeStopped),
        (State::MutationStarted | State::Verifying, Event::Interrupted) => {
            Some(State::ManualRecoveryRequired)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MergeRiskLevel, SimulatedPartitionLayout};

    fn preview(feasible: bool) -> MergePreview {
        MergePreview {
            preview_id: "preview-digest".to_owned(),
            topology_captured_at_unix_ms: 9_990,
            disk_id: "disk-0".to_owned(),
            source_partition_id: "source".to_owned(),
            target_partition_id: "target".to_owned(),
            feasible,
            execution_authorized: false,
            risk_level: MergeRiskLevel::High,
            migration_bytes: 42,
            estimated_duration_seconds: 1,
            requires_restart: false,
            checks: vec![],
            blockers: vec![],
            simulated_layout: feasible.then(|| SimulatedPartitionLayout {
                disk_id: "disk-0".to_owned(),
                disk_size_bytes: 100,
                partitions: vec![],
            }),
            disclaimer: "read only".to_owned(),
        }
    }

    fn evidence(backup: BackupEvidenceState) -> PartitionSafetyEvidence {
        PartitionSafetyEvidence {
            captured_at_unix_ms: 9_995,
            external_power_state: ExternalPowerState::Connected,
            pending_restart_state: PendingRestartState::Clear,
            backup_evidence_state: backup,
            discovery_warnings: vec![],
        }
    }

    fn ready_assessment() -> PartitionSafetyAssessment {
        PartitionSafetyAssessment::try_new(
            &preview(true),
            evidence(BackupEvidenceState::Verified),
            10_000,
        )
        .expect("complete evidence should build")
    }

    #[test]
    fn creates_ready_foundation_without_execution_authority() {
        let assessment = ready_assessment();
        assert_eq!(assessment.status, PartitionSafetyStatus::FoundationReady);
        assert!(assessment.blockers.is_empty());
        assert!(!assessment.execution_authorized);
        assert!(!assessment.write_capability_present);
        assert_eq!(assessment.plan.plan_digest.len(), 64);
    }

    #[test]
    fn blocks_missing_backup_power_restart_and_preview() {
        let mut unsafe_evidence = evidence(BackupEvidenceState::Unavailable);
        unsafe_evidence.external_power_state = ExternalPowerState::OnBattery;
        unsafe_evidence.pending_restart_state = PendingRestartState::Present;
        let assessment =
            PartitionSafetyAssessment::try_new(&preview(false), unsafe_evidence, 10_000)
                .expect("unsafe evidence should produce blockers");
        assert_eq!(assessment.status, PartitionSafetyStatus::Blocked);
        for expected in [
            PartitionSafetyBlockerCode::PreviewBlocked,
            PartitionSafetyBlockerCode::StablePowerUnavailable,
            PartitionSafetyBlockerCode::PendingRestartUnsafe,
            PartitionSafetyBlockerCode::VerifiedBackupMissing,
        ] {
            assert!(
                assessment
                    .blockers
                    .iter()
                    .any(|blocker| blocker.code == expected)
            );
        }
    }

    #[test]
    fn digest_changes_when_safety_evidence_changes() {
        let ready = ready_assessment();
        let mut changed = evidence(BackupEvidenceState::Verified);
        changed.external_power_state = ExternalPowerState::DesktopNoBattery;
        let changed = PartitionSafetyAssessment::try_new(&preview(true), changed, 10_000)
            .expect("changed evidence should build");
        assert_ne!(ready.plan.plan_digest, changed.plan.plan_digest);
    }

    #[test]
    fn recovery_state_machine_completes_only_in_order() {
        let assessment = ready_assessment();
        let mut journal =
            PartitionRecoveryJournal::try_new(&assessment.plan, 10_000).expect("journal");
        for (event, state) in [
            (
                PartitionRecoveryEvent::PreflightPassed,
                PartitionRecoveryState::PreflightValidated,
            ),
            (
                PartitionRecoveryEvent::MigrationPrepared,
                PartitionRecoveryState::MigrationPrepared,
            ),
            (
                PartitionRecoveryEvent::MutationStarted,
                PartitionRecoveryState::MutationStarted,
            ),
            (
                PartitionRecoveryEvent::MutationCommitted,
                PartitionRecoveryState::Verifying,
            ),
            (
                PartitionRecoveryEvent::VerificationPassed,
                PartitionRecoveryState::Completed,
            ),
        ] {
            journal
                .apply(event, journal.updated_at_unix_ms + 1)
                .expect("valid transition");
            assert_eq!(journal.state, state);
        }
        assert!(journal.write_started);
        assert!(!journal.manual_recovery_required);
    }

    #[test]
    fn fault_injection_stops_safely_before_write_and_requires_manual_recovery_after() {
        let assessment = ready_assessment();
        for prewrite_events in [
            vec![],
            vec![PartitionRecoveryEvent::PreflightPassed],
            vec![
                PartitionRecoveryEvent::PreflightPassed,
                PartitionRecoveryEvent::MigrationPrepared,
            ],
        ] {
            let mut journal =
                PartitionRecoveryJournal::try_new(&assessment.plan, 10_000).expect("journal");
            for event in prewrite_events {
                journal
                    .apply(event, journal.updated_at_unix_ms + 1)
                    .expect("setup");
            }
            journal
                .apply(
                    PartitionRecoveryEvent::Interrupted,
                    journal.updated_at_unix_ms + 1,
                )
                .expect("pre-write interruption must be recoverable");
            assert_eq!(journal.state, PartitionRecoveryState::SafeStopped);
            assert!(!journal.write_started);
        }

        let mut after_write =
            PartitionRecoveryJournal::try_new(&assessment.plan, 10_000).expect("journal");
        for event in [
            PartitionRecoveryEvent::PreflightPassed,
            PartitionRecoveryEvent::MigrationPrepared,
            PartitionRecoveryEvent::MutationStarted,
        ] {
            after_write
                .apply(event, after_write.updated_at_unix_ms + 1)
                .expect("setup");
        }
        after_write
            .apply(
                PartitionRecoveryEvent::Interrupted,
                after_write.updated_at_unix_ms + 1,
            )
            .expect("post-write interruption should become manual recovery");
        assert_eq!(
            after_write.state,
            PartitionRecoveryState::ManualRecoveryRequired
        );
        assert!(after_write.manual_recovery_required);
    }

    #[test]
    fn rejects_out_of_order_or_backdated_recovery_events() {
        let assessment = ready_assessment();
        let mut journal =
            PartitionRecoveryJournal::try_new(&assessment.plan, 10_000).expect("journal");
        assert!(matches!(
            journal.apply(PartitionRecoveryEvent::MutationStarted, 10_001),
            Err(PartitionSafetyError::InvalidRecoveryTransition { .. })
        ));
        assert_eq!(
            journal.apply(PartitionRecoveryEvent::PreflightPassed, 9_999),
            Err(PartitionSafetyError::TimestampMovedBackwards)
        );
    }
}
