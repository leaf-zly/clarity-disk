import type { ThemePreference } from "@/types/operations";

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
}
