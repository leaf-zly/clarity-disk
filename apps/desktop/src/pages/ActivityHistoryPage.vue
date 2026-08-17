<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from "vue";
import { Download, History, RefreshCw, Trash2 } from "@lucide/vue";

import {
  clearActivityHistory,
  getActivityHistory,
} from "@/services/operations-service";
import type { ActivityHistorySnapshot } from "@/types/operations";
import { formatBytes } from "@/utils/format-bytes";

type ActivityCategory = "cleanup" | "scan" | "privileged";

interface ActivityRow {
  id: string;
  category: ActivityCategory;
  timestamp: number;
  title: string;
  detail: string;
  bytes: number;
  status: string;
}

const snapshot = shallowRef<ActivityHistorySnapshot>({
  cleanup: [],
  scans: [],
  privileged: [],
});
const category = ref<"all" | ActivityCategory>("all");
const periodDays = ref<0 | 7 | 30>(30);
const query = ref("");
const busy = ref(false);
const errorMessage = ref("");
const clearOpen = ref(false);
const clearPhrase = ref("");

const rows = computed<ActivityRow[]>(() =>
  [
    ...snapshot.value.cleanup.map((event) => ({
      id: event.eventId,
      category: "cleanup" as const,
      timestamp: event.occurredAtUnixMs,
      title: cleanupKindLabel(event.kind),
      detail: event.reason ?? `规则：${event.ruleIds.join("、") || "无"}`,
      bytes: event.totalBytes,
      status: event.kind,
    })),
    ...snapshot.value.scans.map((event) => ({
      id: event.scanId,
      category: "scan" as const,
      timestamp: event.finishedAtUnixMs,
      title: "空间扫描",
      detail: `${event.scannedItems.toLocaleString()} 项 · ${redactRoot(event.rootPath)}`,
      bytes: event.bytesScanned,
      status: event.status,
    })),
    ...snapshot.value.privileged.map((event) => ({
      id: event.requestId,
      category: "privileged" as const,
      timestamp: event.completedAtUnixMs,
      title: "管理员维护",
      detail: event.message,
      bytes: 0,
      status: event.status,
    })),
  ].sort((left, right) => right.timestamp - left.timestamp),
);

const filteredRows = computed(() => {
  const cutoff = periodDays.value
    ? Date.now() - periodDays.value * 86_400_000
    : 0;
  const needle = query.value.trim().toLocaleLowerCase();
  return rows.value.filter(
    (row) =>
      (category.value === "all" || row.category === category.value) &&
      row.timestamp >= cutoff &&
      (!needle ||
        `${row.title} ${row.detail} ${row.status}`
          .toLocaleLowerCase()
          .includes(needle)),
  );
});

