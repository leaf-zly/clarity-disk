/**
 * Stable disk usage category identifiers shared with the Rust domain model.
 */
export type DiskCategoryKind =
  "applications" | "system" | "files" | "development";

/** Risk levels used to control default selection and confirmation behavior. */
export type SuggestionRisk = "safe" | "review" | "confirmationRequired";

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
