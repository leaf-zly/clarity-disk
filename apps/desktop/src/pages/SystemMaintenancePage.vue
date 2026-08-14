<script setup lang="ts">
import { computed, onMounted, shallowRef } from "vue";
import {
  ArchiveRestore,
  CheckCircle2,
  CircleAlert,
  DatabaseZap,
  History,
  LoaderCircle,
  MoonStar,
  ShieldCheck,
} from "@lucide/vue";
import type { Component } from "vue";

import {
  executePrivilegedOperation,
  getPrivilegedAuditEvents,
  getPrivilegedCapabilities,
  prepareMaintenanceExecution,
} from "@/services/privileged-service";
import type {
  MaintenanceCapability,
  MaintenanceOperation,
  PrivilegedAuditEvent,
  PrivilegedCapabilities,
  PrivilegedExecutionChallenge,
  PrivilegedExecutionReport,
} from "@/types/privileged";

/** Immutable presentation metadata paired with one path-free operation. */
interface MaintenanceItem {
  id: string;
  title: string;
  description: string;
  impact: string;
  capability: MaintenanceCapability;
  operation: MaintenanceOperation;
  icon: Component;
}

const capabilities = shallowRef<PrivilegedCapabilities>();
const auditEvents = shallowRef<PrivilegedAuditEvent[]>([]);
const challenge = shallowRef<PrivilegedExecutionChallenge>();
const report = shallowRef<PrivilegedExecutionReport>();
const confirmation = shallowRef("");
const errorMessage = shallowRef<string>();
const isLoading = shallowRef(true);
const isPreparing = shallowRef(false);
const isExecuting = shallowRef(false);

const maintenanceItems: readonly MaintenanceItem[] = [
  {
    id: "disable-hibernation",
    title: "关闭系统休眠",
    description: "由 Windows powercfg 管理 hiberfil.sys，不直接删除系统文件。",
    impact: "会关闭休眠，并可能影响快速启动。",
    capability: "hibernation",
    operation: { operation: "setHibernation", enabled: false },
    icon: MoonStar,
  },
  {
    id: "update-cache",
    title: "维护更新下载缓存",
    description: "短暂停止 Windows Update 与 BITS，只处理固定下载缓存。",
    impact: "正在下载的更新需要重新下载。",
    capability: "windowsUpdateDownloadCache",
    operation: { operation: "resetWindowsUpdateDownloadCache" },
    icon: DatabaseZap,
  },
  {
    id: "restore-point",
    title: "创建系统还原点",
    description: "通过 Windows System Restore 创建应用标记的还原点。",
    impact: "系统策略可能限制创建频率，并占用还原存储空间。",
    capability: "systemRestorePoint",
    operation: { operation: "createSystemRestorePoint" },
    icon: ArchiveRestore,
  },
];

const canConfirm = computed(
  () =>
    Boolean(challenge.value) &&
    confirmation.value === challenge.value?.confirmationPhrase &&
    !isExecuting.value,
);

onMounted(() => void loadStatus());

async function loadStatus(): Promise<void> {
  isLoading.value = true;
  errorMessage.value = undefined;
  try {
    const [nextCapabilities, nextAudit] = await Promise.all([
      getPrivilegedCapabilities(),
      getPrivilegedAuditEvents(),
    ]);
    capabilities.value = nextCapabilities;
    auditEvents.value = nextAudit;
  } catch {
    errorMessage.value = "无法读取管理员服务状态，未执行任何系统修改。";
  } finally {
    isLoading.value = false;
  }
}

function itemAvailable(item: MaintenanceItem): boolean {
  return Boolean(
    capabilities.value?.serviceAvailable &&
    capabilities.value.maintenanceOperations.includes(item.capability),
  );
}

async function prepare(item: MaintenanceItem): Promise<void> {
  isPreparing.value = true;
  errorMessage.value = undefined;
  report.value = undefined;
  challenge.value = undefined;
  confirmation.value = "";
  try {
    challenge.value = await prepareMaintenanceExecution(item.operation);
  } catch (error) {
    errorMessage.value =
      error instanceof Error
        ? error.message
        : "管理员操作准备失败，未修改系统。";
  } finally {
    isPreparing.value = false;
  }
}

async function execute(): Promise<void> {
  const current = challenge.value;
  if (!current || !canConfirm.value) return;
  isExecuting.value = true;
  errorMessage.value = undefined;
  try {
    report.value = await executePrivilegedOperation({
      challengeId: current.challengeId,
      confirmationToken: current.confirmationToken,
      confirmationPhrase: confirmation.value,
    });
    challenge.value = undefined;
    confirmation.value = "";
    auditEvents.value = await getPrivilegedAuditEvents();
  } catch (error) {
    errorMessage.value =
      error instanceof Error
        ? error.message
        : "管理员服务没有完成操作，请检查系统状态。";
  } finally {
    isExecuting.value = false;
  }
}
</script>

