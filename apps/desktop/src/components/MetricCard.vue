<script setup lang="ts">
import type { Component } from "vue";

/**
 * Props for a compact dashboard metric.
 */
interface Props {
  label: string;
  value: string;
  description: string;
  tone: "blue" | "green" | "orange";
  icon: Component;
}

defineProps<Props>();
</script>

<template>
  <article class="metric-card">
    <div class="metric-header">
      <span>{{ label }}</span>
      <span class="metric-icon" :class="tone">
        <component :is="icon" :size="17" aria-hidden="true" />
      </span>
    </div>
    <strong>{{ value }}</strong>
    <p>{{ description }}</p>
  </article>
</template>

<style scoped>
.metric-card {
  min-width: 0;
  padding: 18px;
  border: 1px solid var(--color-border);
  border-radius: 15px;
  background: var(--color-surface);
  box-shadow: 0 6px 20px rgba(29, 35, 46, 0.04);
}

.metric-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: var(--color-text-secondary);
}

.metric-icon {
  width: 32px;
  height: 32px;
  display: grid;
  place-items: center;
  border-radius: 10px;
}

.metric-icon.green {
  color: var(--color-green);
  background: var(--color-green-soft);
}

.metric-icon.blue {
  color: var(--color-blue);
  background: var(--color-blue-soft);
}

.metric-icon.orange {
  color: var(--color-orange);
  background: var(--color-orange-soft);
}

.metric-card strong {
  display: block;
  margin-top: 14px;
  font-size: 1.55rem;
}

.metric-card p {
  margin-top: 5px;
  color: var(--color-text-secondary);
  font-size: 0.87rem;
}
</style>
