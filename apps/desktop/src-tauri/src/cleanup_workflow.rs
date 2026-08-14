//! Application workflow coordinating cleanup scan, plan, audit, and quarantine state.

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::audit_store::AuditStore;
use crate::cleanup_executor;
use crate::cleanup_scan;
use crate::quarantine_store::QuarantineStore;
use clarity_core::{
    AuditEvent, AuditEventKind, CLEANUP_CONFIRMATION_PHRASE, CleanupExecutionChallenge,
    CleanupExecutionItemResult, CleanupExecutionItemStatus, CleanupExecutionReport, CleanupPlan,
    CleanupPreview, ExecuteCleanupRequest, PrepareCleanupExecutionRequest,
    PrepareCleanupPlanRequest, QuarantineEntryStatus, QuarantineIndex, QuarantineRestoreResult,
    RestoreQuarantineRequest,
};

const EXECUTION_CHALLENGE_TTL_MS: u64 = 2 * 60 * 1000;

struct PendingExecution {
    challenge: CleanupExecutionChallenge,
    plan: CleanupPlan,
}

/// Owns the latest in-memory snapshots and privacy-preserving local stores.
pub(crate) struct CleanupWorkflow {
    latest_preview: Mutex<Option<CleanupPreview>>,
    latest_plan: Mutex<Option<CleanupPlan>>,
    pending_executions: Mutex<HashMap<String, PendingExecution>>,
    audit: AuditStore,
    quarantine: QuarantineStore,
}

impl Default for CleanupWorkflow {
    fn default() -> Self {
        Self {
            latest_preview: Mutex::new(None),
            latest_plan: Mutex::new(None),
            pending_executions: Mutex::new(HashMap::new()),
            audit: AuditStore::default(),
            quarantine: QuarantineStore::default(),
        }
    }
}

impl CleanupWorkflow {
    /// Runs a read-only scan, caches its exact snapshot, and records a safe summary.
    ///
    /// # Errors
    ///
    /// Returns an error if scanning or audit persistence fails.
    pub(crate) fn scan(&self) -> Result<CleanupPreview, String> {
        let preview = cleanup_scan::scan_cleanup_preview().map_err(|error| error.to_string())?;
        self.audit
            .record(event_for_preview(&preview))
            .map_err(|error| error.to_string())?;
        *self
            .latest_preview
            .lock()
            .expect("cleanup preview state poisoned") = Some(preview.clone());
        *self
            .latest_plan
            .lock()
            .expect("cleanup plan state poisoned") = None;
        Ok(preview)
    }

    /// Creates a plan only from candidate IDs in the exact latest preview.
    ///
    /// # Errors
    ///
    /// Returns an error for missing or stale scan state, invalid selections, or
    /// local audit persistence failures.
    pub(crate) fn prepare_plan(
        &self,
        request: &PrepareCleanupPlanRequest,
    ) -> Result<CleanupPlan, String> {
        let preview = self
            .latest_preview
            .lock()
            .expect("cleanup preview state poisoned")
            .clone()
            .ok_or_else(|| "请先完成一次清理扫描".to_owned())?;
        let plan = match CleanupPlan::from_selection(&preview, request) {
            Ok(plan) => plan,
            Err(error) => {
                let _ = self.audit.record(AuditEvent {
                    event_id: format!("audit-plan-rejected-{}", unix_ms()),
                    kind: AuditEventKind::PlanRejected,
                    subject_id: request.scan_id.clone(),
                    occurred_at_unix_ms: unix_ms(),
                    reason: Some(error.to_string()),
                    candidate_count: request.candidate_ids.len() as u64,
                    total_bytes: 0,
                    rule_ids: vec![],
                });
                return Err(error.to_string());
            }
        };
        self.audit
            .record(event_for_plan(&plan))
            .map_err(|error| error.to_string())?;
        *self
            .latest_plan
            .lock()
            .expect("cleanup plan state poisoned") = Some(plan.clone());
        Ok(plan)
    }

