<script setup lang="ts">
import { computed } from "vue";
import { BarChart3, CircleStop, FolderSearch, Play } from "@lucide/vue";

import type { SpaceScanSnapshot } from "@/types/dashboard";
import { formatBytes } from "@/utils/format-bytes";

/** Props for the read-only space scan task panel. */
interface Props {
  snapshot: SpaceScanSnapshot | undefined;
  isStarting: boolean;
}

/** User actions for starting and cancelling a scan. */
interface Emits {
  "start-scan": [];
  "cancel-scan": [];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const isActive = computed(() => props.snapshot?.progress.status === "scanning");
</script>

<template>
  <section class="space-scan-card" aria-labelledby="space-scan-title">
    <div class="space-heading">
      <div class="space-icon"><BarChart3 :size="18" aria-hidden="true" /></div>
      <div>
        <h2 id="space-scan-title">空间分析</h2>
        <p>只读分析目录占用，不会修改文件</p>
      </div>
      <button
        v-if="isActive"
        type="button"
        class="stop-button"
        @click="emit('cancel-scan')"
      >
        <CircleStop :size="16" aria-hidden="true" />取消扫描
      </button>
      <button
        v-else
        type="button"
        class="start-button"
        :disabled="isStarting"
        @click="emit('start-scan')"
      >
        <Play :size="16" aria-hidden="true" />{{
          isStarting ? "准备中" : "开始分析"
        }}
      </button>
    </div>

    <div v-if="snapshot" class="space-content">
      <div class="progress-row">
        <div class="progress-track">
          <span :style="{ width: isActive ? '42%' : '100%' }" />
        </div>
        <strong>{{ snapshot.progress.scannedItems.toLocaleString() }}</strong>
        <span>项</span>
      </div>
      <p class="scan-message">
        {{ snapshot.progress.message
        }}<span v-if="snapshot.progress.currentPath">
          · {{ snapshot.progress.currentPath }}</span
        >
      </p>
      <div class="scan-stats">
        <span>已读取 {{ formatBytes(snapshot.progress.bytesScanned) }}</span
        ><span
          >跳过 {{ snapshot.progress.skippedItems.toLocaleString() }} 项</span
        >
      </div>
      <div v-if="snapshot.largestEntries.length" class="largest-list">
        <div
          v-for="entry in snapshot.largestEntries.slice(0, 3)"
          :key="entry.path"
          class="largest-row"
        >
          <FolderSearch :size="15" aria-hidden="true" /><code>{{
            entry.path
          }}</code
          ><strong>{{ formatBytes(entry.bytes) }}</strong>
        </div>
      </div>
    </div>
    <div v-else class="space-empty">选择一个范围开始分析最大文件和目录。</div>
  </section>
</template>

<style scoped>
.space-scan-card {
  margin-top: 14px;
  padding: 20px 21px;
  border: 1px solid var(--color-border);
  border-radius: 16px;
  background: var(--color-surface);
}
.space-heading,
.progress-row,
.scan-stats,
.largest-row {
  display: flex;
  align-items: center;
}
.space-heading {
  gap: 11px;
}
.space-icon {
  display: grid;
  place-items: center;
  width: 34px;
  height: 34px;
  border-radius: 9px;
  color: var(--color-blue);
  background: var(--color-blue-soft);
}
.space-heading h2 {
  font-size: 1rem;
}
.space-heading p,
.scan-message,
.scan-stats,
.space-empty {
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}
.space-heading p {
  margin-top: 4px;
}
.start-button,
.stop-button {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 7px 11px;
  border: 0;
  border-radius: 8px;
  cursor: pointer;
}
.start-button {
  color: white;
  background: var(--color-blue);
}
.stop-button {
  color: #c24141;
  background: #fff1f1;
}
.start-button:disabled {
  opacity: 0.6;
  cursor: progress;
}
.space-content {
  margin-top: 16px;
}
.progress-row {
  gap: 8px;
}
.progress-track {
  height: 7px;
  flex: 1;
  overflow: hidden;
  border-radius: 99px;
  background: var(--color-surface-muted);
}
.progress-track span {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: var(--color-blue);
  transition: width 0.25s ease;
}
.progress-row > span {
  color: var(--color-text-secondary);
  font-size: 0.75rem;
}
.scan-message {
  margin-top: 8px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.scan-stats {
  justify-content: space-between;
  margin-top: 8px;
}
.largest-list {
  display: grid;
  gap: 6px;
  margin-top: 13px;
}
.largest-row {
  gap: 7px;
  min-width: 0;
  padding: 8px 9px;
  border-radius: 8px;
  background: var(--color-surface-muted);
}
.largest-row code {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: inherit;
  color: var(--color-text-secondary);
  font-size: 0.73rem;
}
.largest-row strong {
  font-size: 0.76rem;
}
.space-empty {
  padding-top: 16px;
}
</style>
