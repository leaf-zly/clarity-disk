import { invoke, isTauri } from "@tauri-apps/api/core";

import {
  getAuditEvents,
  getSpaceScanHistory,
} from "@/services/dashboard-service";
import { getPrivilegedAuditEvents } from "@/services/privileged-service";
import type { QuarantineRestoreResult } from "@/types/cleanup-execution";
import type {
  ActivityHistorySnapshot,
  AppSettings,
  AutomaticMaintenanceRunReport,
  DiagnosticsSnapshot,
  ExecuteQuarantineDeletionRequest,
  QuarantineDeletionChallenge,
  QuarantineDeletionReport,
  QuarantineRestoreDestination,
  UpdateRelease,
} from "@/types/operations";

const GIB = 1024 ** 3;
let browserSettings: AppSettings = {
  schemaVersion: 1,
  theme: "system",
  language: "simplifiedChinese",
  logLevel: "standard",
  retainCrashDiagnostics: true,
  notificationsEnabled: true,
  launchAtLogin: false,
  quarantineRetentionDays: 30,
  quarantineMaxBytes: 10 * GIB,
  automaticMaintenance: "disabled",
  ignoredScanRoots: [],
  updateChecksEnabled: true,
};

/** Loads the validated, versioned settings document. */
export async function getAppSettings(): Promise<AppSettings> {
  if (isTauri()) return invoke<AppSettings>("get_app_settings");
  return structuredClone(browserSettings);
}

/** Persists settings after Rust validates all constrained values. */
export async function updateAppSettings(
  settings: AppSettings,
): Promise<AppSettings> {
  if (isTauri())
    return invoke<AppSettings>("update_app_settings", { settings });
  if (
    settings.schemaVersion !== 1 ||
    ![7, 15, 30].includes(settings.quarantineRetentionDays) ||
    ![1, 5, 10, 20]
      .map((value) => value * GIB)
      .includes(settings.quarantineMaxBytes)
  )
    throw new Error("设置包含不受支持的安全策略值");
  browserSettings = structuredClone(settings);
  return structuredClone(browserSettings);
}

/** Runs one conservative scheduler tick; it can only start a read-only scan. */
export async function runAutomaticMaintenance(): Promise<AutomaticMaintenanceRunReport> {
  if (isTauri())
    return invoke<AutomaticMaintenanceRunReport>("run_automatic_maintenance");
  return {
    decision: {
      shouldRun: false,
      reason:
        browserSettings.automaticMaintenance === "disabled"
          ? "disabled"
          : "notDue",
      nextEligibleAtUnixMs: null,
    },
    preview: null,
  };
}

/** Loads privacy-safe activity from cleanup, scan, and privileged stores. */
export async function getActivityHistory(): Promise<ActivityHistorySnapshot> {
  const [cleanup, scans, privileged] = await Promise.all([
    getAuditEvents(),
    getSpaceScanHistory(),
    getPrivilegedAuditEvents(),
  ]);
  return { cleanup, scans, privileged };
}

/** Clears activity summaries without touching quarantine or recovery journals. */
export async function clearActivityHistory(): Promise<void> {
  if (isTauri()) await invoke("clear_activity_history");
}

/** Loads local crash markers and fixed performance baseline measurements. */
export async function getDiagnosticsSnapshot(): Promise<DiagnosticsSnapshot> {
  if (isTauri()) return invoke<DiagnosticsSnapshot>("get_diagnostics_snapshot");
  return { crashReports: [], performanceMetrics: [], retentionEnabled: true };
}

/** Clears local crash markers while preserving audit and recovery records. */
export async function clearCrashDiagnostics(): Promise<void> {
  if (isTauri()) await invoke("clear_crash_diagnostics");
}

