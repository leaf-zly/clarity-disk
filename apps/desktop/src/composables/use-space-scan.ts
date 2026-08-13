import { computed, onUnmounted, shallowReadonly, shallowRef } from "vue";
import type { ComputedRef, ShallowRef } from "vue";

import {
  cancelSpaceScan,
  getSpaceScan,
  getSpaceScanHistory,
  pauseSpaceScan,
  resumeSpaceScan,
  startSpaceScan,
} from "@/services/dashboard-service";
import type {
  SpaceScanHistoryEntry,
  SpaceScanRequest,
  SpaceScanSnapshot,
} from "@/types/dashboard";

const TERMINAL_STATUSES = new Set(["completed", "cancelled", "failed"]);

/** Configuration for the bounded space-scan lifecycle. */
export interface UseSpaceScanOptions {
  /** Poll interval for incremental snapshots. Defaults to 400 milliseconds. */
  pollIntervalMs?: number;
}

/** Reactive state and actions for one active read-only scan at a time. */
export interface UseSpaceScanResult {
  snapshot: Readonly<ShallowRef<SpaceScanSnapshot | undefined>>;
  history: Readonly<ShallowRef<SpaceScanHistoryEntry[]>>;
  error: Readonly<ShallowRef<string | undefined>>;
  isStarting: Readonly<ShallowRef<boolean>>;
  isActive: ComputedRef<boolean>;
  start: (request: SpaceScanRequest) => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  cancel: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  dispose: () => void;
}

/**
 * Owns polling, terminal-state detection, controls, and history refresh for a
 * bounded read-only scan. Disposing the consumer stops UI polling but never
 * terminates a backend task implicitly.
 */
export function useSpaceScan(
  options: UseSpaceScanOptions = {},
): UseSpaceScanResult {
  const snapshot = shallowRef<SpaceScanSnapshot>();
  const history = shallowRef<SpaceScanHistoryEntry[]>([]);
  const error = shallowRef<string>();
  const isStarting = shallowRef(false);
  const isActive = computed(() =>
    ["scanning", "paused"].includes(snapshot.value?.progress.status ?? ""),
  );
  const pollIntervalMs = options.pollIntervalMs ?? 400;
  let activeScanId: string | undefined;
  let pollTimer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  function clearPoll(): void {
    if (pollTimer) clearTimeout(pollTimer);
    pollTimer = undefined;
  }

  function schedulePoll(): void {
    clearPoll();
    if (!disposed && activeScanId)
      pollTimer = setTimeout(() => void poll(), pollIntervalMs);
  }

  async function poll(): Promise<void> {
    if (!activeScanId || disposed) return;
    try {
      snapshot.value = await getSpaceScan(activeScanId);
      if (TERMINAL_STATUSES.has(snapshot.value.progress.status)) {
        activeScanId = undefined;
        clearPoll();
        await refreshHistory();
      } else schedulePoll();
    } catch {
      clearPoll();
      error.value = "无法读取扫描进度，任务可能已结束或应用状态已变化。";
    }
  }

  async function start(request: SpaceScanRequest): Promise<void> {
    clearPoll();
    error.value = undefined;
    isStarting.value = true;
    try {
      const started = await startSpaceScan(request);
      activeScanId = started.scanId;
      await poll();
    } catch {
      activeScanId = undefined;
      error.value = "无法开始扫描，请检查范围、排除路径和访问权限。";
    } finally {
      isStarting.value = false;
    }
  }

  async function pause(): Promise<void> {
    if (!activeScanId) return;
    try {
      await pauseSpaceScan(activeScanId);
      await poll();
    } catch {
      error.value = "暂停扫描失败，请稍后重试。";
    }
  }

  async function resume(): Promise<void> {
    if (!activeScanId) return;
    try {
      await resumeSpaceScan(activeScanId);
      await poll();
    } catch {
      error.value = "继续扫描失败，请稍后重试。";
    }
  }

  async function cancel(): Promise<void> {
    if (!activeScanId) return;
    try {
      await cancelSpaceScan(activeScanId);
      await poll();
    } catch {
      error.value = "取消请求未能送达，请稍后重试。";
    }
  }

  async function refreshHistory(): Promise<void> {
    try {
      history.value = await getSpaceScanHistory();
    } catch {
      error.value = "扫描历史暂时无法读取。";
    }
  }

  function dispose(): void {
    disposed = true;
    clearPoll();
  }

  onUnmounted(dispose);
  return {
    snapshot: shallowReadonly(snapshot),
    history: shallowReadonly(history),
    error: shallowReadonly(error),
    isStarting: shallowReadonly(isStarting),
    isActive,
    start,
    pause,
    resume,
    cancel,
    refreshHistory,
    dispose,
  };
}
