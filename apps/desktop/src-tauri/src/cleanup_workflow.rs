//! Application workflow coordinating cleanup scan, plan, audit, and quarantine state.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use clarity_core::{
    AuditEvent, AuditEventKind, CleanupPlan, CleanupPreview, PrepareCleanupPlanRequest,
    QuarantineIndex,
};

use crate::audit_store::AuditStore;
use crate::cleanup_scan;
use crate::quarantine_store::QuarantineStore;

/// Owns the latest in-memory snapshots and privacy-preserving local stores.
pub(crate) struct CleanupWorkflow {
    latest_preview: Mutex<Option<CleanupPreview>>,
    latest_plan: Mutex<Option<CleanupPlan>>,
    audit: AuditStore,
    quarantine: QuarantineStore,
}

impl Default for CleanupWorkflow {
    fn default() -> Self {
        Self {
            latest_preview: Mutex::new(None),
            latest_plan: Mutex::new(None),
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

    /// Returns newest privacy-preserving cleanup audit events.
    pub(crate) fn audit_events(&self) -> Vec<AuditEvent> {
        self.audit.events()
    }

    /// Returns the latest preview-only quarantine index.
    pub(crate) fn quarantine_index(&self) -> Option<QuarantineIndex> {
        self.quarantine.get()
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
            .sum(),
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

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
}
