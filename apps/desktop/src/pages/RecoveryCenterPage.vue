<script setup lang="ts">
import {
  computed,
  onActivated,
  onDeactivated,
  onMounted,
  ref,
  shallowRef,
} from "vue";
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
import type {
  QuarantineEntryStatus,
  QuarantineExecutionIndex,
} from "@/types/cleanup-execution";
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
const isRefreshing = ref(false);
const stateReadable = ref(false);
const actionsLocked = computed(
  () => busy.value || isRefreshing.value || !stateReadable.value,
);
let confirmationRevision = 0;
const errorMessage = ref("");
const notice = ref("");
const retentionDays = ref<7 | 15 | 30>(30);
const capacityGiB = ref<1 | 5 | 10 | 20>(10);

/** User-facing lifecycle labels; internal enum values never leak into the list. */
const statusLabels: Record<QuarantineEntryStatus, string> = {
  previewOnly: "仅预览",
  staging: "正在隔离",
  copying: "正在复制",
  copyVerified: "副本已校验",
  staged: "可恢复",
  restoring: "正在恢复",
  restored: "已恢复",
  restoreConflict: "原位置存在同名文件",
  expired: "已到期，仍可恢复",
};

/** Invalidates any pending or displayed authorization when its context changes. */
function invalidateConfirmation(): void {
  confirmationRevision += 1;
  challenge.value = null;
  confirmation.value = "";
}

const recoverableEntries = computed(
  () =>
    index.value?.entries.filter((entry) =>
      ["staged", "expired", "restoreConflict", "copyVerified"].includes(
        entry.status,
      ),
    ) ?? [],
);

/**
 * Reloads backend-owned quarantine state and reconciles the local selection.
 * Read failures remain recoverable and are surfaced without exposing backend details.
 */
async function refresh(): Promise<void> {
  if (isRefreshing.value) return;
  isRefreshing.value = true;
  stateReadable.value = false;
  invalidateConfirmation();
  errorMessage.value = "";
  try {
    index.value = await getExecutionQuarantineIndex();
    stateReadable.value = true;
    selectedIds.value = selectedIds.value.filter((entryId) =>
      recoverableEntries.value.some((entry) => entry.entryId === entryId),
    );
    if (index.value) {
      retentionDays.value = index.value.policy.retentionDays;
      capacityGiB.value = (index.value.policy.maxBytes / 1024 ** 3) as
        1 | 5 | 10 | 20;
    }
  } catch {
    errorMessage.value = "隔离区状态暂时无法读取，请稍后重试。";
  } finally {
    isRefreshing.value = false;
  }
}

function toggle(entryId: string): void {
  if (actionsLocked.value) return;
  selectedIds.value = selectedIds.value.includes(entryId)
    ? selectedIds.value.filter((id) => id !== entryId)
    : [...selectedIds.value, entryId];
  invalidateConfirmation();
}

/** Restores a stable selection sequentially, retaining failed entries for retry. */
async function restoreSelected(): Promise<void> {
  if (actionsLocked.value || !selectedIds.value.length) return;
  const entryIds = [...selectedIds.value];
  const restoreDestination = destination.value;
  invalidateConfirmation();
  busy.value = true;
  errorMessage.value = "";
  notice.value = "";
  try {
    const failedIds: string[] = [];
    // Avoid competing filesystem/index writes and wait for every accepted request.
    for (const entryId of entryIds) {
      try {
        const result = await restoreQuarantineEntryTo(
          entryId,
          restoreDestination,
        );
        if (result.status !== "restored") failedIds.push(entryId);
      } catch {
        failedIds.push(entryId);
      }
    }
    selectedIds.value = failedIds;
    await refresh();
    notice.value = `已恢复 ${entryIds.length - failedIds.length} 项，未恢复 ${failedIds.length} 项。`;
    if (failedIds.length) {
      errorMessage.value = [
        errorMessage.value,
        "部分项目未恢复；请检查文件占用或同名冲突，可选择其他恢复位置重试。",
      ]
        .filter(Boolean)
        .join(" ");
    }
  } finally {
    busy.value = false;
  }
}

