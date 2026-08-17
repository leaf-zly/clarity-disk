/** Dashboard subsections rendered by the shared overview workspace. */
export type DashboardSection = "overview" | "space" | "cleanup" | "large-files";

/** Stable identifiers for every page exposed by the application sidebar. */
export type AppSection =
  | DashboardSection
  | "partitions"
  | "partition-safety"
  | "health"
  | "maintenance"
  | "recovery"
  | "history"
  | "settings";

/** Returns whether a sidebar destination belongs to the shared dashboard. */
export function isDashboardSection(
  section: AppSection,
): section is DashboardSection {
  return ["overview", "space", "cleanup", "large-files"].includes(section);
}
