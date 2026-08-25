<script setup lang="ts">
import { computed, shallowRef, watch } from "vue";
import type { DeepReadonly } from "vue";
import {
  ArchiveRestore,
  CheckCircle2,
  KeyRound,
  RotateCcw,
  Save,
  ShieldAlert,
  Trash2,
} from "@lucide/vue";

import type { CleanupPlan } from "@/types/dashboard";
import type {
  CleanupExecutionChallenge,
  CleanupExecutionMode,
  CleanupExecutionReport,
  QuarantineExecutionIndex,
  QuarantineEntryStatus,
  QuarantinePolicy,
} from "@/types/cleanup-execution";
import { formatBytes } from "@/utils/format-bytes";

const GIB = 1024 ** 3;
const QUARANTINE_RULES = new Set([
  "user-temp.v1",
  "browser-cache.v1",
  "thumbnail-cache.v1",
  "build-cache.v1",
]);

/** Restricted execution, policy, and restore state rendered below cleanup preview. */
interface Props {
  plan: DeepReadonly<CleanupPlan> | undefined;
  challenge: DeepReadonly<CleanupExecutionChallenge> | undefined;
  report: DeepReadonly<CleanupExecutionReport> | undefined;
  quarantine: DeepReadonly<QuarantineExecutionIndex> | undefined;
  error: string | undefined;
  isPreparing: boolean;
  isExecuting: boolean;
  restoringEntryId: string | undefined;
  isRestoringBatch: boolean;
  isUpdatingPolicy: boolean;
}

/** Mode-bound execution, backend-ID restore, and fixed-tier policy actions. */
interface Emits {
  "prepare-execution": [mode: CleanupExecutionMode];
  execute: [confirmationPhrase: string];
  restore: [entryId: string];
  "restore-batch": [entryIds: string[]];
  "update-policy": [policy: QuarantinePolicy];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const confirmation = shallowRef("");
const selectedEntryIds = shallowRef<string[]>([]);
const retentionDays = shallowRef<7 | 15 | 30>(30);
const maxGiB = shallowRef<1 | 5 | 10 | 20>(10);
const quarantineCount = computed(
  () =>
    props.plan?.candidates.filter((candidate) =>
      QUARANTINE_RULES.has(candidate.ruleId),
    ).length ?? 0,
);

const recycleBinCount = computed(
  () =>
    props.plan?.candidates.filter(
      (candidate) => candidate.ruleId === "recycle-bin.v1",
    ).length ?? 0,
);
const canExecute = computed(
  () =>
    Boolean(props.challenge) &&
    confirmation.value === props.challenge?.confirmationPhrase &&
    !props.isExecuting,
);
const managedEntries = computed(
  () =>
    props.quarantine?.entries.filter(
      (entry) => entry.status !== "previewOnly",
    ) ?? [],
);
const restorableEntries = computed(() =>
  managedEntries.value.filter((entry) =>
    ["staged", "expired", "restoreConflict", "copyVerified"].includes(
      entry.status,
    ),
  ),
);
const capacityPercent = computed(() => {
  if (!props.quarantine?.policy.maxBytes) return 0;
  return Math.min(
    100,
    (props.quarantine.totalBytes / props.quarantine.policy.maxBytes) * 100,
  );
});
/** Summarizes per-item execution states without treating skipped work as success. */
const executionSummary = computed(() => {
  const results = props.report?.results ?? [];
  const incomplete = results.filter(
    (result) => result.status !== "staged",
  ).length;
  if (incomplete > 0) {
    return {
      tone: "warning",
      title: `已完成 ${results.length - incomplete} 条，${incomplete} 条需要复核`,
      detail: "部分项目被跳过或重新校验失败，未将其计入成功结果。",
    } as const;
  }
  return {
    tone: "success",
    title:
      props.report?.mode === "quarantine"
        ? `本次隔离 ${formatBytes(props.report?.stagedBytes ?? 0)}`
        : `已处理约 ${formatBytes(props.report?.estimatedProcessedBytes ?? 0)}`,
    detail: `${results.length} 条规则完成，一次性令牌已消费`,
  } as const;
});

watch(
  () => props.quarantine?.policy,
  (policy) => {
    if (!policy) return;
    retentionDays.value = policy.retentionDays;
    maxGiB.value = (policy.maxBytes / GIB) as 1 | 5 | 10 | 20;
  },
  { immediate: true },
);

/** Returns a concise localized label for durable transaction state. */
function statusLabel(status: QuarantineEntryStatus): string {
  const labels: Record<QuarantineEntryStatus, string> = {
    previewOnly: "仅预演",
    staging: "准备隔离",
    copying: "跨卷复制中",
    copyVerified: "副本待复核",
    staged: "已隔离",
    restoring: "恢复中",
    restored: "已恢复",
    restoreConflict: "恢复冲突",
    expired: "已到期",
  };
  return labels[status];
}

/** Toggles one backend entry identity for bounded batch restore. */
function toggleEntry(entryId: string, selected: boolean): void {
  const ids = new Set(selectedEntryIds.value);
  if (selected) ids.add(entryId);
  else ids.delete(entryId);
  selectedEntryIds.value = [...ids];
}

function submitExecution(): void {
  if (!canExecute.value) return;
  emit("execute", confirmation.value);
  confirmation.value = "";
}

function submitBatchRestore(): void {
  if (!selectedEntryIds.value.length) return;
  emit("restore-batch", [...selectedEntryIds.value]);
  selectedEntryIds.value = [];
}

function submitPolicy(): void {
  emit("update-policy", {
    retentionDays: retentionDays.value,
    maxBytes: maxGiB.value * GIB,
  });
}
</script>

<template>
  <section class="execution-card" aria-labelledby="execution-title">
    <header class="execution-heading">
      <div class="execution-icon">
        <ArchiveRestore :size="20" aria-hidden="true" />
      </div>
      <div>
        <span>安全执行中心</span>
        <h2 id="execution-title">隔离策略、恢复与系统适配器</h2>
        <p>
          四类用户缓存可恢复隔离 · 回收站独立永久确认 · Windows 更新保持只读
        </p>
      </div>
    </header>

