<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from "vue";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  CircleHelp,
  Download,
  HardDrive,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
  Thermometer,
  Timer,
} from "@lucide/vue";

import {
  downloadDiskHealthReport,
  loadDiskHealthSnapshot,
} from "@/services/health-service";
import type {
  DiskHealthStatus,
  HealthDataCompleteness,
  HealthSignalSeverity,
  IdentityMappingConfidence,
  PhysicalDiskHealth,
  ProviderHealthStatus,
  SmartHealthStatus,
} from "@/types/health";
import { formatBytes } from "@/utils/format-bytes";

const snapshot = shallowRef<Awaited<
  ReturnType<typeof loadDiskHealthSnapshot>
> | null>(null);
const selectedDiskId = ref("");
const isLoading = ref(true);
const loadError = ref("");
let loadSequence = 0;

const selectedDisk = computed<PhysicalDiskHealth | null>(() => {
  const disks = snapshot.value?.disks ?? [];
  return (
    disks.find((disk) => disk.id === selectedDiskId.value) ?? disks[0] ?? null
  );
});
const overallStatus = computed<DiskHealthStatus>(
  () => snapshot.value?.summary.overallStatus ?? "unknown",
);

onMounted(() => void loadHealth());

async function loadHealth(): Promise<void> {
  const sequence = ++loadSequence;
  isLoading.value = true;
  loadError.value = "";
  try {
    const nextSnapshot = await loadDiskHealthSnapshot();
    if (sequence !== loadSequence) return;
    if (!nextSnapshot.readOnly || nextSnapshot.disks.length === 0)
      throw new Error("健康提供程序返回了不安全或空的数据快照。");
    snapshot.value = nextSnapshot;
    if (!nextSnapshot.disks.some((disk) => disk.id === selectedDiskId.value))
      selectedDiskId.value = nextSnapshot.disks[0]?.id ?? "";
  } catch (error: unknown) {
    if (sequence !== loadSequence) return;
    snapshot.value = null;
    loadError.value =
      error instanceof Error ? error.message : "无法读取磁盘健康信息。";
  } finally {
    if (sequence === loadSequence) isLoading.value = false;
  }
}

function statusLabel(status: DiskHealthStatus): string {
  const labels: Record<DiskHealthStatus, string> = {
    good: "状态良好",
    attention: "需要注意",
    critical: "严重警告",
    unknown: "状态未知",
  };
  return labels[status];
}

function providerLabel(status: ProviderHealthStatus): string {
  const labels: Record<ProviderHealthStatus, string> = {
    healthy: "健康",
    warning: "警告",
    unhealthy: "异常",
    unknown: "不可用",
  };
  return labels[status];
}

function smartLabel(status: SmartHealthStatus): string {
  const labels: Record<SmartHealthStatus, string> = {
    passed: "已通过",
    warning: "需注意",
    failed: "失败",
    unavailable: "不可用",
  };
  return labels[status];
}

function completenessLabel(status: HealthDataCompleteness): string {
  const labels: Record<HealthDataCompleteness, string> = {
    complete: "证据完整",
    partial: "部分可用",
    limited: "证据有限",
  };
  return labels[status];
}

function mappingLabel(mapping: IdentityMappingConfidence): string {
  const labels: Record<IdentityMappingConfidence, string> = {
    exact: "唯一身份精确匹配",
    diskNumber: "Windows 磁盘号匹配",
    unknown: "身份映射不可确认",
  };
  return labels[mapping];
}

function severityLabel(severity: HealthSignalSeverity): string {
  const labels: Record<HealthSignalSeverity, string> = {
    critical: "严重",
    warning: "提醒",
    unknown: "未知",
  };
  return labels[severity];
}

function metric(value: number | null, suffix = ""): string {
  return value === null ? "不可用" : value.toLocaleString("zh-CN") + suffix;
}

function powerOnTime(hours: number | null): string {
  if (hours === null) return "不可用";
  const days = Math.floor(hours / 24);
  return days > 0
    ? hours.toLocaleString("zh-CN") +
        " 小时 · " +
        days.toLocaleString("zh-CN") +
        " 天"
    : hours.toLocaleString("zh-CN") + " 小时";
}

function uncorrectedErrors(disk: PhysicalDiskHealth): string {
  if (
    disk.readErrorsUncorrected === null ||
    disk.writeErrorsUncorrected === null
  )
    return "不可用";
  return (
    disk.readErrorsUncorrected + disk.writeErrorsUncorrected
  ).toLocaleString("zh-CN");
}

