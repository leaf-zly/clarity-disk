import type { CleanupPreview, SpaceScanHistoryEntry } from "@/types/dashboard";
import type { PrivilegedAuditEvent } from "@/types/privileged";
import type { CleanupAuditEvent } from "@/types/cleanup-audit";
import type {
  QuarantineExecutionIndex,
  QuarantineRestoreResult,
} from "@/types/cleanup-execution";

/** Supported application appearance preferences. */
export type ThemePreference = "system" | "light" | "dark";

/** Supported interface language preferences. */
export type LanguagePreference = "simplifiedChinese" | "english";

/** Privacy-safe local logging verbosity. */
export type LogLevel = "minimal" | "standard" | "diagnostic";

/** Read-only automatic maintenance cadence. */
export type AutomaticMaintenanceSchedule = "disabled" | "weekly" | "monthly";

/** Versioned application settings persisted by Rust. */
export interface AppSettings {
  schemaVersion: 1;
  theme: ThemePreference;
  language: LanguagePreference;
  logLevel: LogLevel;
  retainCrashDiagnostics: boolean;
  notificationsEnabled: boolean;
  launchAtLogin: boolean;
  quarantineRetentionDays: 7 | 15 | 30;
  quarantineMaxBytes: number;
  automaticMaintenance: AutomaticMaintenanceSchedule;
  ignoredScanRoots: string[];
  updateChecksEnabled: boolean;
}

/** Stable reasons a scheduled read-only scan ran or stayed blocked. */
export type AutomaticMaintenanceReason =
  | "disabled"
  | "notDue"
  | "userActive"
  | "powerUnsafe"
  | "systemUpdateActive"
  | "backupActive"
  | "due";

/** Conservative automatic maintenance evaluation. */
export interface AutomaticMaintenanceDecision {
  shouldRun: boolean;
  reason: AutomaticMaintenanceReason;
  nextEligibleAtUnixMs: number | null;
}

/** Scheduler tick result; the optional operation is always read-only. */
export interface AutomaticMaintenanceRunReport {
  decision: AutomaticMaintenanceDecision;
  preview: CleanupPreview | null;
}

/** Privacy-safe crash marker without a panic payload or user path. */
export interface CrashReport {
  reportId: string;
  appVersion: string;
  occurredAtUnixMs: number;
  sourceFile: string | null;
  sourceLine: number | null;
  classification: string;
}

/** Measured latency compared with one fixed product baseline. */
export interface PerformanceMetric {
  operation: string;
  durationMs: number;
  baselineMs: number;
  withinBaseline: boolean;
  measuredAtUnixMs: number;
}

/** Local diagnostics shown and cleared from settings. */
export interface DiagnosticsSnapshot {
  crashReports: CrashReport[];
  performanceMetrics: PerformanceMetric[];
  retentionEnabled: boolean;
}

/** Backend-resolved restore locations; arbitrary paths are not accepted. */
export type QuarantineRestoreDestination =
  "original" | "desktop" | "documents" | "downloads";

/** One-time irreversible deletion challenge bound to current entry metadata. */
export interface QuarantineDeletionChallenge {
  authorizationId: string;
  entryIds: string[];
  selectionDigest: string;
  confirmationToken: string;
  confirmationPhrase: string;
  expiresAtUnixMs: number;
}

/** Request that consumes a prepared permanent deletion challenge. */
export interface ExecuteQuarantineDeletionRequest {
  authorizationId: string;
  confirmationToken: string;
  confirmationPhrase: string;
}

/** Per-entry irreversible deletion result. */
export interface QuarantineDeletionResult {
  entryId: string;
  status: "permanentlyDeleted" | string;
  reason: string;
}

/** Terminal permanent deletion report with refreshed quarantine state. */
export interface QuarantineDeletionReport {
  results: QuarantineDeletionResult[];
  deletedBytes: number;
  index: QuarantineExecutionIndex;
}

/** Unified activity payload assembled from privacy-safe local stores. */
export interface ActivityHistorySnapshot {
  cleanup: CleanupAuditEvent[];
  scans: SpaceScanHistoryEntry[];
  privileged: PrivilegedAuditEvent[];
}

/** Official GitHub Release metadata accepted by the read-only updater. */
export interface UpdateRelease {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseUrl: string;
  publishedAt: string;
  notes: string;
  hasChecksums: boolean;
  hasWindowsInstaller: boolean;
  signatureRequired: true;
}

/** Alternate restore result reuses the persisted quarantine status contract. */
export type AlternateRestoreResult = QuarantineRestoreResult;
