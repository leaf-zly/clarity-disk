import { invoke, isTauri } from "@tauri-apps/api/core";
import { dashboardFixture } from "@/testing/dashboard-fixture";
import type {
  CleanupPlan,
  CleanupPreview,
  DashboardSnapshot,
} from "@/types/dashboard";

/** Loads the read-only dashboard snapshot. */
export async function loadDashboardSnapshot(): Promise<DashboardSnapshot> {
  if (isTauri()) return invoke<DashboardSnapshot>("get_dashboard_snapshot");
  return structuredClone(dashboardFixture);
}

/** Runs the read-only cleanup scanner and returns candidate metadata snapshots. */
export async function loadCleanupPreview(): Promise<CleanupPreview> {
  if (isTauri()) return invoke<CleanupPreview>("scan_cleanup_preview");
  const candidate = (
    id: string,
    ruleId: string,
    title: string,
    description: string,
    path: string,
    bytes: number,
    itemCount: number,
    risk: "safe" | "review",
    defaultSelected: boolean,
    metadataDigest: string,
  ) => ({
    id,
    ruleId,
    title,
    description,
    path,
    bytes,
    itemCount,
    risk,
    recoverable: true,
    defaultSelected,
    metadataDigest,
    observedAtUnixMs: 1,
  });
  return {
    scan: {
      scanId: "cleanup-preview",
      status: "completed",
      scannedItems: 361,
      skippedItems: 0,
      message: "清理扫描完成（仅预览）",
      sourceVolumeId: "C:",
    },
    candidates: [
      candidate(
        "browser-cache.v1",
        "browser-cache.v1",
        "浏览器缓存",
        "Chrome、Edge 与 Brave 可重新生成的缓存内容",
        "C:\\Users\\当前用户\\AppData\\Local\\浏览器缓存",
        454 * 1024 * 1024,
        120,
        "safe",
        true,
        "fixture-browser-cache",
      ),
      candidate(
        "thumbnail-cache.v1",
        "thumbnail-cache.v1",
        "缩略图缓存",
        "Windows 可重新生成的缩略图数据库",
        "C:\\Users\\当前用户\\AppData\\Local\\Microsoft\\Windows\\Explorer",
        86 * 1024 * 1024,
        8,
        "safe",
        true,
        "fixture-thumbnail-cache",
      ),
      candidate(
        "user-temp.v1",
        "user-temp.v1",
        "用户临时文件",
        "应用运行产生的临时内容，正在使用的项目会被跳过",
        "C:\\Users\\当前用户\\AppData\\Local\\Temp",
        238 * 1024 * 1024,
        64,
        "review",
        false,
        "fixture-user-temp",
      ),
      candidate(
        "recycle-bin.v1",
        "recycle-bin.v1",
        "回收站",
        "已移入 Windows 回收站的项目，永久清空前仍可恢复",
        "C:\\$Recycle.Bin",
        389 * 1024 * 1024,
        37,
        "review",
        false,
        "fixture-recycle-bin",
      ),
      candidate(
        "build-cache.v1",
        "build-cache.v1",
        "应用构建缓存",
        "包管理器和开发工具可重新生成的缓存，首次构建可能变慢",
        "C:\\Users\\当前用户\\AppData\\Local\\npm-cache",
        512 * 1024 * 1024,
        240,
        "review",
        false,
        "fixture-build-cache",
      ),
    ],
    totalReclaimableBytes: 1_679 * 1024 * 1024,
  };
}

/** Creates a review-only plan; execution remains unauthorized. */
export async function prepareCleanupPlan(): Promise<CleanupPlan> {
  if (isTauri()) return invoke<CleanupPlan>("prepare_cleanup_plan");
  const preview = await loadCleanupPreview();
  return {
    planId: `plan-${preview.scan.scanId}`,
    scanId: preview.scan.scanId,
    candidates: preview.candidates.filter(
      (candidate) => candidate.defaultSelected,
    ),
    planDigest: "browser-fixture-plan-digest",
    executionAuthorized: false,
    createdAtUnixMs: Date.now(),
    expiresAtUnixMs: Date.now() + 10 * 60 * 1000,
    sourceVolumeId: preview.scan.sourceVolumeId,
  };
}
