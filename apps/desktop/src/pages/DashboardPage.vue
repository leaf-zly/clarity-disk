<script setup lang="ts">
import {
  computed,
  nextTick,
  onMounted,
  shallowRef,
  toRaw,
  useTemplateRef,
  watch,
} from "vue";
import {
  CalendarClock,
  ChevronRight,
  HeartPulse,
  LoaderCircle,
  ScanSearch,
  TrendingUp,
  WandSparkles,
} from "@lucide/vue";

import CleanupExecutionPanel from "@/components/CleanupExecutionPanel.vue";
import CleanupPreviewPanel from "@/components/CleanupPreviewPanel.vue";
import DiskUsageCard from "@/components/DiskUsageCard.vue";
import DiskVolumeList from "@/components/DiskVolumeList.vue";
import MetricCard from "@/components/MetricCard.vue";
import SpaceScanPanel from "@/components/SpaceScanPanel.vue";
import SuggestionItem from "@/components/SuggestionItem.vue";
import { useCleanupExecution } from "@/composables/use-cleanup-execution";
import { useCleanupWorkflow } from "@/composables/use-cleanup-workflow";
import { useSpaceScan } from "@/composables/use-space-scan";
import {
  getDefaultSpaceScanRequest,
  loadDashboardSnapshot,
} from "@/services/dashboard-service";
import type { CleanupPlan, DashboardSnapshot } from "@/types/dashboard";
import type {
  CleanupExecutionMode,
  QuarantinePolicy,
} from "@/types/cleanup-execution";
import type { DashboardSection } from "@/types/navigation";

/** Sidebar-selected dashboard subsection displayed by this shared workspace. */
interface Props {
  section?: DashboardSection;
}

/** Navigation intents produced by actions inside the dashboard. */
interface Emits {
  navigate: [section: DashboardSection];
}

const props = withDefaults(defineProps<Props>(), { section: "overview" });
const emit = defineEmits<Emits>();
const overviewSection = useTemplateRef<HTMLElement>("overviewSection");
const cleanupSection = useTemplateRef<HTMLElement>("cleanupSection");
const spaceSection = useTemplateRef<HTMLElement>("spaceSection");

const snapshot = shallowRef<DashboardSnapshot>();
const loadError = shallowRef<string>();
const isLoading = shallowRef(true);
const selectedDiskId = shallowRef<string>();
const scanRoot = shallowRef("C:\\");
const scanMaxDepth = shallowRef(8);
const scanMaxEntries = shallowRef(100_000);
const excludedPaths = shallowRef("");
const {
  snapshot: spaceScan,
  history: spaceScanHistory,
  error: spaceScanError,
  isStarting: isStartingSpaceScan,
  start: startScan,
  pause: pauseScan,
  resume: resumeScan,
  cancel: cancelScan,
  refreshHistory: refreshSpaceScanHistory,
} = useSpaceScan();
const {
  preview: cleanupPreview,
  plan: cleanupPlan,
  quarantine,
  auditEvents,
  selectedIds,
  selectedBytes,
  highestRisk,
  error: cleanupError,
  isScanning: isCleanupLoading,
  isPreparingPlan,
  isPreparingQuarantine,
  scan: refreshCleanupPreview,
  setSelected: setCleanupSelected,
  createPlan: preparePlan,
  createQuarantineIndex,
  refreshAuxiliaryState: refreshCleanupAuxiliaryState,
} = useCleanupWorkflow();
const {
  challenge: executionChallenge,
  report: executionReport,
  quarantine: executionQuarantine,
  error: executionError,
  isPreparing: isPreparingExecution,
  isExecuting,
  restoringEntryId,
  isRestoringBatch,
  isUpdatingPolicy,
  prepare: prepareExecution,
  execute: executeCleanup,
  restore: restoreQuarantineEntry,
  restoreBatch: restoreQuarantineBatch,
  updatePolicy: updateQuarantinePolicy,
  refresh: refreshExecutionQuarantine,
} = useCleanupExecution();
const selectedDisk = computed(() => {
  if (!snapshot.value) return undefined;
  return (
    snapshot.value.disks.find((disk) => disk.id === selectedDiskId.value) ??
    snapshot.value.disk
  );
});
const pageCopy = computed(() => {
  const copy: Record<DashboardSection, { title: string; description: string }> =
    {
      overview: {
        title: "下午好",
        description: "查看磁盘容量、清理建议和维护状态。",
      },
      space: {
        title: "空间分析",
        description: "按受限范围执行只读扫描，定位空间占用。",
      },
      cleanup: {
        title: "智能清理",
        description: "先预览、再确认；所有候选均经过后端安全校验。",
      },
      "large-files": {
        title: "大文件",
        description: "完成只读空间分析后查看最大的文件与目录。",
      },
    };
  return copy[props.section];
});