<template>
  <section class="maintenance-page" aria-labelledby="maintenance-title">
    <header class="page-header">
      <div>
        <span class="eyebrow"><ShieldCheck :size="14" />计划八</span>
        <h1 id="maintenance-title">管理员维护</h1>
        <p>普通界面不提权。只有确认后的枚举操作进入一次性管理员代理。</p>
      </div>
      <span
        class="service-pill"
        :class="{ online: capabilities?.serviceAvailable }"
      >
        {{
          isLoading
            ? "正在握手"
            : capabilities?.serviceAvailable
              ? `协议 v${capabilities.schemaVersion}`
              : "服务未安装"
        }}
      </span>
    </header>

    <section class="boundary-card">
      <ShieldCheck :size="22" aria-hidden="true" />
      <div>
        <strong>最小权限边界</strong>
        <p>
          协议不接受脚本、命令、注册表路径或文件路径；请求两分钟过期，首次提交即消费。
        </p>
      </div>
      <code>schema {{ capabilities?.schemaVersion ?? 1 }}</code>
    </section>

    <p v-if="errorMessage" class="message error" role="alert">
      <CircleAlert :size="17" />{{ errorMessage }}
    </p>
    <p v-if="report" class="message result" role="status">
      <CheckCircle2 :size="17" />{{ report.message }}
    </p>

    <div class="maintenance-grid">
      <article
        v-for="item in maintenanceItems"
        :key="item.id"
        class="item-card"
      >
        <span class="item-icon"><component :is="item.icon" :size="21" /></span>
        <div>
          <h2>{{ item.title }}</h2>
          <p>{{ item.description }}</p>
          <small>{{ item.impact }}</small>
        </div>
        <button
          type="button"
          :disabled="!itemAvailable(item) || isPreparing || isExecuting"
          @click="prepare(item)"
        >
          <LoaderCircle v-if="isPreparing" class="spin" :size="15" />
          准备确认
        </button>
      </article>
    </div>

    <section v-if="challenge" class="confirmation-card" aria-label="管理员确认">
      <div>
        <span class="section-kicker">一次性确认</span>
        <h2>Windows 将在提交后显示 UAC</h2>
        <p>{{ challenge.impact }}</p>
      </div>
      <label>
        <span>请输入：{{ challenge.confirmationPhrase }}</span>
        <input v-model="confirmation" autocomplete="off" />
      </label>
      <button type="button" :disabled="!canConfirm" @click="execute">
        <LoaderCircle v-if="isExecuting" class="spin" :size="16" />
        {{ isExecuting ? "等待管理员服务" : "确认并请求管理员权限" }}
      </button>
    </section>

    <section class="partition-gate">
      <div>
        <span class="section-kicker">计划九 · 实验能力</span>
        <h2>真实分区合并执行器</h2>
        <p>
          编译门禁、管理员运行时门禁、已验证独立备份与新鲜磁盘证据必须同时通过。
        </p>
      </div>
      <dl>
        <div>
          <dt>编译门禁</dt>
          <dd>
            {{ capabilities?.partitionWriterCompiled ? "已编译" : "关闭" }}
          </dd>
        </div>
        <div>
          <dt>运行时门禁</dt>
          <dd>
            {{
              capabilities?.partitionWriterRuntimeEnabled ? "已启用" : "关闭"
            }}
          </dd>
        </div>
      </dl>
    </section>

    <section class="history-card">
      <div class="section-heading">
        <div>
          <span class="section-kicker">本地审计</span>
          <h2>最近管理员结果</h2>
        </div>
        <History :size="19" />
      </div>
      <p v-if="!auditEvents.length" class="empty">暂无管理员执行记录。</p>
      <ul v-else>
        <li v-for="event in auditEvents.slice(0, 5)" :key="event.requestId">
          <span>{{ event.status }}</span>
          <strong>{{ event.message }}</strong>
          <time>{{
            new Date(event.completedAtUnixMs).toLocaleString("zh-CN")
          }}</time>
        </li>
      </ul>
    </section>
  </section>
</template>