    /// Creates and persists a quarantine index without moving any file.
    ///
    /// # Errors
    ///
    /// Returns an error when the plan is not the latest validated plan, has
    /// expired or changed, has no eligible entries, or persistence fails.
    pub(crate) fn prepare_quarantine(&self, plan_id: &str) -> Result<QuarantineIndex, String> {
        let preview = self
            .latest_preview
            .lock()
            .expect("cleanup preview state poisoned")
            .clone()
            .ok_or_else(|| "清理预览不存在，请重新扫描".to_owned())?;
        let plan = self
            .latest_plan
            .lock()
            .expect("cleanup plan state poisoned")
            .clone()
            .filter(|plan| plan.plan_id == plan_id)
            .ok_or_else(|| "计划不是当前有效计划，请重新生成".to_owned())?;
        plan.validate_against(&preview)
            .map_err(|error| error.to_string())?;
        let index = QuarantineIndex::from_plan(&plan).map_err(|error| error.to_string())?;
        self.quarantine
            .save(index.clone())
            .map_err(|error| error.to_string())?;
        self.audit
            .record(event_for_quarantine(&index, &plan))
            .map_err(|error| error.to_string())?;
        Ok(index)
    }

    /// Issues a one-time challenge after a fresh scan validates executable candidates.
    ///
    /// # Errors
    ///
    /// Returns an error for stale plans, unsupported rules, changed candidates,
    /// empty selections, or audit persistence failures.
    pub(crate) fn prepare_execution(
        &self,
        request: &PrepareCleanupExecutionRequest,
    ) -> Result<CleanupExecutionChallenge, String> {
        let plan = self.current_plan(&request.plan_id)?;
        let fresh = cleanup_scan::scan_cleanup_preview().map_err(|error| error.to_string())?;
        let candidates = plan
            .validate_fresh_snapshot(&fresh, &request.candidate_ids)
            .map_err(|error| error.to_string())?;
        if candidates
            .iter()
            .any(|candidate| !cleanup_executor::is_executable(candidate))
        {
            return Err("当前选择包含尚未开放执行的规则；仅用户临时文件支持隔离".to_owned());
        }
        let now = unix_ms();
        let (authorization_id, confirmation_token) = execution_credentials()?;
        let challenge = CleanupExecutionChallenge {
            authorization_id: authorization_id.clone(),
            plan_id: plan.plan_id.clone(),
            plan_digest: plan.plan_digest.clone(),
            candidate_ids: request.candidate_ids.clone(),
            confirmation_token,
            confirmation_phrase: CLEANUP_CONFIRMATION_PHRASE.to_owned(),
            expires_at_unix_ms: now.saturating_add(EXECUTION_CHALLENGE_TTL_MS),
        };
        self.audit
            .record(event_for_confirmation(&challenge, &candidates))
            .map_err(|error| error.to_string())?;
        let mut pending = self
            .pending_executions
            .lock()
            .expect("cleanup execution state poisoned");
        pending.retain(|_, item| item.challenge.expires_at_unix_ms >= now);
        pending.insert(
            authorization_id,
            PendingExecution {
                challenge: challenge.clone(),
                plan,
            },
        );
        Ok(challenge)
    }

