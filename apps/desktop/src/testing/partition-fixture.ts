import type {
  MergeBlocker,
  MergePreview,
  MergePreviewRequest,
  PartitionDescriptor,
  PartitionTopology,
} from "@/types/partition";

const GIB = 1024 ** 3;
const MIB = 1024 ** 2;

const partition = (
  id: string,
  diskId: string,
  startOffsetBytes: number,
  sizeBytes: number,
  options: Partial<PartitionDescriptor> = {},
): PartitionDescriptor => ({
  id,
  diskId,
  partitionNumber: 1,
  guid: `fixture-guid-${id}`,
  startOffsetBytes,
  sizeBytes,
  fileSystem: "NTFS",
  label: id,
  mountPoints: [],
  kind: "data",
  isSystem: false,
  isBoot: false,
  isReadOnly: false,
  operationalState: "online",
  encryptionState: "off",
  snapshotState: "none",
  health: "healthy",
  usedBytes: Math.round(sizeBytes * 0.52),
  freeBytes: Math.round(sizeBytes * 0.48),
  ...options,
});

const diskId = "fixture-disk-nvme-0";
const efiStart = MIB;
const efiSize = 260 * MIB;
const msrStart = efiStart + efiSize;
const msrSize = 16 * MIB;
const systemStart = msrStart + msrSize;
const systemSize = 280 * GIB;
const recoveryStart = systemStart + systemSize;
const recoverySize = 980 * MIB;
const workStart = recoveryStart + recoverySize;
const workSize = 300 * GIB;
const archiveStart = workStart + workSize;
const archiveSize = 350 * GIB;
const diskSize = 1024 * GIB;

/** Browser-only topology for UI development; it cannot invoke a disk operation. */
export const partitionTopologyFixture: Readonly<PartitionTopology> = {
  capturedAtUnixMs: Date.now(),
  readOnly: true,
  discoveryWarnings: [],
  disks: [
    {
      id: diskId,
      number: 0,
      friendlyName: "Samsung NVMe SSD",
      busType: "NVMe",
      partitionStyle: "GPT",
      sizeBytes: diskSize,
      layoutKind: "basic",
      health: "healthy",
      isOffline: false,
      partitions: [
        partition("fixture-efi", diskId, efiStart, efiSize, {
          partitionNumber: 1,
          label: "EFI",
          fileSystem: "FAT32",
          kind: "efiSystem",
          isSystem: true,
          usedBytes: null,
          freeBytes: null,
          encryptionState: "notApplicable",
          snapshotState: "notApplicable",
        }),
        partition("fixture-msr", diskId, msrStart, msrSize, {
          partitionNumber: 2,
          label: "MSR",
          fileSystem: null,
          kind: "microsoftReserved",
          usedBytes: null,
          freeBytes: null,
          encryptionState: "notApplicable",
          snapshotState: "notApplicable",
        }),
        partition("fixture-system", diskId, systemStart, systemSize, {
          partitionNumber: 3,
          label: "Windows",
          mountPoints: ["C:\\"],
          kind: "system",
          isSystem: true,
        }),
        partition("fixture-recovery", diskId, recoveryStart, recoverySize, {
          partitionNumber: 4,
          label: "恢复",
          fileSystem: null,
          kind: "recovery",
          usedBytes: null,
          freeBytes: null,
          encryptionState: "notApplicable",
          snapshotState: "notApplicable",
        }),
        partition("fixture-work", diskId, workStart, workSize, {
          partitionNumber: 5,
          label: "工作区",
          mountPoints: ["D:\\"],
          usedBytes: 188 * GIB,
          freeBytes: 112 * GIB,
        }),
        partition("fixture-archive", diskId, archiveStart, archiveSize, {
          partitionNumber: 6,
          label: "资料库",
          mountPoints: ["E:\\"],
          usedBytes: 126 * GIB,
          freeBytes: 224 * GIB,
        }),
        partition(
          `unallocated:${diskId}:${archiveStart + archiveSize}`,
          diskId,
          archiveStart + archiveSize,
          diskSize - archiveStart - archiveSize,
          {
            partitionNumber: 0,
            guid: null,
            label: "未分配",
            fileSystem: null,
            kind: "unallocated",
            operationalState: "notApplicable",
            encryptionState: "notApplicable",
            snapshotState: "notApplicable",
            health: "notApplicable",
            usedBytes: null,
            freeBytes: diskSize - archiveStart - archiveSize,
          },
        ),
      ],
    },
  ],
};

