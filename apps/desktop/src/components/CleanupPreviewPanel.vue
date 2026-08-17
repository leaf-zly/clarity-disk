<script setup lang="ts">
import { computed } from "vue";
import type { DeepReadonly } from "vue";
import {
  ArchiveRestore,
  ChevronDown,
  Clock3,
  Eye,
  FolderSearch,
  Info,
  KeyRound,
  ListChecks,
  ShieldCheck,
} from "@lucide/vue";

import type {
  CleanupPlan,
  CleanupPreview,
  QuarantineIndex,
  RecoveryStrategy,
  SuggestionRisk,
} from "@/types/dashboard";
import type { CleanupAuditEvent } from "@/types/cleanup-audit";
import { formatBytes } from "@/utils/format-bytes";

/** Complete read-only cleanup workflow state rendered by the panel. */
interface Props {
  preview: DeepReadonly<CleanupPreview>;
  selectedIds: readonly string[];
  selectedBytes: number;
  highestRisk: SuggestionRisk | undefined;
  plan: DeepReadonly<CleanupPlan> | undefined;
  quarantine: DeepReadonly<QuarantineIndex> | undefined;
  auditEvents: DeepReadonly<CleanupAuditEvent[]>;
  error: string | undefined;
  isLoading: boolean;
  isPreparingPlan: boolean;
  isPreparingQuarantine: boolean;
}

/** Explicit user intents emitted to the cleanup workflow owner. */
interface Emits {
  "request-scan": [];
  "update-selection": [candidateId: string, selected: boolean];
  "prepare-plan": [];
  "prepare-quarantine": [];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();

const riskGroups = computed(() =>
  (["safe", "review", "confirmationRequired"] as const)
    .map((risk) => ({
      risk,
      candidates: props.preview.candidates.filter(
        (candidate) => candidate.risk === risk,
      ),
    }))
    .filter((group) => group.candidates.length),
);
const selectedQuarantineCount = computed(
  () =>
    props.preview.candidates.filter(
      (candidate) =>
        props.selectedIds.includes(candidate.id) &&
        candidate.quarantineEligible,
    ).length,
);

function riskLabel(risk: SuggestionRisk): string {
  if (risk === "safe") return "低风险";
  if (risk === "review") return "需要复核";
  return "需单独确认";
}

function recoveryLabel(strategy: RecoveryStrategy): string {
  if (strategy === "regenerate") return "应用或 Windows 可重新生成";
  if (strategy === "quarantine") return "可进入隔离区并按项恢复";
  if (strategy === "windowsManaged") return "遵循 Windows 官方维护流程";
  return "不保证自动恢复";
}

function auditLabel(kind: CleanupAuditEvent["kind"]): string {
  if (kind === "scanCompleted") return "扫描完成";
  if (kind === "planCreated") return "计划生成";
  if (kind === "planRejected") return "计划拒绝";
  if (kind === "quarantineIndexCreated") return "隔离区索引";
  if (kind === "executionConfirmationIssued") return "确认已签发";
  if (kind === "executionStarted") return "隔离开始";
  if (kind === "executionCompleted") return "隔离完成";
  if (kind === "quarantineRestoreStarted") return "恢复开始";
  if (kind === "quarantineRestoreCompleted") return "恢复结果";
  return "未知事件";
}
</script>

<template>
  <section class="cleanup-card" aria-labelledby="cleanup-preview-title">
    <header class="cleanup-heading">
      <div class="heading-icon">
        <FolderSearch :size="20" aria-hidden="true" />
      </div>
      <div class="heading-copy">
        <div class="eyebrow"><span class="status-dot" />安全清理中心</div>
        <h2 id="cleanup-preview-title">清理预览</h2>
        <p>{{ preview.scan.message }} · 全程只读，不会删除或移动文件</p>
      </div>
      <button
        class="secondary-button"
        type="button"
        :disabled="isLoading"
        @click="emit('request-scan')"
      >
        {{ isLoading ? "扫描中" : "重新扫描" }}
      </button>
    </header>

    <div v-if="error" class="workflow-error" role="alert">{{ error }}</div>

