import { invoke, isTauri } from "@tauri-apps/api/core";

import { dashboardFixture } from "@/testing/dashboard-fixture";
import type { DashboardSnapshot } from "@/types/dashboard";

/**
 * Loads the dashboard snapshot from the Tauri command layer. Browser-only
 * development uses a cloned fixture so the Vue interface remains independently
 * testable without privileged or platform-specific dependencies.
 *
 * @returns The current dashboard snapshot.
 */
export async function loadDashboardSnapshot(): Promise<DashboardSnapshot> {
  if (isTauri()) {
    return invoke<DashboardSnapshot>("get_dashboard_snapshot");
  }

  return structuredClone(dashboardFixture);
}
