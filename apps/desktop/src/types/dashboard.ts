/** Stable disk usage category identifiers shared with the Rust domain model. */
export type DiskCategoryKind =
  "applications" | "system" | "files" | "development";
/** Risk levels used to control default selection and confirmation behavior. */
export type SuggestionRisk = "safe" | "review" | "confirmationRequired";
/** Lifecycle states emitted by the read-only cleanup scanner. */
export type ScanStatus =
  "idle" | "discovering" | "scanning" | "completed" | "cancelled" | "failed";
/** Progress and provenance for a cleanup scan. */
export interface ScanProgress {
  scanId: string;
  status: ScanStatus;
  scannedItems: number;
  skippedItems: number;
  message: string;
  sourceVolumeId: string | null;
}
/** Recovery behavior a future cleanup executor must preserve. */
export type RecoveryStrategy =
  "regenerate" | "quarantine" | "windowsManaged" | "none";
/** Read-only cleanup candidate produced by a versioned rule. */
export interface CleanupCandidate {
  id: string;
  ruleId: string;
  ruleVersion: string;
  title: string;
  description: string;
  path: string;
  evidence: string[];
  bytes: number;
  itemCount: number;
  risk: SuggestionRisk;
  recoverable: boolean;
  requiresAdmin: boolean;
  recoveryStrategy: RecoveryStrategy;
  quarantineEligible: boolean;
  defaultSelected: boolean;
  metadataDigest: string;
  observedAtUnixMs: number | null;
}
/** Availability state for a rule evaluated during a completed scan. */
export type CleanupRuleAvailability = "available" | "empty" | "unavailable";
/** Result for every enabled cleanup rule, including rules with no candidate. */
export interface CleanupRuleStatus {
  ruleId: string;
  title: string;
  availability: CleanupRuleAvailability;
  reason: string | null;
  requiresAdmin: boolean;
  risk: SuggestionRisk;
}
/** Complete cleanup preview returned by the scanner. */
export interface CleanupPreview {
  scan: ScanProgress;
  candidates: CleanupCandidate[];
  ruleStatuses: CleanupRuleStatus[];
  totalReclaimableBytes: number;
}
/** Candidate identity selection used to build a plan from one exact scan. */
export interface PrepareCleanupPlanRequest {
  scanId: string;
  candidateIds: string[];
}
/** Immutable, non-authorizing plan created from the current preview. */
export interface CleanupPlan {
  planId: string;
  scanId: string;
  candidates: CleanupCandidate[];
  planDigest: string;
  executionAuthorized: boolean;
  createdAtUnixMs: number;
  expiresAtUnixMs: number;
  sourceVolumeId: string | null;
}
/** Privacy-preserving local cleanup audit event. */
export interface AuditEvent {
  eventId: string;
  kind:
    "scanCompleted" | "planCreated" | "planRejected" | "quarantineIndexCreated";
  subjectId: string;
  occurredAtUnixMs: number;
  reason: string | null;
  candidateCount: number;
  totalBytes: number;
  ruleIds: string[];
}
/** Preview-only quarantine entry copied from an immutable cleanup plan. */
export interface QuarantineEntry {
  candidateId: string;
  ruleId: string;
  originalPath: string;
  bytes: number;
  metadataDigest: string;
  status: "previewOnly";
}
/** Persisted quarantine preview; filesMoved is false until an executor exists. */
export interface QuarantineIndex {
  indexId: string;
  planId: string;
  createdAtUnixMs: number;
  entries: QuarantineEntry[];
  filesMoved: boolean;
  totalBytes: number;
}
/** Lifecycle states for a bounded read-only space scan. */
export type SpaceScanStatus =
  "idle" | "scanning" | "paused" | "cancelled" | "completed" | "failed";
/** User-selected scan scope and resource limits. */
export interface SpaceScanRequest {
  rootPath: string;
  maxDepth: number;
  maxEntries: number;
  excludedPaths: string[];
}
/** Progress of a space scan task. Percent and ETA are bounded estimates. */
export interface SpaceScanProgress {
  scanId: string;
  status: SpaceScanStatus;
  scannedItems: number;
  skippedItems: number;
  bytesScanned: number;
  currentPath: string | null;
  message: string;
  percentComplete: number;
  estimatedSecondsRemaining: number | null;
  startedAtUnixMs: number;
  finishedAtUnixMs: number | null;
}
/** Largest file or aggregated directory reported by a space scan. */
export interface SpaceScanEntry {
  path: string;
  bytes: number;
  itemCount: number;
  kind: "file" | "directory" | string;
}
/** File extension aggregate reported by a space scan. */
export interface SpaceScanTypeStat {
  fileType: string;
  bytes: number;
  itemCount: number;
}
/** Pollable incremental result of a space scan task. */
export interface SpaceScanSnapshot {
  progress: SpaceScanProgress;
  largestEntries: SpaceScanEntry[];
  fileTypes: SpaceScanTypeStat[];
}
/** Persisted terminal summary for a recent space scan. */
export interface SpaceScanHistoryEntry {
  scanId: string;
  rootPath: string;
  status: SpaceScanStatus;
  scannedItems: number;
  bytesScanned: number;
  startedAtUnixMs: number;
  finishedAtUnixMs: number;
}
/** Capacity attributed to a user-facing disk category. */
export interface DiskCategory {
  kind: DiskCategoryKind;
  label: string;
  bytes: number;
}
/** Read-only platform metadata for a logical volume. */
export interface DiskMetadata {
  mountPoint: string;
  fileSystem: string;
  deviceType: string;
  isSystemVolume: boolean;
  isRemovable: boolean;
  isReadOnly: boolean;
  healthStatus: "healthy" | "readOnly" | "warning";
  healthNote: string | null;
}
/** Capacity and classification data for a logical volume. */
export interface DiskSummary {
  id: string;
  label: string;
  totalBytes: number;
  usedBytes: number;
  categories: DiskCategory[];
  metadata: DiskMetadata;
}
/** Health information returned by the platform health provider. */
export interface DiskHealth {
  status: string;
  deviceType: string;
  temperatureCelsius: number | null;
  hasWarning: boolean;
}
/** Low-risk cleanup estimate shown before a detailed scan. */
export interface CleanupSummary {
  reclaimableBytes: number;
  categoryCount: number;
}
/** A prioritized maintenance recommendation presented to the user. */
export interface Suggestion {
  id: string;
  title: string;
  description: string;
  risk: SuggestionRisk;
  reclaimableBytes: number;
}
/** Complete dashboard payload returned by the desktop command layer. */
export interface DashboardSnapshot {
  disk: DiskSummary;
  disks: DiskSummary[];
  health: DiskHealth;
  cleanup: CleanupSummary;
  suggestions: Suggestion[];
}
