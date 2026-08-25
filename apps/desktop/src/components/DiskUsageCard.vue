<script setup lang="ts">
import { computed } from "vue";
import { HardDrive, Sparkles } from "@lucide/vue";

import type { DiskCategory, DiskSummary } from "@/types/dashboard";
import { formatBytes } from "@/utils/format-bytes";

/**
 * Props required to render primary disk usage and cleanup opportunity.
 */
interface Props {
  disk: DiskSummary;
  /** Evidence-backed reclaimable bytes; null means the scan is unavailable. */
  reclaimableBytes: number | null;
}

/**
 * Events emitted from the disk usage card.
 */
interface Emits {
  "open-cleanup": [];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();

const availableBytes = computed(() =>
  Math.max(props.disk.totalBytes - props.disk.usedBytes, 0),
);
const usedPercentage = computed(() =>
  props.disk.totalBytes === 0
    ? 0
    : (props.disk.usedBytes / props.disk.totalBytes) * 100,
);

/**
 * Converts category capacity into a percentage of total disk capacity. Using the
 * total instead of categorized bytes preserves the visible uncategorized/free gap.
 */
function categoryWidth(category: DiskCategory): string {
  if (props.disk.totalBytes === 0) {
    return "0%";
  }

  return Math.min((category.bytes / props.disk.totalBytes) * 100, 100) + "%";
}
</script>

<template>
  <section class="disk-card" aria-labelledby="disk-card-title">
    <div class="capacity-panel">
      <div class="disk-label">
        <HardDrive :size="19" aria-hidden="true" />
        <span id="disk-card-title">{{ disk.label }}</span>
      </div>

      <div class="capacity-heading">
        <strong>{{ formatBytes(disk.usedBytes) }}</strong>
        <span>已使用 / {{ formatBytes(disk.totalBytes) }}</span>
      </div>

      <div
        class="usage-bar"
        role="meter"
        aria-label="磁盘已使用空间"
        :aria-valuenow="usedPercentage.toFixed(1)"
        aria-valuemin="0"
        aria-valuemax="100"
      >
        <span
          v-for="category in disk.categories"
          :key="category.kind"
          class="usage-segment"
          :class="category.kind"
          :style="{ width: categoryWidth(category) }"
        />
      </div>

      <div class="legend">
        <span
          v-for="category in disk.categories"
          :key="category.kind"
          class="legend-item"
        >
          <span class="legend-dot" :class="category.kind" />
          {{ category.label }} {{ formatBytes(category.bytes, 1) }}
        </span>
      </div>
      <p v-if="disk.categories.length === 0" class="classification-note">
        已使用空间尚未分类；运行空间分析后查看目录明细
      </p>

      <div class="available-row">
        <span>可用空间</span>
        <strong
          >{{ formatBytes(availableBytes) }} ·
          {{ (100 - usedPercentage).toFixed(1) }}%</strong
        >
      </div>
    </div>

    <aside class="cleanup-panel">
      <div>
        <div class="cleanup-label">
          <span>智能清理建议</span>
          <Sparkles :size="18" aria-hidden="true" />
        </div>
        <strong class="cleanup-value">{{
          reclaimableBytes === null ? "—" : formatBytes(reclaimableBytes)
        }}</strong>
        <p>
          {{
            reclaimableBytes === null
              ? "完成清理扫描后显示"
              : "缓存与临时内容，可安全释放"
          }}
        </p>
      </div>
      <button type="button" @click="emit('open-cleanup')">查看清理项目</button>
    </aside>
  </section>
</template>

<style scoped>
.disk-card {
  position: relative;
  overflow: hidden;
  display: grid;
  grid-template-columns: minmax(0, 1.15fr) minmax(260px, 0.85fr);
  gap: 26px;
  padding: 26px;
  border: 1px solid var(--color-border);
  border-radius: 20px;
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
}

.disk-card::after {
  content: "";
  position: absolute;
  top: -140px;
  right: -110px;
  width: 280px;
  height: 280px;
  border-radius: 50%;
  background: radial-gradient(
    circle,
    color-mix(in srgb, var(--color-blue) 18%, transparent),
    transparent 68%
  );
  pointer-events: none;
}

.capacity-panel,
.cleanup-panel {
  position: relative;
  z-index: 1;
}

.disk-label,
.cleanup-label {
  display: flex;
  align-items: center;
  gap: 9px;
  color: var(--color-text-secondary);
}

.disk-label svg,
.cleanup-label {
  color: var(--color-blue);
}

.capacity-heading {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin: 14px 0 18px;
}

.capacity-heading strong,
.cleanup-value {
  font-size: clamp(1.8rem, 3vw, 2.5rem);
  letter-spacing: -0.04em;
}

.capacity-heading span {
  color: var(--color-text-secondary);
}

.usage-bar {
  height: 11px;
  overflow: hidden;
  display: flex;
  border: 1px solid var(--color-border);
  border-radius: 999px;
  background: var(--color-surface-muted);
}

.usage-segment {
  height: 100%;
}

.applications {
  background: var(--color-blue);
}

.system {
  background: var(--color-purple);
}

.files {
  background: var(--color-green);
}

.development {
  background: var(--color-orange);
}

.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 18px;
  margin-top: 16px;
  color: var(--color-text-secondary);
  font-size: 0.85rem;
}

.legend-item {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}

.classification-note {
  margin-top: 14px;
  color: var(--color-text-secondary);
  font-size: 0.82rem;
}

.legend-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
}

.available-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  margin-top: 18px;
  padding-top: 16px;
  border-top: 1px solid var(--color-border);
  color: var(--color-text-secondary);
}

.available-row strong {
  color: var(--color-text);
}

.cleanup-panel {
  min-height: 220px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 21px;
  border: 1px solid
    color-mix(in srgb, var(--color-blue) 17%, var(--color-border));
  border-radius: 16px;
  background: linear-gradient(
    145deg,
    var(--color-blue-soft),
    color-mix(in srgb, var(--color-surface) 82%, var(--color-blue-soft))
  );
}

.cleanup-value {
  display: block;
  margin: 20px 0 5px;
}

.cleanup-panel p {
  color: var(--color-text-secondary);
}

.cleanup-panel button {
  width: 100%;
  min-height: 42px;
  margin-top: 22px;
  border: 0;
  border-radius: 11px;
  color: white;
  background: linear-gradient(180deg, #2584ff, #126ae5);
  box-shadow:
    0 7px 18px rgba(18, 106, 229, 0.2),
    inset 0 1px 0 rgba(255, 255, 255, 0.28);
  font-weight: 600;
  cursor: pointer;
}

.cleanup-panel button:hover {
  filter: brightness(1.05);
}

@media (max-width: 1080px) {
  .disk-card {
    grid-template-columns: 1fr;
  }

  .cleanup-panel {
    min-height: 190px;
  }
}
</style>
