<script setup lang="ts">
import { computed, onMounted, shallowRef } from "vue";
import {
  AlertTriangle,
  ArrowRight,
  Check,
  CircleX,
  Download,
  Eye,
  HardDrive,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
} from "@lucide/vue";

import PartitionTopologyBar from "@/components/PartitionTopologyBar.vue";
import {
  downloadPartitionPreviewReport,
  loadPartitionTopology,
  previewPartitionMerge,
} from "@/services/partition-service";
import type {
  MergePreview,
  PartitionDescriptor,
  PartitionTopology,
  PhysicalDisk,
} from "@/types/partition";
import { formatBytes } from "@/utils/format-bytes";

const topology = shallowRef<PartitionTopology>();
const preview = shallowRef<MergePreview>();
const selectedDiskId = shallowRef<string>();
const targetPartitionId = shallowRef<string>();
const sourcePartitionId = shallowRef<string>();
const selectedPartitionId = shallowRef<string>();
const loadError = shallowRef<string>();
const previewError = shallowRef<string>();
const isLoading = shallowRef(true);
const isPreviewing = shallowRef(false);

const selectedDisk = computed(() =>
  topology.value?.disks.find((disk) => disk.id === selectedDiskId.value),
);
const selectablePartitions = computed(() =>
  (selectedDisk.value?.partitions ?? []).filter(isSelectableDataPartition),
);
const selectedIds = computed(() =>
  [targetPartitionId.value, sourcePartitionId.value].filter(
    (id): id is string => Boolean(id),
  ),
);
const selectedPartition = computed(() =>
  selectedDisk.value?.partitions.find(
    (partition) => partition.id === selectedPartitionId.value,
  ),
);
const targetPartition = computed(() =>
  selectedDisk.value?.partitions.find(
    (partition) => partition.id === targetPartitionId.value,
  ),
);
const sourcePartition = computed(() =>
  selectedDisk.value?.partitions.find(
    (partition) => partition.id === sourcePartitionId.value,
  ),
);
const canPreview = computed(
  () =>
    Boolean(targetPartitionId.value && sourcePartitionId.value) &&
    targetPartitionId.value !== sourcePartitionId.value &&
    !isPreviewing.value,
);

/** Refreshes all read-only topology data and discards selections from old state. */
async function refreshTopology(): Promise<void> {
  isLoading.value = true;
  loadError.value = undefined;
  previewError.value = undefined;
  preview.value = undefined;
  try {
    const nextTopology = await loadPartitionTopology();
    if (!nextTopology.readOnly) throw new Error("分区发现接口未处于只读模式");
    topology.value = nextTopology;
    selectedDiskId.value = nextTopology.disks[0]?.id;
    chooseDefaultPair(nextTopology.disks[0]);
  } catch {
    loadError.value = "暂时无法读取分区拓扑，未执行任何磁盘修改。";
  } finally {
    isLoading.value = false;
  }
}

/** Selects a disk and chooses its first safe adjacent preview pair, if present. */
function selectDisk(disk: PhysicalDisk): void {
  selectedDiskId.value = disk.id;
  preview.value = undefined;
  previewError.value = undefined;
  chooseDefaultPair(disk);
}

/** Uses topology order to preselect only a left-target/right-source data pair. */
function chooseDefaultPair(disk: PhysicalDisk | undefined): void {
  targetPartitionId.value = undefined;
  sourcePartitionId.value = undefined;
  selectedPartitionId.value = undefined;
  if (!disk) return;
  for (let index = 0; index < disk.partitions.length - 1; index += 1) {
    const target = disk.partitions[index];
    const source = disk.partitions[index + 1];
    if (
      target &&
      source &&
      isSelectableDataPartition(target) &&
      isSelectableDataPartition(source) &&
      target.startOffsetBytes + target.sizeBytes === source.startOffsetBytes
    ) {
      targetPartitionId.value = target.id;
      sourcePartitionId.value = source.id;
      selectedPartitionId.value = target.id;
      return;
    }
  }
}

