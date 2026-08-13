<script setup lang="ts">
import { computed } from "vue";
import {
  BarChart3,
  CircleStop,
  Clock3,
  FileChartColumn,
  FolderSearch,
  History,
  Pause,
  Play,
  RotateCcw,
  ShieldCheck,
} from "@lucide/vue";

import type {
  SpaceScanHistoryEntry,
  SpaceScanSnapshot,
} from "@/types/dashboard";
import { formatBytes } from "@/utils/format-bytes";

/** Props for configuring and observing the bounded read-only scan. */
interface Props {
  snapshot: SpaceScanSnapshot | undefined;
  history: SpaceScanHistoryEntry[];
  error: string | undefined;
  isStarting: boolean;
  scanRoot: string;
  maxDepth: number;
  maxEntries: number;
  excludedPaths: string;
}

/** Type-safe user actions and configuration updates emitted by the panel. */
interface Emits {
  "start-scan": [];
  "pause-scan": [];
  "resume-scan": [];
  "cancel-scan": [];
  "update:scan-root": [value: string];
  "update:max-depth": [value: number];
  "update:max-entries": [value: number];
  "update:excluded-paths": [value: string];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const status = computed(() => props.snapshot?.progress.status ?? "idle");
const isScanning = computed(() => status.value === "scanning");
const isPaused = computed(() => status.value === "paused");
const isActive = computed(() => isScanning.value || isPaused.value);
const canStart = computed(
  () => props.scanRoot.trim().length > 0 && !isActive.value,
);
const progress = computed(() => props.snapshot?.progress.percentComplete ?? 0);
const etaLabel = computed(() => {
  const seconds = props.snapshot?.progress.estimatedSecondsRemaining;
  if (seconds == null) return "正在估算";
  if (seconds < 60) return `约 ${seconds} 秒`;
  return `约 ${Math.ceil(seconds / 60)} 分钟`;
});

function numberValue(event: Event): number {
  return Number((event.target as HTMLInputElement).value);
}

function historyStatus(item: SpaceScanHistoryEntry): string {
  return item.status === "completed"
    ? "已完成"
    : item.status === "cancelled"
      ? "已取消"
      : "失败";
}
</script>

<template>
  <section class="space-scan-card" aria-labelledby="space-scan-title">
    <header class="space-heading">
      <div class="space-title">
        <span class="space-icon"
          ><BarChart3 :size="18" aria-hidden="true"
        /></span>
        <div>
          <h2 id="space-scan-title">空间分析</h2>
          <p>
            <ShieldCheck
              :size="13"
              aria-hidden="true"
            />全程只读，不删除、不移动文件
          </p>
        </div>
      </div>
      <div class="task-actions">
        <button
          v-if="isScanning"
          type="button"
          class="secondary-button"
          @click="emit('pause-scan')"
        >
          <Pause :size="16" aria-hidden="true" />暂停
        </button>
        <button
          v-if="isPaused"
          type="button"
          class="secondary-button"
          @click="emit('resume-scan')"
        >
          <RotateCcw :size="16" aria-hidden="true" />继续
        </button>
        <button
          v-if="isActive"
          type="button"
          class="stop-button"
          @click="emit('cancel-scan')"
        >
          <CircleStop :size="16" aria-hidden="true" />取消
        </button>
        <button
          v-else
          type="button"
          class="start-button"
          :disabled="isStarting || !canStart"
          @click="emit('start-scan')"
        >
          <Play :size="16" aria-hidden="true" />{{
            isStarting ? "准备中" : "开始分析"
          }}
        </button>
      </div>
    </header>

