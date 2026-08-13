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
 * Runs the read-only browser-cache scanner. The command only measures
 * allow-listed roots and never deletes or moves files.
 *
 * @returns A preview containing candidates and scan provenance.
 */
export async function loadCleanupPreview(): Promise<CleanupPreview> {
  if (isTauri()) {
    return invoke<CleanupPreview>("scan_cleanup_preview");
  }

  return {
    scan: {
      scanId: "browser-preview",
      status: "completed",
      scannedItems: 12,
      skippedItems: 0,
      message: "浏览器缓存扫描完成（仅预览）",
    },
    candidates: [
      {
        id: "browser-cache:chrome",
        ruleId: "browser-cache.v1",
        title: "Chrome 缓存",
        description: "可由浏览器重新生成的缓存内容，不会直接删除文件",
        path: "C:\\Users\\当前用户\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache",
        bytes: 454 * 1024 * 1024,
        itemCount: 120,
        risk: "safe",
        recoverable: true,
        defaultSelected: true,
      },
    ],
    totalReclaimableBytes: 454 * 1024 * 1024,
  };
}