/** Handles topology clicks without allowing protected regions as inputs. */
function selectPartition(partitionId: string): void {
  const partition = selectedDisk.value?.partitions.find(
    (item) => item.id === partitionId,
  );
  if (!partition || !isSelectableDataPartition(partition)) return;
  selectedPartitionId.value = partitionId;
  preview.value = undefined;
  previewError.value = undefined;
  if (!targetPartitionId.value || sourcePartitionId.value) {
    targetPartitionId.value = partitionId;
    sourcePartitionId.value = undefined;
    return;
  }
  if (targetPartitionId.value !== partitionId)
    sourcePartitionId.value = partitionId;
}

/** Keeps role selectors mutually exclusive and invalidates the old preview. */
function updateSelection(role: "target" | "source", value: string): void {
  preview.value = undefined;
  previewError.value = undefined;
  if (role === "target") {
    targetPartitionId.value = value || undefined;
    if (sourcePartitionId.value === value) sourcePartitionId.value = undefined;
  } else {
    sourcePartitionId.value = value || undefined;
    if (targetPartitionId.value === value) targetPartitionId.value = undefined;
  }
  selectedPartitionId.value = value || undefined;
}

/** Requests backend re-discovery and renders its non-authorizing result. */
async function runPreview(): Promise<void> {
  if (!targetPartitionId.value || !sourcePartitionId.value) return;
  isPreviewing.value = true;
  previewError.value = undefined;
  try {
    const result = await previewPartitionMerge({
      targetPartitionId: targetPartitionId.value,
      sourcePartitionId: sourcePartitionId.value,
    });
    // A backend regression must fail closed even if the rest of the payload is valid.
    if (result.executionAuthorized) throw new Error("只读预演意外返回执行授权");
    preview.value = result;
  } catch {
    preview.value = undefined;
    previewError.value = "预演未完成，磁盘状态可能已变化，请刷新后重试。";
  } finally {
    isPreviewing.value = false;
  }
}

/** Returns true only for ordinary data partitions eligible for selection. */
function isSelectableDataPartition(partition: PartitionDescriptor): boolean {
  return partition.kind === "data" && !partition.isSystem && !partition.isBoot;
}

/** Builds a stable human-readable partition name without exposing it as input. */
function partitionName(partition: PartitionDescriptor | undefined): string {
  if (!partition) return "未选择";
  return (
    partition.label ||
    partition.mountPoints[0] ||
    `分区 ${partition.partitionNumber}`
  );
}

/** Converts fixed backend enum values into concise Chinese labels. */
function signalLabel(value: string): string {
  const labels: Readonly<Record<string, string>> = {
    healthy: "良好",
    warning: "需注意",
    unhealthy: "异常",
    unknown: "未知",
    notApplicable: "不适用",
    off: "关闭",
    on: "已开启",
    suspended: "已暂停",
    none: "无",
    present: "存在",
    online: "在线",
    offline: "离线",
  };
  return labels[value] ?? value;
}

onMounted(refreshTopology);
</script>

