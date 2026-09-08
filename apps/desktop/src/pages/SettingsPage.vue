<script setup lang="ts">
import {
  computed,
  onDeactivated,
  onMounted,
  ref,
  shallowRef,
  watch,
} from "vue";
import {
  Activity,
  Bell,
  CheckCircle2,
  CloudDownload,
  LockKeyhole,
  Play,
  Save,
  ShieldCheck,
  Trash2,
} from "@lucide/vue";

import {
  clearCrashDiagnostics,
  getAppSettings,
  getDiagnosticsSnapshot,
  runAutomaticMaintenance,
  updateAppSettings,
} from "@/services/operations-service";
import { checkForUpdates } from "@/services/release-service";
import { applyLanguagePreference, translate } from "@/services/locale-service";
import { applyThemePreference } from "@/services/theme-service";
import type {
  AppSettings,
  AutomaticMaintenanceRunReport,
  DiagnosticsSnapshot,
  UpdateRelease,
} from "@/types/operations";

const settings = ref<AppSettings | null>(null);
const diagnostics = shallowRef<DiagnosticsSnapshot | null>(null);
const maintenanceReport = shallowRef<AutomaticMaintenanceRunReport | null>(
  null,
);
const release = shallowRef<UpdateRelease | null>(null);
const ignoredRootsText = ref("");
const busy = ref(false);
const updateBusy = ref(false);
const message = ref("");
const errorMessage = ref("");
const diagnosticsWarning = ref("");
const persistedTheme = shallowRef<AppSettings["theme"]>("system");
const persistedLanguage =
  shallowRef<AppSettings["language"]>("simplifiedChinese");

const performancePassed = computed(
  () =>
    diagnostics.value?.performanceMetrics.filter(
      (metric) => metric.withinBaseline,
    ).length ?? 0,
);

/** Refreshes optional diagnostics without changing the outcome of a settings write. */
async function refreshDiagnostics(): Promise<void> {
  diagnosticsWarning.value = "";
  try {
    diagnostics.value = await getDiagnosticsSnapshot();
  } catch {
    diagnostics.value = null;
    diagnosticsWarning.value =
      "诊断信息暂时无法读取，不影响设置使用或已保存的更改。";
  }
}

