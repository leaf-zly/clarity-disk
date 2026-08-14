<script setup lang="ts">
import { computed, onMounted, shallowRef } from "vue";
import {
  AlertTriangle,
  Check,
  CircleX,
  FileKey2,
  HardDrive,
  LoaderCircle,
  LockKeyhole,
  Power,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
} from "@lucide/vue";

import { assessPartitionMergeSafety } from "@/services/partition-safety-service";
import {
  loadPartitionTopology,
  previewPartitionMerge,
} from "@/services/partition-service";
import type { PartitionSafetyAssessment } from "@/types/partition-safety";
import type {
  MergePreview,
  PartitionDescriptor,
  PartitionTopology,
  PhysicalDisk,
} from "@/types/partition";
import { formatBytes } from "@/utils/format-bytes";

const topology = shallowRef<PartitionTopology>();
const preview = shallowRef<MergePreview>();
const assessment = shallowRef<PartitionSafetyAssessment>();
const selectedDiskId = shallowRef<string>();
const targetPartitionId = shallowRef<string>();
const sourcePartitionId = shallowRef<string>();
const loadError = shallowRef<string>();
const assessmentError = shallowRef<string>();
const isLoading = shallowRef(true);
const isAssessing = shallowRef(false);

const selectedDisk = computed(() =>
  topology.value?.disks.find((disk) => disk.id === selectedDiskId.value),
);
const dataPartitions = computed(() =>
  (selectedDisk.value?.partitions ?? []).filter(isDataPartition),
);
const canAssess = computed(
  () =>
    Boolean(targetPartitionId.value && sourcePartitionId.value) &&
    targetPartitionId.value !== sourcePartitionId.value &&
    !isAssessing.value,
);

onMounted(() => void refreshTopology());

async function refreshTopology(): Promise<void> {
  isLoading.value = true;
  loadError.value = undefined;
  invalidateAssessment();
  try {
    const next = await loadPartitionTopology();
    if (!next.readOnly) throw new Error("partition discovery is not read only");
    topology.value = next;
    selectedDiskId.value = next.disks[0]?.id;
    chooseDefaultPair(next.disks[0]);
  } catch {
    topology.value = undefined;
    loadError.value = "安全基础无法读取分区拓扑，未执行任何磁盘修改。";
  } finally {
    isLoading.value = false;
  }
}

function selectDisk(disk: PhysicalDisk): void {
  selectedDiskId.value = disk.id;
  chooseDefaultPair(disk);
  invalidateAssessment();
}

function chooseDefaultPair(disk: PhysicalDisk | undefined): void {
  targetPartitionId.value = undefined;
  sourcePartitionId.value = undefined;
  if (!disk) return;
  for (let index = 0; index < disk.partitions.length - 1; index += 1) {
    const target = disk.partitions[index];
    const source = disk.partitions[index + 1];
    if (
      target &&
      source &&
      isDataPartition(target) &&
      isDataPartition(source) &&
      target.startOffsetBytes + target.sizeBytes === source.startOffsetBytes
    ) {
      targetPartitionId.value = target.id;
      sourcePartitionId.value = source.id;
      return;
    }
  }
}

function updateSelection(role: "target" | "source", value: string): void {
  invalidateAssessment();
  if (role === "target") {
    targetPartitionId.value = value || undefined;
    if (sourcePartitionId.value === value) sourcePartitionId.value = undefined;
  } else {
    sourcePartitionId.value = value || undefined;
    if (targetPartitionId.value === value) targetPartitionId.value = undefined;
  }
}