<template>
  <section class="partition-page" aria-labelledby="partition-page-title">
    <header class="page-header">
      <div>
        <div class="eyebrow"><Eye :size="14" aria-hidden="true" />只读模式</div>
        <h1 id="partition-page-title">分区预演</h1>
        <p>先看清物理布局与阻塞原因。这里没有删除、格式化或执行入口。</p>
      </div>
      <button
        class="secondary-button"
        type="button"
        :disabled="isLoading"
        @click="refreshTopology"
      >
        <LoaderCircle
          v-if="isLoading"
          class="spin"
          :size="17"
          aria-hidden="true"
        />
        <RefreshCw v-else :size="17" aria-hidden="true" />
        {{ isLoading ? "正在读取" : "刷新拓扑" }}
      </button>
    </header>

    <div v-if="loadError" class="state-card error-card" role="alert">
      <CircleX :size="20" aria-hidden="true" />
      <span>{{ loadError }}</span>
      <button type="button" @click="refreshTopology">重试</button>
    </div>

    <template v-else-if="topology && selectedDisk">
      <div v-if="topology.discoveryWarnings.length" class="warning-list">
        <div
          v-for="warning in topology.discoveryWarnings"
          :key="warning"
          class="state-card warning-card"
          role="status"
        >
          <AlertTriangle :size="18" aria-hidden="true" />{{ warning }}
        </div>
      </div>

      <section class="disk-selector" aria-label="物理磁盘">
        <button
          v-for="disk in topology.disks"
          :key="disk.id"
          class="disk-pill"
          :class="{ active: disk.id === selectedDisk.id }"
          type="button"
          :aria-pressed="disk.id === selectedDisk.id"
          @click="selectDisk(disk)"
        >
          <HardDrive :size="18" aria-hidden="true" />
          <span>
            <strong>磁盘 {{ disk.number }}</strong>
            <small
              >{{ disk.friendlyName }} ·
              {{ formatBytes(disk.sizeBytes) }}</small
            >
          </span>
        </button>
      </section>

      <section
        class="surface topology-card"
        aria-labelledby="current-layout-title"
      >
        <div class="section-heading">
          <div>
            <span class="section-kicker">当前布局</span>
            <h2 id="current-layout-title">
              磁盘 {{ selectedDisk.number }} · {{ selectedDisk.partitionStyle }}
            </h2>
          </div>
          <div class="disk-signals">
            <span>{{ selectedDisk.busType }}</span>
            <span>{{ signalLabel(selectedDisk.health) }}</span>
            <span>{{ selectedDisk.partitions.length }} 个区域</span>
          </div>
        </div>
        <PartitionTopologyBar
          :partitions="selectedDisk.partitions"
          :disk-size-bytes="selectedDisk.sizeBytes"
          :selected-ids="selectedIds"
          interactive
          @select-partition="selectPartition"
        />
        <p class="topology-help">
          点击普通数据分区可依次选择目标与源；带纹理区域是受保护或未分配空间。
        </p>
      </section>

      <div class="workspace-grid">
        <section
          class="surface selection-card"
          aria-labelledby="selection-title"
        >
          <div class="section-heading compact">
            <div>
              <span class="section-kicker">合并方向</span>
              <h2 id="selection-title">选择相邻数据分区</h2>
            </div>
            <span class="readonly-badge"><ShieldCheck :size="14" />仅预演</span>
          </div>

          <div class="selection-flow">
            <label>
              <span>左侧目标分区</span>
              <select
                :value="targetPartitionId"
                @change="
                  updateSelection(
                    'target',
                    ($event.target as HTMLSelectElement).value,
                  )
                "
              >
                <option value="">请选择</option>
                <option
                  v-for="partition in selectablePartitions"
                  :key="partition.id"
                  :value="partition.id"
                >
                  {{ partitionName(partition) }} ·
                  {{ formatBytes(partition.sizeBytes) }}
                </option>
              </select>
            </label>
            <ArrowRight class="flow-arrow" :size="20" aria-hidden="true" />
            <label>
              <span>右侧源分区</span>
              <select
                :value="sourcePartitionId"
                @change="
                  updateSelection(
                    'source',
                    ($event.target as HTMLSelectElement).value,
                  )
                "
              >
                <option value="">请选择</option>
                <option
                  v-for="partition in selectablePartitions"
                  :key="partition.id"
                  :value="partition.id"
                >
                  {{ partitionName(partition) }} ·
                  {{ formatBytes(partition.sizeBytes) }}
                </option>
              </select>
            </label>
          </div>

          <div class="selection-summary">
            <span>{{ partitionName(targetPartition) }}</span>
            <ArrowRight :size="16" aria-hidden="true" />
            <span>{{ partitionName(sourcePartition) }}</span>
          </div>
          <p>模拟方向：迁移右侧源分区数据，再将其容量并入左侧目标分区。</p>

          <button
            class="primary-button"
            type="button"
            :disabled="!canPreview"
            @click="runPreview"
          >
            <LoaderCircle
              v-if="isPreviewing"
              class="spin"
              :size="17"
              aria-hidden="true"
            />
            <Eye v-else :size="17" aria-hidden="true" />
            {{ isPreviewing ? "正在重新核验" : "运行可行性预演" }}
          </button>
          <div v-if="previewError" class="inline-error" role="alert">
            {{ previewError }}
          </div>
        </section>

        <aside class="surface detail-card" aria-labelledby="detail-title">
          <span class="section-kicker">选中区域</span>
          <h2 id="detail-title">{{ partitionName(selectedPartition) }}</h2>
          <template v-if="selectedPartition">
            <dl>
              <div>
                <dt>容量</dt>
                <dd>{{ formatBytes(selectedPartition.sizeBytes) }}</dd>
              </div>
              <div>
                <dt>已使用</dt>
                <dd>
                  {{
                    selectedPartition.usedBytes === null
                      ? "不可用"
                      : formatBytes(selectedPartition.usedBytes)
                  }}
                </dd>
              </div>
              <div>
                <dt>文件系统</dt>
                <dd>{{ selectedPartition.fileSystem ?? "不可用" }}</dd>
              </div>
              <div>
                <dt>BitLocker</dt>
                <dd>{{ signalLabel(selectedPartition.encryptionState) }}</dd>
              </div>
              <div>
                <dt>快照</dt>
                <dd>{{ signalLabel(selectedPartition.snapshotState) }}</dd>
              </div>
              <div>
                <dt>健康</dt>
                <dd>{{ signalLabel(selectedPartition.health) }}</dd>
              </div>
              <div>
                <dt>起始偏移</dt>
                <dd>{{ formatBytes(selectedPartition.startOffsetBytes) }}</dd>
              </div>
              <div>
                <dt>分区 GUID</dt>
                <dd class="monospace">
                  {{ selectedPartition.guid ?? "不可用" }}
                </dd>
              </div>
            </dl>
          </template>
          <p v-else>选择一个普通数据分区查看只读属性。</p>
        </aside>
      </div>

      <section
        v-if="preview"
        class="surface result-card"
        aria-labelledby="preview-result-title"
      >
        <div class="result-header">
          <div class="result-title">
            <span class="result-icon" :class="{ blocked: !preview.feasible }">
              <Check v-if="preview.feasible" :size="19" aria-hidden="true" />
              <CircleX v-else :size="19" aria-hidden="true" />
            </span>
            <div>
              <span class="section-kicker">预演结论</span>
              <h2 id="preview-result-title">
                {{
                  preview.feasible
                    ? "当前条件具备合并可能"
                    : "当前条件不支持合并"
                }}
              </h2>
            </div>
          </div>
          <button
            class="secondary-button compact-button"
            type="button"
            @click="downloadPartitionPreviewReport(preview)"
          >
            <Download :size="16" aria-hidden="true" />导出报告
          </button>
        </div>

        <div class="result-metrics">
          <div>
            <span>预计迁移</span
            ><strong>{{ formatBytes(preview.migrationBytes) }}</strong>
          </div>
          <div>
            <span>传输估算</span
            ><strong
              >约
              {{ Math.ceil(preview.estimatedDurationSeconds / 60) }}
              分钟</strong
            >
          </div>
          <div><span>风险级别</span><strong>高风险</strong></div>
          <div><span>执行授权</span><strong>未授权</strong></div>
        </div>

        <div v-if="preview.simulatedLayout" class="simulation">
          <span class="section-kicker">模拟后布局</span>
          <PartitionTopologyBar
            :partitions="preview.simulatedLayout.partitions"
            :disk-size-bytes="preview.simulatedLayout.diskSizeBytes"
          />
        </div>

        <div v-if="preview.blockers.length" class="blocker-section">
          <h3>需要先处理的阻塞项</h3>
          <div class="blocker-grid">
            <article
              v-for="blocker in preview.blockers"
              :key="`${blocker.code}-${blocker.partitionId}`"
            >
              <AlertTriangle :size="18" aria-hidden="true" />
              <div>
                <strong>{{ blocker.message }}</strong>
                <p>{{ blocker.recoverySuggestion }}</p>
              </div>
            </article>
          </div>
        </div>

        <div class="checks-section">
          <h3>检查证据</h3>
          <ul>
            <li
              v-for="check in preview.checks"
              :key="check.code"
              :class="{ failed: !check.passed }"
            >
              <Check v-if="check.passed" :size="15" aria-hidden="true" />
              <CircleX v-else :size="15" aria-hidden="true" />
              <span>{{ check.message }}</span>
            </li>
          </ul>
        </div>

        <p class="disclaimer">
          <ShieldCheck :size="16" aria-hidden="true" />{{ preview.disclaimer }}
        </p>
      </section>
    </template>
  </section>