function exportReport(): void {
  if (snapshot.value) downloadDiskHealthReport(snapshot.value);
}
</script>

<template>
  <section class="health-page">
    <header class="page-header">
      <div>
        <span class="eyebrow">
          <ShieldCheck :size="14" aria-hidden="true" />只读检测
        </span>
        <h1>磁盘健康</h1>
        <p>统一查看 Windows 存储状态、温度、寿命与可靠性证据。</p>
      </div>
      <div class="header-actions">
        <button
          class="secondary-button"
          type="button"
          :disabled="isLoading"
          @click="loadHealth"
        >
          <RefreshCw
            :class="{ spin: isLoading }"
            :size="16"
            aria-hidden="true"
          />刷新检测
        </button>
        <button
          class="secondary-button"
          type="button"
          :disabled="!snapshot"
          @click="exportReport"
        >
          <Download :size="16" aria-hidden="true" />导出报告
        </button>
      </div>
    </header>

    <div v-if="isLoading" class="state-card" role="status">
      <LoaderCircle class="spin" :size="19" aria-hidden="true" />
      正在读取 Windows 物理磁盘健康证据…
    </div>
    <div v-else-if="loadError" class="state-card error-card" role="alert">
      <AlertTriangle :size="19" aria-hidden="true" />
      <div>
        <strong>健康检测暂时不可用</strong>
        <p>{{ loadError }} 未执行任何磁盘修改。</p>
      </div>
      <button class="secondary-button" type="button" @click="loadHealth">
        重试
      </button>
    </div>

    <template v-else-if="snapshot && selectedDisk">
      <section class="safety-banner" aria-label="检测安全边界">
        <CircleHelp :size="18" aria-hidden="true" />
        <p>
          <strong>未知不代表健康。</strong>
          无法读取的厂商属性会显示为“不可用”；此页面没有修复、写盘或管理员命令。
        </p>
        <span>分区写操作未开放</span>
      </section>

      <div v-if="snapshot.discoveryWarnings.length" class="warning-list">
        <div
          v-for="warning in snapshot.discoveryWarnings"
          :key="warning"
          class="state-card warning-card"
        >
          <AlertTriangle :size="18" aria-hidden="true" />{{ warning }}
        </div>
      </div>

      <section class="summary-grid" aria-label="健康总览">
        <article class="surface overall-card" :class="overallStatus">
          <div class="status-orb">
            <CheckCircle2
              v-if="overallStatus === 'good'"
              :size="32"
              aria-hidden="true"
            />
            <AlertTriangle
              v-else-if="overallStatus !== 'unknown'"
              :size="32"
              aria-hidden="true"
            />
            <CircleHelp v-else :size="32" aria-hidden="true" />
          </div>
          <div>
            <span class="section-kicker">全部物理磁盘</span>
            <h2>{{ statusLabel(overallStatus) }}</h2>
            <p>
              共 {{ snapshot.summary.totalDisks }} 块 · 良好
              {{ snapshot.summary.goodDisks }} · 注意
              {{ snapshot.summary.attentionDisks }} · 严重
              {{ snapshot.summary.criticalDisks }} · 未知
              {{ snapshot.summary.unknownDisks }}
            </p>
          </div>
        </article>
        <article class="surface privacy-card">
          <ShieldCheck :size="21" aria-hidden="true" />
          <div>
            <strong>隐私保护已启用</strong>
            <p>报告仅保留序列号末四位，完整序列号不会进入应用领域或界面。</p>
          </div>
        </article>
      </section>

      <div class="disk-selector" aria-label="选择物理磁盘">
        <button
          v-for="disk in snapshot.disks"
          :key="disk.id"
          class="disk-pill"
          :class="[disk.status, { active: disk.id === selectedDisk.id }]"
          type="button"
          :aria-pressed="disk.id === selectedDisk.id"
          @click="selectedDiskId = disk.id"
        >
          <span class="disk-icon">
            <HardDrive :size="19" aria-hidden="true" />
          </span>
          <span class="disk-copy">
            <strong>{{ disk.friendlyName }}</strong>
            <small>
              磁盘 {{ disk.number }} · {{ formatBytes(disk.sizeBytes) }} ·
              {{ statusLabel(disk.status) }}
            </small>
          </span>
          <span class="status-dot" aria-hidden="true"></span>
        </button>
      </div>

      <section class="surface disk-hero" :class="selectedDisk.status">
        <div class="hero-title">
          <span class="section-kicker">当前设备</span>
          <h2>{{ selectedDisk.friendlyName }}</h2>
          <p>
            {{ selectedDisk.model ?? "型号不可用" }} ·
            {{ selectedDisk.busType }} · {{ selectedDisk.mediaType }}
          </p>
        </div>
        <div class="hero-status">
          <span>{{ statusLabel(selectedDisk.status) }}</span>
          <small>{{ completenessLabel(selectedDisk.dataCompleteness) }}</small>
        </div>
      </section>

      <section class="metric-grid" aria-label="关键健康指标">
        <article class="surface metric-card">
          <span class="metric-icon temperature">
            <Thermometer :size="19" aria-hidden="true" />
          </span>
          <span>当前温度</span>
          <strong>{{ metric(selectedDisk.temperatureCelsius, "°C") }}</strong>
          <small>
            历史最高 {{ metric(selectedDisk.temperatureMaxCelsius, "°C") }}
          </small>
        </article>
        <article class="surface metric-card">
          <span class="metric-icon life">
            <Activity :size="19" aria-hidden="true" />
          </span>
          <span>预计剩余寿命</span>
          <strong>
            {{ metric(selectedDisk.estimatedLifeRemainingPercent, "%") }}
          </strong>
          <small>仅在设备提供磨损计数时显示</small>
          <span
            v-if="selectedDisk.estimatedLifeRemainingPercent !== null"
            class="life-track"
          >
            <i
              :style="{
                width: selectedDisk.estimatedLifeRemainingPercent + '%',
              }"
            ></i>
          </span>
        </article>
        <article class="surface metric-card">
          <span class="metric-icon time">
            <Timer :size="19" aria-hidden="true" />
          </span>
          <span>累计通电</span>
          <strong class="compact-value">
            {{ powerOnTime(selectedDisk.powerOnHours) }}
          </strong>
          <small>来自 Storage Reliability Counter</small>
        </article>
        <article class="surface metric-card">
          <span class="metric-icon errors">
            <AlertTriangle :size="19" aria-hidden="true" />
          </span>
          <span>未纠正错误</span>
          <strong>{{ uncorrectedErrors(selectedDisk) }}</strong>
          <small>读写可靠性计数合计</small>
        </article>
      </section>

      <div class="detail-grid">
        <section class="surface evidence-card" aria-labelledby="evidence-title">
          <div class="section-heading">
            <div>
              <span class="section-kicker">状态证据</span>
              <h2 id="evidence-title">提供程序与自监测</h2>
            </div>
            <span class="evidence-badge" :class="selectedDisk.dataCompleteness">
              {{ completenessLabel(selectedDisk.dataCompleteness) }}
            </span>
          </div>
          <dl class="evidence-list">
            <div>
              <dt>Windows 提供程序</dt>
              <dd>{{ providerLabel(selectedDisk.providerHealth) }}</dd>
            </div>
            <div>
              <dt>自监测结论</dt>
              <dd>{{ smartLabel(selectedDisk.smartStatus) }}</dd>
            </div>
            <div>
              <dt>身份可信度</dt>
              <dd>{{ mappingLabel(selectedDisk.identityMapping) }}</dd>
            </div>
            <div>
              <dt>操作状态</dt>
              <dd>
                {{ selectedDisk.operationalStatus.join("、") || "不可用" }}
              </dd>
            </div>
            <div>
              <dt>已使用寿命</dt>
              <dd>{{ metric(selectedDisk.wearPercentUsed, "%") }}</dd>
            </div>
            <div>
              <dt>读 / 写错误总数</dt>
              <dd>
                {{ metric(selectedDisk.readErrorsTotal) }} /
                {{ metric(selectedDisk.writeErrorsTotal) }}
              </dd>
            </div>
          </dl>
        </section>

        <section class="surface device-card" aria-labelledby="device-title">
          <span class="section-kicker">设备信息</span>
          <h2 id="device-title">硬件与加密</h2>
          <dl class="device-list">
            <div>
              <dt>制造商</dt>
              <dd>{{ selectedDisk.manufacturer ?? "不可用" }}</dd>
            </div>
            <div>
              <dt>型号</dt>
              <dd>{{ selectedDisk.model ?? "不可用" }}</dd>
            </div>
            <div>
              <dt>固件</dt>
              <dd>{{ selectedDisk.firmwareVersion ?? "不可用" }}</dd>
            </div>
            <div>
              <dt>序列号</dt>
              <dd>
                {{
                  selectedDisk.serialSuffix
                    ? "•••• " + selectedDisk.serialSuffix
                    : "不可用"
                }}
              </dd>
            </div>
            <div>
              <dt>容量</dt>
              <dd>{{ formatBytes(selectedDisk.sizeBytes) }}</dd>
            </div>
            <div>
              <dt>逻辑 / 物理扇区</dt>
              <dd>
                {{ metric(selectedDisk.logicalSectorBytes, " B") }} /
                {{ metric(selectedDisk.physicalSectorBytes, " B") }}
              </dd>
            </div>
          </dl>
          <div class="encryption-summary">
            <strong>BitLocker 卷汇总</strong>
            <span>已保护 {{ selectedDisk.encryption.protectedVolumes }}</span>
            <span>未保护 {{ selectedDisk.encryption.unprotectedVolumes }}</span>
            <span>未知 {{ selectedDisk.encryption.unknownVolumes }}</span>
          </div>
        </section>
      </div>

      <section class="surface signals-card" aria-labelledby="signals-title">
        <div class="section-heading">
          <div>
            <span class="section-kicker">安全建议</span>
            <h2 id="signals-title">健康信号</h2>
          </div>
          <span>{{ selectedDisk.signals.length }} 项</span>
        </div>
        <div v-if="selectedDisk.signals.length" class="signal-grid">
          <article
            v-for="signal in selectedDisk.signals"
            :key="signal.code"
            class="signal-item"
            :class="signal.severity"
          >
            <span class="signal-icon">
              <AlertTriangle
                v-if="signal.severity !== 'unknown'"
                :size="18"
                aria-hidden="true"
              />
              <CircleHelp v-else :size="18" aria-hidden="true" />
            </span>
            <div>
              <span class="signal-level">{{
                severityLabel(signal.severity)
              }}</span>
              <h3>{{ signal.title }}</h3>
              <p>{{ signal.detail }}</p>
              <strong>{{ signal.recommendation }}</strong>
            </div>
          </article>
        </div>
        <div v-else class="empty-signals">
          <CheckCircle2 :size="23" aria-hidden="true" />
          <div>
            <strong>当前证据未发现警告</strong>
            <p>该结论仅适用于本次快照；请继续保持可靠备份。</p>
          </div>
        </div>
      </section>

      <footer class="snapshot-footer">
        检测时间：{{
          new Date(snapshot.capturedAtUnixMs).toLocaleString("zh-CN")
        }}
        · Windows 统一存储可靠性数据可能不包含厂商专有 SMART 属性。
      </footer>
    </template>
  </section>