async function runSafetyAssessment(): Promise<void> {
  if (!targetPartitionId.value || !sourcePartitionId.value) return;
  isAssessing.value = true;
  assessmentError.value = undefined;
  assessment.value = undefined;
  preview.value = undefined;
  const request = {
    targetPartitionId: targetPartitionId.value,
    sourcePartitionId: sourcePartitionId.value,
  };
  try {
    const nextPreview = await previewPartitionMerge(request);
    if (nextPreview.executionAuthorized)
      throw new Error("preview unexpectedly authorized execution");
    preview.value = nextPreview;
    const nextAssessment = await assessPartitionMergeSafety(request);
    // The plan-seven contract is useful only while both execution channels are absent.
    if (
      nextAssessment.executionAuthorized ||
      nextAssessment.writeCapabilityPresent
    )
      throw new Error("safety assessment exposed a writer");
    assessment.value = nextAssessment;
  } catch {
    assessmentError.value =
      "安全基础评估未完成，系统证据可能已变化；没有执行任何分区写入。";
  } finally {
    isAssessing.value = false;
  }
}

function invalidateAssessment(): void {
  preview.value = undefined;
  assessment.value = undefined;
  assessmentError.value = undefined;
}

function isDataPartition(partition: PartitionDescriptor): boolean {
  return partition.kind === "data" && !partition.isSystem && !partition.isBoot;
}

function partitionName(partitionId: string | undefined): string {
  const partition = selectedDisk.value?.partitions.find(
    (item) => item.id === partitionId,
  );
  if (!partition) return "未选择";
  return (
    partition.label ||
    partition.mountPoints[0] ||
    "分区 " + partition.partitionNumber
  );
}
</script>

