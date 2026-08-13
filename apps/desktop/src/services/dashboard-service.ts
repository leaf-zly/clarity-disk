import { invoke, isTauri } from "@tauri-apps/api/core";

import { dashboardFixture } from "@/testing/dashboard-fixture";
import type { CleanupPreview, DashboardSnapshot } from "@/types/dashboard";

/**
 * Loads the dashboard snapshot from the Tauri command layer. Browser-only
 * development uses a cloned fixture so the Vue interface remains independently
 * testable without privileged or platform-specific dependencies.
 *
 * @returns The current dashboard snapshot.
 */
export async function loadDashboardSnapshot(): Promise<DashboardSnapshot> {
  if (isTauri()) {
    return invoke<DashboardSnapshot>("get_dashboard_snapshot");
  }

  return structuredClone(dashboardFixture);
}

/**
 * Runs the read-only cleanup scanner. The command only measures allow-listed
 * roots and never deletes or moves files.
 *
 * @returns A preview containing candidates and scan provenance.
 */
export async function loadCleanupPreview(): Promise<CleanupPreview> {
  if (isTauri()) {
    return invoke<CleanupPreview>("scan_cleanup_preview");
  }

  return {
    scan: {
      scanId: "cleanup-preview",
      status: "completed",
      scannedItems: 84,
      skippedItems: 0,
      message: "清理扫描完成（仅预览）",
    },
    candidates: [
      {
        id: "browser-cache.v1",
        ruleId: "browser-cache.v1",
        title: "浏览器缓存",
        description: "Chrome、Edge 与 Brave 可重新生成的缓存内容",
        path: "C:\\Users\\当前用户\\AppData\\Local\\浏览器缓存",
        bytes: 454 * 1024 * 1024,
        itemCount: 120,
        risk: "safe",
        recoverable: true,
        defaultSelected: true,
      },
      {
        id: "thumbnail-cache.v1",
        ruleId: "thumbnail-cache.v1",
        title: "缩略图缓存",
        description: "Windows 可重新生成的缩略图数据库",
        path: "C:\\Users\\当前用户\\AppData\\Local\\Microsoft\\Windows\\Explorer",
        bytes: 86 * 1024 * 1024,
        itemCount: 8,
        risk: "safe",
        recoverable: true,
        defaultSelected: true,
      },
      {
        id: "user-temp.v1",
        ruleId: "user-temp.v1",
        title: "用户临时文件",
        description: "应用运行产生的临时内容，正在使用的项目会被跳过",
        path: "C:\\Users\\当前用户\\AppData\\Local\\Temp",
        bytes: 238 * 1024 * 1024,
        itemCount: 64,
        risk: "review",
        recoverable: true,
        defaultSelected: false,
      },
    ],
    totalReclaimableBytes: 778 * 1024 * 1024,
  };
}