</template>

<style scoped>
.partition-page {
  width: min(1240px, 100%);
  margin: 0 auto;
}

.page-header,
.section-heading,
.result-header,
.result-title,
.disk-signals,
.readonly-badge,
.disclaimer {
  display: flex;
  align-items: center;
}

.page-header {
  justify-content: space-between;
  gap: 28px;
  margin-bottom: 26px;
}

.page-header h1 {
  margin: 4px 0 7px;
  font-size: clamp(2rem, 4vw, 2.65rem);
  letter-spacing: -0.045em;
}

.page-header p,
.topology-help,
.selection-card > p,
.detail-card > p,
.blocker-grid p {
  color: var(--color-text-secondary);
}

.eyebrow,
.section-kicker {
  color: var(--color-blue);
  font-size: 0.76rem;
  font-weight: 650;
  letter-spacing: 0.04em;
}

.eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 9px;
  border-radius: 999px;
  background: var(--color-blue-soft);
}

.secondary-button,
.primary-button {
  min-height: 42px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 9px 15px;
  border-radius: 12px;
  font-weight: 600;
  cursor: pointer;
}

.secondary-button {
  border: 1px solid var(--color-border);
  background: color-mix(in srgb, var(--color-surface) 86%, transparent);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.04);
}

.primary-button {
  width: 100%;
  margin-top: 20px;
  border: 0;
  color: white;
  background: linear-gradient(145deg, #2787ff, #1266df);
  box-shadow: 0 8px 22px rgba(18, 102, 223, 0.2);
}

.secondary-button:disabled,
.primary-button:disabled {
  opacity: 0.55;
  cursor: default;
}

.surface,
.state-card {
  border: 1px solid var(--color-border);
  border-radius: 20px;
  background: color-mix(in srgb, var(--color-surface) 94%, transparent);
  box-shadow: var(--shadow-card);
}

.state-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 15px 17px;
  margin-bottom: 14px;
}

