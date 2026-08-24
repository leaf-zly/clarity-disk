import { afterEach, describe, expect, it } from "vitest";

import {
  applyLanguagePreference,
  currentLanguage,
  translate,
} from "@/services/locale-service";

describe("locale service", () => {
  afterEach(() => applyLanguagePreference("simplifiedChinese"));

  it("applies English copy and document language immediately", () => {
    applyLanguagePreference("english");

    expect(currentLanguage.value).toBe("english");
    expect(document.documentElement.lang).toBe("en");
    expect(translate("settingsTitle")).toBe("Settings & Privacy");
  });
});