<template>
  <section class="safety-page" aria-labelledby="safety-page-title">
    <header class="page-header">
      <div>
        <span class="eyebrow">
          <ShieldCheck :size="14" aria-hidden="true" />计划七
        </span>
        <h1 id="safety-page-title">分区安全基础</h1>
        <p>生成不可变计划并验证写入前条件。当前版本没有分区写入能力。</p>
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
        <RefreshCw v-else :size="17" aria-hidden="true" />刷新证据
      </button>
    </header>

    <section class="boundary-banner">
      <LockKeyhole :size="20" aria-hidden="true" />
      <div>
        <strong>写入能力未安装</strong>
        <p>
          本页只评估拓扑、供电、待重启、备份和恢复协议，不创建执行令牌，也不调用
          diskpart。
        </p>
      </div>
      <span>execution = false</span>
    </section>

    <div v-if="loadError" class="state-card error-card" role="alert">
      <CircleX :size="19" aria-hidden="true" />{{ loadError }}
      <button type="button" @click="refreshTopology">重试</button>
    </div>

    <template v-else-if="topology && selectedDisk">
      <section class="disk-selector" aria-label="物理磁盘">
        <button
          v-for="disk in topology.disks"
          :key="disk.id"
          class="disk-pill"
          :class="{ active: disk.id === selectedDisk.id }"
          type="button"
          @click="selectDisk(disk)"
        >
          <HardDrive :size="19" aria-hidden="true" />
          <span>
            <strong>{{ disk.friendlyName }}</strong>
            <small
              >磁盘 {{ disk.number }} · {{ formatBytes(disk.sizeBytes) }}</small
            >
          </span>
        </button>
      </section>

      <section class="surface selection-card">
        <div class="section-heading">
          <div>
            <span class="section-kicker">后端身份选择</span>
            <h2>安全评估对象</h2>
          </div>
          <span class="readonly-badge">
            <ShieldCheck :size="14" aria-hidden="true" />只读重新发现
          </span>
        </div>
        <div class="selection-grid">
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
                v-for="partition in dataPartitions"
                :key="partition.id"
                :value="partition.id"
              >
                {{ partitionName(partition.id) }} ·
                {{ formatBytes(partition.sizeBytes) }}
              </option>
            </select>
          </label>
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
                v-for="partition in dataPartitions"
                :key="partition.id"
                :value="partition.id"
              >
                {{ partitionName(partition.id) }} ·
                {{ formatBytes(partition.sizeBytes) }}
              </option>
            </select>
          </label>
        </div>
        <button
          class="primary-button"
          type="button"
          :disabled="!canAssess"
          @click="runSafetyAssessment"
        >
          <LoaderCircle
            v-if="isAssessing"
            class="spin"
            :size="17"
            aria-hidden="true"
          />
          <FileKey2 v-else :size="17" aria-hidden="true" />
          {{ isAssessing ? "正在重新发现与绑定" : "生成不可变安全计划" }}
        </button>
        <p v-if="assessmentError" class="inline-error" role="alert">
          {{ assessmentError }}
        </p>
      </section>

      <template v-if="assessment">
        <section
          class="surface outcome-card"
          :class="{ ready: assessment.status === 'foundationReady' }"
        >
          <div class="outcome-title">
            <span class="outcome-icon">
              <Check
                v-if="assessment.status === 'foundationReady'"
                :size="23"
                aria-hidden="true"
              />
              <AlertTriangle v-else :size="23" aria-hidden="true" />
            </span>
            <div>
              <span class="section-kicker">安全基础结论</span>
              <h2>
                {{
                  assessment.status === "foundationReady"
                    ? "前置条件已建模"
                    : "仍有安全前置条件未满足"
                }}
              </h2>
              <p>{{ assessment.disclaimer }}</p>
            </div>
          </div>
          <div class="authorization-state">
            <span>执行授权</span><strong>未授权</strong> <span>写入能力</span
            ><strong>不存在</strong>
          </div>
        </section>

        <section class="plan-grid">
          <article class="surface plan-card">
            <div class="section-heading">
              <div>
                <span class="section-kicker">不可变计划</span>
                <h2>摘要绑定</h2>
              </div>
              <FileKey2 :size="20" aria-hidden="true" />
            </div>
            <dl>
              <div>
                <dt>计划摘要</dt>
                <dd class="monospace">
                  {{ assessment.plan.planDigest.slice(0, 16) }}…
                </dd>
              </div>
              <div>
                <dt>计划版本</dt>
                <dd>v{{ assessment.plan.schemaVersion }}</dd>
              </div>
              <div>
                <dt>恢复协议</dt>
                <dd>v{{ assessment.plan.recoverySchemaVersion }}</dd>
              </div>
              <div>
                <dt>预计迁移</dt>
                <dd>{{ formatBytes(assessment.plan.migrationBytes) }}</dd>
              </div>
              <div>
                <dt>有效期至</dt>
                <dd>
                  {{
                    new Date(
                      assessment.plan.expiresAtUnixMs,
                    ).toLocaleTimeString("zh-CN")
                  }}
                </dd>
              </div>
            </dl>
          </article>

          <article class="surface recovery-card">
            <div class="section-heading">
              <div>
                <span class="section-kicker">恢复状态机</span>
                <h2>故障停止策略</h2>
              </div>
              <RotateCcw :size="20" aria-hidden="true" />
            </div>
            <ol>
              <li><span>1</span>计划与新鲜预检</li>
              <li><span>2</span>迁移数据独立校验</li>
              <li><span>3</span>元数据变更检查点</li>
              <li><span>4</span>后置条件验证</li>
            </ol>
            <p>
              写入前中断进入安全停止；元数据写入开始后中断必须进入人工恢复，禁止自动猜测继续。
            </p>
          </article>
        </section>

        <section class="surface checks-card">
          <div class="section-heading">
            <div>
              <span class="section-kicker">系统证据</span>
              <h2>写入前条件</h2>
            </div>
            <Power :size="20" aria-hidden="true" />
          </div>
          <ul>
            <li
              v-for="check in assessment.checks"
              :key="check.code"
              :class="{ failed: !check.passed }"
            >
              <span>
                <Check v-if="check.passed" :size="15" aria-hidden="true" />
                <CircleX v-else :size="15" aria-hidden="true" />
              </span>
              {{ check.message }}
            </li>
          </ul>
        </section>

        <section
          v-if="assessment.blockers.length"
          class="surface blockers-card"
        >
          <span class="section-kicker">必须先处理</span>
          <h2>安全阻塞项</h2>
          <div class="blocker-grid">
            <article v-for="blocker in assessment.blockers" :key="blocker.code">
              <AlertTriangle :size="18" aria-hidden="true" />
              <div>
                <strong>{{ blocker.message }}</strong>
                <p>{{ blocker.recoverySuggestion }}</p>
              </div>
            </article>
          </div>
        </section>
      </template>
    </template>
  </section>
