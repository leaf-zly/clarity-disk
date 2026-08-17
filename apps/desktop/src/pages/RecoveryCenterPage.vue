<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from "vue";
import { ArchiveRestore, RefreshCw, ShieldAlert, Trash2 } from "@lucide/vue";

import {
  getExecutionQuarantineIndex,
  updateQuarantinePolicy,
} from "@/services/cleanup-execution-service";
import {
  executeQuarantineDeletion,
  prepareQuarantineDeletion,
  restoreQuarantineEntryTo,
} from "@/services/operations-service";
import type { QuarantineExecutionIndex } from "@/types/cleanup-execution";
import type {
  QuarantineDeletionChallenge,
  QuarantineRestoreDestination,
} from "@/types/operations";
import { formatBytes } from "@/utils/format-bytes";

const index = shallowRef<QuarantineExecutionIndex | null>(null);
const selectedIds = ref<string[]>([]);
const destination = ref<QuarantineRestoreDestination>("original");
const challenge = shallowRef<QuarantineDeletionChallenge | null>(null);
const confirmation = ref("");
const busy = ref(false);
const errorMessage = ref("");
const notice = ref("");
const retentionDays = ref<7 | 15 | 30>(30);
const capacityGiB = ref<1 | 5 | 10 | 20>(10);

const recoverableEntries = computed(
  () =>
    index.value?.entries.filter((entry) =>
      ["staged", "expired", "restoreConflict", "copyVerified"].includes(
        entry.status,
      ),
    ) ?? [],
);

/** Reloads backend-owned quarantine state and reconciles the local selection. */
async function refresh(): Promise<void> {
  errorMessage.value = "";
  index.value = await getExecutionQuarantineIndex();
  selectedIds.value = selectedIds.value.filter((entryId) =>
    recoverableEntries.value.some((entry) => entry.entryId === entryId),
  );
  if (index.value) {
    retentionDays.value = index.value.policy.retentionDays;
    capacityGiB.value = (index.value.policy.maxBytes / 1024 ** 3) as
      1 | 5 | 10 | 20;
  }
}

function toggle(entryId: string): void {
  selectedIds.value = selectedIds.value.includes(entryId)
    ? selectedIds.value.filter((id) => id !== entryId)
    : [...selectedIds.value, entryId];
  challenge.value = null;
  confirmation.value = "";
}

