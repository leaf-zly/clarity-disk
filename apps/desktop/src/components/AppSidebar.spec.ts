import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it } from "vitest";

import AppSidebar from "@/components/AppSidebar.vue";
import { applyLanguagePreference } from "@/services/locale-service";

describe("AppSidebar", () => {
  beforeEach(() => applyLanguagePreference("simplifiedChinese"));

  it("uses localized accessible names and emits the selected destination", async () => {
    const wrapper = mount(AppSidebar, {
      props: { activeSection: "overview" },
    });
    const cleanup = wrapper.get('button[aria-label="智能清理"]');

    await cleanup.trigger("click");

    expect(
      wrapper.get('button[aria-label="概览"]').attributes("aria-current"),
    ).toBe("page");
    expect(wrapper.emitted("update:activeSection")?.at(-1)).toEqual([
      "cleanup",
    ]);
  });

  it("updates accessible names with the selected language", () => {
    applyLanguagePreference("english");
    const wrapper = mount(AppSidebar, {
      props: { activeSection: "settings" },
    });

    expect(wrapper.get('button[aria-label="Settings"]').text()).toBe(
      "Settings",
    );
  });
});
