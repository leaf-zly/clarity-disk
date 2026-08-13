<script setup lang="ts">
import { computed } from "vue";
import { CheckCircle2, HardDrive, LockKeyhole, ShieldAlert } from "@lucide/vue";

import type { DiskSummary } from "@/types/dashboard";
import { formatBytes } from "@/utils/format-bytes";

interface Props {
  disks: DiskSummary[];
  activeDiskId: string;
}

interface Emits {
  "select-disk": [disk: DiskSummary];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();

const sortedDisks = computed(() =>
  [...props.disks].sort(
    (left, right) =>
      Number(right.metadata.isSystemVolume) -
        Number(left.metadata.isSystemVolume) || left.id.localeCompare(right.id),
  ),
);

function healthLabel(disk: DiskSummary): string {
  if (disk.metadata.healthStatus === "readOnly") {
    return "只读卷";
  }

  if (disk.metadata.healthStatus === "warning") {
    return "需要注意";
  }

  return "状态良好";
}
</script>

<template>
  <section class="volume-card" aria-labelledby="volume-list-title">
    <div class="section-title">
      <div>
        <h2 id="volume-list-title">磁盘与卷</h2>
        <p>只读发现 · 不会修改文件或分区</p>
      </div>
      <span class="volume-count">{{ sortedDisks.length }} 个卷</span>
    </div>

    <div class="volume-list">
      <button
        v-for="disk in sortedDisks"
        :key="disk.id"
        class="volume-row"
        :class="{ active: disk.id === activeDiskId }"
        type="button"
        :aria-pressed="disk.id === activeDiskId"
        @click="emit('select-disk', disk)"
      >
        <span class="volume-icon"
          ><HardDrive :size="18" aria-hidden="true"
        /></span>
        <span class="volume-copy">
          <strong>{{ disk.label }}</strong>
          <span>
            {{ disk.metadata.fileSystem || "文件系统未知" }} ·
            {{ disk.metadata.deviceType }} ·
            {{ formatBytes(disk.usedBytes) }} 已用 /
            {{ formatBytes(disk.totalBytes) }}
          </span>
        </span>
        <span class="volume-state" :class="disk.metadata.healthStatus">
          <ShieldAlert
            v-if="disk.metadata.healthStatus === 'warning'"
            :size="15"
            aria-hidden="true"
          />
          <LockKeyhole
            v-else-if="disk.metadata.healthStatus === 'readOnly'"
            :size="15"
            aria-hidden="true"
          />
          <CheckCircle2 v-else :size="15" aria-hidden="true" />
          {{ healthLabel(disk) }}
        </span>
      </button>
    </div>
  </section>
</template>

<style scoped>
.volume-card {
  margin-top: 14px;
  padding: 19px 21px;
  border: 1px solid var(--color-border);
  border-radius: 16px;
  background: var(--color-surface);
}

.section-title,
.volume-row {
  display: flex;
  align-items: center;
}

.section-title {
  justify-content: space-between;
  gap: 12px;
}

.section-title h2 {
  font-size: 1rem;
}

.section-title p {
  margin-top: 4px;
  color: var(--color-text-secondary);
  font-size: 0.82rem;
}

.volume-count {
  color: var(--color-text-secondary);
  font-size: 0.82rem;
}

.volume-list {
  display: grid;
  gap: 7px;
  margin-top: 14px;
}

.volume-row {
  width: 100%;
  gap: 11px;
  padding: 11px 12px;
  border: 1px solid transparent;
  border-radius: 11px;
  color: var(--color-text);
  background: var(--color-surface-muted);
  text-align: left;
  cursor: pointer;
}

.volume-row:hover,
.volume-row.active {
  border-color: color-mix(in srgb, var(--color-blue) 32%, var(--color-border));
  background: var(--color-blue-soft);
}

.volume-icon {
  display: grid;
  place-items: center;
  width: 31px;
  height: 31px;
  flex: 0 0 auto;
  border-radius: 9px;
  color: var(--color-blue);
  background: var(--color-surface);
}

.volume-copy {
  min-width: 0;
  display: grid;
  gap: 4px;
  flex: 1;
}

.volume-copy strong,
.volume-copy span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.volume-copy strong {
  font-size: 0.88rem;
}

.volume-copy span {
  color: var(--color-text-secondary);
  font-size: 0.78rem;
}

.volume-state {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  flex: 0 0 auto;
  color: var(--color-green);
  font-size: 0.78rem;
}

.volume-state.readOnly {
  color: var(--color-orange);
}

.volume-state.warning {
  color: #ff6b6b;
}

@media (max-width: 700px) {
  .volume-state {
    display: none;
  }
}
</style>
