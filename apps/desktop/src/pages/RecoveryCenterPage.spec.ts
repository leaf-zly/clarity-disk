import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import RecoveryCenterPage from "@/pages/RecoveryCenterPage.vue";

describe("RecoveryCenterPage", () => {
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
});
