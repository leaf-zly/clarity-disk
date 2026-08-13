import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import SpaceScanPanel from "@/components/SpaceScanPanel.vue";
import type { SpaceScanSnapshot } from "@/types/dashboard";

function snapshot(
  status: "scanning" | "paused" | "completed",
): SpaceScanSnapshot {
  return {
    progress: {
      scanId: "scan-1",
      status,
      scannedItems: 320,
      skippedItems: 4,
      bytesScanned: 1024 * 1024,
      currentPath: "C:\\data\\active",
      message:
        status === "paused"
          ? "扫描已暂停"
          : status === "completed"
            ? "空间扫描完成（仅读取）"
            : "正在分析目录空间",
      percentComplete: status === "completed" ? 100 : 32,
      estimatedSecondsRemaining: status === "completed" ? 0 : 12,
      startedAtUnixMs: 1,
      finishedAtUnixMs: status === "completed" ? 2 : null,
    },
    largestEntries: [
      {
        path: "C:\\data\\video.iso",
        bytes: 1024 * 1024,
        itemCount: 1,
        kind: "file",
      },
    ],
    fileTypes: [{ fileType: ".iso", bytes: 1024 * 1024, itemCount: 1 }],
  };
}

function mountPanel(
  status: "scanning" | "paused" | "completed" | undefined = undefined,
) {
  return mount(SpaceScanPanel, {
    props: {
      snapshot: status ? snapshot(status) : undefined,
      history: [],
      error: undefined,
      isStarting: false,
      scanRoot: "C:\\data",
      maxDepth: 8,
      maxEntries: 100_000,
      excludedPaths: "C:\\data\\documents",
    },
  });
}

describe("SpaceScanPanel", () => {
  it("emits the configured request action and configuration updates", async () => {
    const wrapper = mountPanel();
    await wrapper.get('input[type="text"]').setValue("D:\\media");
    await wrapper.get("button.start-button").trigger("click");

    expect(wrapper.emitted("update:scan-root")?.[0]).toEqual(["D:\\media"]);
    expect(wrapper.emitted("start-scan")).toHaveLength(1);
    expect(wrapper.text()).toContain("全程只读");
  });

  it("offers pause and cancel while scanning, then resume while paused", async () => {
    const scanning = mountPanel("scanning");
    expect(scanning.text()).toContain("暂停");
    expect(scanning.text()).toContain("取消");
    expect(scanning.text()).toContain("32%");
    expect(scanning.text()).toContain("video.iso");
    expect(scanning.text()).toContain(".iso");
    await scanning.get("button.secondary-button").trigger("click");
    expect(scanning.emitted("pause-scan")).toHaveLength(1);

    const paused = mountPanel("paused");
    expect(paused.text()).toContain("继续");
    await paused.get("button.secondary-button").trigger("click");
    expect(paused.emitted("resume-scan")).toHaveLength(1);
  });

  it("renders persisted history and a user-readable error", () => {
    const wrapper = mount(SpaceScanPanel, {
      props: {
        snapshot: snapshot("completed"),
        history: [
          {
            scanId: "old",
            rootPath: "D:\\archive",
            status: "completed",
            scannedItems: 8,
            bytesScanned: 2048,
            startedAtUnixMs: 1,
            finishedAtUnixMs: 2,
          },
        ],
        error: "扫描历史暂时无法读取。",
        isStarting: false,
        scanRoot: "C:\\data",
        maxDepth: 8,
        maxEntries: 100_000,
        excludedPaths: "",
      },
    });

    expect(wrapper.get('[role="alert"]').text()).toContain("扫描历史");
    expect(wrapper.text()).toContain("D:\\archive");
    expect(wrapper.text()).toContain("已完成");
  });
});