/** Requests a one-time challenge that must still belong to the active selection. */
async function prepareDeletion(): Promise<void> {
  if (actionsLocked.value || !selectedIds.value.length) return;
  invalidateConfirmation();
  const revision = confirmationRevision;
  busy.value = true;
  errorMessage.value = "";
  notice.value = "";
  try {
    const prepared = await prepareQuarantineDeletion([...selectedIds.value]);
    if (revision !== confirmationRevision) return;
    challenge.value = prepared;
    confirmation.value = "";
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

async function deletePermanently(): Promise<void> {
  if (
    actionsLocked.value ||
    !challenge.value ||
    confirmation.value !== challenge.value.confirmationPhrase
  )
    return;
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
  if (actionsLocked.value) return;
  invalidateConfirmation();
  busy.value = true;
  errorMessage.value = "";
  notice.value = "";
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
// Mounted and initial activation run together; refresh's in-flight guard deduplicates them.
onActivated(() => {
  if (!busy.value) void refresh();
});
onDeactivated(invalidateConfirmation);
</script>

<template>
  <section class="recovery-page" aria-labelledby="recovery-title">
    <header class="page-header">
      <div>
        <p class="eyebrow">恢复与隔离</p>
        <h1 id="recovery-title">恢复中心</h1>
        <p class="subtitle">
          找回已隔离的文件。恢复不会覆盖同名文件，永久删除前需要再次确认。
        </p>
      </div>
      <button
        class="secondary"
        type="button"
        :disabled="busy || isRefreshing"
        @click="refresh"
      >
        <RefreshCw
          :class="{ spin: isRefreshing }"
          :size="16"
          aria-hidden="true"
        />
        {{ isRefreshing ? "刷新中" : "刷新" }}
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
          <p>选择文件恢复到原位置，或保存到其他恢复文件夹。</p>
        </div>
        <span>{{ selectedIds.length }} 项已选择</span>
      </div>
      <div v-if="isRefreshing && !index" class="empty" aria-live="polite">
        <RefreshCw class="spin" :size="32" aria-hidden="true" />
        <strong>正在读取隔离区</strong>
        <span>只读取本机隔离索引，不会修改文件。</span>
      </div>
      <div
        v-else-if="!recoverableEntries.length && !errorMessage"
        class="empty"
      >
        <ArchiveRestore :size="32" aria-hidden="true" />
        <strong>当前没有可恢复项目</strong>
        <span>执行受限清理后，项目会出现在这里。</span>
      </div>
      <ul v-else class="entry-list">
        <li v-for="entry in recoverableEntries" :key="entry.entryId">
          <label>
            <input
              type="checkbox"
              :disabled="actionsLocked"
              :checked="selectedIds.includes(entry.entryId)"
              @change="toggle(entry.entryId)"
            />
            <span class="entry-main">
              <strong>{{ entry.originalPath.split(/[\\/]/).at(-1) }}</strong>
              <small>{{ entry.originalPath }}</small>
            </span>
            <span class="entry-meta">
              {{ formatBytes(entry.bytes) }} · {{ statusLabels[entry.status] }}
            </span>
          </label>
        </li>
      </ul>

      <div class="action-row">
        <label class="field-inline">
          恢复位置
          <select v-model="destination" :disabled="actionsLocked">
            <option value="original">原位置</option>
            <option value="desktop">桌面 / Clarity Disk Restored</option>
            <option value="documents">文档 / Clarity Disk Restored</option>
            <option value="downloads">下载 / Clarity Disk Restored</option>
          </select>
        </label>
        <button
          class="primary"
          type="button"
          :disabled="actionsLocked || !selectedIds.length"
          @click="restoreSelected"
        >
          恢复所选
        </button>
        <button
          class="danger-outline"
          type="button"
          :disabled="actionsLocked || !selectedIds.length"
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
          将永久删除
          {{ challenge.entryIds.length }} 个隔离项目。本次确认两分钟内有效。
        </p>
        <label>
          输入“{{ challenge.confirmationPhrase }}”
          <input
            v-model="confirmation"
            :disabled="actionsLocked"
            autocomplete="off"
          />
        </label>
      </div>
      <button
        class="danger"
        type="button"
        :disabled="
          actionsLocked || confirmation !== challenge.confirmationPhrase
        "
        @click="deletePermanently"
      >
        确认永久删除
      </button>
    </article>

    <article class="panel policy-panel">
      <div>
        <h2>隔离策略</h2>
        <p>到期后提醒处理，不会自动删除。隔离文件仍占用磁盘空间。</p>
      </div>
      <label
        >保留期
        <select v-model="retentionDays" :disabled="actionsLocked">
          <option :value="7">7 天</option>
          <option :value="15">15 天</option>
          <option :value="30">30 天</option>
        </select>
      </label>
      <label
        >容量上限
        <select v-model="capacityGiB" :disabled="actionsLocked">
          <option :value="1">1 GB</option>
          <option :value="5">5 GB</option>
          <option :value="10">10 GB</option>
          <option :value="20">20 GB</option>
        </select>
      </label>
      <button
        class="secondary"
        type="button"
        :disabled="actionsLocked"
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
.spin {
  animation: spin 0.85s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
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