</template>

<style scoped>
.health-page {
  width: min(1240px, 100%);
  margin: 0 auto;
}
.page-header,
.header-actions,
.safety-banner,
.summary-grid,
.overall-card,
.privacy-card,
.disk-pill,
.disk-hero,
.section-heading,
.empty-signals,
.snapshot-footer {
  display: flex;
  align-items: center;
}
.page-header {
  justify-content: space-between;
  gap: 28px;
  margin-bottom: 24px;
}
.page-header h1 {
  margin: 5px 0 7px;
  font-size: clamp(2rem, 4vw, 2.65rem);
  letter-spacing: -0.045em;
}
.page-header p,
.overall-card p,
.privacy-card p,
.metric-card small,
.hero-title p,
.signal-item p,
.empty-signals p,
.snapshot-footer {
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
.header-actions {
  gap: 8px;
}
.secondary-button {
  min-height: 40px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 8px 13px;
  border: 1px solid var(--color-border);
  border-radius: 12px;
  background: color-mix(in srgb, var(--color-surface) 88%, transparent);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.04);
  font-weight: 600;
  cursor: pointer;
}
.secondary-button:disabled {
  opacity: 0.5;
  cursor: default;
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
.state-card p {
  margin-top: 3px;
  color: var(--color-text-secondary);
  font-size: 0.8rem;
}
.state-card button {
  margin-left: auto;
}
.error-card,
.warning-card {
  color: var(--color-orange);
}
.safety-banner {
  gap: 10px;
  margin-bottom: 14px;
  padding: 12px 14px;
  border: 1px solid color-mix(in srgb, var(--color-blue) 15%, transparent);
  border-radius: 14px;
  background: var(--color-blue-soft);
  font-size: 0.8rem;
}
.safety-banner svg {
  flex: 0 0 auto;
  color: var(--color-blue);
}
.safety-banner p {
  flex: 1;
  line-height: 1.55;
}
.safety-banner > span {
  padding: 5px 9px;
  border-radius: 999px;
  color: var(--color-green);
  background: var(--color-green-soft);
  white-space: nowrap;
}
.warning-list {
  display: grid;
  gap: 8px;
  margin-bottom: 14px;
}
.summary-grid {
  align-items: stretch;
  gap: 14px;
  margin-bottom: 14px;
}
.overall-card {
  flex: 1;
  gap: 16px;
  padding: 19px 21px;
}
.status-orb {
  width: 56px;
  height: 56px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  border-radius: 50%;
  color: var(--color-green);
  background: var(--color-green-soft);
}
.overall-card.attention .status-orb,
.overall-card.critical .status-orb {
  color: var(--color-orange);
  background: var(--color-orange-soft);
}
.overall-card.unknown .status-orb {
  color: var(--color-text-secondary);
  background: var(--color-surface-muted);
}
.overall-card h2 {
  margin: 3px 0 5px;
  font-size: 1.35rem;
}
.overall-card p,
.privacy-card p {
  font-size: 0.78rem;
  line-height: 1.5;
}
.privacy-card {
  width: min(360px, 34%);
  gap: 12px;
  padding: 18px;
}
.privacy-card svg {
  flex: 0 0 auto;
  color: var(--color-blue);
}
.privacy-card strong {
  display: block;
  margin-bottom: 4px;
  font-size: 0.88rem;
}
.disk-selector {
  display: flex;
  gap: 9px;
  margin-bottom: 14px;
  overflow-x: auto;
  padding: 1px;
}
.disk-pill {
  min-width: 270px;
  flex: 1;
  gap: 11px;
  padding: 11px 13px;
  border: 1px solid var(--color-border);
  border-radius: 15px;
  background: var(--color-surface-muted);
  text-align: left;
  cursor: pointer;
}
.disk-pill.active {
  border-color: color-mix(in srgb, var(--color-blue) 60%, transparent);
  background: var(--color-blue-soft);
}
.disk-icon {
  width: 34px;
  height: 34px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  border-radius: 10px;
  color: var(--color-blue);
  background: var(--color-blue-soft);
}
.disk-copy {
  min-width: 0;
  display: grid;
  gap: 3px;
  flex: 1;
}
.disk-copy strong,
.disk-copy small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.disk-copy small {
  color: var(--color-text-secondary);
  font-size: 0.72rem;
}
.status-dot {
  width: 8px;
  height: 8px;
  flex: 0 0 auto;
  border-radius: 50%;
  background: var(--color-green);
  box-shadow: 0 0 0 4px var(--color-green-soft);
}
.disk-pill.attention .status-dot,
.disk-pill.critical .status-dot {
  background: var(--color-orange);
  box-shadow: 0 0 0 4px var(--color-orange-soft);
}
.disk-pill.unknown .status-dot {
  background: var(--color-text-secondary);
  box-shadow: 0 0 0 4px var(--color-surface-muted);
}
.disk-hero {
  justify-content: space-between;
  gap: 20px;
  padding: 22px;
  margin-bottom: 14px;
  background:
    radial-gradient(
      circle at 92% 10%,
      var(--color-green-soft),
      transparent 38%
    ),
    var(--color-surface);
}
.disk-hero.attention,
.disk-hero.critical {
  background:
    radial-gradient(
      circle at 92% 10%,
      var(--color-orange-soft),
      transparent 38%
    ),
    var(--color-surface);
}
.hero-title h2 {
  margin: 4px 0 6px;
  font-size: 1.38rem;
  letter-spacing: -0.025em;
}
.hero-title p {
  font-size: 0.8rem;
}
.hero-status {
  display: grid;
  gap: 4px;
  text-align: right;
}
.hero-status span {
  font-size: 1.12rem;
  font-weight: 700;
}
.hero-status small {
  color: var(--color-text-secondary);
}
.metric-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
  margin-bottom: 14px;
}
.metric-card {
  min-height: 150px;
  display: grid;
  align-content: start;
  gap: 7px;
  padding: 17px;
}
.metric-card > span:not(.metric-icon):not(.life-track) {
  color: var(--color-text-secondary);
  font-size: 0.74rem;
}
.metric-card strong {
  margin-top: 3px;
  font-size: 1.42rem;
  letter-spacing: -0.035em;
}
.metric-card .compact-value {
  font-size: 0.98rem;
  line-height: 1.4;
}
.metric-icon {
  width: 34px;
  height: 34px;
  display: grid;
  place-items: center;
  margin-bottom: 4px;
  border-radius: 10px;
  color: var(--color-blue);
  background: var(--color-blue-soft);
}
.metric-icon.temperature,
.metric-icon.errors {
  color: var(--color-orange);
  background: var(--color-orange-soft);
}
.metric-icon.life {
  color: var(--color-green);
  background: var(--color-green-soft);
}
.life-track {
  height: 4px;
  display: block;
  margin-top: 3px;
  overflow: hidden;
  border-radius: 99px;
  background: var(--color-surface-muted);
}
.life-track i {
  height: 100%;
  display: block;
  border-radius: inherit;
  background: var(--color-green);
}
.detail-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.2fr) minmax(320px, 0.8fr);
  gap: 14px;
  margin-bottom: 14px;
}
.evidence-card,
.device-card,
.signals-card {
  padding: 21px;
}
.section-heading {
  justify-content: space-between;
  gap: 15px;
  margin-bottom: 16px;
}
.section-heading h2,
.device-card h2 {
  margin-top: 4px;
  font-size: 1.05rem;
}
.evidence-badge {
  padding: 5px 9px;
  border-radius: 999px;
  color: var(--color-green);
  background: var(--color-green-soft);
  font-size: 0.72rem;
}
.evidence-badge.partial,
.evidence-badge.limited {
  color: var(--color-orange);
  background: var(--color-orange-soft);
}
.evidence-list,
.device-list {
  display: grid;
  gap: 0;
  margin: 0;
}
.evidence-list {
  grid-template-columns: repeat(2, minmax(0, 1fr));
  column-gap: 24px;
}
.evidence-list div,
.device-list div {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 0;
  border-bottom: 1px solid var(--color-border);
}
.evidence-list dt,
.device-list dt {
  color: var(--color-text-secondary);
  font-size: 0.76rem;
}
.evidence-list dd,
.device-list dd {
  margin: 0;
  font-size: 0.78rem;
  text-align: right;
  overflow-wrap: anywhere;
}
.device-card h2 {
  margin-bottom: 12px;
}
.encryption-summary {
  display: flex;
  gap: 7px;
  flex-wrap: wrap;
  margin-top: 16px;
}
.encryption-summary strong {
  width: 100%;
  font-size: 0.78rem;
}
.encryption-summary span {
  padding: 5px 8px;
  border-radius: 8px;
  background: var(--color-surface-muted);
  color: var(--color-text-secondary);
  font-size: 0.7rem;
}
.signals-card {
  margin-bottom: 12px;
}
.signals-card > .section-heading > span {
  color: var(--color-text-secondary);
  font-size: 0.75rem;
}
.signal-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 9px;
}
.signal-item {
  display: flex;
  gap: 10px;
  padding: 13px;
  border: 1px solid color-mix(in srgb, var(--color-orange) 18%, transparent);
  border-radius: 13px;
  background: var(--color-orange-soft);
}
.signal-item.unknown {
  border-color: var(--color-border);
  background: var(--color-surface-muted);
}
.signal-icon {
  flex: 0 0 auto;
  color: var(--color-orange);
}
.signal-item.unknown .signal-icon {
  color: var(--color-text-secondary);
}
.signal-level {
  color: var(--color-orange);
  font-size: 0.66rem;
  font-weight: 700;
}
.signal-item.unknown .signal-level {
  color: var(--color-text-secondary);
}
.signal-item h3 {
  margin: 3px 0 4px;
  font-size: 0.84rem;
}
.signal-item p {
  font-size: 0.73rem;
  line-height: 1.5;
}
.signal-item strong {
  display: block;
  margin-top: 7px;
  font-size: 0.72rem;
  line-height: 1.45;
}
.empty-signals {
  gap: 10px;
  padding: 13px;
  border-radius: 13px;
  color: var(--color-green);
  background: var(--color-green-soft);
}
.empty-signals p {
  margin-top: 3px;
  font-size: 0.74rem;
}
.snapshot-footer {
  justify-content: center;
  padding: 8px;
  font-size: 0.7rem;
  text-align: center;
}
.spin {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
@media (max-width: 1100px) {
  .metric-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .detail-grid {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 760px) {
  .page-header {
    align-items: flex-start;
  }
  .header-actions {
    flex-direction: column;
  }
  .summary-grid {
    align-items: stretch;
    flex-direction: column;
  }
  .privacy-card {
    width: 100%;
  }
  .metric-grid,
  .evidence-list,
  .signal-grid {
    grid-template-columns: 1fr;
  }
  .safety-banner {
    align-items: flex-start;
  }
  .safety-banner > span {
    display: none;
  }
}
</style>