/** Restores one backend entry to an enumerated user location. */
export async function restoreQuarantineEntryTo(
  entryId: string,
  destination: QuarantineRestoreDestination,
): Promise<QuarantineRestoreResult> {
  if (!isTauri())
    return {
      entryId,
      status: "restored",
      reason: `已恢复到${destinationLabel(destination)}`,
    };
  return invoke<QuarantineRestoreResult>("restore_quarantine_entry_to", {
    request: { entryId, destination },
  });
}

/** Prepares irreversible deletion using backend IDs only. */
export async function prepareQuarantineDeletion(
  entryIds: string[],
): Promise<QuarantineDeletionChallenge> {
  if (!isTauri()) {
    const now = Date.now();
    return {
      authorizationId: `browser-delete-${now}`,
      entryIds,
      selectionDigest: "browser-fixture",
      confirmationToken: `browser-token-${now}`,
      confirmationPhrase: "确认永久删除隔离项目",
      expiresAtUnixMs: now + 120_000,
    };
  }
  return invoke<QuarantineDeletionChallenge>("prepare_quarantine_deletion", {
    request: { entryIds },
  });
}

/** Consumes a one-time irreversible deletion challenge. */
export async function executeQuarantineDeletion(
  request: ExecuteQuarantineDeletionRequest,
): Promise<QuarantineDeletionReport> {
  if (!isTauri()) throw new Error("浏览器预览不会永久删除任何本地文件。");
  return invoke<QuarantineDeletionReport>("execute_quarantine_deletion", {
    request,
  });
}

/** Checks the official GitHub Release endpoint without downloading or installing. */
export async function checkForUpdates(): Promise<UpdateRelease> {
  const response = await fetch(
    "https://api.github.com/repos/leaf-zly/clarity-disk/releases/latest",
    { headers: { Accept: "application/vnd.github+json" } },
  );
  if (!response.ok) throw new Error(`更新服务返回 HTTP ${response.status}`);
  const payload = (await response.json()) as unknown;
  if (!isReleasePayload(payload)) throw new Error("更新元数据格式无效");
  const latestVersion = payload.tag_name.replace(/^v/, "");
  const assetNames = payload.assets.map((asset) => asset.name.toLowerCase());
  return {
    currentVersion: __APP_VERSION__,
    latestVersion,
    updateAvailable: compareVersions(latestVersion, __APP_VERSION__) > 0,
    releaseUrl: payload.html_url,
    publishedAt: payload.published_at,
    notes: payload.body.slice(0, 8_000),
    hasChecksums: assetNames.includes("sha256sums.txt"),
    hasWindowsInstaller: assetNames.some((name) => name.endsWith(".exe")),
    signatureRequired: true,
  };
}

interface GitHubReleasePayload {
  tag_name: string;
  html_url: string;
  published_at: string;
  body: string;
  assets: Array<{ name: string }>;
}

function isReleasePayload(value: unknown): value is GitHubReleasePayload {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<GitHubReleasePayload>;
  return (
    typeof candidate.tag_name === "string" &&
    /^v?\d+\.\d+\.\d+/.test(candidate.tag_name) &&
    typeof candidate.html_url === "string" &&
    candidate.html_url.startsWith(
      "https://github.com/leaf-zly/clarity-disk/releases/",
    ) &&
    typeof candidate.published_at === "string" &&
    typeof candidate.body === "string" &&
    Array.isArray(candidate.assets) &&
    candidate.assets.every((asset) => typeof asset?.name === "string")
  );
}

function compareVersions(left: string, right: string): number {
  const normalize = (value: string) =>
    value
      .split("-")[0]
      ?.split(".")
      .slice(0, 3)
      .map((part) => Number.parseInt(part ?? "0", 10)) ?? [0, 0, 0];
  const leftParts = normalize(left);
  const rightParts = normalize(right);
  for (let index = 0; index < 3; index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (difference !== 0) return Math.sign(difference);
  }
  return 0;
}

function destinationLabel(destination: QuarantineRestoreDestination): string {
  return {
    original: "原位置",
    desktop: "桌面",
    documents: "文档",
    downloads: "下载目录",
  }[destination];
}
