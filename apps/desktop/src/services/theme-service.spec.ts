import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises } from "@vue/test-utils";
import { isTauri } from "@tauri-apps/api/core";
import capability from "../../src-tauri/capabilities/default.json";

import { applyThemePreference } from "@/services/theme-service";

const { setTheme } = vi.hoisted(() => ({ setTheme: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTheme }),
}));

describe("theme service", () => {
  beforeEach(() => {
    vi.mocked(isTauri).mockReturnValue(false);
    setTheme.mockReset().mockResolvedValue(undefined);
  });
  afterEach(() => {
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.removeProperty("color-scheme");
  });

  it.each([
    ["light", "light"],
    ["dark", "dark"],
    ["system", "light dark"],
  ] as const)("applies %s across the document", (theme, colorScheme) => {
    applyThemePreference(theme);

    expect(document.documentElement.dataset.theme).toBe(theme);
    expect(document.documentElement.style.colorScheme).toBe(colorScheme);
    expect(setTheme).not.toHaveBeenCalled();
  });

  it.each(["light", "dark", "system"] as const)(
    "synchronizes native %s appearance",
    async (theme) => {
      vi.mocked(isTauri).mockReturnValue(true);
      applyThemePreference(theme);
      await flushPromises();
      expect(setTheme).toHaveBeenCalledWith(theme === "system" ? null : theme);
    },
  );

  it("grants native theme changes only to the main window", () => {
    expect(capability.windows).toEqual(["main"]);
    expect(capability.permissions).toContain("core:window:allow-set-theme");
  });

  it("keeps document appearance usable if the native call fails", async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    setTheme.mockRejectedValueOnce(new Error("native unavailable"));
    applyThemePreference("dark");
    await flushPromises();
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