/**
 * Creates a deterministic browser preview that mirrors the Rust P6 contract.
 * It is deliberately non-authorizing and exists only outside the Tauri runtime.
 */
export function createPartitionMergeFixture(
  request: MergePreviewRequest,
): MergePreview {
  const topology = partitionTopologyFixture;
  const allPartitions = topology.disks.flatMap((disk) => disk.partitions);
  const source = allPartitions.find(
    (item) => item.id === request.sourcePartitionId,
  );
  const target = allPartitions.find(
    (item) => item.id === request.targetPartitionId,
  );
  if (!source || !target) throw new Error("分区身份不存在，请刷新拓扑。");

  const blockers: MergeBlocker[] = [];
  if (source.diskId !== target.diskId)
    blockers.push({
      code: "differentPhysicalDisk",
      partitionId: null,
      message: "分区合并不能跨越物理磁盘",
      recoverySuggestion: "选择同一物理磁盘上的两个数据分区。",
    });
  if (target.startOffsetBytes + target.sizeBytes !== source.startOffsetBytes)
    blockers.push({
      code: "notAdjacentOrUnsupportedDirection",
      partitionId: source.id,
      message: "当前只支持将右侧相邻数据分区预演合并到左侧目标分区",
      recoverySuggestion: "选择左侧目标分区及其紧邻右侧的源分区。",
    });
  for (const candidate of [source, target]) {
    if (candidate.kind !== "data" || candidate.isSystem || candidate.isBoot)
      blockers.push({
        code: "protectedPartition",
        partitionId: candidate.id,
        message: "系统、启动、EFI、恢复或保留分区默认受保护",
        recoverySuggestion: "请选择普通 NTFS 数据分区。",
      });
  }

  const feasible = blockers.length === 0;
  const disk = topology.disks.find((item) => item.id === target.diskId);
  const migrationBytes = source.usedBytes ?? 0;
  return {
    previewId: `browser-partition-preview-${topology.capturedAtUnixMs}`,
    topologyCapturedAtUnixMs: topology.capturedAtUnixMs,
    diskId: target.diskId,
    sourcePartitionId: source.id,
    targetPartitionId: target.id,
    feasible,
    executionAuthorized: false,
    riskLevel: "high",
    migrationBytes,
    estimatedDurationSeconds: Math.max(
      1,
      Math.ceil(migrationBytes / (100 * MIB)),
    ),
    requiresRestart: false,
    checks: [
      {
        code: "samePhysicalDisk",
        passed: source.diskId === target.diskId,
        message:
          source.diskId === target.diskId
            ? "两个分区位于同一物理磁盘"
            : "分区合并不能跨越物理磁盘",
      },
      {
        code: "adjacentAndOrdered",
        passed:
          target.startOffsetBytes + target.sizeBytes ===
          source.startOffsetBytes,
        message:
          target.startOffsetBytes + target.sizeBytes === source.startOffsetBytes
            ? "源分区紧邻目标分区右侧，可模拟向右扩展"
            : "分区顺序或相邻关系不受支持",
      },
      {
        code: "encryptionDisabled",
        passed:
          source.encryptionState === "off" && target.encryptionState === "off",
        message: "BitLocker 已确认关闭",
      },
      {
        code: "noSnapshots",
        passed:
          source.snapshotState === "none" && target.snapshotState === "none",
        message: "未发现依赖所选卷的快照",
      },
      {
        code: "migrationSizeKnown",
        passed: source.usedBytes !== null,
        message: "已读取需要迁移的数据量",
      },
    ],
    blockers,
    simulatedLayout:
      feasible && disk
        ? {
            diskId: disk.id,
            diskSizeBytes: disk.sizeBytes,
            partitions: disk.partitions
              .filter((item) => item.id !== source.id)
              .map((item) => ({
                id: item.id,
                startOffsetBytes: item.startOffsetBytes,
                sizeBytes:
                  item.id === target.id
                    ? item.sizeBytes + source.sizeBytes
                    : item.sizeBytes,
                kind: item.kind,
                isExpandedTarget: item.id === target.id,
              })),
          }
        : null,
    disclaimer:
      "此结果仅为只读预演，不会修改磁盘；执行前必须重新发现并重新确认。",
  };
}
