import type { PartitionSafetyAssessment } from "@/types/partition-safety";

/** Stable maintenance capabilities advertised by the versioned broker. */
export type MaintenanceCapability =
  "hibernation" | "windowsUpdateDownloadCache" | "systemRestorePoint";

/** Path-free, allow-listed maintenance request accepted by Rust. */
export type MaintenanceOperation =
  | { operation: "setHibernation"; enabled: boolean }
  | { operation: "resetWindowsUpdateDownloadCache" }
  | { operation: "createSystemRestorePoint" };

/** Read-only capability handshake; both partition gates must be true. */
export interface PrivilegedCapabilities {
  schemaVersion: number;
  serviceVersion: string;
  serviceAvailable: boolean;
  maintenanceOperations: MaintenanceCapability[];
  partitionWriterCompiled: boolean;
  partitionWriterRuntimeEnabled: boolean;
}

/** Short-lived backend-owned confirmation challenge. */
export interface PrivilegedExecutionChallenge {
  challengeId: string;
  confirmationToken: string;
  confirmationPhrase: string;
  expiresAtUnixMs: number;
  impact: string;
}

/** Only frontend input accepted when consuming a prepared challenge. */
export interface ExecutePrivilegedRequest {
  challengeId: string;
  confirmationToken: string;
  confirmationPhrase: string;
}

/** Terminal broker outcomes with an explicit recovery boundary. */
export type PrivilegedExecutionStatus =
  "completed" | "safeStopped" | "manualRecoveryRequired" | "rejected";

/** Result returned after UAC, execution and postcondition verification. */
export interface PrivilegedExecutionReport {
  requestId: string;
  status: PrivilegedExecutionStatus;
  message: string;
  completedAtUnixMs: number;
  recoveryState: string | null;
}

/** Fresh partition assessment with an optional fully gated challenge. */
export interface PartitionExecutionPreparation {
  assessment: PartitionSafetyAssessment;
  challenge: PrivilegedExecutionChallenge | null;
}

/** Privacy-preserving privileged terminal event. */
export interface PrivilegedAuditEvent {
  requestId: string;
  status: string;
  message: string;
  completedAtUnixMs: number;
  planDigest: string | null;
}
