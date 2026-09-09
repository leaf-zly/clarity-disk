<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef } from "vue";
import { CloudDownload, RefreshCw, ShieldCheck } from "@lucide/vue";
import {
  checkOnlineUpdate,
  UPDATE_CHANNEL,
  updateErrorMessage,
  type OnlineUpdate,
} from "@/services/online-update-service";
import { formatBytes } from "@/utils/format-bytes";

/** Settings-controlled network consent; toggling it never starts a background download. */
const props = defineProps<{ enabled: boolean }>();
const phase = ref<
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "verifying"
  | "ready"
  | "installing"
  | "current"
>("idle");
const update = shallowRef<OnlineUpdate | null>(null);
const error = ref("");
const downloaded = ref(0);
const total = ref<number>();
const confirmed = ref(false);
const currentVersion = __APP_VERSION__;
let disposed = false;
const busy = computed(() =>
  ["checking", "downloading", "verifying", "installing"].includes(phase.value),
);
const progress = computed(() =>
  total.value
    ? Math.min(100, Math.floor((downloaded.value / total.value) * 100))
    : undefined,
);

/** Drops downloaded resources when replaced, discarded, or eventually unmounted. */
async function discard(): Promise<void> {
  const previous = update.value;
  update.value = null;
  confirmed.value = false;
  await previous?.close().catch(() => undefined);
}

/** Performs only a metadata check; old candidates are discarded before retry. */
async function checkNow(): Promise<void> {
  if (busy.value || !props.enabled) return;
  phase.value = "checking";
  error.value = "";
  try {
    await discard();
    const result = await checkOnlineUpdate();
    if (disposed) {
      await result?.close();
      return;
    }
    update.value = result;
    phase.value = result ? "available" : "current";
  } catch (cause) {
    error.value = updateErrorMessage(cause);
    phase.value = "idle";
  }
}

/** Downloads and waits for cryptographic verification; does not run the installer. */
async function download(): Promise<void> {
  if (phase.value !== "available" || !update.value || !props.enabled) return;
  phase.value = "downloading";
  error.value = "";
  downloaded.value = 0;
  total.value = undefined;
  confirmed.value = false;
  try {
    await update.value.download((event) => {
      if (event.event === "Started") total.value = event.data.contentLength;
      else if (event.event === "Progress")
        downloaded.value += event.data.chunkLength;
      else phase.value = "verifying";
    });
    phase.value = "ready";
  } catch (cause) {
    error.value = updateErrorMessage(cause);
    await discard();
    phase.value = "idle";
  } finally {
    if (disposed) await discard();
  }
}

/** Installs only after the user acknowledges shutdown and the backend accepts a safe reservation. */
async function install(): Promise<void> {
  if (phase.value !== "ready" || !confirmed.value || !update.value) return;
  phase.value = "installing";
  error.value = "";
  try {
    await update.value.install();
    // A successful Windows install exits the app. If it returns, require a fresh
    // download instead of reusing a consumed native byte resource.
    await discard();
    phase.value = "idle";
  } catch (cause) {
    error.value = updateErrorMessage(cause);
    await discard();
    phase.value = "idle";
  }
}

onBeforeUnmount(() => {
  disposed = true;
  if (!busy.value) void discard();
});
</script>

