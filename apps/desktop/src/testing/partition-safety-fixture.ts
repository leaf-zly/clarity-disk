import { createPartitionMergeFixture } from "@/testing/partition-fixture";
import type { PartitionSafetyAssessment } from "@/types/partition-safety";
import type { MergePreviewRequest } from "@/types/partition";

/**
 * Creates browser-only safety evidence without reading or writing a disk.
 * Backup remains blocked because a UI fixture cannot prove recoverability.
 */
export function createPartitionSafetyFixture(
  request: MergePreviewRequest,
): PartitionSafetyAssessment {
  const preview = createPartitionMergeFixture(request);
  const now = Date.now();
  const digest = `fixture-safety-${preview.previewId.slice(-20)}`;
  return {
    assessmentId: `partition-safety:${digest}`,
    plan: {
      schemaVersion: 1,
      planDigest: digest,
      previewId: preview.previewId,
      topologyCapturedAtUnixMs: preview.topologyCapturedAtUnixMs,
      evidenceCapturedAtUnixMs: now,
      createdAtUnixMs: now,
      expiresAtUnixMs: now + 5 * 60 * 1000,
      operation: "mergeAdjacentDataPartitions",
      diskId: preview.diskId,
      sourcePartitionId: preview.sourcePartitionId,
      targetPartitionId: preview.targetPartitionId,
      migrationBytes: preview.migrationBytes,
      recoverySchemaVersion: 1,
    },
    status: "blocked",
    checks: [
      {
        code: "previewFeasible",
        passed: preview.feasible,
        message: preview.feasible
          ? "只读合并预演已通过"
          : "当前分区预演仍有阻塞项",
      },
      {
        code: "evidenceFresh",
        passed: true,
        message: "拓扑和系统证据处于新鲜窗口内",
      },
      {
        code: "stablePower",
        passed: true,
        message: "已确认外接电源或桌面设备",
      },
      {
        code: "noPendingRestart",
        passed: true,
        message: "未发现受支持的 Windows 待重启标记",
      },
      {
        code: "verifiedBackup",
        passed: false,
        message: "缺少可独立验证的最新备份",
      },
      {
        code: "recoveryProtocolPrepared",
        passed: true,
        message: "恢复状态机协议 v1 已绑定计划摘要",
      },
      {
        code: "writeCapabilityDisabled",
        passed: true,
        message: "当前评估未注册分区写入能力或执行令牌",
      },
    ],
    blockers: [
      {
        code: "verifiedBackupMissing",
        message: "缺少可独立验证的最新备份",
        recoverySuggestion:
          "先在其他物理设备创建并验证备份；用户勾选不能替代恢复验证。",
      },
    ],
    discoveryWarnings: [],
    executionAuthorized: false,
    writeCapabilityPresent: false,
    disclaimer:
      "这是分区安全评估，不是执行批准；只有重新发现状态且全部门禁通过后，才可请求一次性管理员执行。",
  };
}