    <div class="scope-panel" :class="{ disabled: isActive }">
      <label class="field field-wide">
        <span>扫描范围</span>
        <input
          :value="scanRoot"
          type="text"
          :disabled="isActive"
          spellcheck="false"
          aria-describedby="scope-hint"
          @input="
            emit('update:scan-root', ($event.target as HTMLInputElement).value)
          "
        />
      </label>
      <label class="field">
        <span>最大深度</span>
        <input
          :value="maxDepth"
          type="number"
          min="0"
          max="64"
          :disabled="isActive"
          @input="emit('update:max-depth', numberValue($event))"
        />
      </label>
      <label class="field">
        <span>最大条目</span>
        <select
          :value="maxEntries"
          :disabled="isActive"
          @change="emit('update:max-entries', numberValue($event))"
        >
          <option :value="10000">10,000</option>
          <option :value="100000">100,000</option>
          <option :value="500000">500,000</option>
          <option :value="1000000">1,000,000</option>
        </select>
      </label>
      <label class="field excluded-field">
        <span>排除目录 <small>每行一个，且必须位于扫描范围内</small></span>
        <textarea
          :value="excludedPaths"
          rows="2"
          :disabled="isActive"
          spellcheck="false"
          placeholder="例如 C:\Users\你的用户名\Documents"
          @input="
            emit(
              'update:excluded-paths',
              ($event.target as HTMLTextAreaElement).value,
            )
          "
        />
      </label>
      <p id="scope-hint" class="scope-hint">
        进度和剩余时间依据条目上限估算；受权限或文件变化影响的项目会安全跳过。
      </p>
    </div>

    <p v-if="error" class="scan-error" role="alert">{{ error }}</p>

    <div v-if="snapshot" class="space-content" aria-live="polite">
      <div class="progress-copy">
        <div>
          <strong>{{ snapshot.progress.message }}</strong>
          <span
            >{{ progress }}% ·
            {{ snapshot.progress.scannedItems.toLocaleString() }} 项</span
          >
        </div>
        <span v-if="isActive" class="eta"
          ><Clock3 :size="14" aria-hidden="true" />预计剩余
          {{ etaLabel }}（估算）</span
        >
      </div>
      <div
        class="progress-track"
        role="progressbar"
        aria-label="扫描进度估算"
        aria-valuemin="0"
        aria-valuemax="100"
        :aria-valuenow="progress"
      >
        <span :style="{ width: `${progress}%` }" />
      </div>
      <p
        v-if="snapshot.progress.currentPath"
        class="current-path"
        :title="snapshot.progress.currentPath"
      >
        {{ snapshot.progress.currentPath }}
      </p>

      <div class="scan-stats">
        <span
          ><strong>{{ formatBytes(snapshot.progress.bytesScanned) }}</strong
          >已读取</span
        >
        <span
          ><strong>{{ snapshot.progress.skippedItems.toLocaleString() }}</strong
          >已安全跳过</span
        >
        <span
          ><strong>{{ snapshot.largestEntries.length }}</strong
          >大文件与目录</span
        >
        <span
          ><strong>{{ snapshot.fileTypes.length }}</strong
          >文件类型</span
        >
      </div>

      <div class="result-grid">
        <section class="result-panel" aria-labelledby="largest-title">
          <h3 id="largest-title">
            <FolderSearch :size="16" aria-hidden="true" />最大项目
          </h3>
          <div v-if="snapshot.largestEntries.length" class="result-list">
            <div
              v-for="entry in snapshot.largestEntries.slice(0, 8)"
              :key="`${entry.kind}-${entry.path}`"
              class="result-row"
            >
              <span class="result-path" :title="entry.path">{{
                entry.path
              }}</span>
              <small>{{
                entry.kind === "directory"
                  ? `${entry.itemCount.toLocaleString()} 个文件`
                  : "文件"
              }}</small>
              <strong>{{ formatBytes(entry.bytes) }}</strong>
            </div>
          </div>
          <p v-else class="empty-copy">扫描后将在这里显示结果。</p>
        </section>

        <section class="result-panel" aria-labelledby="types-title">
          <h3 id="types-title">
            <FileChartColumn :size="16" aria-hidden="true" />文件类型
          </h3>
          <div v-if="snapshot.fileTypes.length" class="type-list">
            <div
              v-for="item in snapshot.fileTypes.slice(0, 8)"
              :key="item.fileType"
              class="type-row"
            >
              <code>{{ item.fileType }}</code>
              <span>{{ item.itemCount.toLocaleString() }} 项</span>
              <strong>{{ formatBytes(item.bytes) }}</strong>
            </div>
          </div>
          <p v-else class="empty-copy">暂无文件类型统计。</p>
        </section>
      </div>
    </div>
    <div v-else class="space-empty">
      <FolderSearch :size="24" aria-hidden="true" /><span
        >选择范围并开始分析，结果会持续显示在这里。</span
      >
    </div>

