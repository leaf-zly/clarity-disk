/** Physical-disk layout technologies relevant to merge feasibility. */
export type DiskLayoutKind = "basic" | "dynamic" | "storageSpaces" | "unknown";

/** Structural roles used to protect system-managed partitions. */
export type PartitionKind =
  | "data"
  | "efiSystem"
  | "microsoftReserved"
  | "recovery"
  | "system"
  | "unallocated"
  | "unknown";

/** Conservative health states returned by the Windows storage provider. */
export type TopologyHealth =
  "healthy" | "warning" | "unhealthy" | "unknown" | "notApplicable";

/** BitLocker states used by the fail-closed preview engine. */
export type EncryptionState =
  "off" | "on" | "suspended" | "notApplicable" | "unknown";

/** Shadow-copy states used by the fail-closed preview engine. */
export type SnapshotState = "none" | "present" | "unknown" | "notApplicable";

/** Current provider-visible online state for a partition. */
export type PartitionOperationalState =
  "online" | "offline" | "unknown" | "notApplicable";

/** A real partition or synthetic unallocated region in the topology. */
export interface PartitionDescriptor {
  id: string;
  diskId: string;
  partitionNumber: number;
  guid: string | null;
  startOffsetBytes: number;
  sizeBytes: number;
  fileSystem: string | null;
  label: string | null;
  mountPoints: string[];
  kind: PartitionKind;
  isSystem: boolean;
  isBoot: boolean;
  isReadOnly: boolean;
  operationalState: PartitionOperationalState;
  encryptionState: EncryptionState;
  snapshotState: SnapshotState;
  health: TopologyHealth;
  usedBytes: number | null;
  freeBytes: number | null;
}

/** Read-only physical disk metadata and its ordered regions. */
export interface PhysicalDisk {
  id: string;
  number: number;
  friendlyName: string;
  busType: string;
  partitionStyle: string;
  sizeBytes: number;
  layoutKind: DiskLayoutKind;
  health: TopologyHealth;
  isOffline: boolean;
  partitions: PartitionDescriptor[];
}

/** Point-in-time partition topology returned by the backend. */
export interface PartitionTopology {
  capturedAtUnixMs: number;
  disks: PhysicalDisk[];
  discoveryWarnings: string[];
  /** P6 contract invariant; a false value must never be rendered as usable. */
  readOnly: boolean;
}

/** Backend-identity-only input for one read-only merge preview. */
export interface MergePreviewRequest {
  sourcePartitionId: string;
  targetPartitionId: string;
}

/** Stable identifiers for every feasibility check. */
export type MergeCheckCode =
  | "distinctPartitions"
  | "samePhysicalDisk"
  | "adjacentAndOrdered"
  | "supportedDiskLayout"
  | "diskHealthyAndOnline"
  | "partitionIdentityAndRole"
  | "supportedFileSystem"
  | "encryptionDisabled"
  | "noSnapshots"
  | "partitionHealthyAndOnline"
  | "migrationSizeKnown";

/** One evidence-bearing pass/fail result from the Rust rules engine. */
export interface MergeCheck {
  code: MergeCheckCode;
  passed: boolean;
  message: string;
}

/** Stable identifiers for actionable preview blockers. */
export type MergeBlockerCode =
  | "samePartition"
  | "differentPhysicalDisk"
  | "notAdjacentOrUnsupportedDirection"
  | "unsupportedDiskLayout"
  | "diskNotHealthyOrOnline"
  | "protectedPartition"
  | "partitionIdentityUnknown"
  | "unsupportedFileSystem"
  | "encryptionNotConfirmedOff"
  | "snapshotStateUnsafe"
  | "partitionNotHealthyOrOnline"
  | "migrationSizeUnknown";

/** One blocker and the safe recovery recommendation generated for it. */
export interface MergeBlocker {
  code: MergeBlockerCode;
  partitionId: string | null;
  message: string;
  recoverySuggestion: string;
}

/** One region in a simulated post-merge layout. */
export interface SimulatedPartition {
  id: string;
  startOffsetBytes: number;
  sizeBytes: number;
  kind: PartitionKind;
  isExpandedTarget: boolean;
}

/** Simulated post-merge layout that never authorizes a disk write. */
export interface SimulatedPartitionLayout {
  diskId: string;
  diskSizeBytes: number;
  partitions: SimulatedPartition[];
}

/** Complete P6 feasibility result bound to one topology snapshot. */
export interface MergePreview {
  previewId: string;
  topologyCapturedAtUnixMs: number;
  diskId: string;
  sourcePartitionId: string;
  targetPartitionId: string;
  feasible: boolean;
  /** Always false because P6 deliberately exposes no writer. */
  executionAuthorized: false;
  riskLevel: "high";
  migrationBytes: number;
  estimatedDurationSeconds: number;
  requiresRestart: boolean;
  checks: MergeCheck[];
  blockers: MergeBlocker[];
  simulatedLayout: SimulatedPartitionLayout | null;
  disclaimer: string;
}
