<script setup lang="ts">
import { computed, shallowRef } from "vue";
import type { DeepReadonly } from "vue";
import {
  ArchiveRestore,
  CheckCircle2,
  KeyRound,
  RotateCcw,
  ShieldAlert,
} from "@lucide/vue";

import type { CleanupPlan } from "@/types/dashboard";
import type {
  CleanupExecutionChallenge,
  CleanupExecutionReport,
  QuarantineExecutionIndex,
  QuarantineEntryStatus,
} from "@/types/cleanup-execution";
import { formatBytes } from "@/utils/format-bytes";

/** Restricted execution and restore state rendered below the cleanup preview. */
interface Props {
  plan: DeepReadonly<CleanupPlan> | undefined;
  challenge: DeepReadonly<CleanupExecutionChallenge> | undefined;
  report: DeepReadonly<CleanupExecutionReport> | undefined;
  quarantine: DeepReadonly<QuarantineExecutionIndex> | undefined;
  error: string | undefined;
  isPreparing: boolean;
  isExecuting: boolean;
  restoringEntryId: string | undefined;
}

/** Explicit confirmation and backend-owned restore actions emitted by the panel. */
interface Emits {
  "prepare-execution": [];
  execute: [confirmationPhrase: string];
  restore: [entryId: string];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const confirmation = shallowRef("");
const executableCount = computed(
  () =>
    props.plan?.candidates.filter(
      (candidate) => candidate.ruleId === "user-temp.v1",
    ).length ?? 0,
);
const canExecute = computed(
  () =>
    Boolean(props.challenge) &&
    confirmation.value === props.challenge?.confirmationPhrase &&
    !props.isExecuting,
);
const stagedEntries = computed(
  () =>
    props.quarantine?.entries.filter(
      (entry) => entry.status !== "previewOnly",
    ) ?? [],
);

function statusLabel(status: QuarantineEntryStatus): string {
  if (status === "staged") return "已隔离";
  if (status === "restored") return "已恢复";
  if (status === "restoreConflict") return "恢复冲突";
  if (status === "staging") return "恢复记录处理中";
  return "仅预演";
}

function submitExecution(): void {
  if (!canExecute.value) return;
  emit("execute", confirmation.value);
  confirmation.value = "";
}
</script>

<template>
  <section class="execution-card" aria-labelledby="execution-title">
    <header class="execution-heading">
      <div class="execution-icon">
        <ArchiveRestore :size="20" aria-hidden="true" />
      </div>
      <div>
        <span>计划三 · 受限写入</span>
        <h2 id="execution-title">隔离执行与恢复</h2>
        <p>仅支持用户临时文件 · 不删除 · 不提权 · 不调用 shell</p>
      </div>
      <button
        class="verify-button"
        type="button"
        :disabled="
          !plan || !executableCount || isPreparing || Boolean(challenge)
        "
        @click="emit('prepare-execution')"
      >
        <KeyRound :size="16" aria-hidden="true" />
        {{ isPreparing ? "正在重新校验" : "获取一次性确认" }}
      </button>
    </header>

    <div v-if="error" class="execution-error" role="alert">{{ error }}</div>

    <div
      v-if="challenge"
      class="confirmation-box"
      role="group"
      aria-labelledby="confirmation-title"
    >
      <ShieldAlert :size="22" aria-hidden="true" />
      <div>
        <strong id="confirmation-title">最后确认</strong>
        <p>Rust 已完成一次新扫描。令牌两分钟内有效且只能使用一次。</p>
        <label>
          <span>请输入“{{ challenge.confirmationPhrase }}”</span>
          <input
            v-model="confirmation"
            type="text"
            autocomplete="off"
            :placeholder="challenge.confirmationPhrase"
          />
        </label>
      </div>
      <button type="button" :disabled="!canExecute" @click="submitExecution">
        {{ isExecuting ? "正在移入隔离区" : "确认并执行隔离" }}
      </button>
    </div>

