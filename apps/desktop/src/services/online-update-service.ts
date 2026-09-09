import { invoke, isTauri } from "@tauri-apps/api/core";
import { check, type DownloadEvent } from "@tauri-apps/plugin-updater";

/** Update channel is fixed at build time; a preview build never receives stable manifests. */
export const UPDATE_CHANNEL =
  import.meta.env.VITE_UPDATE_CHANNEL === "stable" ? "stable" : "preview";

/** Verified update resource. Download and explicit installation are separate operations. */
export interface OnlineUpdate {
  readonly version: string;
  readonly notes: string;
  /** Downloads with a five-minute timeout and verifies the pinned Minisign key before resolving. */
  download(onEvent: (event: DownloadEvent) => void): Promise<void>;
  /** Requires a completed verified download and a native shutdown reservation; exits on Windows. */
  install(): Promise<void>;
  /** Releases the native resource and any downloaded bytes without installing. */
  close(): Promise<void>;
}

/**
 * Checks the compiled-in HTTPS update feed, never a caller-provided URL.
 * Returns null for an up-to-date install; browser previews cannot download or install.
 */
export async function checkOnlineUpdate(): Promise<OnlineUpdate | null> {
  if (!isTauri()) throw new Error("在线更新仅在已安装的 Windows 应用中可用。");
  const update = await check({ timeout: 20_000 });
  if (!update) return null;
  let verified = false;
  return {
    version: update.version,
    notes: (update.body ?? "此版本未提供更新说明。").slice(0, 8_000),
    async download(onEvent) {
      verified = false;
      // Finished only means bytes arrived. The plugin verifies the signature after
      // that event; installation becomes available only after download resolves.
      await update.download(onEvent, { timeout: 300_000 });
      verified = true;
    },
    async install() {
      if (!verified) throw new Error("更新包尚未完成签名校验，请重新下载。");
      await invoke("reserve_update_installation");
      try {
        await update.install();
      } finally {
        // Windows normally exits here; release the reservation if launch fails or returns.
        await invoke("release_update_installation");
      }
    },
    async close() {
      verified = false;
      await update.close();
    },
  };
}

/** Converts native failures to actionable messages without claiming a failed check means up-to-date. */
export function updateErrorMessage(error: unknown): string {
  const detail = error instanceof Error ? error.message : String(error);
  if (/signature|minisign|pubkey|verification|签名|校验/i.test(detail)) {
    return "更新包签名校验失败，已禁止安装。请重新检查更新；持续失败请联系维护者。";
  }
  if (
    /timeout|timed out|network|fetch|connect|request|http|release json/i.test(
      detail,
    )
  ) {
    return "无法连接更新服务，或更新尚未发布。请检查网络后重试；当前版本可继续使用。";
  }
  if (/正在执行|更新安全状态|正在安装|仅在已安装/.test(detail)) return detail;
  return "更新操作未完成，请重试。不会自动安装未校验的文件。";
}
