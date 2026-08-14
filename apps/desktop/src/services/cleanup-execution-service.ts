import { invoke, isTauri } from "@tauri-apps/api/core";

import type { CleanupPlan } from "@/types/dashboard";
import type {
  CleanupExecutionChallenge,
  CleanupExecutionReport,
  ExecuteCleanupRequest,
  PrepareCleanupExecutionRequest,
  QuarantineExecutionIndex,
  QuarantineRestoreResult,
} from "@/types/cleanup-execution";

let browserChallenge: CleanupExecutionChallenge | undefined;
let browserIndex: QuarantineExecutionIndex | undefined;

/**
 * Requests a one-time confirmation after backend fresh-scan validation.
 *
 * @param request Backend-owned plan and candidate identities; paths are forbidden.
 * @param browserPlan Read-only fixture context used only outside Tauri.
 * @returns A short-lived challenge that must be consumed exactly once.
 */
export async function prepareCleanupExecution(
  request: PrepareCleanupExecutionRequest,
  browserPlan: CleanupPlan,
): Promise<CleanupExecutionChallenge> {
  if (isTauri())
    return invoke<CleanupExecutionChallenge>("prepare_cleanup_execution", {
      request,
    });
  const candidates = request.candidateIds.map((id) => {
    const candidate = browserPlan.candidates.find((item) => item.id === id);
    if (!candidate || candidate.ruleId !== "user-temp.v1")
      throw new Error("当前选择包含尚未开放执行的规则");
    return candidate;
  });
  if (!candidates.length) throw new Error("当前计划没有可执行的隔离项目");
  const now = Date.now();
  browserChallenge = {
    authorizationId: `authorization-${browserPlan.planId}-${now}`,
    planId: browserPlan.planId,
    planDigest: browserPlan.planDigest,
    candidateIds: request.candidateIds,
    confirmationToken: `fixture-token-${now}`,
    confirmationPhrase: "确认移入隔离区",
    expiresAtUnixMs: now + 2 * 60 * 1000,
  };
  return structuredClone(browserChallenge);
}

/**
 * Consumes a one-time token and executes only the backend allow-listed operation.
 *
 * @param request Backend-issued authorization plus the user's exact confirmation phrase.
 * @param browserPlan Read-only fixture context used only outside Tauri.
 * @returns Actual staged capacity and per-candidate terminal results.
 * @remarks Submission consumes the current challenge even when validation fails.
 */
export async function executeCleanup(
  request: ExecuteCleanupRequest,
  browserPlan: CleanupPlan,
): Promise<CleanupExecutionReport> {
  if (isTauri())
    return invoke<CleanupExecutionReport>("execute_cleanup", { request });
  const challenge = browserChallenge;
  browserChallenge = undefined;
  if (
    !challenge ||
    challenge.authorizationId !== request.authorizationId ||
    challenge.confirmationToken !== request.confirmationToken ||
    challenge.confirmationPhrase !== request.confirmationPhrase ||
    challenge.expiresAtUnixMs < Date.now()
  ) {
    throw new Error("确认令牌不存在、已过期或已使用");
  }
  const candidates = browserPlan.candidates.filter((candidate) =>
    challenge.candidateIds.includes(candidate.id),
  );
  const now = Date.now();
  const entries = candidates.map((candidate, index) => ({
    entryId: `fixture-entry-${index + 1}`,
    candidateId: candidate.id,
    ruleId: candidate.ruleId,
    originalPath: `${candidate.path}\\fixture-${index + 1}.tmp`,
    quarantinePath: `C:\\Users\\当前用户\\AppData\\Local\\ClarityDisk\\quarantine\\fixture-entry-${index + 1}`,
    bytes: candidate.bytes,
    metadataDigest: candidate.metadataDigest,
    status: "staged" as const,
    movedAtUnixMs: now,
    restoredAtUnixMs: null,
  }));
  browserIndex = {
    indexId: `quarantine-${browserPlan.planId}`,
    planId: browserPlan.planId,
    createdAtUnixMs: now,
    entries,
    filesMoved: entries.length > 0,
    totalBytes: entries.reduce((total, entry) => total + entry.bytes, 0),
  };
  return {
    executionId: `execution-${browserPlan.planId}-${now}`,
    planId: browserPlan.planId,
    results: candidates.map((candidate) => ({
      candidateId: candidate.id,
      status: "staged",
      stagedBytes: candidate.bytes,
      stagedItems: 1,
      skippedItems: 0,
      reason: "已安全移入隔离区",
    })),
    stagedBytes: browserIndex.totalBytes,
    startedAtUnixMs: now,
    finishedAtUnixMs: now,
    executionAuthorized: true,
  };
}

/**
 * Loads the latest backend-owned quarantine execution state.
 *
 * @returns The persisted index, or `null` before any preview or execution exists.
 */
export async function getExecutionQuarantineIndex(): Promise<QuarantineExecutionIndex | null> {
  if (isTauri())
    return invoke<QuarantineExecutionIndex | null>("get_quarantine_index");
  return browserIndex ? structuredClone(browserIndex) : null;
}

/**
 * Restores one backend-indexed item by identity without accepting a path.
 *
 * @param entryId Stable identity returned by the backend quarantine index.
 * @returns Conflict-safe terminal restore status.
 */
export async function restoreQuarantineEntry(
  entryId: string,
): Promise<QuarantineRestoreResult> {
  const request = { entryId };
  if (isTauri())
    return invoke<QuarantineRestoreResult>("restore_quarantine_entry", {
      request,
    });
  const entry = browserIndex?.entries.find((item) => item.entryId === entryId);
  if (!entry) throw new Error("隔离区项目不存在");
  entry.status = "restored";
  entry.restoredAtUnixMs = Date.now();
  if (browserIndex) {
    browserIndex.totalBytes = browserIndex.entries
      .filter((item) => item.status !== "restored")
      .reduce((total, item) => total + item.bytes, 0);
    browserIndex.filesMoved = browserIndex.entries.some(
      (item) => item.status === "staged" || item.status === "restoreConflict",
    );
  }
  return { entryId, status: "restored", reason: "已恢复到原位置" };
}