/** Refreshes dashboard discovery while preserving user-readable errors. */
async function refreshDashboard(): Promise<void> {
  isLoading.value = true;
  loadError.value = undefined;
  try {
    snapshot.value = await loadDashboardSnapshot();
    selectedDiskId.value ??= snapshot.value.disk.id;
  } catch {
    loadError.value = "暂时无法读取磁盘状态，请稍后重试。";
  } finally {
    isLoading.value = false;
  }
}

/** Starts and polls a bounded read-only scan until a terminal state. */
async function startSpaceAnalysis(): Promise<void> {
  const excluded = excludedPaths.value
    .split(/\r?\n/)
    .map((path) => path.trim())
    .filter(Boolean);
  await startScan({
    rootPath: scanRoot.value.trim(),
    maxDepth: scanMaxDepth.value,
    maxEntries: scanMaxEntries.value,
    excludedPaths: excluded,
  });
}

/** Applies backend-generated defaults when the selected volume changes. */
async function applySelectedVolumeScope(): Promise<void> {
  const rootPath = selectedDisk.value?.metadata.mountPoint;
  if (!rootPath) return;
  try {
    const request = await getDefaultSpaceScanRequest(rootPath);
    scanRoot.value = request.rootPath;
    scanMaxDepth.value = request.maxDepth;
    scanMaxEntries.value = request.maxEntries;
    excludedPaths.value = request.excludedPaths.join("\n");
  } catch {
    scanRoot.value = rootPath;
    excludedPaths.value = "";
  }
}

/** Returns a mutable snapshot for service calls without exposing reactive state. */
function currentCleanupPlan(): CleanupPlan | undefined {
  return cleanupPlan.value
    ? structuredClone(toRaw(cleanupPlan.value) as CleanupPlan)
    : undefined;
}

/** Freshly validates executable candidates and requests a one-time challenge. */
async function prepareCleanupExecution(
  mode: CleanupExecutionMode,
): Promise<void> {
  await prepareExecution(currentCleanupPlan(), mode);
  await refreshCleanupAuxiliaryState().catch(() => undefined);
}

/** Submits the exact confirmation phrase with the current one-time challenge. */
async function executeConfirmedCleanup(
  confirmationPhrase: string,
): Promise<void> {
  await executeCleanup(currentCleanupPlan(), confirmationPhrase);
  await refreshCleanupAuxiliaryState().catch(() => undefined);
}

/** Restores a bounded backend-ID selection and refreshes the audit timeline. */
async function restoreCleanupBatch(entryIds: string[]): Promise<void> {
  await restoreQuarantineBatch(entryIds);
  await refreshCleanupAuxiliaryState().catch(() => undefined);
}

/** Updates fixed-tier quarantine policy without deleting content. */
async function updateCleanupPolicy(policy: QuarantinePolicy): Promise<void> {
  await updateQuarantinePolicy(policy);
  await refreshCleanupAuxiliaryState().catch(() => undefined);
}

/** Restores one backend entry and refreshes the shared audit timeline. */
async function restoreCleanupEntry(entryId: string): Promise<void> {
  await restoreQuarantineEntry(entryId);
  await refreshCleanupAuxiliaryState().catch(() => undefined);
}

watch(selectedDiskId, () => void applySelectedVolumeScope());
watch(
  () => props.section,
  () => void focusSection("smooth"),
  { flush: "post" },
);

onMounted(() => void initializeDashboard());

