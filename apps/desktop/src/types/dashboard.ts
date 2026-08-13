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
/** Read-only cleanup candidate produced by a versioned rule. */
export interface CleanupCandidate {
  id: string;
  ruleId: string;
  title: string;
  description: string;
  path: string;
  bytes: number;
  itemCount: number;
  risk: SuggestionRisk;
  recoverable: boolean;
  defaultSelected: boolean;
  metadataDigest: string;
  observedAtUnixMs: number | null;
}
/** Complete cleanup preview returned by the scanner. */
export interface CleanupPreview {
  scan: ScanProgress;
  candidates: CleanupCandidate[];
  totalReclaimableBytes: number;
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
