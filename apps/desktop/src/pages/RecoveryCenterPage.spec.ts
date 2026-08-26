import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import RecoveryCenterPage from "@/pages/RecoveryCenterPage.vue";
import { getExecutionQuarantineIndex } from "@/services/cleanup-execution-service";

vi.mock("@/services/cleanup-execution-service", async (importOriginal) => ({
  ...(await importOriginal<
    typeof import("@/services/cleanup-execution-service")
  >()),
  getExecutionQuarantineIndex: vi.fn(),
}));

describe("RecoveryCenterPage", () => {
  beforeEach(() => {
    vi.mocked(getExecutionQuarantineIndex).mockResolvedValue(null);
  });

  it("keeps irreversible deletion behind a disabled explicit action without entries", async () => {
    const wrapper = mount(RecoveryCenterPage);
    await flushPromises();

    expect(wrapper.get("h1").text()).toBe("恢复中心");
    expect(wrapper.text()).toContain("当前没有可恢复项目");
    const deleteButton = wrapper
      .findAll("button")
      .find((button) => button.text().includes("永久删除"));
    expect(deleteButton?.attributes("disabled")).toBeDefined();
    expect(wrapper.text()).toContain("到期仍可恢复");
  });

  it("surfaces a recoverable error when quarantine state cannot be read", async () => {
    vi.mocked(getExecutionQuarantineIndex).mockRejectedValueOnce(
      new Error("backend unavailable"),
    );

    const wrapper = mount(RecoveryCenterPage);
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toBe(
      "隔离区状态暂时无法读取，请稍后重试。",
    );
    expect(wrapper.text()).not.toContain("当前没有可恢复项目");
    expect(
      wrapper.get("button.secondary").attributes("disabled"),
    ).toBeUndefined();
  });
});
