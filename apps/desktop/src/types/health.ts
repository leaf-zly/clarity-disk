/** Conservative physical-disk health conclusions. */
export type DiskHealthStatus = "good" | "attention" | "critical" | "unknown";

/** Health states reported by the Windows storage provider. */
export type ProviderHealthStatus =
  "healthy" | "warning" | "unhealthy" | "unknown";

/** Provider-backed self-monitoring conclusions. */
export type SmartHealthStatus = "passed" | "warning" | "failed" | "unavailable";

/** Confidence used when joining disk and reliability-provider identities. */
export type IdentityMappingConfidence = "exact" | "diskNumber" | "unknown";

/** Completeness of the provider evidence used for a health conclusion. */
export type HealthDataCompleteness = "complete" | "partial" | "limited";

/** Severity attached to one health signal. */
export type HealthSignalSeverity = "critical" | "warning" | "unknown";

/** Stable identifiers for health evidence and provider limitations. */
export type DiskHealthSignalCode =
  | "diskOffline"
  | "providerUnhealthy"
  | "providerWarning"
  | "providerUnknown"
  | "smartFailure"
  | "smartWarning"
  | "smartUnavailable"
  | "temperatureHigh"
  | "temperatureCritical"
  | "wearHigh"
  | "wearCritical"
  | "uncorrectedErrors"
  | "correctedErrors"
  | "dataIncomplete"
  | "identityMappingUnknown";

/** Mounted-volume BitLocker counts associated with a physical disk. */
export interface DiskEncryptionSummary {
  protectedVolumes: number;
  unprotectedVolumes: number;
  unknownVolumes: number;
}

/** One evidence-bearing warning, critical finding, or unknown state. */
export interface DiskHealthSignal {
  code: DiskHealthSignalCode;
  severity: HealthSignalSeverity;
  title: string;
  detail: string;
  recommendation: string;
}

/** Evaluated read-only health information for one physical disk. */
export interface PhysicalDiskHealth {
  id: string;
  number: number;
  friendlyName: string;
  manufacturer: string | null;
  model: string | null;
  firmwareVersion: string | null;
  /** Redacted suffix containing no more than four serial characters. */
  serialSuffix: string | null;
  busType: string;
  mediaType: string;
  sizeBytes: number;
  operationalStatus: string[];
  isOffline: boolean;
  providerHealth: ProviderHealthStatus;
  smartStatus: SmartHealthStatus;
  identityMapping: IdentityMappingConfidence;
  temperatureCelsius: number | null;
  temperatureMaxCelsius: number | null;
  wearPercentUsed: number | null;
  estimatedLifeRemainingPercent: number | null;
  powerOnHours: number | null;
  readErrorsTotal: number | null;
  readErrorsUncorrected: number | null;
  writeErrorsTotal: number | null;
  writeErrorsUncorrected: number | null;
  logicalSectorBytes: number | null;
  physicalSectorBytes: number | null;
  encryption: DiskEncryptionSummary;
  dataCompleteness: HealthDataCompleteness;
  status: DiskHealthStatus;
  signals: DiskHealthSignal[];
  /** F11 invariant: the health feature exposes no partition writer. */
  partitionWritesBlocked: true;
}

/** Aggregate counts for one point-in-time health snapshot. */
export interface DiskHealthSummary {
  totalDisks: number;
  goodDisks: number;
  attentionDisks: number;
  criticalDisks: number;
  unknownDisks: number;
  overallStatus: DiskHealthStatus;
}

/** Privacy-preserving read-only health snapshot returned by the backend. */
export interface DiskHealthSnapshot {
  capturedAtUnixMs: number;
  disks: PhysicalDiskHealth[];
  summary: DiskHealthSummary;
  discoveryWarnings: string[];
  /** Must remain true; false indicates a contract violation. */
  readOnly: true;
}