.state-card button {
  margin-left: auto;
}

.error-card,
.warning-card {
  color: var(--color-orange);
}

.disk-selector {
  display: flex;
  gap: 10px;
  margin-bottom: 14px;
  overflow-x: auto;
}

.disk-pill {
  min-width: 240px;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 11px 13px;
  border: 1px solid var(--color-border);
  border-radius: 14px;
  background: var(--color-surface-muted);
  text-align: left;
  cursor: pointer;
}

.disk-pill.active {
  border-color: color-mix(in srgb, var(--color-blue) 65%, transparent);
  background: var(--color-blue-soft);
}

.disk-pill span {
  display: grid;
  gap: 2px;
}

.disk-pill small {
  color: var(--color-text-secondary);
}

.topology-card,
.selection-card,
.detail-card,
.result-card {
  padding: 22px;
}

.section-heading {
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 18px;
}

.section-heading h2,
.detail-card h2,
.result-header h2 {
  margin-top: 4px;
  font-size: 1.12rem;
  letter-spacing: -0.015em;
}

.disk-signals {
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.disk-signals span,
.readonly-badge {
  padding: 5px 9px;
  border-radius: 999px;
  color: var(--color-text-secondary);
  background: var(--color-surface-muted);
  font-size: 0.73rem;
}

.topology-help {
  margin-top: 5px;
  font-size: 0.8rem;
}

.workspace-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.55fr) minmax(300px, 0.8fr);
  gap: 14px;
  margin-top: 14px;
}

.section-heading.compact {
  margin-bottom: 22px;
}

.readonly-badge {
  gap: 5px;
  color: var(--color-green);
  background: var(--color-green-soft);
}

