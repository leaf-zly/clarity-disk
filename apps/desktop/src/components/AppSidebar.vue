<script setup lang="ts">
import type { Component } from "vue";
import {
  Activity,
  Files,
  LayoutDashboard,
  Layers3,
  PanelsTopLeft,
  PieChart,
  RotateCcw,
  Settings2,
  Sparkles,
} from "@lucide/vue";

/**
 * Props controlling the selected application section.
 */
interface Props {
  activeSection: string;
}

/**
 * Events emitted when the user navigates through the application shell.
 */
interface Emits {
  "update:activeSection": [section: string];
}

interface NavigationItem {
  id: string;
  label: string;
  icon: Component;
}

defineProps<Props>();
const emit = defineEmits<Emits>();

const primaryItems: readonly NavigationItem[] = [
  { id: "overview", label: "概览", icon: LayoutDashboard },
  { id: "space", label: "空间分析", icon: PieChart },
  { id: "cleanup", label: "智能清理", icon: Sparkles },
  { id: "large-files", label: "大文件", icon: Files },
];

const diskItems: readonly NavigationItem[] = [
  { id: "partitions", label: "分区管理", icon: PanelsTopLeft },
  { id: "health", label: "磁盘健康", icon: Activity },
];

const secondaryItems: readonly NavigationItem[] = [
  { id: "recovery", label: "恢复中心", icon: RotateCcw },
  { id: "settings", label: "设置", icon: Settings2 },
];
</script>

<template>
  <aside class="sidebar">
    <div class="brand" aria-label="澄盘">
      <span class="brand-mark"><Layers3 :size="18" aria-hidden="true" /></span>
      <span class="brand-name">澄盘</span>
    </div>

    <nav class="navigation" aria-label="主要导航">
      <span class="group-label">管理</span>
      <button
        v-for="item in primaryItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: activeSection === item.id }"
        type="button"
        :aria-current="activeSection === item.id ? 'page' : undefined"
        @click="emit('update:activeSection', item.id)"
      >
        <component :is="item.icon" :size="18" aria-hidden="true" />
        <span>{{ item.label }}</span>
      </button>

      <span class="group-label">磁盘</span>
      <button
        v-for="item in diskItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: activeSection === item.id }"
        type="button"
        :aria-current="activeSection === item.id ? 'page' : undefined"
        @click="emit('update:activeSection', item.id)"
      >
        <component :is="item.icon" :size="18" aria-hidden="true" />
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <nav class="secondary-navigation" aria-label="辅助导航">
      <button
        v-for="item in secondaryItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: activeSection === item.id }"
        type="button"
        :aria-current="activeSection === item.id ? 'page' : undefined"
        @click="emit('update:activeSection', item.id)"
      >
        <component :is="item.icon" :size="18" aria-hidden="true" />
        <span>{{ item.label }}</span>
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
