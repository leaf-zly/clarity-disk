<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

import AppSidebar from "@/components/AppSidebar.vue";
import DashboardPage from "@/pages/DashboardPage.vue";
import ActivityHistoryPage from "@/pages/ActivityHistoryPage.vue";
import DiskHealthPage from "@/pages/DiskHealthPage.vue";
import PartitionSafetyPage from "@/pages/PartitionSafetyPage.vue";
import PartitionPreviewPage from "@/pages/PartitionPreviewPage.vue";
import SystemMaintenancePage from "@/pages/SystemMaintenancePage.vue";
import RecoveryCenterPage from "@/pages/RecoveryCenterPage.vue";
import SettingsPage from "@/pages/SettingsPage.vue";
import { runAutomaticMaintenance } from "@/services/operations-service";

const activeSection = ref("overview");
let maintenanceTimer: ReturnType<typeof setInterval> | undefined;

onMounted(() => {
  void runAutomaticMaintenance().catch(() => {
    // Unknown power/update/backup evidence safely blocks the read-only tick.
  });
  maintenanceTimer = setInterval(
    () => {
      void runAutomaticMaintenance().catch(() => undefined);
    },
    15 * 60 * 1000,
  );
});

onBeforeUnmount(() => {
  if (maintenanceTimer) clearInterval(maintenanceTimer);
});
</script>

<template>
  <div class="app-shell">
    <AppSidebar v-model:active-section="activeSection" />
    <main class="app-content">
      <PartitionPreviewPage v-if="activeSection === 'partitions'" />
      <DiskHealthPage v-else-if="activeSection === 'health'" />
      <PartitionSafetyPage v-else-if="activeSection === 'partition-safety'" />
      <SystemMaintenancePage v-else-if="activeSection === 'maintenance'" />
      <RecoveryCenterPage v-else-if="activeSection === 'recovery'" />
      <ActivityHistoryPage v-else-if="activeSection === 'history'" />
      <SettingsPage v-else-if="activeSection === 'settings'" />
      <DashboardPage v-else />
    </main>
  </div>
</template>

<style scoped>
.app-shell {
  min-height: 100vh;
  display: grid;
  grid-template-columns: 224px minmax(0, 1fr);
  background: var(--color-canvas);
}

.app-content {
  min-width: 0;
  padding: 34px 38px 44px;
}

@media (max-width: 900px) {
  .app-shell {
    grid-template-columns: 76px minmax(0, 1fr);
  }

  .app-content {
    padding: 28px 24px 36px;
  }
}
</style>