    <div class="mode-actions">
      <button
        class="verify-button"
        type="button"
        :disabled="
          !plan || !quarantineCount || isPreparing || Boolean(challenge)
        "
        @click="emit('prepare-execution', 'quarantine')"
      >
        <KeyRound :size="16" aria-hidden="true" />{{
          isPreparing ? "正在校验" : "隔离 " + quarantineCount + " 条缓存规则"
        }}
      </button>
      <button
        class="danger-button"
        type="button"
        :disabled="
          !plan || !recycleBinCount || isPreparing || Boolean(challenge)
        "
        @click="emit('prepare-execution', 'windowsRecycleBin')"
      >
        <Trash2 :size="16" aria-hidden="true" />永久清空回收站
      </button>
      <span class="readonly-note"
        >Windows Update：只读，等待独立管理员服务</span
      >
    </div>

    <div v-if="error" class="execution-error" role="alert">{{ error }}</div>

    <div
      v-if="challenge"
      class="confirmation-box"
      :data-danger="challenge.mode === 'windowsRecycleBin'"
      role="group"
    >
      <ShieldAlert :size="22" aria-hidden="true" />
      <div>
        <strong>{{
          challenge.mode === "quarantine"
            ? "确认可恢复隔离"
            : "确认不可恢复操作"
        }}</strong>
        <p>Rust 已重新扫描并绑定执行模式；令牌两分钟内有效且只能使用一次。</p>
        <label
          ><span>请输入“{{ challenge.confirmationPhrase }}”</span
          ><input
            v-model="confirmation"
            autocomplete="off"
            :placeholder="challenge.confirmationPhrase"
        /></label>
      </div>
      <button type="button" :disabled="!canExecute" @click="submitExecution">
        {{ isExecuting ? "正在执行" : "确认执行" }}
      </button>
    </div>

