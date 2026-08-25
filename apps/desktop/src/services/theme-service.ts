import type { ThemePreference } from "@/types/operations";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Applies one validated appearance preference to the complete application.
 *
 * The data attribute selects explicit compatibility variables, while
 * `color-scheme` keeps native controls aligned with the chosen appearance.
 */
export function applyThemePreference(theme: ThemePreference): void {
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.style.colorScheme = theme === "system" ? "light dark" : theme;

  // The browser only controls the document. In the packaged desktop app the
  // native title bar is a separate window surface and must be updated through
  // Tauri; `null` deliberately delegates the system preference to Windows.
  if (isTauri()) {
    void getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .catch(() => {
        // A missing window permission must not prevent the web UI from
        // applying its theme or make settings appear to have failed.
      });
  }
}