<template>
  <article
    class="update-card"
    aria-labelledby="online-update-title"
    :aria-busy="busy"
  >
    <header>
      <CloudDownload :size="20" aria-hidden="true" />
      <div>
        <h2 id="online-update-title">在线更新</h2>
        <p>
          当前版本 {{ currentVersion }} ·
          {{ UPDATE_CHANNEL === "preview" ? "测试版通道" : "正式版通道" }}
        </p>
      </div>
    </header>
    <p v-if="UPDATE_CHANNEL === 'preview'" class="notice">
      测试包使用专用更新签名校验，未做 Windows
      发布者签名，可能出现安全提示。正式版仍要求 Authenticode 与 SHA-256。
    </p>
    <p v-else class="notice">
      仅接收正式发布的更新。发布流程要求 Authenticode 与
      SHA-256，下载后还会验证专用更新签名。
    </p>
    <p v-if="!enabled" class="notice">
      检查更新已关闭。启用并保存上方设置后可以检查和下载。
    </p>
    <button type="button" :disabled="busy || !enabled" @click="checkNow">
      <RefreshCw :size="15" aria-hidden="true" />{{
        phase === "checking" ? "正在检查…" : "检查更新"
      }}
    </button>
    <p v-if="error" role="alert" class="error">{{ error }}</p>
    <p v-if="phase === 'current'" role="status">当前已是此通道的最新版本。</p>
    <div v-if="update" class="update-detail">
      <strong>发现新版本 {{ update.version }}</strong>
      <details>
        <summary>更新说明</summary>
        <p class="release-notes">{{ update.notes }}</p>
      </details>
      <button
        v-if="phase === 'available'"
        type="button"
        class="primary"
        :disabled="!enabled"
        @click="download"
      >
        下载更新
      </button>
      <div
        v-if="phase === 'downloading' || phase === 'verifying'"
        role="status"
      >
        <progress
          :value="progress"
          max="100"
          aria-label="更新下载进度"
        ></progress>
        <p>
          {{
            phase === "verifying"
              ? "下载完成，正在验证签名…"
              : `已下载 ${formatBytes(downloaded)}${total ? ` / ${formatBytes(total)}` : ""}`
          }}
        </p>
      </div>
      <div v-if="phase === 'ready'" class="install-confirmation">
        <p role="status">
          <ShieldCheck :size="16" aria-hidden="true" /> 更新包已通过签名校验。
        </p>
        <label
          ><input
            v-model="confirmed"
            type="checkbox"
          />我已保存工作并停止扫描。安装会关闭 Clarity
          Disk，完成后重新启动。</label
        >
        <button
          type="button"
          class="primary"
          :disabled="!confirmed"
          @click="install"
        >
          安装并重启
        </button>
      </div>
      <p v-if="phase === 'installing'" role="status">
        正在启动安装程序，请勿重复操作…
      </p>
    </div>
    <a
      href="https://github.com/leaf-zly/clarity-disk/releases"
      target="_blank"
      rel="noreferrer"
      >查看 GitHub 发布记录</a
    >
  </article>
</template>

<style scoped>
.update-card {
  display: grid;
  align-content: start;
  gap: 14px;
  padding: 22px;
  border: 1px solid var(--color-border);
  border-radius: 18px;
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
  min-width: 0;
}
header {
  display: flex;
  align-items: center;
  gap: 12px;
}
header > svg {
  color: var(--color-blue);
  flex-shrink: 0;
}
h2 {
  font-size: 1.05rem;
}
p,
label {
  font-size: 0.85rem;
  line-height: 1.6;
}
header p,
.notice {
  color: var(--color-text-secondary);
}
button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: fit-content;
  min-height: 38px;
  padding: 8px 14px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-surface-muted);
  color: var(--color-text);
  cursor: pointer;
}
button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.primary {
  background: var(--color-blue);
  border: 0;
  color: white;
}
.update-detail,
.install-confirmation {
  display: grid;
  gap: 12px;
}
.install-confirmation {
  padding: 14px;
  background: var(--color-surface-muted);
  border-radius: 12px;
}
.install-confirmation label {
  display: flex;
  align-items: start;
  gap: 8px;
}
.install-confirmation input {
  margin-top: 5px;
}
.install-confirmation p {
  color: var(--color-green);
}
.release-notes {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 200px;
  overflow: auto;
}
summary {
  cursor: pointer;
  color: var(--color-text-secondary);
}
progress {
  width: 100%;
  accent-color: var(--color-blue);
}
.error {
  color: var(--color-red, #c9342f);
}
a {
  color: var(--color-blue);
  font-size: 0.82rem;
}
</style>
