import { invoke, isTauri } from "@tauri-apps/api/core";
import { dashboardFixture } from "@/testing/dashboard-fixture";
import type {
  AuditEvent,
  CleanupCandidate,
  CleanupPlan,
  CleanupPreview,
  DashboardSnapshot,
  PrepareCleanupPlanRequest,
  QuarantineIndex,
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
const browserAuditEvents: AuditEvent[] = [];
let browserCleanupPreview: CleanupPreview | undefined;
let browserQuarantineIndex: QuarantineIndex | undefined;

/** Loads the read-only dashboard snapshot. */
export async function loadDashboardSnapshot(): Promise<DashboardSnapshot> {
  if (isTauri()) return invoke<DashboardSnapshot>("get_dashboard_snapshot");
  return structuredClone(dashboardFixture);
}

/** Runs the read-only cleanup scanner and returns evidence-bearing candidates. */
export async function loadCleanupPreview(): Promise<CleanupPreview> {
  if (isTauri()) return invoke<CleanupPreview>("scan_cleanup_preview");
  const candidate = (
    id: string,
    title: string,
    description: string,
    path: string,
    bytes: number,
    itemCount: number,
    risk: CleanupCandidate["risk"],
    options: Partial<CleanupCandidate> = {},
  ): CleanupCandidate => ({
    id,
    ruleId: id,
    ruleVersion: "1",
    title,
    description,
    path,
    evidence: ["命中固定允许目录", `只读统计到 ${itemCount} 个项目`],
    bytes,
    itemCount,
    risk,
    recoverable: true,
    requiresAdmin: false,
    recoveryStrategy: risk === "safe" ? "regenerate" : "quarantine",
    quarantineEligible: id === "user-temp.v1",
    defaultSelected: risk === "safe",
    metadataDigest: `fixture-${id}`,
    observedAtUnixMs: Date.now(),
    ...options,
  });
  const candidates = [
    candidate(
      "browser-cache.v1",
      "浏览器缓存",
      "Chrome、Edge 与 Brave 可重新生成的缓存内容",
      "C:\\Users\\当前用户\\AppData\\Local\\浏览器缓存",
      454 * 1024 * 1024,
      120,
      "safe",
    ),
    candidate(
      "thumbnail-cache.v1",
      "缩略图缓存",
      "Windows 可重新生成的缩略图数据库",
      "C:\\Users\\当前用户\\AppData\\Local\\Microsoft\\Windows\\Explorer",
      86 * 1024 * 1024,
      8,
      "safe",
    ),
    candidate(
      "user-temp.v1",
      "用户临时文件",
      "应用运行产生的临时内容，正在使用的项目会被跳过",
      "C:\\Users\\当前用户\\AppData\\Local\\Temp",
      238 * 1024 * 1024,
      64,
      "review",
    ),
    candidate(
      "recycle-bin.v1",
      "回收站",
      "已移入 Windows 回收站的项目，永久清空前仍可恢复",
      "C:\\$Recycle.Bin",
      389 * 1024 * 1024,
      37,
      "review",
      { recoveryStrategy: "windowsManaged", quarantineEligible: false },
    ),
    candidate(
      "build-cache.v1",
      "应用构建缓存",
      "包管理器和开发工具可重新生成的缓存，首次构建可能变慢",
      "C:\\Users\\当前用户\\AppData\\Local\\npm-cache",
      512 * 1024 * 1024,
      240,
      "review",
      { recoveryStrategy: "regenerate", quarantineEligible: false },
    ),
    candidate(
      "windows-update-download-cache.v1",
      "Windows 更新下载缓存",
      "系统管理的更新安装缓存；本阶段仅统计，不停止服务、不删除",
      "C:\\Windows\\SoftwareDistribution\\Download",
      640 * 1024 * 1024,
      28,
      "confirmationRequired",
      {
        requiresAdmin: true,
        recoveryStrategy: "windowsManaged",
        quarantineEligible: false,
        defaultSelected: false,
      },
    ),
  ];
  const preview: CleanupPreview = {
    scan: {
      scanId: `cleanup-fixture-${Date.now()}`,
      status: "completed",
      scannedItems: 497,
      skippedItems: 2,
      message: "清理扫描完成（仅预览）",
      sourceVolumeId: "C:",
    },
    candidates,
    ruleStatuses: candidates.map((item) => ({
      ruleId: item.ruleId,
      title: item.title,
      availability: "available",
      reason: null,
      requiresAdmin: item.requiresAdmin,
      risk: item.risk,
    })),
    totalReclaimableBytes: candidates.reduce(
      (total, item) => total + item.bytes,
      0,
    ),
  };
  browserCleanupPreview = structuredClone(preview);
  recordBrowserAudit("scanCompleted", preview.scan.scanId, candidates);
  return preview;
}

/** Creates a review-only plan from IDs in the exact latest preview. */
export async function prepareCleanupPlan(
  request: PrepareCleanupPlanRequest,
): Promise<CleanupPlan> {
  if (isTauri())
    return invoke<CleanupPlan>("prepare_cleanup_plan", { request });
  const preview = browserCleanupPreview;
  if (!preview || preview.scan.scanId !== request.scanId)
    throw new Error("磁盘状态已变化，请重新扫描");
  if (!request.candidateIds.length) throw new Error("请至少选择一个项目");
  if (new Set(request.candidateIds).size !== request.candidateIds.length)
    throw new Error("清理计划包含重复候选项");
  const candidates = request.candidateIds.map((candidateId) => {
    const item = preview.candidates.find(
      (candidate) => candidate.id === candidateId,
    );
    if (!item) throw new Error("清理计划包含未知候选项");
    return item;
  });
  const now = Date.now();
  const plan: CleanupPlan = {
    planId: `plan-${preview.scan.scanId}-${now}`,
    scanId: preview.scan.scanId,
    candidates,
    planDigest: `browser-fixture-plan-${now}`,
    executionAuthorized: false,
    createdAtUnixMs: now,
    expiresAtUnixMs: now + 10 * 60 * 1000,
    sourceVolumeId: preview.scan.sourceVolumeId,
  };
  recordBrowserAudit("planCreated", plan.planId, candidates);
  return plan;
}

/** Builds a persisted preview-only quarantine index from an eligible plan. */
export async function prepareQuarantineIndex(
  plan: CleanupPlan,
): Promise<QuarantineIndex> {
  if (isTauri())
    return invoke<QuarantineIndex>("prepare_quarantine_index", {
      planId: plan.planId,
    });
  const entries = plan.candidates
    .filter((candidate) => candidate.quarantineEligible)
    .map((candidate) => ({
      candidateId: candidate.id,
      ruleId: candidate.ruleId,
      originalPath: candidate.path,
      bytes: candidate.bytes,
      metadataDigest: candidate.metadataDigest,
      status: "previewOnly" as const,
    }));
  if (!entries.length) throw new Error("当前计划没有可进入隔离区预演的项目");
  browserQuarantineIndex = {
    indexId: `quarantine-${plan.planId}`,
    planId: plan.planId,
    createdAtUnixMs: Date.now(),
    entries,
    filesMoved: false,
    totalBytes: entries.reduce((total, entry) => total + entry.bytes, 0),
  };
  recordBrowserAudit(
    "quarantineIndexCreated",
    browserQuarantineIndex.indexId,
    plan.candidates.filter((candidate) => candidate.quarantineEligible),
  );
  return structuredClone(browserQuarantineIndex);
}

/** Returns the latest preview-only quarantine index. */
export async function getQuarantineIndex(): Promise<QuarantineIndex | null> {
  if (isTauri()) return invoke<QuarantineIndex | null>("get_quarantine_index");
  return browserQuarantineIndex
    ? structuredClone(browserQuarantineIndex)
    : null;
}

/** Returns newest local cleanup audit events without file paths or contents. */
export async function getAuditEvents(): Promise<AuditEvent[]> {
  if (isTauri()) return invoke<AuditEvent[]>("get_audit_events");
  return structuredClone(browserAuditEvents);
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
  if (isTauri())
    return invoke<SpaceScanRequest>("get_default_space_scan_request", {
      rootPath,
    });
  return { rootPath, maxDepth: 8, maxEntries: 100_000, excludedPaths: [] };
}

function recordBrowserAudit(
  kind: AuditEvent["kind"],
  subjectId: string,
  candidates: CleanupCandidate[],
): void {
  browserAuditEvents.unshift({
    eventId: `audit-${kind}-${Date.now()}-${browserAuditEvents.length}`,
    kind,
    subjectId,
    occurredAtUnixMs: Date.now(),
    reason:
      kind === "quarantineIndexCreated"
        ? "仅生成隔离区索引，未移动文件"
        : kind === "planCreated"
          ? "仅生成复核计划，执行未授权"
          : "只读清理扫描完成",
    candidateCount: candidates.length,
    totalBytes: candidates.reduce(
      (total, candidate) => total + candidate.bytes,
      0,
    ),
    ruleIds: candidates.map((candidate) => candidate.ruleId),
  });
  browserAuditEvents.splice(200);
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
  task.snapshot = createBrowserSnapshot(
    scanId,
    status,
    task.snapshot.progress.percentComplete,
  );
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