    <div class="preview-summary">
      <div>
        <span>发现空间</span
        ><strong>{{ formatBytes(preview.totalReclaimableBytes) }}</strong>
      </div>
      <div>
        <span>当前选择</span><strong>{{ formatBytes(selectedBytes) }}</strong>
      </div>
      <div>
        <span>已扫描项目</span
        ><strong>{{ preview.scan.scannedItems.toLocaleString() }}</strong>
      </div>
      <div>
        <span>规则覆盖</span
        ><strong>{{ preview.ruleStatuses.length }} 条</strong>
      </div>
    </div>

    <div v-if="riskGroups.length" class="risk-groups">
      <section
        v-for="group in riskGroups"
        :key="group.risk"
        class="risk-section"
        :data-risk="group.risk"
      >
        <div class="risk-heading">
          <div>
            <span class="risk-dot" /><strong>{{
              riskLabel(group.risk)
            }}</strong>
          </div>
          <span>{{ group.candidates.length }} 项</span>
        </div>
        <details
          v-for="candidate in group.candidates"
          :key="candidate.id"
          class="candidate-card"
        >
          <summary>
            <label class="candidate-check" @click.stop>
              <input
                type="checkbox"
                :checked="selectedIds.includes(candidate.id)"
                :aria-label="`选择${candidate.title}`"
                @change="
                  emit(
                    'update-selection',
                    candidate.id,
                    ($event.target as HTMLInputElement).checked,
                  )
                "
              />
              <span><ShieldCheck :size="17" aria-hidden="true" /></span>
            </label>
            <div class="candidate-copy">
              <strong>{{ candidate.title }}</strong>
              <span>{{ candidate.description }}</span>
            </div>
            <div class="candidate-stats">
              <strong>{{ formatBytes(candidate.bytes) }}</strong
              ><span>{{ candidate.itemCount.toLocaleString() }} 个项目</span>
            </div>
            <ChevronDown class="detail-chevron" :size="18" aria-hidden="true" />
          </summary>
          <div class="candidate-detail">
            <div class="detail-grid">
              <div>
                <span>规则版本</span
                ><strong
                  >{{ candidate.ruleId }} · v{{ candidate.ruleVersion }}</strong
                >
              </div>
              <div>
                <span>恢复策略</span
                ><strong>{{
                  recoveryLabel(candidate.recoveryStrategy)
                }}</strong>
              </div>
              <div>
                <span>管理员权限</span
                ><strong>{{
                  candidate.requiresAdmin ? "未来执行需要" : "当前扫描不需要"
                }}</strong>
              </div>
              <div>
                <span>隔离区资格</span
                ><strong>{{
                  candidate.quarantineEligible ? "可生成预演索引" : "不适用"
                }}</strong>
              </div>
            </div>
            <code>{{ candidate.path }}</code>
            <ul>
              <li v-for="evidence in candidate.evidence" :key="evidence">
                {{ evidence }}
              </li>
            </ul>
            <p v-if="candidate.requiresAdmin" class="admin-note">
              <KeyRound
                :size="15"
                aria-hidden="true"
              />当前不会请求管理员权限，也不会停止 Windows 服务。
            </p>
          </div>
        </details>
      </section>
    </div>
    <div v-else class="empty-preview">
      <Eye :size="18" aria-hidden="true" /><span>未发现允许预览的清理项目</span>
    </div>

    <details class="rule-statuses">
      <summary>
        <ListChecks :size="17" aria-hidden="true" /><strong>全部规则状态</strong
        ><span>{{ preview.ruleStatuses.length }} 条已评估</span
        ><ChevronDown :size="17" aria-hidden="true" />
      </summary>
      <div class="rule-status-grid">
        <div v-for="status in preview.ruleStatuses" :key="status.ruleId">
          <strong>{{ status.title }}</strong
          ><span>{{
            status.availability === "available"
              ? "已发现"
              : status.availability === "empty"
                ? "无内容"
                : "已跳过"
          }}</span>
          <small>{{ status.reason ?? status.ruleId }}</small>
        </div>
      </div>
    </details>

