<script setup lang="ts">
import { ref } from "vue";

import AppSidebar from "@/components/AppSidebar.vue";
import DashboardPage from "@/pages/DashboardPage.vue";
import DiskHealthPage from "@/pages/DiskHealthPage.vue";
import PartitionSafetyPage from "@/pages/PartitionSafetyPage.vue";
import PartitionPreviewPage from "@/pages/PartitionPreviewPage.vue";
import SystemMaintenancePage from "@/pages/SystemMaintenancePage.vue";

const activeSection = ref("overview");
</script>

<template>
  <div class="app-shell">
    <AppSidebar v-model:active-section="activeSection" />
    <main class="app-content">
      <PartitionPreviewPage v-if="activeSection === 'partitions'" />
      <DiskHealthPage v-else-if="activeSection === 'health'" />
      <PartitionSafetyPage v-else-if="activeSection === 'partition-safety'" />
      <SystemMaintenancePage v-else-if="activeSection === 'maintenance'" />
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
