<script setup lang="ts">
import { ref } from "vue";

import AppSidebar from "@/components/AppSidebar.vue";
import DashboardPage from "@/pages/DashboardPage.vue";

const activeSection = ref("overview");
</script>

<template>
  <div class="app-shell">
    <AppSidebar v-model:active-section="activeSection" />
    <main class="app-content">
      <DashboardPage />
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