    <div class="workflow-footer">
      <div>
        <span
          >已选择 {{ selectedIds.length }} 项 · 最高风险
          {{ highestRisk ? riskLabel(highestRisk) : "无" }}</span
        ><strong>{{ formatBytes(selectedBytes) }}</strong>
      </div>
      <button
        class="primary-button"
        type="button"
        :disabled="!selectedIds.length || isPreparingPlan"
        @click="emit('prepare-plan')"
      >
        {{ isPreparingPlan ? "正在校验" : "生成只读安全计划" }}
      </button>
    </div>

    <div class="workflow-columns">
      <article class="workflow-card">
        <div class="workflow-card-heading">
          <div>
            <ArchiveRestore :size="18" aria-hidden="true" /><strong
              >隔离区预演</strong
            >
          </div>
          <span>未移动文件</span>
        </div>
        <template v-if="quarantine">
          <strong class="workflow-value">{{
            formatBytes(quarantine.totalBytes)
          }}</strong>
          <p>{{ quarantine.entries.length }} 个索引条目 · filesMoved = false</p>
        </template>
        <template v-else
          ><p>从当前安全计划生成可恢复项目的元数据索引。</p></template
        >
        <button
          type="button"
          :disabled="!plan || !selectedQuarantineCount || isPreparingQuarantine"
          @click="emit('prepare-quarantine')"
        >
          {{
            isPreparingQuarantine
              ? "生成中"
              : `生成隔离区索引${selectedQuarantineCount ? `（${selectedQuarantineCount}）` : ""}`
          }}
        </button>
      </article>

      <article class="workflow-card audit-card">
        <div class="workflow-card-heading">
          <div>
            <Clock3 :size="18" aria-hidden="true" /><strong>本地审计</strong>
          </div>
          <span>不含文件内容</span>
        </div>
        <div v-if="auditEvents.length" class="audit-list">
          <div v-for="event in auditEvents.slice(0, 4)" :key="event.eventId">
            <strong>{{ auditLabel(event.kind) }}</strong
            ><span
              >{{ formatBytes(event.totalBytes) }} ·
              {{ event.candidateCount }} 项</span
            ><small>{{ event.reason }}</small>
          </div>
        </div>
        <p v-else>完成扫描或生成计划后，安全摘要会保存在本机。</p>
      </article>
    </div>