/** Loads editable preferences independently from optional diagnostic data. */
async function load(): Promise<void> {
  busy.value = true;
  errorMessage.value = "";
  try {
    const nextSettings = await getAppSettings();
    persistedTheme.value = nextSettings.theme;
    persistedLanguage.value = nextSettings.language;
    applyLanguagePreference(nextSettings.language);
    settings.value = nextSettings;
    ignoredRootsText.value = nextSettings.ignoredScanRoots.join("\n");
    await refreshDiagnostics();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

/** Persists validated preferences; ancillary read errors never imply rollback. */
async function save(): Promise<void> {
  if (!settings.value || busy.value) return;
  busy.value = true;
  message.value = "";
  errorMessage.value = "";
  try {
    const payload: AppSettings = {
      ...settings.value,
      ignoredScanRoots: ignoredRootsText.value
        .split(/\r?\n/)
        .map((value) => value.trim())
        .filter(Boolean),
    };
    settings.value = await updateAppSettings(payload);
    ignoredRootsText.value = settings.value.ignoredScanRoots.join("\n");
    persistedTheme.value = settings.value.theme;
    persistedLanguage.value = settings.value.language;
    applyLanguagePreference(settings.value.language);
    applyThemePreference(settings.value.theme);
    message.value = "设置已验证并保存。";
    await refreshDiagnostics();
  } catch (error) {
    errorMessage.value = settingsSaveError(error);
  } finally {
    busy.value = false;
  }
}

async function runMaintenance(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  message.value = "";
  errorMessage.value = "";
  try {
    maintenanceReport.value = await runAutomaticMaintenance();
    message.value = maintenanceLabel(maintenanceReport.value);
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}

async function checkUpdate(): Promise<void> {
  if (updateBusy.value) return;
  updateBusy.value = true;
  errorMessage.value = "";
  try {
    release.value = await checkForUpdates();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    updateBusy.value = false;
  }
}

/** Clears local markers and reports subsequent read failures separately. */
async function clearDiagnostics(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  message.value = "";
  errorMessage.value = "";
  try {
    await clearCrashDiagnostics();
    message.value = "本地崩溃标记已清除。";
    await refreshDiagnostics();
  } catch {
    errorMessage.value = "本地崩溃标记清除失败，请稍后重试。";
  } finally {
    busy.value = false;
  }
}

function settingsSaveError(error: unknown): string {
  const detail = error instanceof Error ? error.message : String(error);
  if (/launch-at-login|登录启动|startup/i.test(detail)) {
    return "Windows 登录启动设置保存失败，其他设置未更改。";
  }
  return `设置保存失败：${detail}`;
}

function maintenanceLabel(report: AutomaticMaintenanceRunReport): string {
  if (report.preview)
    return `只读维护扫描完成，发现 ${report.preview.candidates.length} 类候选；没有自动删除。`;
  const labels: Record<string, string> = {
    disabled: "自动维护未启用。",
    notDue: "尚未到下一次维护时间。",
    userActive: "检测到用户正在使用设备，本次已避让。",
    powerUnsafe: "无法确认稳定供电，本次已暂停。",
    systemUpdateActive: "Windows 更新正在运行，本次已避让。",
    backupActive: "备份任务正在运行，本次已避让。",
  };
  return labels[report.decision.reason] ?? "本次维护未运行。";
}

watch(
  () => settings.value?.theme,
  (theme) => {
    if (theme) applyThemePreference(theme);
  },
);

watch(
  () => settings.value?.language,
  (language) => {
    if (language) applyLanguagePreference(language);
  },
);

onDeactivated(() => {
  if (!settings.value) return;
  if (settings.value.theme !== persistedTheme.value) {
    settings.value.theme = persistedTheme.value;
    applyThemePreference(persistedTheme.value);
  }
  if (settings.value.language !== persistedLanguage.value) {
    settings.value.language = persistedLanguage.value;
    applyLanguagePreference(persistedLanguage.value);
  }
});

onMounted(() => void load());
</script>

<template>
  <section class="settings-page" aria-labelledby="settings-title">
    <header class="page-header">
      <div>
        <p class="eyebrow">{{ translate("localFirst") }}</p>
        <h1 id="settings-title">{{ translate("settingsTitle") }}</h1>
        <p>所有偏好使用版本化本地配置；忽略规则不能绕过执行器安全校验。</p>
      </div>
      <button
        class="primary"
        type="button"
        :disabled="busy || !settings"
        @click="save"
      >
        <Save :size="16" />{{ translate("saveChanges") }}
      </button>
    </header>

    <p v-if="message" class="message success" role="status">{{ message }}</p>
    <p v-if="errorMessage" class="message error" role="alert">
      {{ errorMessage }}
    </p>
    <p v-if="diagnosticsWarning" class="message" role="status">
      {{ diagnosticsWarning }}
    </p>

    <template v-if="settings">
      <fieldset class="settings-grid" :disabled="busy" aria-label="应用偏好">
        <article class="card">
          <div class="card-title">
            <ShieldCheck :size="20" />
            <div>
              <h2>{{ translate("appearance") }}</h2>
              <p>{{ translate("appearanceDescription") }}</p>
            </div>
          </div>
          <label
            >{{ translate("theme")
            }}<select v-model="settings.theme">
              <option value="system">跟随系统</option>
              <option value="light">浅色</option>
              <option value="dark">深色</option>
            </select></label
          >
          <label
            >{{ translate("language")
            }}<select v-model="settings.language">
              <option value="simplifiedChinese">
                {{ translate("simplifiedChinese") }}
              </option>
              <option value="english">{{ translate("english") }}</option>
            </select></label
          >
          <label class="switch"
            ><input v-model="settings.launchAtLogin" type="checkbox" /><span>{{
              translate("launchAtLogin")
            }}</span></label
          >
          <label class="switch"
            ><input
              v-model="settings.notificationsEnabled"
              type="checkbox"
            /><span>{{ translate("notifications") }}</span></label
          >
        </article>

        <article class="card">
          <div class="card-title">
            <LockKeyhole :size="20" />
            <div>
              <h2>{{ translate("privacyDiagnostics") }}</h2>
              <p>日志不会保存文件内容或完整敏感路径。</p>
            </div>
          </div>
          <label
            >本地日志级别<select v-model="settings.logLevel">
              <option value="minimal">最少</option>
              <option value="standard">标准</option>
              <option value="diagnostic">诊断（含性能时序）</option>
            </select></label
          >
          <label class="switch"
            ><input
              v-model="settings.retainCrashDiagnostics"
              type="checkbox"
            /><span>保留隐私安全的崩溃标记</span></label
          >
          <label class="switch"
            ><input
              v-model="settings.updateChecksEnabled"
              type="checkbox"
            /><span>允许检查官方 GitHub Release 元数据</span></label
          >
          <button
            class="secondary"
            type="button"
            :disabled="busy || !diagnostics?.crashReports.length"
            @click="clearDiagnostics"
          >
            <Trash2 :size="15" />清除崩溃标记
          </button>
        </article>

        <article class="card wide">
          <div class="card-title">
            <Activity :size="20" />
            <div>
              <h2>自动维护</h2>
              <p>
                只在空闲、稳定供电且更新/备份未运行时执行只读扫描；不会自动删除。
              </p>
            </div>
          </div>
          <div class="inline-fields">
            <label
              >频率<select v-model="settings.automaticMaintenance">
                <option value="disabled">关闭</option>
                <option value="weekly">每周</option>
                <option value="monthly">每月</option>
              </select></label
            >
            <label
              >隔离保留<select v-model="settings.quarantineRetentionDays">
                <option :value="7">7 天</option>
                <option :value="15">15 天</option>
                <option :value="30">30 天</option>
              </select></label
            >
            <label
              >容量上限<select v-model="settings.quarantineMaxBytes">
                <option :value="1 * 1024 ** 3">1 GB</option>
                <option :value="5 * 1024 ** 3">5 GB</option>
                <option :value="10 * 1024 ** 3">10 GB</option>
                <option :value="20 * 1024 ** 3">20 GB</option>
              </select></label
            >
            <button
              class="secondary"
              type="button"
              :disabled="busy"
              @click="runMaintenance"
            >
              <Play :size="15" />立即评估
            </button>
          </div>
          <p v-if="maintenanceReport" class="inline-note">
            {{ maintenanceLabel(maintenanceReport) }}
          </p>
        </article>

        <article class="card wide">
          <div class="card-title">
            <Bell :size="20" />
            <div>
              <h2>扫描忽略范围</h2>
              <p>
                每行一个绝对 Windows 路径，最多 32
                项；此设置只影响普通只读扫描。
              </p>
            </div>
          </div>
          <textarea
            v-model="ignoredRootsText"
            rows="4"
            placeholder="D:\Projects\archive"
          ></textarea>
        </article>
      </fieldset>

      <div class="release-grid">
        <article class="card">
          <div class="card-title">
            <CloudDownload :size="20" />
            <div>
              <h2>更新与发布</h2>
              <p>
                仅查询官方仓库；安装包必须同时具备 Authenticode 与 SHA-256。
              </p>
            </div>
          </div>
          <button
            class="secondary"
            type="button"
            :disabled="updateBusy || !settings.updateChecksEnabled"
            @click="checkUpdate"
          >
            {{ updateBusy ? "检查中…" : "检查更新" }}
          </button>
          <div v-if="release" class="release-result">
            <strong>{{
              !release.publishedAt
                ? "尚未发布正式版本"
                : release.updateAvailable
                  ? `发现 ${release.latestVersion}`
                  : `已是最新 ${release.currentVersion}`
            }}</strong>
            <p v-if="!release.publishedAt">
              当前为功能测试构建；正式发布后才会提供签名安装包和校验文件。
            </p>
            <span
              v-if="release.publishedAt"
              :class="{ passed: release.hasChecksums }"
              >SHA-256 {{ release.hasChecksums ? "可用" : "缺失" }}</span
            >
            <span
              v-if="release.publishedAt"
              :class="{ passed: release.hasWindowsInstaller }"
              >Windows 安装包
              {{ release.hasWindowsInstaller ? "可用" : "缺失" }}</span
            >
            <a :href="release.releaseUrl" target="_blank" rel="noreferrer"
              >在 GitHub 查看发布页面</a
            >
          </div>
        </article>

        <article class="card">
          <div class="card-title">
            <CheckCircle2 :size="20" />
            <div>
              <h2>性能基线</h2>
              <p>首页 1.5 秒、分区发现 5 秒、清理扫描 30 秒。</p>
            </div>
          </div>
          <strong class="metric"
            >{{ performancePassed }} /
            {{ diagnostics?.performanceMetrics.length ?? 0 }}</strong
          >
          <p>最近进程内测量符合基线</p>
          <ul class="metric-list">
            <li
              v-for="metric in diagnostics?.performanceMetrics.slice(0, 4)"
              :key="`${metric.operation}-${metric.measuredAtUnixMs}`"
            >
              <span>{{ metric.operation }}</span
              ><strong :class="{ failed: !metric.withinBaseline }"
                >{{ metric.durationMs }} ms</strong
              >
            </li>
          </ul>
        </article>
      </div>
    </template>
  </section>
</template>

<style scoped>
fieldset.settings-grid {
  border: 0;
  padding: 0;
  margin: 0;
  min-width: 0;
}

.settings-page {
  max-width: 1180px;
  margin: 0 auto;
  display: grid;
  gap: 20px;
}
.page-header,
.card-title,
.inline-fields {
  display: flex;
  align-items: center;
  gap: 12px;
}
.page-header {
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
  margin: 5px 0 8px;
  font-size: clamp(1.8rem, 3vw, 2.5rem);
  letter-spacing: -0.04em;
}
h2 {
  font-size: 1rem;
}
p {
  color: var(--color-text-secondary);
}
.settings-grid,
.release-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
}
.wide {
  grid-column: 1 / -1;
}
.card {
  padding: 19px;
  display: grid;
  align-content: start;
  gap: 15px;
  border: 1px solid var(--color-border);
  border-radius: 18px;
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
}
.card-title {
  align-items: flex-start;
}
.card-title > div {
  display: grid;
  gap: 4px;
}
.card-title svg {
  color: var(--color-blue);
}
label {
  display: grid;
  gap: 6px;
  color: var(--color-text-secondary);
  font-size: 0.82rem;
}
select,
textarea {
  width: 100%;
  padding: 9px 10px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  color: var(--color-text);
  background: var(--color-surface-muted);
  font: inherit;
}
.switch {
  display: flex;
  align-items: center;
  gap: 9px;
  color: var(--color-text);
}
.inline-fields {
  align-items: end;
  flex-wrap: wrap;
}
.inline-fields label {
  min-width: 150px;
  flex: 1;
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
.primary {
  color: white;
  border: 0;
  background: var(--color-blue);
}
.secondary {
  justify-self: start;
  border: 1px solid var(--color-border);
  color: var(--color-text);
  background: var(--color-surface-muted);
}
button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.message,
.inline-note {
  padding: 11px 13px;
  border-radius: 10px;
}
.success,
.inline-note {
  color: var(--color-green);
  background: var(--color-green-soft);
}
.error {
  color: #c9342f;
  background: color-mix(in srgb, #d93b36 10%, transparent);
}
.release-result {
  display: grid;
  gap: 8px;
  padding: 13px;
  border-radius: 12px;
  background: var(--color-surface-muted);
}
.release-result span {
  color: var(--color-orange);
}
.release-result span.passed {
  color: var(--color-green);
}
.release-result a {
  color: var(--color-blue);
}
.metric {
  font-size: 2rem;
}
.metric-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 7px;
}
.metric-list li {
  display: flex;
  justify-content: space-between;
}
.metric-list strong {
  color: var(--color-green);
}
.metric-list strong.failed {
  color: var(--color-orange);
}
@media (max-width: 900px) {
  .settings-grid,
  .release-grid {
    grid-template-columns: 1fr;
  }
  .wide {
    grid-column: auto;
  }
  .page-header {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
