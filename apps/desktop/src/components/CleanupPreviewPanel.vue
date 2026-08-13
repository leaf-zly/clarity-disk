<script setup lang="ts">
import { computed } from "vue";
import { Eye, FolderSearch, Info, ShieldCheck } from "@lucide/vue";

import type {
  CleanupPlan,
  CleanupPreview,
  SuggestionRisk,
} from "@/types/dashboard";
import { formatBytes } from "@/utils/format-bytes";

/** Read-only preview data and scan state rendered by the panel. */
interface Props {
  preview: CleanupPreview;
  isLoading: boolean;
  plan: CleanupPlan | undefined;
  isPreparingPlan: boolean;
}

/** User action emitted when the allow-listed scan should run again. */
interface Emits {
  "request-scan": [];
  "prepare-plan": [];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();

const selectedBytes = computed(() =>
  props.preview.candidates
    .filter((candidate) => candidate.defaultSelected)
    .reduce((total, candidate) => total + candidate.bytes, 0),
);

function riskLabel(risk: SuggestionRisk): string {
  if (risk === "safe") return "低风险";
  if (risk === "review") return "需要复核";
  return "需单独确认";
}
</script>

<template>
  <section class="cleanup-card" aria-labelledby="cleanup-preview-title">
    <div class="cleanup-heading">
      <div class="heading-icon">
        <FolderSearch :size="19" aria-hidden="true" />
      </div>
      <div>
        <h2 id="cleanup-preview-title">清理预览</h2>
        <p>{{ preview.scan.message }} · 只读扫描，不会删除文件</p>
      </div>
      <button
        class="rescan-button"
        type="button"
        :disabled="isLoading"
        @click="emit('request-scan')"
      >
        {{ isLoading ? "扫描中" : "重新扫描" }}
      </button>
      <button
        class="plan-button"
        type="button"
        :disabled="isPreparingPlan"
        @click="emit('prepare-plan')"
      >
        {{ isPreparingPlan ? "生成中" : "生成安全计划" }}
      </button>
    </div>

    <div class="preview-summary">
      <div>
        <span>预计可释放</span>
        <strong>{{ formatBytes(preview.totalReclaimableBytes) }}</strong>
      </div>
      <div>
        <span>默认安全项目</span>
        <strong>{{ formatBytes(selectedBytes) }}</strong>
      </div>
      <div>
        <span>已扫描项目</span>
        <strong>{{ preview.scan.scannedItems.toLocaleString() }}</strong>
      </div>
    </div>

    <div v-if="preview.candidates.length" class="candidate-list">
      <article
        v-for="candidate in preview.candidates"
        :key="candidate.id"
        class="candidate-row"
      >
        <div class="candidate-icon">
          <ShieldCheck :size="17" aria-hidden="true" />
        </div>
        <div class="candidate-copy">
          <strong>{{ candidate.title }}</strong>
          <span>{{ candidate.description }}</span>
          <code>{{ candidate.path }}</code>
        </div>
        <span class="candidate-risk">{{ riskLabel(candidate.risk) }}</span>
        <strong class="candidate-size">{{
          formatBytes(candidate.bytes)
        }}</strong>
      </article>
    </div>
    <div v-else class="empty-preview">
      <Eye :size="18" aria-hidden="true" />
      <span>未发现允许预览的清理项目</span>
    </div>

    <div class="preview-note">
      <Info :size="16" aria-hidden="true" />
      <span
        >当前阶段只生成清理建议，执行按钮将在安全计划和隔离区完成后开放。</span
      >
    </div>
    <div v-if="plan" class="plan-note" role="status">
      <strong>计划已生成，仅供复核</strong>
      <span>摘要 {{ plan.planDigest }} · 未授权执行</span>
    </div>
  </section>
</template>

<style scoped>
.cleanup-card {
  margin-top: 14px;
  padding: 20px 21px;
  border: 1px solid var(--color-border);
  border-radius: 16px;
  background: var(--color-surface);
}
.cleanup-heading,
.preview-summary,
.candidate-row,
.preview-note {
  display: flex;
  align-items: center;
}
.cleanup-heading {
  gap: 11px;
}
.heading-icon,
.candidate-icon {
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  border-radius: 9px;
  color: var(--color-blue);
  background: var(--color-blue-soft);
}
.heading-icon {
  width: 34px;
  height: 34px;
}
.cleanup-heading h2 {
  font-size: 1rem;
}
.cleanup-heading p {
  margin-top: 4px;
  color: var(--color-text-secondary);
  font-size: 0.8rem;
}
.rescan-button {
  margin-left: auto;
  padding: 7px 11px;
  border: 1px solid var(--color-border);
  border-radius: 8px;
  color: var(--color-blue);
  background: transparent;
  cursor: pointer;
}
.plan-button {
  margin-left: 6px;
  padding: 7px 11px;
  border: 0;
  border-radius: 8px;
  color: white;
  background: var(--color-blue);
  cursor: pointer;
}
.plan-button:disabled {
  cursor: progress;
  opacity: 0.6;
}
.rescan-button:disabled {
  cursor: progress;
  opacity: 0.6;
}
.preview-summary {
  gap: 10px;
  margin-top: 17px;
}
.preview-summary > div {
  min-width: 0;
  flex: 1;
  padding: 11px 13px;
  border-radius: 10px;
  background: var(--color-surface-muted);
}
.preview-summary span,
.preview-summary strong {
  display: block;
}
.preview-summary span {
  color: var(--color-text-secondary);
  font-size: 0.76rem;
}
.preview-summary strong {
  margin-top: 6px;
  font-size: 1.05rem;
}
.candidate-list {
  display: grid;
  gap: 7px;
  margin-top: 14px;
}
.candidate-row {
  gap: 10px;
  padding: 11px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
}
.candidate-icon {
  width: 29px;
  height: 29px;
  color: var(--color-green);
  background: color-mix(in srgb, var(--color-green) 12%, var(--color-surface));
}
.candidate-copy {
  min-width: 0;
  display: grid;
  gap: 3px;
  flex: 1;
}
.candidate-copy strong,
.candidate-copy span,
.candidate-copy code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.candidate-copy strong {
  font-size: 0.86rem;
}
.candidate-copy span,
.candidate-copy code {
  color: var(--color-text-secondary);
  font-size: 0.75rem;
}
.candidate-copy code {
  font-family: inherit;
}
.candidate-risk {
  padding: 4px 7px;
  border-radius: 999px;
  color: var(--color-green);
  background: color-mix(in srgb, var(--color-green) 10%, var(--color-surface));
  font-size: 0.72rem;
}
.candidate-size {
  flex: 0 0 auto;
  font-size: 0.84rem;
}
.empty-preview {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 82px;
  margin-top: 14px;
  color: var(--color-text-secondary);
  font-size: 0.82rem;
}
.preview-note {
  gap: 7px;
  margin-top: 14px;
  padding-top: 13px;
  border-top: 1px solid var(--color-border);
  color: var(--color-text-secondary);
  font-size: 0.75rem;
}
.plan-note {
  display: grid;
  gap: 3px;
  margin-top: 10px;
  padding: 10px 12px;
  border-radius: 9px;
  color: var(--color-text-secondary);
  background: var(--color-surface-muted);
  font-size: 0.75rem;
}
.plan-note strong {
  color: var(--color-text-primary);
}
@media (max-width: 700px) {
  .preview-summary {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .preview-summary > div:last-child {
    grid-column: span 2;
  }
  .candidate-risk {
    display: none;
  }
}
</style>
