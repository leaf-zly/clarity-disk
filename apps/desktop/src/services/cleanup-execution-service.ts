import { invoke, isTauri } from "@tauri-apps/api/core";

import type { CleanupPlan } from "@/types/dashboard";
import type {
  CleanupExecutionChallenge,
  CleanupExecutionReport,
  ExecuteCleanupRequest,
  PrepareCleanupExecutionRequest,
  QuarantineExecutionIndex,
  QuarantineRestoreBatchReport,
  QuarantineRestoreResult,
  UpdateQuarantinePolicyRequest,
} from "@/types/cleanup-execution";

const GIB = 1024 ** 3;
const QUARANTINE_RULES = new Set([
  "user-temp.v1",
  "browser-cache.v1",
  "thumbnail-cache.v1",
  "build-cache.v1",
]);
let browserChallenge: CleanupExecutionChallenge | undefined;
let browserIndex: QuarantineExecutionIndex | undefined;

/** Requests a one-time confirmation bound to one execution mode. */
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
    if (!candidate) throw new Error("清理候选已变化，请重新扫描");
    return candidate;
  });
  const valid =
    request.mode === "quarantine"
      ? candidates.length > 0 &&
        candidates.every((item) => QUARANTINE_RULES.has(item.ruleId))
      : candidates.length === 1 && candidates[0]?.ruleId === "recycle-bin.v1";
  if (!valid) throw new Error("当前选择混合了不兼容或尚未开放的清理规则");
  const now = Date.now();
  browserChallenge = {
    authorizationId: "authorization-" + browserPlan.planId + "-" + now,
    planId: browserPlan.planId,
    planDigest: browserPlan.planDigest,
    candidateIds: request.candidateIds,
    mode: request.mode,
    confirmationToken: "fixture-token-" + now,
    confirmationPhrase:
      request.mode === "quarantine" ? "确认移入隔离区" : "确认永久清空回收站",
    expiresAtUnixMs: now + 2 * 60 * 1000,
  };
  return structuredClone(browserChallenge);
}

/** Consumes one challenge; browser mode simulates state without filesystem writes. */
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
  )
    throw new Error("确认令牌不存在、已过期或已使用");
  const candidates = browserPlan.candidates.filter((candidate) =>
    challenge.candidateIds.includes(candidate.id),
  );
  const now = Date.now();
  if (challenge.mode === "quarantine") {
    const entries = candidates.map((candidate, index) => ({
      entryId: "fixture-entry-" + (index + 1),
      candidateId: candidate.id,
      ruleId: candidate.ruleId,
      originalPath: candidate.path + "\\fixture-" + (index + 1) + ".tmp",
      quarantinePath:
        "C:\\Users\\当前用户\\AppData\\Local\\ClarityDisk\\quarantine\\fixture-entry-" +
        (index + 1),
      bytes: candidate.bytes,
      metadataDigest: candidate.metadataDigest,
      status: "staged" as const,
      movedAtUnixMs: now,
      restoredAtUnixMs: null,
      expiresAtUnixMs: now + 30 * 86_400_000,
      transferKind: "rename" as const,
      integrityDigest: null,
      transactionPath: null,
    }));
    browserIndex = {
      indexId: "quarantine-" + browserPlan.planId,
      planId: browserPlan.planId,
      createdAtUnixMs: now,
      entries,
      filesMoved: entries.length > 0,
      totalBytes: entries.reduce((total, entry) => total + entry.bytes, 0),
      policy: { retentionDays: 30, maxBytes: 10 * GIB },
    };
  }
  const stagedBytes =
    challenge.mode === "quarantine" ? (browserIndex?.totalBytes ?? 0) : 0;
  const estimatedProcessedBytes =
    challenge.mode === "windowsRecycleBin"
      ? candidates.reduce((total, item) => total + item.bytes, 0)
      : 0;
  return {
    executionId: "execution-" + browserPlan.planId + "-" + now,
    planId: browserPlan.planId,
    mode: challenge.mode,
    results: candidates.map((candidate) => ({
      candidateId: candidate.id,
      status: "staged",
      stagedBytes: challenge.mode === "quarantine" ? candidate.bytes : 0,
      stagedItems: challenge.mode === "quarantine" ? 1 : candidate.itemCount,
      skippedItems: 0,
      reason:
        challenge.mode === "quarantine"
          ? "已安全移入隔离区"
          : "已通过 Windows 官方接口清空回收站",
    })),
    stagedBytes,
    estimatedProcessedBytes,
    startedAtUnixMs: now,
    finishedAtUnixMs: now,
    executionAuthorized: true,
  };
}

/** Loads the latest backend-owned quarantine state. */
export async function getExecutionQuarantineIndex(): Promise<QuarantineExecutionIndex | null> {
  if (isTauri())
    return invoke<QuarantineExecutionIndex | null>("get_quarantine_index");
  return browserIndex ? structuredClone(browserIndex) : null;
}

/** Restores one backend-indexed item by identity, never by path. */
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
  recomputeBrowserIndex();
  return { entryId, status: "restored", reason: "已恢复到原位置" };
}

/** Restores a bounded set of backend IDs and returns the refreshed index. */
export async function restoreQuarantineBatch(
  entryIds: string[],
): Promise<QuarantineRestoreBatchReport> {
  const request = { entryIds };
  if (isTauri())
    return invoke<QuarantineRestoreBatchReport>("restore_quarantine_batch", {
      request,
    });
  if (
    !entryIds.length ||
    entryIds.length > 100 ||
    new Set(entryIds).size !== entryIds.length
  )
    throw new Error("批量恢复必须选择 1 到 100 个不重复项目");
  const results = await Promise.all(entryIds.map(restoreQuarantineEntry));
  if (!browserIndex) throw new Error("隔离区索引不存在");
  return { results, index: structuredClone(browserIndex) };
}

/** Updates retention/capacity using reviewed tiers only. */
export async function updateQuarantinePolicy(
  request: UpdateQuarantinePolicyRequest,
): Promise<QuarantineExecutionIndex> {
  if (isTauri())
    return invoke<QuarantineExecutionIndex>("update_quarantine_policy", {
      request,
    });
  if (
    ![7, 15, 30].includes(request.retentionDays) ||
    ![1, 5, 10, 20].map((n) => n * GIB).includes(request.maxBytes)
  )
    throw new Error("隔离策略不在允许范围内");
  browserIndex ??= {
    indexId: "quarantine-policy",
    planId: "",
    createdAtUnixMs: Date.now(),
    entries: [],
    filesMoved: false,
    totalBytes: 0,
    policy: { retentionDays: 30, maxBytes: 10 * GIB },
  };
  browserIndex.policy = request;
  browserIndex.entries.forEach((entry) => {
    entry.expiresAtUnixMs = entry.movedAtUnixMs
      ? entry.movedAtUnixMs + request.retentionDays * 86_400_000
      : null;
  });
  return structuredClone(browserIndex);
}

function recomputeBrowserIndex(): void {
  if (!browserIndex) return;
  browserIndex.totalBytes = browserIndex.entries
    .filter((item) => item.status !== "restored")
    .reduce((total, item) => total + item.bytes, 0);
  browserIndex.filesMoved = browserIndex.entries.some((item) =>
    ["staged", "expired", "restoreConflict", "copyVerified"].includes(
      item.status,
    ),
  );
}