<style scoped>
.maintenance-page {
  width: min(1120px, 100%);
  margin: 0 auto;
}
.page-header,
.boundary-card,
.item-card,
.section-heading,
.partition-gate {
  display: flex;
  align-items: center;
}
.page-header {
  justify-content: space-between;
  gap: 24px;
  margin-bottom: 20px;
}
.page-header h1 {
  margin: 6px 0;
  font-size: clamp(2rem, 4vw, 2.65rem);
  letter-spacing: -0.045em;
}
.page-header p,
.boundary-card p,
.item-card p,
.partition-gate p,
.confirmation-card p {
  color: var(--color-text-secondary);
}
.eyebrow,
.section-kicker {
  color: var(--color-blue);
  font-size: 0.75rem;
  font-weight: 650;
}
.eyebrow {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  padding: 5px 9px;
  border-radius: 999px;
  background: var(--color-blue-soft);
}
.service-pill {
  padding: 7px 11px;
  border-radius: 999px;
  color: var(--color-orange);
  background: var(--color-orange-soft);
  font-size: 0.73rem;
  font-weight: 650;
}
.service-pill.online {
  color: var(--color-green);
  background: var(--color-green-soft);
}
.boundary-card,
.confirmation-card,
.partition-gate,
.history-card,
.item-card {
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
}
.boundary-card {
  gap: 12px;
  padding: 15px 17px;
  border-radius: 17px;
}
.boundary-card > div {
  flex: 1;
}
.boundary-card p {
  margin-top: 3px;
  font-size: 0.76rem;
}
.boundary-card code {
  color: var(--color-blue);
  font-size: 0.7rem;
}
.message {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 12px;
  padding: 12px 14px;
  border-radius: 13px;
  font-size: 0.8rem;
}
.message.error {
  color: var(--color-orange);
  background: var(--color-orange-soft);
}
.message.result {
  color: var(--color-green);
  background: var(--color-green-soft);
}
.maintenance-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  margin-top: 14px;
}
.item-card {
  align-items: flex-start;
  flex-direction: column;
  gap: 12px;
  padding: 18px;
  border-radius: 18px;
}
.item-icon {
  width: 38px;
  height: 38px;
  display: grid;
  place-items: center;
  color: var(--color-blue);
  border-radius: 12px;
  background: var(--color-blue-soft);
}
.item-card h2 {
  margin-bottom: 6px;
  font-size: 1rem;
}
.item-card p {
  min-height: 42px;
  font-size: 0.75rem;
  line-height: 1.5;
}
.item-card small {
  display: block;
  margin-top: 9px;
  color: var(--color-orange);
  line-height: 1.45;
}
.item-card button,
.confirmation-card button {
  width: 100%;
  min-height: 38px;
  border: 0;
  border-radius: 11px;
  color: white;
  background: var(--color-blue);
  font-weight: 650;
  cursor: pointer;
}
button:disabled {
  opacity: 0.45;
  cursor: default;
}
.confirmation-card {
  display: grid;
  grid-template-columns: 1fr minmax(260px, 0.8fr) 230px;
  align-items: end;
  gap: 18px;
  padding: 20px;
  margin-top: 14px;
  border-radius: 18px;
  border-color: color-mix(in srgb, var(--color-orange) 30%, transparent);
}
.confirmation-card h2,
.partition-gate h2,
.history-card h2 {
  margin: 4px 0 6px;
  font-size: 1.05rem;
}
.confirmation-card p {
  font-size: 0.75rem;
  line-height: 1.5;
}
.confirmation-card label {
  display: grid;
  gap: 7px;
  font-size: 0.72rem;
  color: var(--color-text-secondary);
}
.confirmation-card input {
  min-height: 39px;
  padding: 0 10px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  color: var(--color-text);
  background: var(--color-surface-muted);
}
.partition-gate {
  justify-content: space-between;
  gap: 28px;
  padding: 20px;
  margin-top: 14px;
  border-radius: 18px;
}
.partition-gate p {
  max-width: 650px;
  font-size: 0.76rem;
}
.partition-gate dl {
  min-width: 230px;
  margin: 0;
}
.partition-gate dl div {
  display: flex;
  justify-content: space-between;
  gap: 14px;
  padding: 5px 0;
  font-size: 0.73rem;
}
.partition-gate dt {
  color: var(--color-text-secondary);
}
.partition-gate dd {
  margin: 0;
  font-weight: 650;
}
.history-card {
  padding: 20px;
  margin-top: 14px;
  border-radius: 18px;
}
.section-heading {
  justify-content: space-between;
}
.empty {
  color: var(--color-text-secondary);
  font-size: 0.76rem;
}
.history-card ul {
  display: grid;
  gap: 7px;
  padding: 0;
  margin: 12px 0 0;
  list-style: none;
}
.history-card li {
  display: grid;
  grid-template-columns: 120px 1fr auto;
  gap: 10px;
  padding: 9px 10px;
  border-radius: 10px;
  background: var(--color-surface-muted);
  font-size: 0.72rem;
}
.history-card time,
.history-card li > span {
  color: var(--color-text-secondary);
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
  .maintenance-grid {
    grid-template-columns: 1fr;
  }
  .confirmation-card {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 650px) {
  .page-header,
  .partition-gate {
    align-items: flex-start;
    flex-direction: column;
  }
  .partition-gate dl {
    width: 100%;
  }
}
</style>