async function refresh(): Promise<void> {
  busy.value = true;
  errorMessage.value = "";
  try {
    snapshot.value = await getActivityHistory();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

/** Downloads only the already-redacted rows currently visible to the user. */
function exportVisibleHistory(): void {
  const payload = {
    schemaVersion: 1,
    exportedAt: new Date().toISOString(),
    privacy: "不包含文件内容；扫描根仅保留卷标识",
    events: filteredRows.value,
  };
  const blob = new Blob([JSON.stringify(payload, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `clarity-disk-history-${Date.now()}.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}

async function clearHistory(): Promise<void> {
  if (clearPhrase.value !== "确认清除本地历史") return;
  busy.value = true;
  errorMessage.value = "";
  try {
    await clearActivityHistory();
    snapshot.value = { cleanup: [], scans: [], privileged: [] };
    clearOpen.value = false;
    clearPhrase.value = "";
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

function cleanupKindLabel(kind: string): string {
  const labels: Record<string, string> = {
    scanCompleted: "清理扫描完成",
    planCreated: "清理计划已生成",
    planRejected: "清理计划已拒绝",
    executionStarted: "受限清理开始",
    executionCompleted: "受限清理完成",
    quarantineRestoreCompleted: "隔离项目恢复",
    quarantineDeletionCompleted: "隔离项目永久删除",
  };
  return labels[kind] ?? "清理安全事件";
}

function redactRoot(rootPath: string): string {
  return /^[a-z]:[\\/]?$/i.test(rootPath) ? rootPath : "已脱敏扫描范围";
}

onMounted(() => void refresh());
</script>

<template>
  <section class="history-page" aria-labelledby="history-title">
    <header class="page-header">
      <div>
        <p class="eyebrow">本地与可审计</p>
        <h1 id="history-title">活动历史</h1>
        <p>筛选、导出或清除本地摘要；隔离文件与分区恢复日志不会随历史清除。</p>
      </div>
      <div class="header-actions">
        <button type="button" :disabled="busy" @click="refresh">
          <RefreshCw :size="16" />刷新
        </button>
        <button
          type="button"
          :disabled="!filteredRows.length"
          @click="exportVisibleHistory"
        >
          <Download :size="16" />导出
        </button>
      </div>
    </header>

    <div class="filters" aria-label="历史筛选">
      <select v-model="category" aria-label="事件类别">
        <option value="all">全部类别</option>
        <option value="cleanup">清理</option>
        <option value="scan">空间扫描</option>
        <option value="privileged">管理员维护</option>
      </select>
      <select v-model="periodDays" aria-label="时间范围">
        <option :value="7">最近 7 天</option>
        <option :value="30">最近 30 天</option>
        <option :value="0">全部时间</option>
      </select>
      <input
        v-model="query"
        type="search"
        placeholder="搜索状态或说明"
        aria-label="搜索活动历史"
      />
      <span>{{ filteredRows.length }} 条</span>
    </div>

    <p v-if="errorMessage" class="error" role="alert">{{ errorMessage }}</p>
    <article class="timeline-card">
      <div v-if="!filteredRows.length" class="empty">
        <History :size="34" aria-hidden="true" /><strong
          >没有符合条件的活动</strong
        ><span>完成扫描或维护后，隐私安全的摘要会显示在这里。</span>
      </div>
      <ol v-else class="timeline">
        <li v-for="row in filteredRows" :key="`${row.category}-${row.id}`">
          <span class="dot" :class="row.category"></span>
          <div class="event-main">
            <strong>{{ row.title }}</strong>
            <p>{{ row.detail }}</p>
          </div>
          <div class="event-meta">
            <time :datetime="new Date(row.timestamp).toISOString()">{{
              new Date(row.timestamp).toLocaleString()
            }}</time>
            <span>{{ row.bytes ? formatBytes(row.bytes) : row.status }}</span>
          </div>
        </li>
      </ol>
    </article>

    <article class="privacy-card">
      <div>
        <h2>隐私与保留</h2>
        <p>
          导出只包含当前筛选后的脱敏摘要，不包含文件内容或完整自定义扫描路径。
        </p>
      </div>
      <button
        class="danger-outline"
        type="button"
        @click="clearOpen = !clearOpen"
      >
        <Trash2 :size="16" />清除历史
      </button>
    </article>
    <article v-if="clearOpen" class="clear-confirmation">
      <div>
        <strong>清除扫描与审计摘要？</strong>
        <p>不会删除隔离内容，也不会移除分区恢复日志。</p>
      </div>
      <label
        >输入“确认清除本地历史”<input v-model="clearPhrase" autocomplete="off"
      /></label>
      <button
        class="danger"
        type="button"
        :disabled="busy || clearPhrase !== '确认清除本地历史'"
        @click="clearHistory"
      >
        确认清除
      </button>
    </article>
  </section>
</template>

<style scoped>
.history-page {
  max-width: 1180px;
  margin: 0 auto;
  display: grid;
  gap: 20px;
}
.page-header,
.header-actions,
.filters,
.privacy-card,
.clear-confirmation {
  display: flex;
  align-items: center;
  gap: 12px;
}
.page-header,
.privacy-card {
  justify-content: space-between;
}
.eyebrow {
  color: var(--color-blue);
  font-size: 0.76rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
h1 {
  margin: 5px 0 8px;
  font-size: clamp(1.8rem, 3vw, 2.5rem);
  letter-spacing: -0.04em;
}
h2 {
  font-size: 1rem;
}
p,
.filters span {
  color: var(--color-text-secondary);
}
button,
select,
input {
  min-height: 38px;
  padding: 8px 12px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  color: var(--color-text);
  background: var(--color-surface);
}
button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
}
button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.filters {
  padding: 12px;
  border-radius: 14px;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
}
.filters input {
  min-width: 220px;
  flex: 1;
  background: var(--color-surface-muted);
}
.timeline-card,
.privacy-card,
.clear-confirmation {
  border: 1px solid var(--color-border);
  border-radius: 18px;
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
}
.timeline-card {
  overflow: hidden;
}
.timeline {
  list-style: none;
  margin: 0;
  padding: 0;
}
.timeline li {
  display: grid;
  grid-template-columns: 14px minmax(0, 1fr) auto;
  gap: 14px;
  align-items: start;
  padding: 16px 18px;
  border-bottom: 1px solid var(--color-border);
}
.timeline li:last-child {
  border-bottom: 0;
}
.dot {
  width: 9px;
  height: 9px;
  margin-top: 5px;
  border-radius: 50%;
  background: var(--color-blue);
}
.dot.scan {
  background: var(--color-purple);
}
.dot.privileged {
  background: var(--color-orange);
}
.event-main {
  display: grid;
  gap: 5px;
}
.event-main p {
  font-size: 0.86rem;
}
.event-meta {
  display: grid;
  justify-items: end;
  gap: 5px;
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}
.empty {
  min-height: 300px;
  display: grid;
  place-items: center;
  align-content: center;
  gap: 8px;
  color: var(--color-text-secondary);
}
.privacy-card,
.clear-confirmation {
  padding: 18px;
}
.privacy-card > div,
.clear-confirmation > div {
  display: grid;
  gap: 6px;
  flex: 1;
}
.danger-outline {
  color: #d23b36;
  background: transparent;
  border-color: color-mix(in srgb, #d23b36 35%, var(--color-border));
}
.danger {
  color: white;
  background: #c9342f;
  border: 0;
}
.clear-confirmation label {
  display: grid;
  gap: 5px;
  color: var(--color-text-secondary);
  font-size: 0.8rem;
}
.error {
  padding: 12px;
  border-radius: 10px;
  color: #c9342f;
  background: color-mix(in srgb, #d23b36 10%, transparent);
}
@media (max-width: 900px) {
  .page-header,
  .privacy-card,
  .clear-confirmation {
    align-items: stretch;
    flex-direction: column;
  }
  .filters {
    flex-wrap: wrap;
  }
  .event-meta {
    display: none;
  }
}
</style>
