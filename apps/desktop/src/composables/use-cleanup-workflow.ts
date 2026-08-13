import { computed, readonly, shallowRef } from "vue";

import {
  getAuditEvents,
  getQuarantineIndex,
  loadCleanupPreview,
  prepareCleanupPlan,
  prepareQuarantineIndex,
} from "@/services/dashboard-service";
import type {
  AuditEvent,
  CleanupPlan,
  CleanupPreview,
  QuarantineIndex,
} from "@/types/dashboard";

/** Reactive API for the read-only cleanup preview, plan, audit, and quarantine flow. */
export function useCleanupWorkflow() {
  const preview = shallowRef<CleanupPreview>();
  const plan = shallowRef<CleanupPlan>();
  const quarantine = shallowRef<QuarantineIndex>();
  const auditEvents = shallowRef<AuditEvent[]>([]);
  const selectedIds = shallowRef<string[]>([]);
  const error = shallowRef<string>();
  const isScanning = shallowRef(false);
  const isPreparingPlan = shallowRef(false);
  const isPreparingQuarantine = shallowRef(false);
  const selectedCandidates = computed(() =>
    (preview.value?.candidates ?? []).filter((candidate) =>
      selectedIds.value.includes(candidate.id),
    ),
  );
  const selectedBytes = computed(() =>
    selectedCandidates.value.reduce(
      (total, candidate) => total + candidate.bytes,
      0,
    ),
  );
  const highestRisk = computed(() => {
    const risks = selectedCandidates.value.map((candidate) => candidate.risk);
    if (risks.includes("confirmationRequired")) return "confirmationRequired";
    if (risks.includes("review")) return "review";
    return risks.length ? "safe" : undefined;
  });

  /** Refreshes the exact preview and resets stale plan state and selections. */
  async function scan(): Promise<void> {
    isScanning.value = true;
    error.value = undefined;
    try {
      preview.value = await loadCleanupPreview();
      selectedIds.value = preview.value.candidates
        .filter((candidate) => candidate.defaultSelected)
        .map((candidate) => candidate.id);
      plan.value = undefined;
      await refreshAuxiliaryState();
    } catch (cause) {
      error.value = readableError(cause, "清理扫描暂时无法完成。");
    } finally {
      isScanning.value = false;
    }
  }

  /** Updates candidate selection without exposing mutable preview state. */
  function setSelected(candidateId: string, selected: boolean): void {
    const ids = new Set(selectedIds.value);
    if (selected) ids.add(candidateId);
    else ids.delete(candidateId);
    selectedIds.value = [...ids];
    plan.value = undefined;
  }

  /** Generates a non-authorizing plan after the backend validates selected IDs. */
  async function createPlan(): Promise<void> {
    if (!preview.value || !selectedIds.value.length) {
      error.value = "请至少选择一个清理项目。";
      return;
    }
    isPreparingPlan.value = true;
    error.value = undefined;
    try {
      plan.value = await prepareCleanupPlan({
        scanId: preview.value.scan.scanId,
        candidateIds: selectedIds.value,
      });
      await refreshAudit();
    } catch (cause) {
      error.value = readableError(cause, "安全计划生成失败，请重新扫描。");
    } finally {
      isPreparingPlan.value = false;
    }
  }

  /** Creates metadata-only quarantine preview for eligible selected candidates. */
  async function createQuarantineIndex(): Promise<void> {
    if (!plan.value) {
      error.value = "请先生成当前选择的安全计划。";
      return;
    }
    isPreparingQuarantine.value = true;
    error.value = undefined;
    try {
      quarantine.value = await prepareQuarantineIndex(plan.value);
      await refreshAudit();
    } catch (cause) {
      error.value = readableError(cause, "隔离区预演生成失败。");
    } finally {
      isPreparingQuarantine.value = false;
    }
  }

  /** Reloads persisted audit and quarantine state without starting a new scan. */
  async function refreshAuxiliaryState(): Promise<void> {
    const [events, index] = await Promise.all([
      getAuditEvents(),
      getQuarantineIndex(),
    ]);
    auditEvents.value = events;
    quarantine.value = index ?? undefined;
  }

  async function refreshAudit(): Promise<void> {
    auditEvents.value = await getAuditEvents();
  }

  return {
    preview: readonly(preview),
    plan: readonly(plan),
    quarantine: readonly(quarantine),
    auditEvents: readonly(auditEvents),
    selectedIds: readonly(selectedIds),
    selectedBytes,
    highestRisk,
    error: readonly(error),
    isScanning: readonly(isScanning),
    isPreparingPlan: readonly(isPreparingPlan),
    isPreparingQuarantine: readonly(isPreparingQuarantine),
    scan,
    setSelected,
    createPlan,
    createQuarantineIndex,
    refreshAuxiliaryState,
  };
}

function readableError(cause: unknown, fallback: string): string {
  return cause instanceof Error && cause.message ? cause.message : fallback;
}
