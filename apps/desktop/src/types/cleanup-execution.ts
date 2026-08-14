import type { CleanupCandidate, CleanupPlan } from "@/types/dashboard";

/** Candidate identities used to request a freshly validated confirmation. */
export interface PrepareCleanupExecutionRequest {
  planId: string;
  candidateIds: string[];
}

/** Short-lived, one-time challenge for a restricted quarantine execution. */
export interface CleanupExecutionChallenge {
  authorizationId: string;
  planId: string;
  planDigest: string;
  candidateIds: string[];
  confirmationToken: string;
  confirmationPhrase: string;
  expiresAtUnixMs: number;
}

/** Explicit confirmation that consumes a one-time execution challenge. */
export interface ExecuteCleanupRequest {
  authorizationId: string;
  confirmationToken: string;
  confirmationPhrase: string;
}

/** Terminal candidate outcome from the restricted executor. */
export type CleanupExecutionItemStatus =
  "staged" | "partiallyStaged" | "skipped" | "revalidationFailed";

/** Per-candidate result from a restricted cleanup execution. */
export interface CleanupExecutionItemResult {
  candidateId: string;
  status: CleanupExecutionItemStatus;
  stagedBytes: number;
  stagedItems: number;
  skippedItems: number;
  reason: string;
}

/** Completed report for one consumed and explicitly confirmed execution. */
export interface CleanupExecutionReport {
  executionId: string;
  planId: string;
  results: CleanupExecutionItemResult[];
  stagedBytes: number;
  startedAtUnixMs: number;
  finishedAtUnixMs: number;
  executionAuthorized: boolean;
}

/** Lifecycle state persisted for a backend-owned quarantine entry. */
export type QuarantineEntryStatus =
  "previewOnly" | "staging" | "staged" | "restored" | "restoreConflict";

/** Backend-owned quarantine entry; paths are display-only and never submitted. */
export interface QuarantineExecutionEntry {
  entryId: string;
  candidateId: string;
  ruleId: string;
  originalPath: string;
  quarantinePath: string | null;
  bytes: number;
  metadataDigest: string;
  status: QuarantineEntryStatus;
  movedAtUnixMs: number | null;
  restoredAtUnixMs: number | null;
}

/** Persisted quarantine state returned after staging or restoration. */
export interface QuarantineExecutionIndex {
  indexId: string;
  planId: string;
  createdAtUnixMs: number;
  entries: QuarantineExecutionEntry[];
  filesMoved: boolean;
  totalBytes: number;
}

/** Conflict-safe restore result for one backend-indexed entry. */
export interface QuarantineRestoreResult {
  entryId: string;
  status: QuarantineEntryStatus;
  reason: string;
}

/** Input needed by browser fixtures to mirror a backend execution challenge. */
export interface BrowserExecutionContext {
  plan: CleanupPlan;
  candidates: CleanupCandidate[];
}
