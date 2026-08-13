import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent, h } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useSpaceScan } from "@/composables/use-space-scan";
import {
  getSpaceScan,
  getSpaceScanHistory,
  startSpaceScan,
} from "@/services/dashboard-service";
import type { SpaceScanSnapshot } from "@/types/dashboard";

vi.mock("@/services/dashboard-service", () => ({
  startSpaceScan: vi.fn(),
  getSpaceScan: vi.fn(),
  cancelSpaceScan: vi.fn(),
  pauseSpaceScan: vi.fn(),
  resumeSpaceScan: vi.fn(),
  getSpaceScanHistory: vi.fn(),
}));

const running: SpaceScanSnapshot = {
  progress: {
    scanId: "scan-1",
    status: "scanning",
    scannedItems: 1,
    skippedItems: 0,
    bytesScanned: 1,
    currentPath: "C:\\",
    message: "扫描中",
    percentComplete: 1,
    estimatedSecondsRemaining: 9,
    startedAtUnixMs: 1,
    finishedAtUnixMs: null,
  },
  largestEntries: [],
  fileTypes: [],
};
const completed: SpaceScanSnapshot = {
  ...running,
  progress: {
    ...running.progress,
    status: "completed",
    message: "完成",
    percentComplete: 100,
    estimatedSecondsRemaining: 0,
    finishedAtUnixMs: 2,
  },
};

describe("useSpaceScan", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(startSpaceScan).mockResolvedValue({ scanId: "scan-1" });
    vi.mocked(getSpaceScanHistory).mockResolvedValue([]);
  });

  afterEach(() => vi.useRealTimers());

  it("polls until terminal state and refreshes history once", async () => {
    vi.mocked(getSpaceScan)
      .mockResolvedValueOnce(running)
      .mockResolvedValueOnce(completed);
    let scan: ReturnType<typeof useSpaceScan> | undefined;
    const Host = defineComponent({
      setup() {
        scan = useSpaceScan({ pollIntervalMs: 100 });
        return () => h("div");
      },
    });
    mount(Host);

    await scan!.start({
      rootPath: "C:\\",
      maxDepth: 8,
      maxEntries: 1000,
      excludedPaths: [],
    });
    expect(getSpaceScan).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(100);
    await flushPromises();

    expect(getSpaceScan).toHaveBeenCalledTimes(2);
    expect(getSpaceScanHistory).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(500);
    expect(getSpaceScan).toHaveBeenCalledTimes(2);
  });

  it("clears pending polling when the consumer unmounts", async () => {
    vi.mocked(getSpaceScan).mockResolvedValue(running);
    let scan: ReturnType<typeof useSpaceScan> | undefined;
    const Host = defineComponent({
      setup() {
        scan = useSpaceScan({ pollIntervalMs: 100 });
        return () => h("div");
      },
    });
    const wrapper = mount(Host);
    await scan!.start({
      rootPath: "C:\\",
      maxDepth: 8,
      maxEntries: 1000,
      excludedPaths: [],
    });
    wrapper.unmount();
    await vi.advanceTimersByTimeAsync(500);

    expect(getSpaceScan).toHaveBeenCalledTimes(1);
  });
});