.selection-flow {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: end;
  gap: 12px;
}

.selection-flow label {
  display: grid;
  gap: 8px;
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}

select {
  width: 100%;
  min-height: 42px;
  padding: 0 11px;
  border: 1px solid var(--color-border);
  border-radius: 11px;
  color: var(--color-text);
  background: var(--color-surface-muted);
  font: inherit;
}

.flow-arrow {
  margin-bottom: 11px;
  color: var(--color-text-secondary);
}

.selection-summary {
  display: flex;
  align-items: center;
  gap: 9px;
  margin: 18px 0 7px;
  font-weight: 650;
}

.selection-card > p {
  font-size: 0.8rem;
  line-height: 1.55;
}

.inline-error {
  margin-top: 12px;
  color: var(--color-orange);
  font-size: 0.82rem;
}

.detail-card dl {
  display: grid;
  gap: 0;
  margin: 16px 0 0;
}

.detail-card dl div {
  display: grid;
  grid-template-columns: 92px minmax(0, 1fr);
  gap: 12px;
  padding: 9px 0;
  border-bottom: 1px solid var(--color-border);
}

.detail-card dt {
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}

.detail-card dd {
  min-width: 0;
  margin: 0;
  overflow-wrap: anywhere;
  font-size: 0.8rem;
  text-align: right;
}

.monospace {
  font-family: "Cascadia Mono", Consolas, monospace;
  font-size: 0.7rem !important;
}

.result-card {
  margin-top: 14px;
}

.result-header {
  justify-content: space-between;
  gap: 18px;
}

.result-title {
  gap: 12px;
}

.result-icon {
  width: 38px;
  height: 38px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  border-radius: 50%;
  color: var(--color-green);
  background: var(--color-green-soft);
}

.result-icon.blocked {
  color: var(--color-orange);
  background: var(--color-orange-soft);
}

.compact-button {
  min-height: 36px;
  padding: 7px 11px;
  font-size: 0.8rem;
}

.result-metrics {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 8px;
  margin: 22px 0;
}

.result-metrics div {
  display: grid;
  gap: 5px;
  padding: 13px;
  border-radius: 12px;
  background: var(--color-surface-muted);
}

.result-metrics span {
  color: var(--color-text-secondary);
  font-size: 0.72rem;
}

.result-metrics strong {
  font-size: 0.92rem;
}

.simulation {
  padding: 18px 0 10px;
  border-top: 1px solid var(--color-border);
}

.blocker-section,
.checks-section {
  margin-top: 20px;
}

.blocker-section h3,
.checks-section h3 {
  margin: 0 0 10px;
  font-size: 0.9rem;
}

.blocker-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.blocker-grid article {
  display: flex;
  gap: 9px;
  padding: 12px;
  border: 1px solid color-mix(in srgb, var(--color-orange) 20%, transparent);
  border-radius: 12px;
  color: var(--color-orange);
  background: var(--color-orange-soft);
}

.blocker-grid article svg {
  flex: 0 0 auto;
}

.blocker-grid strong {
  font-size: 0.8rem;
}

.blocker-grid p {
  margin-top: 4px;
  font-size: 0.74rem;
  line-height: 1.45;
}

.checks-section ul {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 16px;
  padding: 0;
  margin: 0;
  list-style: none;
}

.checks-section li {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--color-green);
  font-size: 0.78rem;
}

.checks-section li.failed {
  color: var(--color-orange);
}

.disclaimer {
  gap: 8px;
  margin-top: 22px;
  padding: 12px 14px;
  border-radius: 12px;
  color: var(--color-text-secondary);
  background: var(--color-surface-muted);
  font-size: 0.78rem;
  line-height: 1.5;
}

.spin {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 1050px) {
  .workspace-grid {
    grid-template-columns: 1fr;
  }

  .result-metrics {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (max-width: 760px) {
  .page-header {
    align-items: flex-start;
  }

  .selection-flow {
    grid-template-columns: 1fr;
  }

  .flow-arrow {
    display: none;
  }

  .blocker-grid,
  .checks-section ul {
    grid-template-columns: 1fr;
  }
}
</style>