    <div
      v-if="report"
      class="report-strip"
      :class="executionSummary.tone"
      role="status"
    >
      <CheckCircle2
        v-if="executionSummary.tone === 'success'"
        :size="18"
        aria-hidden="true"
      />
      <ShieldAlert v-else :size="18" aria-hidden="true" />
      <div>
        <strong>{{ executionSummary.title }}</strong>
        <span>{{ executionSummary.detail }}</span>
      </div>
      <code>authorized = {{ report.executionAuthorized }}</code>
    </div>

    <div class="policy-panel">
      <div class="policy-copy">
        <strong>隔离区策略</strong><span>到期只标记，不自动永久删除</span>
      </div>
      <label
        >保留期<select v-model.number="retentionDays">
          <option :value="7">7 天</option>
          <option :value="15">15 天</option>
          <option :value="30">30 天</option>
        </select></label
      >
      <label
        >容量上限<select v-model.number="maxGiB">
          <option :value="1">1 GiB</option>
          <option :value="5">5 GiB</option>
          <option :value="10">10 GiB</option>
          <option :value="20">20 GiB</option>
        </select></label
      >
      <button type="button" :disabled="isUpdatingPolicy" @click="submitPolicy">
        <Save :size="15" aria-hidden="true" />{{
          isUpdatingPolicy ? "保存中" : "保存策略"
        }}
      </button>
    </div>
    <div class="capacity" aria-label="隔离区容量">
      <div>
        <span>已用 {{ formatBytes(quarantine?.totalBytes ?? 0) }}</span
        ><span
          >上限 {{ formatBytes(quarantine?.policy.maxBytes ?? 10 * GIB) }}</span
        >
      </div>
      <progress :value="capacityPercent" max="100" />
    </div>