    <section class="history-panel" aria-labelledby="scan-history-title">
      <h3 id="scan-history-title">
        <History :size="16" aria-hidden="true" />最近扫描
      </h3>
      <div v-if="history.length" class="history-list">
        <div
          v-for="item in history.slice(0, 5)"
          :key="item.scanId"
          class="history-row"
        >
          <span class="history-status" :data-status="item.status">{{
            historyStatus(item)
          }}</span>
          <span class="history-path" :title="item.rootPath">{{
            item.rootPath
          }}</span>
          <small>{{ new Date(item.finishedAtUnixMs).toLocaleString() }}</small>
          <strong>{{ formatBytes(item.bytesScanned) }}</strong>
        </div>
      </div>
      <p v-else class="empty-copy">完成或取消扫描后，记录会保存在本机。</p>
    </section>
  </section>
</template>

<style scoped>
.space-scan-card {
  margin-top: 14px;
  padding: 22px;
  border: 1px solid var(--color-border);
  border-radius: 18px;
  background: var(--color-surface);
  box-shadow: 0 14px 36px rgba(17, 24, 39, 0.04);
}
.space-heading,
.space-title,
.space-title p,
.task-actions,
.progress-copy,
.progress-copy > div,
.eta,
.result-panel h3,
.history-panel h3 {
  display: flex;
  align-items: center;
}
.space-heading {
  justify-content: space-between;
  gap: 18px;
}
.space-title {
  gap: 12px;
}
.space-icon {
  display: grid;
  place-items: center;
  width: 38px;
  height: 38px;
  border-radius: 11px;
  color: var(--color-blue);
  background: var(--color-blue-soft);
}
.space-title h2 {
  font-size: 1.02rem;
}
.space-title p {
  gap: 4px;
  margin-top: 4px;
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}
.task-actions {
  gap: 8px;
}
.task-actions button {
  min-height: 36px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
  border: 0;
  border-radius: 10px;
  font-weight: 600;
  cursor: pointer;
}
.start-button {
  color: #fff;
  background: linear-gradient(180deg, #2988ff, #1473e6);
  box-shadow: 0 5px 14px rgba(20, 115, 230, 0.2);
}
.secondary-button {
  color: var(--color-blue);
  background: var(--color-blue-soft);
}
.stop-button {
  color: #b54040;
  background: #fff0f0;
}
.task-actions button:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}
.scope-panel {
  display: grid;
  grid-template-columns: minmax(0, 2fr) minmax(100px, 0.55fr) minmax(
      130px,
      0.7fr
    );
  gap: 12px;
  margin-top: 19px;
  padding: 15px;
  border-radius: 14px;
  background: var(--color-surface-muted);
}
.field {
  display: grid;
  gap: 7px;
  color: var(--color-text-secondary);
  font-size: 0.75rem;
  font-weight: 600;
}
.field span {
  display: flex;
  justify-content: space-between;
  gap: 8px;
}
.field small {
  font-weight: 400;
}
.field input,
.field select,
.field textarea {
  width: 100%;
  min-height: 38px;
  padding: 8px 10px;
  border: 1px solid var(--color-border);
  border-radius: 9px;
  color: var(--color-text-primary);
  background: var(--color-surface);
  font: inherit;
  font-weight: 400;
  outline: none;
}
.field textarea {
  resize: vertical;
  line-height: 1.45;
}
.field input:focus,
.field select:focus,
.field textarea:focus {
  border-color: var(--color-blue);
  box-shadow: 0 0 0 3px var(--color-blue-soft);
}
.excluded-field {
  grid-column: 1 / -1;
}
.scope-hint {
  grid-column: 1 / -1;
  color: var(--color-text-secondary);
  font-size: 0.72rem;
}
.scope-panel.disabled {
  opacity: 0.72;
}
.scan-error {
  margin-top: 13px;
  padding: 10px 12px;
  border-radius: 10px;
  color: #9c3131;
  background: #fff2f2;
  font-size: 0.78rem;
}
.space-content {
  margin-top: 19px;
}
.progress-copy {
  justify-content: space-between;
  gap: 14px;
}
.progress-copy > div {
  gap: 10px;
}
.progress-copy strong {
  font-size: 0.86rem;
}
.progress-copy span,
.eta {
  color: var(--color-text-secondary);
  font-size: 0.75rem;
}
.eta {
  gap: 5px;
}
.progress-track {
  height: 8px;
  margin-top: 10px;
  overflow: hidden;
  border-radius: 99px;
  background: var(--color-surface-muted);
}
.progress-track span {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, #2584ff, #61a8ff);
  transition: width 0.3s ease;
}
.current-path {
  margin-top: 8px;
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.scan-stats {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 9px;
  margin-top: 15px;
}
.scan-stats span {
  display: grid;
  gap: 3px;
  padding: 10px;
  border: 1px solid var(--color-border);
  border-radius: 11px;
  color: var(--color-text-secondary);
  font-size: 0.7rem;
}
.scan-stats strong {
  color: var(--color-text-primary);
  font-size: 0.83rem;
}
.result-grid {
  display: grid;
  grid-template-columns: 1.15fr 0.85fr;
  gap: 12px;
  margin-top: 12px;
}
.result-panel,
.history-panel {
  padding: 14px;
  border: 1px solid var(--color-border);
  border-radius: 13px;
}
.result-panel h3,
.history-panel h3 {
  gap: 7px;
  margin-bottom: 10px;
  font-size: 0.82rem;
}
.result-list,
.type-list,
.history-list {
  display: grid;
  gap: 2px;
}
.result-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  gap: 9px;
  align-items: center;
  min-height: 33px;
  border-bottom: 1px solid var(--color-border);
  font-size: 0.72rem;
}
.result-row:last-child,
.type-row:last-child,
.history-row:last-child {
  border-bottom: 0;
}
.result-path,
.history-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.result-row small,
.type-row span,
.history-row small {
  color: var(--color-text-secondary);
}
.type-row {
  display: grid;
  grid-template-columns: minmax(70px, 1fr) auto auto;
  gap: 9px;
  align-items: center;
  min-height: 33px;
  border-bottom: 1px solid var(--color-border);
  font-size: 0.72rem;
}
.type-row code {
  font-family: inherit;
  font-weight: 650;
}
.space-empty {
  min-height: 105px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}
.history-panel {
  margin-top: 12px;
}
.history-row {
  display: grid;
  grid-template-columns: 56px minmax(0, 1fr) auto auto;
  gap: 10px;
  align-items: center;
  min-height: 35px;
  border-bottom: 1px solid var(--color-border);
  font-size: 0.71rem;
}
.history-status {
  padding: 3px 5px;
  border-radius: 6px;
  color: #28794b;
  background: #eaf7ef;
  text-align: center;
}
.history-status[data-status="cancelled"],
.history-status[data-status="failed"] {
  color: #9c5c23;
  background: #fff3e5;
}
.empty-copy {
  color: var(--color-text-secondary);
  font-size: 0.74rem;
}
@media (max-width: 850px) {
  .space-heading {
    align-items: flex-start;
  }
  .scope-panel {
    grid-template-columns: 1fr 1fr;
  }
  .field-wide {
    grid-column: 1 / -1;
  }
  .result-grid {
    grid-template-columns: 1fr;
  }
  .scan-stats {
    grid-template-columns: 1fr 1fr;
  }
}
@media (max-width: 620px) {
  .space-heading {
    display: grid;
  }
  .task-actions {
    width: 100%;
  }
  .task-actions button {
    flex: 1;
    justify-content: center;
  }
  .scope-panel {
    grid-template-columns: 1fr;
  }
  .field,
  .field-wide,
  .excluded-field {
    grid-column: 1;
  }
  .progress-copy {
    align-items: flex-start;
    flex-direction: column;
  }
  .history-row {
    grid-template-columns: 56px minmax(0, 1fr) auto;
  }
  .history-row small {
    display: none;
  }
}
</style>
