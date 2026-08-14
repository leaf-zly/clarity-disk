<script setup lang="ts">
import { computed } from "vue";

import { formatBytes } from "@/utils/format-bytes";
import type {
  PartitionDescriptor,
  PartitionKind,
  SimulatedPartition,
} from "@/types/partition";

/** Props for rendering current or simulated disk regions as one topology bar. */
interface Props {
  partitions: readonly (PartitionDescriptor | SimulatedPartition)[];
  diskSizeBytes: number;
  selectedIds?: readonly string[];
  interactive?: boolean;
}

/** Emits backend partition identities selected from the current topology. */
interface Emits {
  selectPartition: [partitionId: string];
}

const props = withDefaults(defineProps<Props>(), {
  selectedIds: () => [],
  interactive: false,
});
const emit = defineEmits<Emits>();

const kindLabels: Readonly<Record<PartitionKind, string>> = {
  data: "数据",
  efiSystem: "EFI",
  microsoftReserved: "MSR",
  recovery: "恢复",
  system: "系统",
  unallocated: "未分配",
  unknown: "未知",
};

const regions = computed(() =>
  props.partitions.map((partition) => ({
    ...partition,
    width: Math.max(
      1.2,
      Math.min(100, (partition.sizeBytes / props.diskSizeBytes) * 100),
    ),
    label: regionLabel(partition),
    expanded:
      "isExpandedTarget" in partition && partition.isExpandedTarget === true,
  })),
);

/** Builds a compact label while retaining exact capacity in the tooltip. */
function regionLabel(
  partition: PartitionDescriptor | SimulatedPartition,
): string {
  if ("label" in partition && partition.label) return partition.label;
  if ("mountPoints" in partition && partition.mountPoints[0])
    return partition.mountPoints[0];
  return kindLabels[partition.kind];
}

/** Prevents protected and synthetic regions from becoming preview inputs. */
function isSelectable(
  partition: PartitionDescriptor | SimulatedPartition,
): boolean {
  return (
    props.interactive &&
    "fileSystem" in partition &&
    partition.kind === "data" &&
    !partition.isSystem &&
    !partition.isBoot
  );
}
</script>

<template>
  <div class="topology-scroll" role="group" aria-label="磁盘分区拓扑">
    <div class="topology-bar">
      <button
        v-for="region in regions"
        :key="region.id"
        class="region"
        :class="[
          `kind-${region.kind}`,
          {
            selected: selectedIds.includes(region.id),
            expanded: region.expanded,
            selectable: isSelectable(region),
          },
        ]"
        :style="{ flexGrow: region.width }"
        :disabled="!isSelectable(region)"
        :aria-pressed="
          isSelectable(region) ? selectedIds.includes(region.id) : undefined
        "
        :title="`${region.label} · ${formatBytes(region.sizeBytes)}`"
        type="button"
        @click="emit('selectPartition', region.id)"
      >
        <span class="region-label">{{ region.label }}</span>
        <span class="region-size">{{ formatBytes(region.sizeBytes) }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.topology-scroll {
  width: 100%;
  overflow-x: auto;
  padding: 4px 2px 8px;
  scrollbar-width: thin;
}

.topology-bar {
  min-width: 720px;
  min-height: 82px;
  display: flex;
  gap: 4px;
}

.region {
  min-width: 34px;
  min-height: 76px;
  display: flex;
  flex-basis: 0;
  flex-direction: column;
  justify-content: flex-end;
  align-items: flex-start;
  gap: 3px;
  padding: 10px;
  overflow: hidden;
  border: 1px solid color-mix(in srgb, var(--region-color) 28%, transparent);
  border-radius: 12px;
  color: var(--color-text);
  background: linear-gradient(
    155deg,
    color-mix(in srgb, var(--region-color) 17%, var(--color-surface)),
    color-mix(in srgb, var(--region-color) 6%, var(--color-surface))
  );
  text-align: left;
  transition:
    transform 160ms ease,
    border-color 160ms ease,
    box-shadow 160ms ease;
}

.region:disabled {
  opacity: 0.76;
}

.region.selectable {
  cursor: pointer;
}

.region.selectable:hover {
  transform: translateY(-2px);
  border-color: color-mix(in srgb, var(--region-color) 70%, transparent);
}

.region.selected {
  border-color: var(--color-blue);
  box-shadow:
    0 0 0 3px var(--color-blue-soft),
    0 8px 20px rgba(22, 119, 255, 0.12);
}

.region.expanded {
  border-color: var(--color-green);
  box-shadow: 0 0 0 3px var(--color-green-soft);
}

.kind-data {
  --region-color: var(--color-blue);
}

.kind-system,
.kind-efiSystem {
  --region-color: var(--color-purple);
}

.kind-recovery,
.kind-microsoftReserved {
  --region-color: var(--color-orange);
}

.kind-unallocated,
.kind-unknown {
  --region-color: var(--color-text-secondary);
  background-image: repeating-linear-gradient(
    135deg,
    transparent,
    transparent 7px,
    color-mix(in srgb, var(--region-color) 8%, transparent) 7px,
    color-mix(in srgb, var(--region-color) 8%, transparent) 14px
  );
}

.region-label,
.region-size {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.region-label {
  font-size: 0.8rem;
  font-weight: 650;
}

.region-size {
  color: var(--color-text-secondary);
  font-size: 0.7rem;
}
</style>