    <div v-if="report" class="report-strip" role="status">
      <CheckCircle2 :size="18" aria-hidden="true" />
      <div>
        <strong>本次隔离 {{ formatBytes(report.stagedBytes) }}</strong
        ><span>{{ report.results.length }} 条候选规则已完成，令牌已消费</span>
      </div>
      <code>authorized = {{ report.executionAuthorized }}</code>
    </div>

    <div class="quarantine-heading">
      <div>
        <strong>恢复中心</strong
        ><span>后端索引 {{ stagedEntries.length }} 项</span>
      </div>
      <span>恢复不会覆盖原位置已有内容</span>
    </div>
    <div v-if="stagedEntries.length" class="quarantine-list">
      <article v-for="entry in stagedEntries" :key="entry.entryId">
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
            !['staged', 'restoreConflict'].includes(entry.status) ||
            restoringEntryId === entry.entryId
          "
          @click="emit('restore', entry.entryId)"
        >
          <RotateCcw :size="15" aria-hidden="true" />
          {{ restoringEntryId === entry.entryId ? "恢复中" : "恢复" }}
        </button>
      </article>
    </div>
    <div v-else class="execution-empty">
      尚无真实隔离项目。预演索引不会显示为已移动。
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
.confirmation-box,
.report-strip,
.quarantine-heading,
.quarantine-heading > div,
.quarantine-list article,
.verify-button,
.quarantine-list button {
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
input {
  font: inherit;
}
.verify-button {
  gap: 6px;
  min-height: 36px;
  padding: 0 12px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  color: #b86a00;
  background: var(--color-surface-muted);
  cursor: pointer;
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
.confirmation-box input:focus {
  border-color: var(--color-blue);
  box-shadow: 0 0 0 3px var(--color-blue-soft);
}
.confirmation-box button {
  min-height: 36px;
  padding: 0 13px;
  border: 0;
  border-radius: 10px;
  color: white;
  background: #d87900;
  cursor: pointer;
}
.report-strip {
  gap: 9px;
  margin-top: 14px;
  padding: 11px 12px;
  border-radius: 11px;
  color: var(--color-green);
  background: color-mix(in srgb, var(--color-green) 9%, var(--color-surface));
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
.quarantine-heading {
  justify-content: space-between;
  margin-top: 17px;
  padding-top: 15px;
  border-top: 1px solid var(--color-border);
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
.entry-status {
  min-width: 56px;
  padding: 4px 7px;
  border-radius: 999px;
  color: var(--color-blue);
  background: var(--color-blue-soft);
  font-size: 0.64rem;
  text-align: center;
}
.entry-status[data-status="restored"] {
  color: var(--color-green);
  background: color-mix(in srgb, var(--color-green) 10%, var(--color-surface));
}
.entry-status[data-status="restoreConflict"] {
  color: #c96900;
  background: color-mix(in srgb, #ff9f0a 11%, var(--color-surface));
}
.entry-copy {
  min-width: 0;
  display: grid;
  gap: 2px;
  flex: 1;
}
.entry-copy strong {
  overflow: hidden;
  font-size: 0.75rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.entry-copy code {
  overflow: hidden;
  color: var(--color-text-secondary);
  font-family: inherit;
  font-size: 0.64rem;
  text-overflow: ellipsis;
  white-space: nowrap;
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
.execution-empty {
  margin-top: 10px;
  padding: 16px;
  border-radius: 11px;
  color: var(--color-text-secondary);
  background: var(--color-surface-muted);
  font-size: 0.72rem;
  text-align: center;
}
@media (max-width: 700px) {
  .execution-heading {
    flex-wrap: wrap;
  }
  .verify-button {
    margin-left: 49px;
  }
  .confirmation-box {
    flex-wrap: wrap;
  }
  .confirmation-box button {
    width: 100%;
  }
  .quarantine-heading > span {
    display: none;
  }
  .entry-copy code {
    max-width: 160px;
  }
}
</style>
