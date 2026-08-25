import type { CleanupCandidate, CleanupPlan } from "@/types/dashboard";

/** Mutually exclusive restricted execution boundaries. */
export type CleanupExecutionMode = "quarantine" | "windowsRecycleBin";

/** Candidate identities used to request a freshly validated confirmation. */
export interface PrepareCleanupExecutionRequest {
  planId: string;
  candidateIds: string[];
  mode: CleanupExecutionMode;
}

/** Short-lived, one-time challenge for a restricted quarantine execution. */
export interface CleanupExecutionChallenge {
  authorizationId: string;
  planId: string;
  planDigest: string;
  candidateIds: string[];
  mode: CleanupExecutionMode;
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
  mode: CleanupExecutionMode;
  results: CleanupExecutionItemResult[];
  stagedBytes: number;
  estimatedProcessedBytes: number;
  startedAtUnixMs: number;
  finishedAtUnixMs: number;
  executionAuthorized: boolean;
}

/** Lifecycle state persisted for a backend-owned quarantine entry. */
export type QuarantineEntryStatus =
  | "previewOnly"
  | "staging"
  | "copying"
  | "copyVerified"
  | "staged"
  | "restoring"
  | "restored"
  | "restoreConflict"
  | "expired";

/** Transfer mechanism recorded for recovery and crash reconciliation. */
export type QuarantineTransferKind = "rename" | "verifiedCopy";

/** Fixed-tier quarantine policy; arbitrary values are rejected by Rust. */
export interface QuarantinePolicy {
  retentionDays: 7 | 15 | 30;
  maxBytes: number;
}

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
  expiresAtUnixMs: number | null;
  transferKind: QuarantineTransferKind;
  integrityDigest: string | null;
  transactionPath: string | null;
}

/** Persisted quarantine state returned after staging or restoration. */
export interface QuarantineExecutionIndex {
  indexId: string;
  planId: string;
  createdAtUnixMs: number;
  entries: QuarantineExecutionEntry[];
  filesMoved: boolean;
  totalBytes: number;
  policy: QuarantinePolicy;
}

/** Conflict-safe restore result for one backend-indexed entry. */
export interface QuarantineRestoreResult {
  entryId: string;
  status: QuarantineEntryStatus;
  reason: string;
}

/** Bounded batch restore result with the latest persisted index. */
export interface QuarantineRestoreBatchReport {
  results: QuarantineRestoreResult[];
  index: QuarantineExecutionIndex;
}

/** Fixed-tier policy update accepted by the restricted backend. */
export interface UpdateQuarantinePolicyRequest {
  retentionDays: 7 | 15 | 30;
  maxBytes: number;
}
/** Input needed by browser fixtures to mirror a backend execution challenge. */
export interface BrowserExecutionContext {
  plan: CleanupPlan;
  candidates: CleanupCandidate[];
}
