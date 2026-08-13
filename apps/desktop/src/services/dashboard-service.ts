import { invoke, isTauri } from "@tauri-apps/api/core";
import { dashboardFixture } from "@/testing/dashboard-fixture";
import type {
  CleanupPlan,
  CleanupPreview,
  DashboardSnapshot,
  SpaceScanHistoryEntry,
  SpaceScanRequest,
  SpaceScanSnapshot,
  SpaceScanStatus,
} from "@/types/dashboard";

interface BrowserScanTask {
  request: SpaceScanRequest;
  snapshot: SpaceScanSnapshot;
  polls: number;
}

const browserScanTasks = new Map<string, BrowserScanTask>();
const browserScanHistory: SpaceScanHistoryEntry[] = [];

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

/** Starts a bounded read-only space scan for the selected root. */
export async function startSpaceScan(
  request: SpaceScanRequest,
): Promise<{ scanId: string }> {
  if (isTauri())
    return invoke<{ scanId: string }>("start_space_scan", { request });
  const scanId = `space-fixture-${Date.now()}`;
  browserScanTasks.set(scanId, {
    request: structuredClone(request),
    polls: 0,
    snapshot: createBrowserSnapshot(scanId, "scanning", 6),
  });
  return { scanId };
}

/** Polls a running space scan task and returns its newest incremental snapshot. */
export async function getSpaceScan(scanId: string): Promise<SpaceScanSnapshot> {
  if (isTauri()) return invoke<SpaceScanSnapshot>("get_space_scan", { scanId });
  const task = browserScanTasks.get(scanId);
  if (!task) throw new Error("扫描任务不存在或已过期");
  if (task.snapshot.progress.status === "scanning") {
    task.polls += 1;
    const percent = Math.min(100, 6 + task.polls * 31);
    task.snapshot = createBrowserSnapshot(
      scanId,
      percent >= 100 ? "completed" : "scanning",
      percent,
    );
    if (task.snapshot.progress.status === "completed")
      recordBrowserHistory(task);
  }
  return structuredClone(task.snapshot);
}

/** Requests cooperative cancellation of a running space scan. */
export async function cancelSpaceScan(scanId: string): Promise<void> {
  if (isTauri()) return void (await invoke("cancel_space_scan", { scanId }));
  updateBrowserTask(scanId, "cancelled");
}

/** Pauses a running scan at a cooperative boundary. */
export async function pauseSpaceScan(scanId: string): Promise<void> {
  if (isTauri()) return void (await invoke("pause_space_scan", { scanId }));
  updateBrowserTask(scanId, "paused");
}

/** Resumes a cooperatively paused scan. */
export async function resumeSpaceScan(scanId: string): Promise<void> {
  if (isTauri()) return void (await invoke("resume_space_scan", { scanId }));
  updateBrowserTask(scanId, "scanning");
}

/** Returns recent terminal scan summaries from local application state. */
export async function getSpaceScanHistory(): Promise<SpaceScanHistoryEntry[]> {
  if (isTauri())
    return invoke<SpaceScanHistoryEntry[]>("get_space_scan_history");
  return structuredClone(browserScanHistory);
}

/** Returns a conservative default request for a discovered volume root. */
export async function getDefaultSpaceScanRequest(
  rootPath: string,
): Promise<SpaceScanRequest> {
  if (isTauri()) {
    return invoke<SpaceScanRequest>("get_default_space_scan_request", {
      rootPath,
    });
  }
  return { rootPath, maxDepth: 8, maxEntries: 100_000, excludedPaths: [] };
}
function createBrowserSnapshot(
  scanId: string,
  status: SpaceScanStatus,
  percent: number,
): SpaceScanSnapshot {
  const terminal = ["completed", "cancelled", "failed"].includes(status);
  return {
    progress: {
      scanId,
      status,
      scannedItems: Math.round((1_280 * percent) / 100),
      skippedItems: 2,
      bytesScanned: Math.round((2_840 * 1024 * 1024 * percent) / 100),
      currentPath: terminal ? null : "C:\\Users\\当前用户\\AppData\\Local",
      message:
        status === "paused"
          ? "扫描已暂停"
          : status === "cancelled"
            ? "扫描已取消"
            : status === "completed"
              ? "空间扫描完成（仅读取）"
              : "正在分析目录空间",
      percentComplete: status === "completed" ? 100 : Math.min(99, percent),
      estimatedSecondsRemaining: terminal
        ? 0
        : Math.max(1, Math.ceil((100 - percent) / 20)),
      startedAtUnixMs: Date.now() - 1_500,
      finishedAtUnixMs: terminal ? Date.now() : null,
    },
    largestEntries: [
      {
        path: "C:\\Users\\当前用户\\AppData\\Local\\Packages",
        bytes: 1_240 * 1024 * 1024,
        itemCount: 480,
        kind: "directory",
      },
      {
        path: "C:\\Users\\当前用户\\Downloads\\installer.iso",
        bytes: 860 * 1024 * 1024,
        itemCount: 1,
        kind: "file",
      },
    ],
    fileTypes: [
      { fileType: ".iso", bytes: 860 * 1024 * 1024, itemCount: 1 },
      { fileType: ".zip", bytes: 430 * 1024 * 1024, itemCount: 12 },
    ],
  };
}

function updateBrowserTask(scanId: string, status: SpaceScanStatus): void {
  const task = browserScanTasks.get(scanId);
  if (!task) throw new Error("扫描任务不存在或已过期");
  const percent = task.snapshot.progress.percentComplete;
  task.snapshot = createBrowserSnapshot(scanId, status, percent);
  if (["cancelled", "failed"].includes(status)) recordBrowserHistory(task);
}

function recordBrowserHistory(task: BrowserScanTask): void {
  const progress = task.snapshot.progress;
  if (
    !progress.finishedAtUnixMs ||
    browserScanHistory.some((entry) => entry.scanId === progress.scanId)
  )
    return;
  browserScanHistory.unshift({
    scanId: progress.scanId,
    rootPath: task.request.rootPath,
    status: progress.status,
    scannedItems: progress.scannedItems,
    bytesScanned: progress.bytesScanned,
    startedAtUnixMs: progress.startedAtUnixMs,
    finishedAtUnixMs: progress.finishedAtUnixMs,
  });
  browserScanHistory.splice(20);
}
