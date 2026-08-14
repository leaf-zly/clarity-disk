import { readonly, shallowRef } from "vue";

import {
  executeCleanup,
  getExecutionQuarantineIndex,
  prepareCleanupExecution,
  restoreQuarantineEntry,
} from "@/services/cleanup-execution-service";
import type { CleanupPlan } from "@/types/dashboard";
import type {
  CleanupExecutionChallenge,
  CleanupExecutionReport,
  QuarantineExecutionIndex,
} from "@/types/cleanup-execution";

/** Reactive state machine for one-time confirmation, execution, and restoration. */
export function useCleanupExecution() {
  const challenge = shallowRef<CleanupExecutionChallenge>();
  const report = shallowRef<CleanupExecutionReport>();
  const quarantine = shallowRef<QuarantineExecutionIndex>();
  const error = shallowRef<string>();
  const isPreparing = shallowRef(false);
  const isExecuting = shallowRef(false);
  const restoringEntryId = shallowRef<string>();

  /** Freshly validates executable plan candidates and requests a short-lived challenge. */
  async function prepare(plan: CleanupPlan | undefined): Promise<void> {
    if (!plan) {
      error.value = "请先生成包含用户临时文件的安全计划。";
      return;
    }
    const candidateIds = plan.candidates
      .filter((candidate) => candidate.ruleId === "user-temp.v1")
      .map((candidate) => candidate.id);
    if (!candidateIds.length) {
      error.value = "当前计划没有已开放执行的隔离项目。";
      return;
    }
    isPreparing.value = true;
    error.value = undefined;
    try {
      challenge.value = await prepareCleanupExecution(
        { planId: plan.planId, candidateIds },
        plan,
      );
      report.value = undefined;
    } catch (cause) {
      error.value = readableError(cause, "重新校验失败，请重新扫描。");
    } finally {
      isPreparing.value = false;
    }
  }

  /** Consumes the current challenge after exact-phrase confirmation. */
  async function execute(
    plan: CleanupPlan | undefined,
    confirmationPhrase: string,
  ): Promise<void> {
    if (!plan || !challenge.value) {
      error.value = "一次性确认不存在或已经失效。";
      return;
    }
    isExecuting.value = true;
    error.value = undefined;
    const current = challenge.value;
    // Clear client state before awaiting so rapid double clicks cannot submit
    // the same one-time token twice from the UI.
    challenge.value = undefined;
    try {
      report.value = await executeCleanup(
        {
          authorizationId: current.authorizationId,
          confirmationToken: current.confirmationToken,
          confirmationPhrase,
        },
        plan,
      );
      await refresh();
    } catch (cause) {
      error.value = readableError(
        cause,
        "隔离执行失败；令牌已失效，请重新校验。",
      );
    } finally {
      isExecuting.value = false;
    }
  }

  /** Restores one backend-owned entry by ID and refreshes persisted state. */
  async function restore(entryId: string): Promise<void> {
    restoringEntryId.value = entryId;
    error.value = undefined;
    try {
      const result = await restoreQuarantineEntry(entryId);
      if (result.status === "restoreConflict") error.value = result.reason;
      await refresh();
    } catch (cause) {
      error.value = readableError(cause, "恢复失败，未覆盖任何文件。");
    } finally {
      restoringEntryId.value = undefined;
    }
  }

  /** Reloads the latest persisted execution quarantine state. */
  async function refresh(): Promise<void> {
    quarantine.value = (await getExecutionQuarantineIndex()) ?? undefined;
  }

  return {
    challenge: readonly(challenge),
    report: readonly(report),
    quarantine: readonly(quarantine),
    error: readonly(error),
    isPreparing: readonly(isPreparing),
    isExecuting: readonly(isExecuting),
    restoringEntryId: readonly(restoringEntryId),
    prepare,
    execute,
    restore,
    refresh,
  };
}

function readableError(cause: unknown, fallback: string): string {
  return cause instanceof Error && cause.message ? cause.message : fallback;
}