async function initializeDashboard(): Promise<void> {
  await refreshDashboard();
  await Promise.allSettled([
    refreshCleanupPreview(),
    refreshSpaceScanHistory(),
    refreshExecutionQuarantine(),
    applySelectedVolumeScope(),
  ]);
  await focusSection("auto");
}

/** Scrolls the shared dashboard to the destination represented by the sidebar. */
async function focusSection(behavior: ScrollBehavior): Promise<void> {
  await nextTick();
  const target =
    props.section === "overview"
      ? overviewSection.value
      : props.section === "cleanup"
        ? cleanupSection.value
        : spaceSection.value;
  target?.scrollIntoView?.({ behavior, block: "start" });
}
</script>

<template>
  <section class="dashboard" aria-labelledby="dashboard-title">
    <header class="page-header">
      <div>
        <h1 id="dashboard-title">{{ pageCopy.title }}</h1>
        <p>{{ pageCopy.description }}</p>
      </div>
      <button
        class="scan-button"
        type="button"
        :disabled="isLoading"
        @click="refreshDashboard"
      >
        <LoaderCircle
          v-if="isLoading"
          class="spin"
          :size="18"
          aria-hidden="true"
        />
        <ScanSearch v-else :size="18" aria-hidden="true" />
        {{ isLoading ? "正在扫描" : "开始智能扫描" }}
      </button>
    </header>

    <div v-if="loadError" class="error-state" role="alert">
      <span>{{ loadError }}</span
      ><button type="button" @click="refreshDashboard">重试</button>
    </div>

    <template v-else-if="snapshot">
      <div ref="overviewSection" class="dashboard-section">
        <DiskUsageCard
          :disk="selectedDisk ?? snapshot.disk"
          :reclaimable-bytes="snapshot.cleanup.reclaimableBytes"
          @open-cleanup="emit('navigate', 'cleanup')"
        />
        <DiskVolumeList
          :disks="snapshot.disks"
          :active-disk-id="selectedDiskId ?? snapshot.disk.id"
          @select-disk="(disk) => (selectedDiskId = disk.id)"
        />
      </div>

      <div ref="cleanupSection" class="dashboard-section">
        <CleanupPreviewPanel
          v-if="cleanupPreview"
          :preview="cleanupPreview"
          :selected-ids="selectedIds"
          :selected-bytes="selectedBytes"
          :highest-risk="highestRisk"
          :is-loading="isCleanupLoading"
          :error="cleanupError"
          :plan="cleanupPlan"
          :quarantine="quarantine"
          :audit-events="auditEvents"
          :is-preparing-plan="isPreparingPlan"
          :is-preparing-quarantine="isPreparingQuarantine"
          @request-scan="refreshCleanupPreview"
          @update-selection="setCleanupSelected"
          @prepare-plan="preparePlan"
          @prepare-quarantine="createQuarantineIndex"
        />

        <CleanupExecutionPanel
          :plan="cleanupPlan"
          :challenge="executionChallenge"
          :report="executionReport"
          :quarantine="executionQuarantine"
          :error="executionError"
          :is-preparing="isPreparingExecution"
          :is-executing="isExecuting"
          :restoring-entry-id="restoringEntryId"
          :is-restoring-batch="isRestoringBatch"
          :is-updating-policy="isUpdatingPolicy"
          @prepare-execution="prepareCleanupExecution"
          @execute="executeConfirmedCleanup"
          @restore="restoreCleanupEntry"
          @restore-batch="restoreCleanupBatch"
          @update-policy="updateCleanupPolicy"
        />
      </div>

      <div ref="spaceSection" class="dashboard-section">
        <SpaceScanPanel
          :snapshot="spaceScan"
          :history="spaceScanHistory"
          :error="spaceScanError"
          :is-starting="isStartingSpaceScan"
          v-model:scan-root="scanRoot"
          v-model:max-depth="scanMaxDepth"
          v-model:max-entries="scanMaxEntries"
          v-model:excluded-paths="excludedPaths"
          @start-scan="startSpaceAnalysis"
          @pause-scan="pauseScan"
          @resume-scan="resumeScan"
          @cancel-scan="cancelScan"
        />
      </div>

      <div class="section-heading">
        <h2>状态概览</h2>
        <button type="button">
          查看报告 <ChevronRight :size="15" aria-hidden="true" />
        </button>
      </div>
      <div class="metrics-grid">
        <MetricCard
          label="磁盘健康"
          :value="snapshot.health.status"
          :description="
            snapshot.health.deviceType +
            ' · ' +
            (snapshot.health.temperatureCelsius ?? '—') +
            '°C · 无异常'
          "
          tone="green"
          :icon="HeartPulse"
        />
        <MetricCard
          label="上次清理"
          value="21.04 GB"
          description="昨天 17:13 · 已安全完成"
          tone="blue"
          :icon="WandSparkles"
        />
        <MetricCard
          label="自动维护"
          value="每周"
          description="仅清理安全缓存"
          tone="orange"
          :icon="CalendarClock"
        />
      </div>

      <div class="section-heading">
        <h2>智能建议</h2>
        <button type="button">
          全部建议 <ChevronRight :size="15" aria-hidden="true" />
        </button>
      </div>
      <div class="recommendations-grid">
        <div class="suggestions-panel">
          <SuggestionItem
            v-for="suggestion in snapshot.suggestions"
            :key="suggestion.id"
            :suggestion="suggestion"
          />
        </div>
        <aside class="monthly-card">
          <div class="monthly-label">
            <span>本月累计释放</span
            ><TrendingUp :size="18" aria-hidden="true" />
          </div>
          <strong>28.6 GB</strong>
          <p>相当于约 7,300 张高清照片</p>
        </aside>
      </div>
    </template>

    <div v-else class="loading-state" aria-live="polite">
      <LoaderCircle class="spin" :size="24" aria-hidden="true" /><span
        >正在读取磁盘状态</span
      >
    </div>
  </section>
