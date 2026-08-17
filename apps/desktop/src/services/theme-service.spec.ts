import { afterEach, describe, expect, it } from "vitest";

import { applyThemePreference } from "@/services/theme-service";

describe("theme service", () => {
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
  });
});
