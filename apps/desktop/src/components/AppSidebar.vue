<script setup lang="ts">
import {
  Activity,
  Files,
  History,
  LayoutDashboard,
  Layers3,
  PanelsTopLeft,
  PieChart,
  RotateCcw,
  Settings2,
  ShieldCheck,
  Sparkles,
  Wrench,
} from "@lucide/vue";
import type { AppSection } from "@/types/navigation";
import { translate } from "@/services/locale-service";

/**
 * Props controlling the selected application section.
 */
interface Props {
  activeSection: AppSection;
}

/**
 * Events emitted when the user navigates through the application shell.
 */
interface Emits {
  "update:activeSection": [section: AppSection];
}

defineProps<Props>();
const emit = defineEmits<Emits>();

const primaryItems = [
  { id: "overview", label: "overview", icon: LayoutDashboard },
  { id: "space", label: "space", icon: PieChart },
  { id: "cleanup", label: "cleanup", icon: Sparkles },
  { id: "large-files", label: "largeFiles", icon: Files },
] as const;

const diskItems = [
  { id: "partitions", label: "partitions", icon: PanelsTopLeft },
  { id: "partition-safety", label: "partitionSafety", icon: ShieldCheck },
  { id: "health", label: "health", icon: Activity },
] as const;

const secondaryItems = [
  { id: "maintenance", label: "maintenance", icon: Wrench },
  { id: "recovery", label: "recovery", icon: RotateCcw },
  { id: "history", label: "history", icon: History },
  { id: "settings", label: "settings", icon: Settings2 },
] as const;
</script>

<template>
  <aside class="sidebar">
    <div class="brand" aria-label="Clarity Disk">
      <span class="brand-mark"><Layers3 :size="18" aria-hidden="true" /></span>
      <span class="brand-name">{{ translate("brand") }}</span>
    </div>

    <nav class="navigation" aria-label="主要导航">
      <span class="group-label">{{ translate("manage") }}</span>
      <button
        v-for="item in primaryItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: activeSection === item.id }"
        type="button"
        :aria-label="translate(item.label)"
        :aria-current="activeSection === item.id ? 'page' : undefined"
        @click="emit('update:activeSection', item.id)"
      >
        <component :is="item.icon" :size="18" aria-hidden="true" />
        <span>{{ translate(item.label) }}</span>
      </button>

      <span class="group-label">{{ translate("disk") }}</span>
      <button
        v-for="item in diskItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: activeSection === item.id }"
        type="button"
        :aria-label="translate(item.label)"
        :aria-current="activeSection === item.id ? 'page' : undefined"
        @click="emit('update:activeSection', item.id)"
      >
        <component :is="item.icon" :size="18" aria-hidden="true" />
        <span>{{ translate(item.label) }}</span>
      </button>
    </nav>

    <nav class="secondary-navigation" aria-label="辅助导航">
      <button
        v-for="item in secondaryItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: activeSection === item.id }"
        type="button"
        :aria-label="translate(item.label)"
        :aria-current="activeSection === item.id ? 'page' : undefined"
        @click="emit('update:activeSection', item.id)"
      >
        <component :is="item.icon" :size="18" aria-hidden="true" />
        <span>{{ translate(item.label) }}</span>
      </button>
    </nav>
  </aside>
</template>

<style scoped>
.sidebar {
  position: sticky;
  top: 0;
  height: 100vh;
  display: flex;
  flex-direction: column;
  padding: 22px 14px 16px;
  background: var(--color-sidebar);
  border-right: 1px solid var(--color-border);
  backdrop-filter: blur(24px) saturate(145%);
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 42px;
  padding: 0 10px;
  margin-bottom: 18px;
}

.brand-mark {
  width: 30px;
  height: 30px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  color: white;
  border-radius: 9px;
  background: linear-gradient(145deg, #4d9cff, #2068ed 62%, #1555ca);
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.35),
    0 6px 16px rgba(24, 105, 230, 0.2);
}

.brand-name {
  font-size: 1.05rem;
  font-weight: 600;
}

.navigation,
.secondary-navigation {
  display: grid;
  gap: 4px;
}

.secondary-navigation {
  margin-top: auto;
}

.group-label {
  padding: 14px 10px 6px;
  color: var(--color-text-secondary);
  font-size: 0.75rem;
}

.nav-item {
  width: 100%;
  min-height: 40px;
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 8px 11px;
  border: 0;
  border-radius: 10px;
  color: var(--color-text);
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.nav-item svg {
  flex: 0 0 auto;
  color: var(--color-text-secondary);
}

.nav-item:hover {
  background: color-mix(in srgb, var(--color-text) 5%, transparent);
}

.nav-item.active {
  background: var(--color-surface);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
  font-weight: 600;
}

.nav-item.active svg {
  color: var(--color-blue);
}

@media (max-width: 900px) {
  .sidebar {
    padding-inline: 10px;
  }

  .brand {
    justify-content: center;
    padding: 0;
  }

  .brand-name,
  .group-label,
  .nav-item span {
    display: none;
  }

  .nav-item {
    justify-content: center;
    padding: 9px;
  }
}
</style>