async function restoreSelected(): Promise<void> {
  if (!selectedIds.value.length) return;
  busy.value = true;
  errorMessage.value = "";
  notice.value = "";
  try {
    const results = await Promise.all(
      selectedIds.value.map((entryId) =>
        restoreQuarantineEntryTo(entryId, destination.value),
      ),
    );
    notice.value = results.map((result) => result.reason).join("；");
    selectedIds.value = [];
    await refresh();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

async function prepareDeletion(): Promise<void> {
  if (!selectedIds.value.length) return;
  busy.value = true;
  errorMessage.value = "";
  notice.value = "";
  try {
    challenge.value = await prepareQuarantineDeletion(selectedIds.value);
    confirmation.value = "";
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

async function deletePermanently(): Promise<void> {
  if (!challenge.value) return;
  busy.value = true;
  errorMessage.value = "";
  try {
    const report = await executeQuarantineDeletion({
      authorizationId: challenge.value.authorizationId,
      confirmationToken: challenge.value.confirmationToken,
      confirmationPhrase: confirmation.value,
    });
    index.value = report.index;
    notice.value = `已永久删除 ${formatBytes(report.deletedBytes)}，失败项目仍保留在隔离区。`;
    selectedIds.value = [];
    challenge.value = null;
    confirmation.value = "";
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
    challenge.value = null;
  } finally {
    busy.value = false;
  }
}

async function savePolicy(): Promise<void> {
  busy.value = true;
  errorMessage.value = "";
  try {
    index.value = await updateQuarantinePolicy({
      retentionDays: retentionDays.value,
      maxBytes: capacityGiB.value * 1024 ** 3,
    });
    notice.value = "隔离策略已更新；到期只会标记，不会自动永久删除。";
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

onMounted(() => void refresh());
</script>

<template>
  <section class="recovery-page" aria-labelledby="recovery-title">
    <header class="page-header">
      <div>
        <p class="eyebrow">恢复与隔离</p>
        <h1 id="recovery-title">恢复中心</h1>
        <p class="subtitle">
          所有操作只使用后端条目标识；恢复不覆盖，永久删除需再次确认。
        </p>
      </div>
      <button class="secondary" type="button" :disabled="busy" @click="refresh">
        <RefreshCw :size="16" aria-hidden="true" />刷新
      </button>
    </header>

    <div class="summary-grid">
      <article class="summary-card">
        <span>可恢复项目</span><strong>{{ recoverableEntries.length }}</strong>
      </article>
      <article class="summary-card">
        <span>隔离占用</span
        ><strong>{{ formatBytes(index?.totalBytes ?? 0) }}</strong>
      </article>
      <article class="summary-card safe">
        <span>默认策略</span><strong>到期仍可恢复</strong>
      </article>
    </div>

    <p v-if="errorMessage" class="message error" role="alert">
      {{ errorMessage }}
    </p>
    <p v-if="notice" class="message success" role="status">{{ notice }}</p>

    <article class="panel">
      <div class="panel-heading">
        <div>
          <h2>隔离项目</h2>
          <p>路径仅用于展示，不会从界面回传给执行器。</p>
        </div>
        <span>{{ selectedIds.length }} 项已选择</span>
      </div>
      <div v-if="!recoverableEntries.length" class="empty">
        <ArchiveRestore :size="32" aria-hidden="true" />
        <strong>当前没有可恢复项目</strong>
        <span>执行受限清理后，项目会出现在这里。</span>
      </div>
      <ul v-else class="entry-list">
        <li v-for="entry in recoverableEntries" :key="entry.entryId">
          <label>
            <input
              type="checkbox"
              :checked="selectedIds.includes(entry.entryId)"
              @change="toggle(entry.entryId)"
            />
            <span class="entry-main">
              <strong>{{ entry.originalPath.split(/[\\/]/).at(-1) }}</strong>
              <small>{{ entry.originalPath }}</small>
            </span>
            <span class="entry-meta">
              {{ formatBytes(entry.bytes) }} · {{ entry.status }}
            </span>
          </label>
        </li>
      </ul>

      <div class="action-row">
        <label class="field-inline">
          恢复位置
          <select v-model="destination">
            <option value="original">原位置</option>
            <option value="desktop">桌面 / Clarity Disk Restored</option>
            <option value="documents">文档 / Clarity Disk Restored</option>
            <option value="downloads">下载 / Clarity Disk Restored</option>
          </select>
        </label>
        <button
          class="primary"
          type="button"
          :disabled="busy || !selectedIds.length"
          @click="restoreSelected"
        >
          恢复所选
        </button>
        <button
          class="danger-outline"
          type="button"
          :disabled="busy || !selectedIds.length"
          @click="prepareDeletion"
        >
          <Trash2 :size="16" aria-hidden="true" />永久删除…
        </button>
      </div>
    </article>

    <article v-if="challenge" class="danger-panel">
      <ShieldAlert :size="22" aria-hidden="true" />
      <div>
        <h2>此操作无法恢复</h2>
        <p>
          已绑定
          {{ challenge.entryIds.length }} 个当前隔离项目，令牌将在两分钟后失效。
        </p>
        <label>
          输入“{{ challenge.confirmationPhrase }}”
          <input v-model="confirmation" autocomplete="off" />
        </label>
      </div>
      <button
        class="danger"
        type="button"
        :disabled="busy || confirmation !== challenge.confirmationPhrase"
        @click="deletePermanently"
      >
        确认永久删除
      </button>
    </article>

    <article class="panel policy-panel">
      <div>
        <h2>隔离策略</h2>
        <p>保留期和容量仅接受经过评审的固定档位；策略不会自动擦除文件。</p>
      </div>
      <label
        >保留期
        <select v-model="retentionDays">
          <option :value="7">7 天</option>
          <option :value="15">15 天</option>
          <option :value="30">30 天</option>
        </select>
      </label>
      <label
        >容量上限
        <select v-model="capacityGiB">
          <option :value="1">1 GB</option>
          <option :value="5">5 GB</option>
          <option :value="10">10 GB</option>
          <option :value="20">20 GB</option>
        </select>
      </label>
      <button
        class="secondary"
        type="button"
        :disabled="busy"
        @click="savePolicy"
      >
        保存策略
      </button>
    </article>
  </section>
</template>

<style scoped>
.recovery-page {
  display: grid;
  gap: 22px;
  max-width: 1180px;
  margin: 0 auto;
}
.page-header,
.panel-heading,
.action-row,
.policy-panel,
.danger-panel {
  display: flex;
  align-items: center;
  gap: 16px;
}
.page-header,
.panel-heading {
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
  margin-top: 5px;
  font-size: clamp(1.8rem, 3vw, 2.5rem);
  letter-spacing: -0.04em;
}
h2 {
  font-size: 1.05rem;
}
.subtitle,
.panel p,
.policy-panel p,
small {
  color: var(--color-text-secondary);
}
.subtitle {
  margin-top: 8px;
}
.summary-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 14px;
}
.summary-card,
.panel,
.danger-panel {
  border: 1px solid var(--color-border);
  border-radius: 18px;
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
}
.summary-card {
  padding: 18px;
  display: grid;
  gap: 8px;
}
.summary-card span {
  color: var(--color-text-secondary);
  font-size: 0.82rem;
}
.summary-card strong {
  font-size: 1.35rem;
}
.summary-card.safe {
  background: var(--color-green-soft);
}
.panel {
  padding: 20px;
}
.entry-list {
  list-style: none;
  margin: 18px 0;
  padding: 0;
  display: grid;
  gap: 8px;
}
.entry-list label {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 13px;
  border-radius: 12px;
  background: var(--color-surface-muted);
}
.entry-main {
  min-width: 0;
  display: grid;
  gap: 3px;
  flex: 1;
}
.entry-main small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.entry-meta {
  color: var(--color-text-secondary);
  font-size: 0.8rem;
}
.empty {
  min-height: 180px;
  display: grid;
  place-items: center;
  align-content: center;
  gap: 8px;
  color: var(--color-text-secondary);
}
.action-row {
  justify-content: flex-end;
  flex-wrap: wrap;
}
.field-inline {
  margin-right: auto;
}
label {
  display: grid;
  gap: 6px;
  color: var(--color-text-secondary);
  font-size: 0.82rem;
}
select,
input {
  min-height: 38px;
  padding: 7px 10px;
  border: 1px solid var(--color-border);
  border-radius: 9px;
  color: var(--color-text);
  background: var(--color-surface-muted);
}
button {
  min-height: 38px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 8px 14px;
  border-radius: 10px;
  cursor: pointer;
}
button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.primary {
  border: 0;
  color: white;
  background: var(--color-blue);
}
.secondary {
  border: 1px solid var(--color-border);
  background: var(--color-surface);
}
.danger-outline {
  border: 1px solid color-mix(in srgb, #d93b36 35%, var(--color-border));
  color: #d93b36;
  background: transparent;
}
.danger {
  border: 0;
  color: white;
  background: #c9342f;
}
.danger-panel {
  padding: 18px;
  align-items: flex-start;
  background: color-mix(in srgb, #d93b36 8%, var(--color-surface));
}
.danger-panel > div {
  flex: 1;
  display: grid;
  gap: 8px;
}
.danger-panel label {
  margin-top: 5px;
}
.policy-panel {
  align-items: end;
}
.policy-panel > div {
  flex: 1;
}
.message {
  padding: 12px 14px;
  border-radius: 10px;
}
.message.error {
  color: #c9342f;
  background: color-mix(in srgb, #d93b36 10%, transparent);
}
.message.success {
  color: var(--color-green);
  background: var(--color-green-soft);
}
@media (max-width: 900px) {
  .summary-grid {
    grid-template-columns: 1fr;
  }
  .policy-panel,
  .danger-panel {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