    /// Consumes a one-time challenge and stages freshly validated direct children.
    ///
    /// The challenge is removed before filesystem work, so retries can never
    /// replay a partially completed operation.
    ///
    /// # Errors
    ///
    /// Returns an error for missing, expired, replayed, or mismatched confirmation,
    /// changed candidates, unsupported rules, or state persistence failures.
    pub(crate) fn execute(
        &self,
        request: &ExecuteCleanupRequest,
    ) -> Result<CleanupExecutionReport, String> {
        let pending = self
            .pending_executions
            .lock()
            .expect("cleanup execution state poisoned")
            .remove(&request.authorization_id)
            .ok_or_else(|| "确认令牌不存在、已过期或已使用".to_owned())?;
        validate_confirmation(&pending.challenge, request)?;
        let started_at_unix_ms = unix_ms();
        let fresh = cleanup_scan::scan_cleanup_preview().map_err(|error| error.to_string())?;
        let candidates = pending
            .plan
            .validate_fresh_snapshot(&fresh, &pending.challenge.candidate_ids)
            .map_err(|error| error.to_string())?;
        self.audit
            .record(event_for_execution_start(&pending.challenge, &candidates))
            .map_err(|error| error.to_string())?;
        let mut results = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            let result = cleanup_executor::stage_candidate(
                &pending.plan.plan_id,
                candidate,
                &self.quarantine,
            )
            .unwrap_or_else(|error| CleanupExecutionItemResult {
                candidate_id: candidate.id.clone(),
                status: CleanupExecutionItemStatus::Skipped,
                staged_bytes: 0,
                staged_items: 0,
                skipped_items: candidate.item_count,
                reason: error.to_string(),
            });
            results.push(result);
        }
        let staged_bytes = results
            .iter()
            .map(|result| result.staged_bytes)
            .fold(0_u64, u64::saturating_add);
        let finished_at_unix_ms = unix_ms();
        let report = CleanupExecutionReport {
            execution_id: format!("execution-{}-{finished_at_unix_ms}", pending.plan.plan_id),
            plan_id: pending.plan.plan_id,
            results,
            staged_bytes,
            started_at_unix_ms,
            finished_at_unix_ms,
            execution_authorized: true,
        };
        self.audit
            .record(event_for_execution(&report, &candidates))
            .map_err(|error| error.to_string())?;
        Ok(report)
    }

    /// Restores one backend-indexed quarantine entry without overwriting conflicts.
    ///
    /// # Errors
    ///
    /// Returns an error when the entry is unknown, not staged, outside the
    /// application quarantine root, or persistence fails.
    pub(crate) fn restore(
        &self,
        request: &RestoreQuarantineRequest,
    ) -> Result<QuarantineRestoreResult, String> {
        let index = self
            .quarantine
            .get()
            .ok_or_else(|| "隔离区索引不存在".to_owned())?;
        let entry = index
            .entries
            .iter()
            .find(|entry| entry.entry_id == request.entry_id)
            .cloned()
            .ok_or_else(|| "隔离区项目不存在".to_owned())?;
        self.audit
            .record(event_for_restore_start(&entry))
            .map_err(|error| error.to_string())?;
        let (updated, result) =
            cleanup_executor::restore_entry(&entry).map_err(|error| error.to_string())?;
        self.quarantine
            .replace_entry(updated)
            .map_err(|error| error.to_string())?;
        self.audit
            .record(event_for_restore(&result, &entry))
            .map_err(|error| error.to_string())?;
        Ok(result)
    }

    /// Returns newest privacy-preserving cleanup audit events.
    pub(crate) fn audit_events(&self) -> Vec<AuditEvent> {
        self.audit.events()
    }

    /// Returns the latest quarantine index with preserved recovery records.
    pub(crate) fn quarantine_index(&self) -> Option<QuarantineIndex> {
        self.quarantine.get()
    }

    fn current_plan(&self, plan_id: &str) -> Result<CleanupPlan, String> {
        self.latest_plan
            .lock()
            .expect("cleanup plan state poisoned")
            .clone()
            .filter(|plan| plan.plan_id == plan_id)
            .ok_or_else(|| "计划不是当前有效计划，请重新生成".to_owned())
    }
}

fn event_for_preview(preview: &CleanupPreview) -> AuditEvent {
    AuditEvent {
        event_id: format!("audit-scan-{}", preview.scan.scan_id),
        kind: AuditEventKind::ScanCompleted,
        subject_id: preview.scan.scan_id.clone(),
        occurred_at_unix_ms: unix_ms(),
        reason: Some(format!(
            "跳过 {} 个不可读或不安全项目",
            preview.scan.skipped_items
        )),
        candidate_count: preview.candidates.len() as u64,
        total_bytes: preview.total_reclaimable_bytes,
        rule_ids: preview
            .rule_statuses
            .iter()
            .map(|status| status.rule_id.clone())
            .collect(),
    }
}