    <div class="preview-note">
      <Info :size="16" aria-hidden="true" /><span
        >当前预览只负责发现、计划、审计和隔离区索引，不会删除、移动文件或执行系统命令。</span
      >
    </div>
    <div v-if="plan" class="plan-note" role="status">
      <strong>安全计划已生成，仅供复核</strong
      ><span
        >{{ plan.candidates.length }} 项 · 摘要 {{ plan.planDigest }} ·
        executionAuthorized = false</span
      >
    </div>
  </section>
</template>

<style scoped>
.cleanup-card {
  margin-top: 14px;
  padding: 22px;
  border: 1px solid var(--color-border);
  border-radius: 20px;
  background: color-mix(in srgb, var(--color-surface) 96%, transparent);
  box-shadow: 0 16px 42px rgba(20, 32, 52, 0.05);
}
.cleanup-heading,
.cleanup-heading > *,
.risk-heading,
.risk-heading > div,
.candidate-card summary,
.candidate-check,
.workflow-footer,
.workflow-card-heading,
.workflow-card-heading > div,
.preview-note,
.rule-statuses summary {
  display: flex;
  align-items: center;
}
.cleanup-heading {
  gap: 12px;
}
.heading-icon {
  width: 38px;
  height: 38px;
  display: grid;
  place-items: center;
  border-radius: 12px;
  color: var(--color-blue);
  background: var(--color-blue-soft);
}
.heading-copy {
  min-width: 0;
  flex: 1;
}
.eyebrow {
  gap: 6px;
  margin-bottom: 4px;
  color: var(--color-blue);
  font-size: 0.7rem;
  font-weight: 650;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-green);
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--color-green) 12%, transparent);
}
.cleanup-heading h2 {
  font-size: 1.1rem;
  letter-spacing: -0.015em;
}
.cleanup-heading p,
.workflow-card p {
  margin-top: 4px;
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}
button {
  font: inherit;
}
.secondary-button,
.primary-button,
.workflow-card button {
  min-height: 36px;
  padding: 0 13px;
  border-radius: 10px;
  cursor: pointer;
}
.secondary-button {
  border: 1px solid var(--color-border);
  color: var(--color-blue);
  background: var(--color-surface);
}
.primary-button {
  border: 0;
  color: white;
  background: linear-gradient(180deg, #2686ff, #126be8);
  box-shadow: 0 7px 18px rgba(18, 107, 232, 0.2);
  font-weight: 650;
}
button:disabled {
  cursor: not-allowed;
  opacity: 0.48;
}
.workflow-error {
  margin-top: 14px;
  padding: 10px 12px;
  border-radius: 10px;
  color: #b42318;
  background: color-mix(in srgb, #ff453a 10%, var(--color-surface));
  font-size: 0.78rem;
}
.preview-summary {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 9px;
  margin-top: 18px;
}
.preview-summary > div {
  padding: 12px 14px;
  border: 1px solid color-mix(in srgb, var(--color-border) 68%, transparent);
  border-radius: 12px;
  background: var(--color-surface-muted);
}
.preview-summary span,
.preview-summary strong {
  display: block;
}
.preview-summary span {
  color: var(--color-text-secondary);
  font-size: 0.7rem;
}
.preview-summary strong {
  margin-top: 6px;
  font-size: 1rem;
}
.risk-groups {
  display: grid;
  gap: 17px;
  margin-top: 20px;
}
.risk-heading {
  justify-content: space-between;
  padding: 0 2px 7px;
  color: var(--color-text-secondary);
  font-size: 0.73rem;
}
.risk-heading > div {
  gap: 7px;
}
.risk-heading strong {
  color: var(--color-text);
  font-size: 0.78rem;
}
.risk-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--color-green);
}
[data-risk="review"] .risk-dot {
  background: #ff9f0a;
}
[data-risk="confirmationRequired"] .risk-dot {
  background: #ff453a;
}
.candidate-card {
  margin-bottom: 7px;
  border: 1px solid var(--color-border);
  border-radius: 13px;
  background: var(--color-surface);
  overflow: hidden;
}
.candidate-card summary {
  gap: 11px;
  padding: 12px;
  list-style: none;
  cursor: pointer;
}
.candidate-card summary::-webkit-details-marker {
  display: none;
}
.candidate-check input {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}
.candidate-check span {
  width: 31px;
  height: 31px;
  display: grid;
  place-items: center;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  color: var(--color-text-secondary);
  background: var(--color-surface-muted);
}
.candidate-check input:checked + span {
  color: white;
  border-color: var(--color-blue);
  background: var(--color-blue);
}
.candidate-check input:focus-visible + span {
  outline: 2px solid var(--color-blue);
  outline-offset: 2px;
}
.candidate-copy {
  min-width: 0;
  display: grid;
  gap: 3px;
  flex: 1;
}
.candidate-copy strong {
  font-size: 0.83rem;
}
.candidate-copy span {
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.candidate-stats {
  display: grid;
  gap: 2px;
  min-width: 96px;
  text-align: right;
}
.candidate-stats strong {
  font-size: 0.83rem;
}
.candidate-stats span {
  color: var(--color-text-secondary);
  font-size: 0.68rem;
}
.detail-chevron {
  color: var(--color-text-secondary);
  transition: transform 0.2s ease;
}
.candidate-card[open] .detail-chevron,
.rule-statuses[open] summary > svg:last-child {
  transform: rotate(180deg);
}
.candidate-detail {
  padding: 2px 14px 14px 54px;
  border-top: 1px solid color-mix(in srgb, var(--color-border) 70%, transparent);
}
.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 8px;
  margin-top: 12px;
}
.detail-grid > div {
  padding: 9px 10px;
  border-radius: 9px;
  background: var(--color-surface-muted);
}
.detail-grid span,
.detail-grid strong {
  display: block;
}
.detail-grid span {
  color: var(--color-text-secondary);
  font-size: 0.66rem;
}
.detail-grid strong {
  margin-top: 3px;
  font-size: 0.72rem;
}
.candidate-detail code {
  display: block;
  overflow: auto;
  margin-top: 10px;
  padding: 9px;
  border-radius: 8px;
  color: var(--color-text-secondary);
  background: var(--color-surface-muted);
  font-family: inherit;
  font-size: 0.68rem;
  white-space: nowrap;
}
.candidate-detail ul {
  margin: 9px 0 0;
  padding-left: 18px;
  color: var(--color-text-secondary);
  font-size: 0.7rem;
}
.admin-note {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 9px;
  color: #b86a00;
  font-size: 0.7rem;
}
.rule-statuses {
  margin-top: 15px;
  border: 1px solid var(--color-border);
  border-radius: 12px;
}
.rule-statuses summary {
  gap: 8px;
  padding: 11px 13px;
  cursor: pointer;
  list-style: none;
  font-size: 0.74rem;
}
.rule-statuses summary span {
  margin-left: auto;
  color: var(--color-text-secondary);
}
.rule-status-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 7px;
  padding: 0 11px 11px;
}
.rule-status-grid > div {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 3px 8px;
  padding: 9px 10px;
  border-radius: 9px;
  background: var(--color-surface-muted);
  font-size: 0.7rem;
}
.rule-status-grid span,
.rule-status-grid small {
  color: var(--color-text-secondary);
}
.rule-status-grid small {
  grid-column: 1 / -1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.workflow-footer {
  justify-content: space-between;
  gap: 16px;
  margin-top: 17px;
  padding: 14px 0 0;
  border-top: 1px solid var(--color-border);
}
.workflow-footer > div {
  display: grid;
  gap: 3px;
}
.workflow-footer span {
  color: var(--color-text-secondary);
  font-size: 0.7rem;
}
.workflow-footer strong {
  font-size: 1.05rem;
}
.workflow-columns {
  display: grid;
  grid-template-columns: 1fr 1.15fr;
  gap: 10px;
  margin-top: 15px;
}
.workflow-card {
  min-height: 148px;
  padding: 14px;
  border: 1px solid var(--color-border);
  border-radius: 14px;
  background: linear-gradient(
    145deg,
    var(--color-surface),
    var(--color-surface-muted)
  );
}
.workflow-card-heading {
  justify-content: space-between;
  font-size: 0.75rem;
}
.workflow-card-heading > div {
  gap: 7px;
}
.workflow-card-heading > span {
  color: var(--color-text-secondary);
  font-size: 0.64rem;
}
.workflow-value {
  display: block;
  margin-top: 16px;
  font-size: 1.15rem;
}
.workflow-card button {
  margin-top: 15px;
  border: 1px solid var(--color-border);
  color: var(--color-blue);
  background: var(--color-surface);
  font-size: 0.72rem;
}
.audit-list {
  display: grid;
  gap: 5px;
  margin-top: 10px;
}
.audit-list > div {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 2px 8px;
  padding: 6px 8px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--color-surface) 75%, transparent);
  font-size: 0.68rem;
}
.audit-list span,
.audit-list small {
  color: var(--color-text-secondary);
}
.audit-list small {
  grid-column: 1 / -1;
}
.preview-note {
  gap: 7px;
  margin-top: 14px;
  color: var(--color-text-secondary);
  font-size: 0.7rem;
}
.plan-note {
  display: grid;
  gap: 3px;
  margin-top: 10px;
  padding: 10px 12px;
  border-radius: 10px;
  color: var(--color-text-secondary);
  background: var(--color-blue-soft);
  font-size: 0.7rem;
}
.plan-note strong {
  color: var(--color-text);
}
.empty-preview {
  min-height: 90px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}
@media (max-width: 820px) {
  .preview-summary {
    grid-template-columns: 1fr 1fr;
  }
  .workflow-columns {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 620px) {
  .cleanup-card {
    padding: 16px;
  }
  .cleanup-heading {
    flex-wrap: wrap;
  }
  .secondary-button {
    margin-left: 50px;
  }
  .candidate-stats {
    min-width: 74px;
  }
  .candidate-copy span {
    display: none;
  }
  .candidate-detail {
    padding-left: 14px;
  }
  .detail-grid,
  .rule-status-grid {
    grid-template-columns: 1fr;
  }
  .workflow-footer {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
