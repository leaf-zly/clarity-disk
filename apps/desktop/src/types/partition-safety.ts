/** System power conclusions used by plan-seven safety preflight. */
export type ExternalPowerState =
  "connected" | "desktopNoBattery" | "onBattery" | "unknown";

/** Supported Windows pending-restart conclusions. */
export type PendingRestartState = "clear" | "present" | "unknown";

/** Independently verifiable backup conclusions. */
export type BackupEvidenceState =
  "verified" | "stale" | "unavailable" | "unknown";

/** Only future partition operation represented by the current safety model. */
export type PartitionOperationKind = "mergeAdjacentDataPartitions";

/** Immutable, expiring plan bound to fresh topology and system evidence. */
export interface ImmutablePartitionPlan {
  schemaVersion: number;
  planDigest: string;
  previewId: string;
  topologyCapturedAtUnixMs: number;
  evidenceCapturedAtUnixMs: number;
  createdAtUnixMs: number;
  expiresAtUnixMs: number;
  operation: PartitionOperationKind;
  diskId: string;
  sourcePartitionId: string;
  targetPartitionId: string;
  migrationBytes: number;
  recoverySchemaVersion: number;
}

/** Aggregate readiness of the non-authorizing safety foundation. */
export type PartitionSafetyStatus = "foundationReady" | "blocked";

/** Stable plan-seven precondition identifiers. */
export type PartitionSafetyCheckCode =
  | "previewFeasible"
  | "evidenceFresh"
  | "stablePower"
  | "noPendingRestart"
  | "verifiedBackup"
  | "recoveryProtocolPrepared"
  | "writeCapabilityDisabled";

/** One evidence-bearing safety precondition. */
export interface PartitionSafetyCheck {
  code: PartitionSafetyCheckCode;
  passed: boolean;
  message: string;
}

/** Stable safety-foundation blocker identifiers. */
export type PartitionSafetyBlockerCode =
  | "previewBlocked"
  | "evidenceExpired"
  | "stablePowerUnavailable"
  | "pendingRestartUnsafe"
  | "verifiedBackupMissing";

/** One safety blocker and its non-bypass remediation. */
export interface PartitionSafetyBlocker {
  code: PartitionSafetyBlockerCode;
  message: string;
  recoverySuggestion: string;
}

/** Complete plan-seven assessment; it can never authorize execution. */
export interface PartitionSafetyAssessment {
  assessmentId: string;
  plan: ImmutablePartitionPlan;
  status: PartitionSafetyStatus;
  checks: PartitionSafetyCheck[];
  blockers: PartitionSafetyBlocker[];
  discoveryWarnings: string[];
  executionAuthorized: false;
  writeCapabilityPresent: false;
  disclaimer: string;
}