fn event_for_plan(plan: &CleanupPlan) -> AuditEvent {
    AuditEvent {
        event_id: format!("audit-plan-{}", plan.plan_id),
        kind: AuditEventKind::PlanCreated,
        subject_id: plan.plan_id.clone(),
        occurred_at_unix_ms: unix_ms(),
        reason: Some("仅生成复核计划，执行未授权".to_owned()),
        candidate_count: plan.candidates.len() as u64,
        total_bytes: plan
            .candidates
            .iter()
            .map(|candidate| candidate.bytes)
            .fold(0_u64, u64::saturating_add),
        rule_ids: plan
            .candidates
            .iter()
            .map(|candidate| candidate.rule_id.clone())
            .collect(),
    }
}

fn event_for_quarantine(index: &QuarantineIndex, plan: &CleanupPlan) -> AuditEvent {
    AuditEvent {
        event_id: format!("audit-quarantine-{}", index.index_id),
        kind: AuditEventKind::QuarantineIndexCreated,
        subject_id: index.index_id.clone(),
        occurred_at_unix_ms: unix_ms(),
        reason: Some("仅生成隔离区索引，未移动文件".to_owned()),
        candidate_count: index.entries.len() as u64,
        total_bytes: index.total_bytes,
        rule_ids: plan
            .candidates
            .iter()
            .filter(|candidate| candidate.quarantine_eligible)
            .map(|candidate| candidate.rule_id.clone())
            .collect(),
    }
}

fn event_for_confirmation(
    challenge: &CleanupExecutionChallenge,
    candidates: &[clarity_core::CleanupCandidate],
) -> AuditEvent {
    AuditEvent {
        event_id: format!("audit-confirmation-{}", challenge.authorization_id),
        kind: AuditEventKind::ExecutionConfirmationIssued,
        subject_id: challenge.authorization_id.clone(),
        occurred_at_unix_ms: unix_ms(),
        reason: Some("新扫描校验通过，已签发一次性确认令牌".to_owned()),
        candidate_count: candidates.len() as u64,
        total_bytes: candidates
            .iter()
            .map(|candidate| candidate.bytes)
            .fold(0_u64, u64::saturating_add),
        rule_ids: candidates
            .iter()
            .map(|candidate| candidate.rule_id.clone())
            .collect(),
    }
}

fn event_for_execution_start(
    challenge: &CleanupExecutionChallenge,
    candidates: &[clarity_core::CleanupCandidate],
) -> AuditEvent {
    AuditEvent {
        event_id: format!("audit-execution-start-{}", challenge.authorization_id),
        kind: AuditEventKind::ExecutionStarted,
        subject_id: challenge.authorization_id.clone(),
        occurred_at_unix_ms: unix_ms(),
        reason: Some("一次性确认已消费，新扫描复核通过，即将开始受限隔离".to_owned()),
        candidate_count: candidates.len() as u64,
        total_bytes: candidates
            .iter()
            .map(|candidate| candidate.bytes)
            .fold(0_u64, u64::saturating_add),
        rule_ids: candidates
            .iter()
            .map(|candidate| candidate.rule_id.clone())
            .collect(),
    }
}

fn event_for_execution(
    report: &CleanupExecutionReport,
    candidates: &[clarity_core::CleanupCandidate],
) -> AuditEvent {
    AuditEvent {
        event_id: format!("audit-execution-{}", report.execution_id),
        kind: AuditEventKind::ExecutionCompleted,
        subject_id: report.execution_id.clone(),
        occurred_at_unix_ms: report.finished_at_unix_ms,
        reason: Some("受限隔离执行完成；锁定或变化项目已跳过".to_owned()),
        candidate_count: report.results.len() as u64,
        total_bytes: report.staged_bytes,
        rule_ids: candidates
            .iter()
            .map(|candidate| candidate.rule_id.clone())
            .collect(),
    }
}