</template>

<style scoped>
.dashboard {
  max-width: 1180px;
  margin: 0 auto;
}
.dashboard-section {
  scroll-margin-top: 24px;
}
.page-header,
.section-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
}
.page-header {
  margin-bottom: 25px;
}
.page-header h1 {
  font-size: clamp(1.8rem, 3vw, 2.35rem);
  letter-spacing: -0.035em;
}
.page-header p {
  margin-top: 6px;
  color: var(--color-text-secondary);
}
.scan-button {
  min-height: 43px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 17px;
  border: 0;
  border-radius: 11px;
  color: white;
  background: linear-gradient(180deg, #2584ff, #126ae5);
  box-shadow:
    0 7px 18px rgba(18, 106, 229, 0.22),
    inset 0 1px 0 rgba(255, 255, 255, 0.28);
  font-weight: 600;
  cursor: pointer;
}
.scan-button:disabled {
  cursor: progress;
  opacity: 0.75;
}
.section-heading {
  margin: 27px 2px 13px;
}
.section-heading h2 {
  font-size: 1rem;
}
.section-heading button {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 5px;
  border: 0;
  color: var(--color-blue);
  background: transparent;
  cursor: pointer;
}
.metrics-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
}
.recommendations-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.18fr) minmax(270px, 0.82fr);
  gap: 14px;
}
.suggestions-panel,
.monthly-card {
  border: 1px solid var(--color-border);
  border-radius: 15px;
  background: var(--color-surface);
}
.suggestions-panel {
  padding: 5px 18px;
}
.monthly-card {
  min-height: 184px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  padding: 21px;
  background: linear-gradient(
    155deg,
    var(--color-surface),
    var(--color-surface-muted)
  );
}
.monthly-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--color-text-secondary);
}
.monthly-card strong {
  margin: 18px 0 5px;
  font-size: clamp(1.8rem, 3vw, 2.35rem);
  letter-spacing: -0.04em;
}
.monthly-card p {
  color: var(--color-text-secondary);
}
.loading-state,
.error-state {
  min-height: 300px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  border: 1px solid var(--color-border);
  border-radius: 18px;
  color: var(--color-text-secondary);
  background: var(--color-surface);
}
.error-state button {
  border: 0;
  color: var(--color-blue);
  background: transparent;
  cursor: pointer;
}
.spin {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
@media (max-width: 1080px) {
  .recommendations-grid {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 820px) {
  .metrics-grid {
    grid-template-columns: 1fr;
  }
}
</style>