</template>

<style scoped>
.safety-page {
  width: min(1160px, 100%);
  margin: 0 auto;
}
.page-header,
.boundary-banner,
.disk-pill,
.section-heading,
.readonly-badge,
.outcome-card,
.outcome-title {
  display: flex;
  align-items: center;
}
.page-header {
  justify-content: space-between;
  gap: 28px;
  margin-bottom: 22px;
}
.page-header h1 {
  margin: 5px 0 7px;
  font-size: clamp(2rem, 4vw, 2.65rem);
  letter-spacing: -0.045em;
}
.page-header p,
.boundary-banner p,
.outcome-card p,
.recovery-card p,
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
  min-height: 40px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 8px 14px;
  border-radius: 12px;
  font-weight: 600;
  cursor: pointer;
}
.secondary-button {
  border: 1px solid var(--color-border);
  background: var(--color-surface);
}
.primary-button {
  width: 100%;
  margin-top: 18px;
  border: 0;
  color: white;
  background: linear-gradient(145deg, #2787ff, #1266df);
  box-shadow: 0 8px 22px rgba(18, 102, 223, 0.2);
}
.secondary-button:disabled,
.primary-button:disabled {
  opacity: 0.5;
  cursor: default;
}
.boundary-banner {
  gap: 12px;
  padding: 14px 16px;
  margin-bottom: 14px;
  border: 1px solid color-mix(in srgb, var(--color-blue) 18%, transparent);
  border-radius: 16px;
  background: var(--color-blue-soft);
}
.boundary-banner svg {
  flex: 0 0 auto;
  color: var(--color-blue);
}
.boundary-banner div {
  flex: 1;
}
.boundary-banner p {
  margin-top: 3px;
  font-size: 0.78rem;
  line-height: 1.5;
}
.boundary-banner > span {
  padding: 5px 9px;
  border-radius: 999px;
  color: var(--color-green);
  background: var(--color-green-soft);
  font:
    600 0.7rem "Cascadia Mono",
    monospace;
}
.surface,
.state-card {
  border: 1px solid var(--color-border);
  background: color-mix(in srgb, var(--color-surface) 94%, transparent);
  box-shadow: var(--shadow-card);
}
.surface {
  border-radius: 20px;
}
.state-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 15px 17px;
  border-radius: 16px;
}
.state-card button {
  margin-left: auto;
}
.error-card,
.inline-error {
  color: var(--color-orange);
}
.disk-selector {
  display: flex;
  gap: 9px;
  margin-bottom: 14px;
  overflow-x: auto;
}
.disk-pill {
  min-width: 260px;
  gap: 10px;
  padding: 11px 13px;
  border: 1px solid var(--color-border);
  border-radius: 14px;
  background: var(--color-surface-muted);
  text-align: left;
  cursor: pointer;
}
.disk-pill.active {
  border-color: color-mix(in srgb, var(--color-blue) 60%, transparent);
  background: var(--color-blue-soft);
}
.disk-pill svg {
  color: var(--color-blue);
}
.disk-pill span {
  display: grid;
  gap: 3px;
}
.disk-pill small {
  color: var(--color-text-secondary);
}
.selection-card,
.plan-card,
.recovery-card,
.checks-card,
.blockers-card {
  padding: 21px;
}
.section-heading {
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
}
.section-heading h2,
.blockers-card h2 {
  margin-top: 4px;
  font-size: 1.06rem;
}
.section-heading > svg {
  color: var(--color-blue);
}
.readonly-badge {
  gap: 6px;
  padding: 5px 9px;
  border-radius: 999px;
  color: var(--color-green);
  background: var(--color-green-soft);
  font-size: 0.72rem;
}
.selection-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
.selection-grid label {
  display: grid;
  gap: 7px;
  color: var(--color-text-secondary);
  font-size: 0.76rem;
}
select {
  min-height: 42px;
  padding: 0 11px;
  border: 1px solid var(--color-border);
  border-radius: 11px;
  color: var(--color-text);
  background: var(--color-surface-muted);
  font: inherit;
}
.inline-error {
  margin-top: 10px;
  font-size: 0.78rem;
}
.outcome-card {
  justify-content: space-between;
  gap: 20px;
  padding: 21px;
  margin-top: 14px;
  border-color: color-mix(in srgb, var(--color-orange) 22%, transparent);
  background:
    radial-gradient(circle at 90% 0, var(--color-orange-soft), transparent 42%),
    var(--color-surface);
}
.outcome-card.ready {
  border-color: color-mix(in srgb, var(--color-green) 22%, transparent);
  background:
    radial-gradient(circle at 90% 0, var(--color-green-soft), transparent 42%),
    var(--color-surface);
}
.outcome-title {
  gap: 12px;
}
.outcome-icon {
  width: 42px;
  height: 42px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  border-radius: 50%;
  color: var(--color-orange);
  background: var(--color-orange-soft);
}
.ready .outcome-icon {
  color: var(--color-green);
  background: var(--color-green-soft);
}
.outcome-title h2 {
  margin: 4px 0 5px;
  font-size: 1.15rem;
}
.outcome-title p {
  font-size: 0.76rem;
  line-height: 1.5;
}
.authorization-state {
  display: grid;
  grid-template-columns: auto auto;
  gap: 5px 14px;
  font-size: 0.72rem;
}
.authorization-state span {
  color: var(--color-text-secondary);
}
.authorization-state strong {
  color: var(--color-green);
  text-align: right;
}
.plan-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
  margin-top: 14px;
}
.plan-card dl {
  display: grid;
  gap: 0;
  margin: 0;
}
.plan-card dl div {
  display: flex;
  justify-content: space-between;
  gap: 14px;
  padding: 9px 0;
  border-bottom: 1px solid var(--color-border);
}
.plan-card dt {
  color: var(--color-text-secondary);
  font-size: 0.74rem;
}
.plan-card dd {
  margin: 0;
  font-size: 0.77rem;
  text-align: right;
}
.monospace {
  font-family: "Cascadia Mono", monospace;
}
.recovery-card ol {
  display: grid;
  gap: 7px;
  padding: 0;
  margin: 0;
  list-style: none;
}
.recovery-card li {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 10px;
  background: var(--color-surface-muted);
  font-size: 0.76rem;
}
.recovery-card li span {
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  color: var(--color-blue);
  background: var(--color-blue-soft);
  font-size: 0.65rem;
}
.recovery-card p {
  margin-top: 12px;
  font-size: 0.73rem;
  line-height: 1.55;
}
.checks-card,
.blockers-card {
  margin-top: 14px;
}
.checks-card ul {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 7px 14px;
  padding: 0;
  margin: 0;
  list-style: none;
}
.checks-card li {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--color-green);
  font-size: 0.76rem;
}
.checks-card li.failed {
  color: var(--color-orange);
}
.checks-card li > span {
  width: 22px;
  height: 22px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  border-radius: 50%;
  background: var(--color-green-soft);
}
.checks-card li.failed > span {
  background: var(--color-orange-soft);
}
.blockers-card > h2 {
  margin-bottom: 13px;
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
.blocker-grid article > svg {
  flex: 0 0 auto;
}
.blocker-grid strong {
  font-size: 0.8rem;
}
.blocker-grid p {
  margin-top: 4px;
  font-size: 0.72rem;
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
@media (max-width: 900px) {
  .plan-grid {
    grid-template-columns: 1fr;
  }
  .outcome-card {
    align-items: flex-start;
    flex-direction: column;
  }
}
@media (max-width: 700px) {
  .selection-grid,
  .checks-card ul,
  .blocker-grid {
    grid-template-columns: 1fr;
  }
  .boundary-banner > span {
    display: none;
  }
}
</style>