    <div class="quarantine-heading">
      <div>
        <strong>恢复中心</strong
        ><span>{{ managedEntries.length }} 个后端索引项目</span>
      </div>
      <button
        type="button"
        :disabled="!selectedEntryIds.length || isRestoringBatch"
        @click="submitBatchRestore"
      >
        <RotateCcw :size="15" aria-hidden="true" />{{
          isRestoringBatch
            ? "批量恢复中"
            : "恢复已选 " + selectedEntryIds.length + " 项"
        }}
      </button>
    </div>
    <div v-if="managedEntries.length" class="quarantine-list">
      <article v-for="entry in managedEntries" :key="entry.entryId">
        <input
          class="entry-check"
          type="checkbox"
          :aria-label="'选择 ' + entry.entryId"
          :disabled="
            !restorableEntries.some((item) => item.entryId === entry.entryId)
          "
          :checked="selectedEntryIds.includes(entry.entryId)"
          @change="
            toggleEntry(
              entry.entryId,
              ($event.target as HTMLInputElement).checked,
            )
          "
        />
        <div class="entry-status" :data-status="entry.status">
          {{ statusLabel(entry.status) }}
        </div>
        <div class="entry-copy">
          <strong>{{ entry.originalPath.split(/[\\/]/).at(-1) }}</strong
          ><code>{{ entry.originalPath }}</code>
        </div>
        <strong>{{ formatBytes(entry.bytes) }}</strong>
        <button
          type="button"
          :disabled="
            !restorableEntries.some((item) => item.entryId === entry.entryId) ||
            restoringEntryId === entry.entryId
          "
          @click="emit('restore', entry.entryId)"
        >
          <RotateCcw :size="15" aria-hidden="true" />{{
            restoringEntryId === entry.entryId ? "恢复中" : "恢复"
          }}
        </button>
      </article>
    </div>
    <div v-else class="execution-empty">
      尚无真实隔离项目。预演索引不会显示为已移动。
    </div>
    <div class="permanent-delete-lock">
      <ShieldAlert :size="15" aria-hidden="true" /><span
        >永久删除与受控位置恢复已移至侧栏“恢复中心”，并使用独立一次性确认。</span
      >
    </div>
  </section>
</template>

<style scoped>
.execution-card {
  margin-top: 14px;
  padding: 21px;
  border: 1px solid var(--color-border);
  border-radius: 20px;
  background: var(--color-surface);
  box-shadow: 0 16px 42px rgba(20, 32, 52, 0.045);
}
.execution-heading,
.execution-heading > *,
.mode-actions,
.confirmation-box,
.report-strip,
.policy-panel,
.capacity > div,
.quarantine-heading,
.quarantine-heading > div,
.quarantine-list article,
.verify-button,
.danger-button,
.policy-panel button,
.quarantine-heading button,
.quarantine-list button,
.permanent-delete-lock {
  display: flex;
  align-items: center;
}
.execution-heading {
  gap: 12px;
}
.execution-heading > div:nth-child(2) {
  flex: 1;
}
.execution-heading span {
  color: #af6500;
  font-size: 0.68rem;
  font-weight: 650;
  letter-spacing: 0.04em;
}
.execution-heading h2 {
  margin-top: 3px;
  font-size: 1rem;
}
.execution-heading p {
  margin-top: 3px;
  color: var(--color-text-secondary);
  font-size: 0.72rem;
}
.execution-icon {
  width: 37px;
  height: 37px;
  display: grid;
  place-items: center;
  border-radius: 11px;
  color: #d87900;
  background: color-mix(in srgb, #ff9f0a 13%, var(--color-surface));
}
button,
input,
select {
  font: inherit;
}
.mode-actions {
  gap: 8px;
  margin-top: 17px;
  flex-wrap: wrap;
}
.verify-button,
.danger-button,
.policy-panel button,
.quarantine-heading button {
  gap: 6px;
  min-height: 36px;
  padding: 0 12px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-surface-muted);
  cursor: pointer;
}
.verify-button {
  color: #b86a00;
}
.danger-button {
  color: #c52b22;
}
.readonly-note {
  margin-left: auto;
  color: var(--color-text-secondary);
  font-size: 0.68rem;
}
button:disabled {
  cursor: not-allowed;
  opacity: 0.48;
}
.execution-error {
  margin-top: 13px;
  padding: 10px 12px;
  border-radius: 10px;
  color: #b42318;
  background: color-mix(in srgb, #ff453a 10%, var(--color-surface));
  font-size: 0.75rem;
}
.confirmation-box {
  align-items: flex-start;
  gap: 12px;
  margin-top: 16px;
  padding: 15px;
  border: 1px solid color-mix(in srgb, #ff9f0a 35%, var(--color-border));
  border-radius: 14px;
  color: #b86a00;
  background: color-mix(in srgb, #ff9f0a 7%, var(--color-surface));
}
.confirmation-box[data-danger="true"] {
  color: #c52b22;
  border-color: color-mix(in srgb, #ff453a 35%, var(--color-border));
  background: color-mix(in srgb, #ff453a 7%, var(--color-surface));
}
.confirmation-box > div {
  flex: 1;
}
.confirmation-box strong {
  color: var(--color-text);
  font-size: 0.83rem;
}
.confirmation-box p {
  margin-top: 3px;
  color: var(--color-text-secondary);
  font-size: 0.7rem;
}
.confirmation-box label {
  display: grid;
  gap: 5px;
  margin-top: 10px;
}
.confirmation-box label span {
  color: var(--color-text-secondary);
  font-size: 0.68rem;
}
.confirmation-box input {
  min-height: 35px;
  padding: 0 10px;
  border: 1px solid var(--color-border);
  border-radius: 9px;
  color: var(--color-text);
  background: var(--color-surface);
  outline: none;
}
.confirmation-box > button {
  min-height: 36px;
  padding: 0 13px;
  border: 0;
  border-radius: 10px;
  color: white;
  background: #d87900;
  cursor: pointer;
}
.confirmation-box[data-danger="true"] > button {
  background: #c52b22;
}
.report-strip {
  gap: 9px;
  margin-top: 14px;
  padding: 11px 12px;
  border-radius: 11px;
  color: var(--color-green);
  background: color-mix(in srgb, var(--color-green) 9%, var(--color-surface));
}
.report-strip.warning {
  color: var(--color-orange);
  background: color-mix(in srgb, var(--color-orange) 10%, var(--color-surface));
}
.report-strip > div {
  display: grid;
  gap: 2px;
  flex: 1;
}
.report-strip strong {
  color: var(--color-text);
  font-size: 0.76rem;
}
.report-strip span,
.report-strip code {
  color: var(--color-text-secondary);
  font-family: inherit;
  font-size: 0.66rem;
}
.policy-panel {
  gap: 12px;
  margin-top: 17px;
  padding-top: 16px;
  border-top: 1px solid var(--color-border);
  flex-wrap: wrap;
}
.policy-copy {
  display: grid;
  gap: 2px;
  margin-right: auto;
}
.policy-copy strong {
  font-size: 0.8rem;
}
.policy-copy span,
.policy-panel label {
  color: var(--color-text-secondary);
  font-size: 0.67rem;
}
.policy-panel label {
  display: grid;
  gap: 4px;
}
.policy-panel select {
  min-height: 34px;
  padding: 0 28px 0 9px;
  border: 1px solid var(--color-border);
  border-radius: 9px;
  color: var(--color-text);
  background: var(--color-surface-muted);
}
.policy-panel button {
  color: var(--color-blue);
}
.capacity {
  margin-top: 10px;
}
.capacity > div {
  justify-content: space-between;
  color: var(--color-text-secondary);
  font-size: 0.66rem;
}
.capacity progress {
  width: 100%;
  height: 7px;
  margin-top: 5px;
  accent-color: var(--color-blue);
}
.quarantine-heading {
  justify-content: space-between;
  margin-top: 17px;
}
.quarantine-heading > div {
  gap: 8px;
}
.quarantine-heading strong {
  font-size: 0.78rem;
}
.quarantine-heading span {
  color: var(--color-text-secondary);
  font-size: 0.67rem;
}
.quarantine-heading button {
  color: var(--color-blue);
}
.quarantine-list {
  display: grid;
  gap: 7px;
  margin-top: 10px;
}
.quarantine-list article {
  gap: 10px;
  padding: 10px;
  border: 1px solid var(--color-border);
  border-radius: 11px;
}
.entry-check {
  accent-color: var(--color-blue);
}
.entry-status {
  min-width: 66px;
  padding: 4px 7px;
  border-radius: 999px;
  color: var(--color-blue);
  background: var(--color-blue-soft);
  font-size: 0.64rem;
  text-align: center;
}
.entry-status[data-status="expired"],
.entry-status[data-status="restoreConflict"],
.entry-status[data-status="copyVerified"] {
  color: #c96900;
  background: color-mix(in srgb, #ff9f0a 11%, var(--color-surface));
}
.entry-status[data-status="restored"] {
  color: var(--color-green);
  background: color-mix(in srgb, var(--color-green) 10%, var(--color-surface));
}
.entry-copy {
  min-width: 0;
  display: grid;
  gap: 2px;
  flex: 1;
}
.entry-copy strong,
.entry-copy code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.entry-copy strong {
  font-size: 0.75rem;
}
.entry-copy code {
  color: var(--color-text-secondary);
  font-family: inherit;
  font-size: 0.64rem;
}
.quarantine-list > article > strong {
  font-size: 0.72rem;
}
.quarantine-list button {
  gap: 5px;
  min-height: 31px;
  padding: 0 9px;
  border: 1px solid var(--color-border);
  border-radius: 8px;
  color: var(--color-blue);
  background: var(--color-surface-muted);
  cursor: pointer;
  font-size: 0.68rem;
}
.execution-empty,
.permanent-delete-lock {
  margin-top: 10px;
  padding: 13px;
  border-radius: 11px;
  color: var(--color-text-secondary);
  background: var(--color-surface-muted);
  font-size: 0.7rem;
}
.execution-empty {
  text-align: center;
}
.permanent-delete-lock {
  gap: 7px;
  justify-content: center;
}
@media (max-width: 700px) {
  .execution-heading,
  .confirmation-box {
    flex-wrap: wrap;
  }
  .confirmation-box > button {
    width: 100%;
  }
  .readonly-note {
    width: 100%;
    margin-left: 0;
  }
  .quarantine-list article {
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .entry-copy {
    width: calc(100% - 100px);
    flex: none;
  }
  .policy-copy {
    width: 100%;
  }
}
</style>