fn event_for_restore(
    result: &QuarantineRestoreResult,
    entry: &clarity_core::QuarantineEntry,
) -> AuditEvent {
    AuditEvent {
        event_id: format!("audit-restore-{}-{}", entry.entry_id, unix_ms()),
        kind: AuditEventKind::QuarantineRestoreCompleted,
        subject_id: entry.entry_id.clone(),
        occurred_at_unix_ms: unix_ms(),
        reason: Some(result.reason.clone()),
        candidate_count: 1,
        total_bytes: if result.status == QuarantineEntryStatus::Restored {
            entry.bytes
        } else {
            0
        },
        rule_ids: vec![entry.rule_id.clone()],
    }
}

fn event_for_restore_start(entry: &clarity_core::QuarantineEntry) -> AuditEvent {
    AuditEvent {
        event_id: format!("audit-restore-start-{}-{}", entry.entry_id, unix_ms()),
        kind: AuditEventKind::QuarantineRestoreStarted,
        subject_id: entry.entry_id.clone(),
        occurred_at_unix_ms: unix_ms(),
        reason: Some("恢复来源与目标将由受限执行器重新校验".to_owned()),
        candidate_count: 1,
        total_bytes: entry.bytes,
        rule_ids: vec![entry.rule_id.clone()],
    }
}

fn validate_confirmation(
    challenge: &CleanupExecutionChallenge,
    request: &ExecuteCleanupRequest,
) -> Result<(), String> {
    if unix_ms() > challenge.expires_at_unix_ms {
        return Err("确认令牌已过期，请重新校验".to_owned());
    }
    if request.confirmation_token != challenge.confirmation_token {
        return Err("确认令牌不匹配".to_owned());
    }
    if request.confirmation_phrase != challenge.confirmation_phrase {
        return Err("确认文字不匹配".to_owned());
    }
    Ok(())
}

fn execution_credentials() -> Result<(String, String), String> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|error| format!("无法生成安全确认令牌，未授权执行：{error}"))?;
    let encode = |bytes: &[u8]| {
        bytes.iter().fold(String::new(), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
    };
    Ok((
        format!("authorization-{}", encode(&random[..12])),
        encode(&random),
    ))
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use clarity_core::{CleanupExecutionChallenge, ExecuteCleanupRequest};

    use super::{execution_credentials, validate_confirmation};

    fn challenge(expires_at_unix_ms: u64) -> CleanupExecutionChallenge {
        CleanupExecutionChallenge {
            authorization_id: "authorization-1".to_owned(),
            plan_id: "plan-1".to_owned(),
            plan_digest: "digest".to_owned(),
            candidate_ids: vec!["user-temp.v1".to_owned()],
            confirmation_token: "token".to_owned(),
            confirmation_phrase: "确认移入隔离区".to_owned(),
            expires_at_unix_ms,
        }
    }

    #[test]
    fn execution_credentials_are_random_and_full_length() {
        let first = execution_credentials().expect("secure randomness should be available");
        let second = execution_credentials().expect("secure randomness should be available");

        assert_ne!(first, second);
        assert!(first.0.starts_with("authorization-"));
        assert_eq!(first.1.len(), 64);
    }

    #[test]
    fn confirmation_requires_an_unexpired_exact_token_and_phrase() {
        let valid = challenge(u64::MAX);
        let request = ExecuteCleanupRequest {
            authorization_id: valid.authorization_id.clone(),
            confirmation_token: valid.confirmation_token.clone(),
            confirmation_phrase: valid.confirmation_phrase.clone(),
        };
        assert!(validate_confirmation(&valid, &request).is_ok());

        let mut wrong_phrase = request.clone();
        wrong_phrase.confirmation_phrase = "确认".to_owned();
        assert!(validate_confirmation(&valid, &wrong_phrase).is_err());
        assert!(validate_confirmation(&challenge(0), &request).is_err());
    }
}
