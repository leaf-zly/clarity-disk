import { invoke, isTauri } from "@tauri-apps/api/core";

import {
  createPartitionMergeFixture,
  partitionTopologyFixture,
} from "@/testing/partition-fixture";
import type {
  MergePreview,
  MergePreviewRequest,
  PartitionTopology,
} from "@/types/partition";

/** Loads a fresh read-only physical-disk and partition topology. */
export async function loadPartitionTopology(): Promise<PartitionTopology> {
  if (isTauri()) return invoke<PartitionTopology>("get_partition_topology");
  return structuredClone(partitionTopologyFixture);
}

/**
 * Requests a fresh backend re-discovery and non-authorizing merge preview.
 * Only stable partition IDs cross the command boundary.
 */
export async function previewPartitionMerge(
  request: MergePreviewRequest,
): Promise<MergePreview> {
  if (isTauri())
    return invoke<MergePreview>("preview_partition_merge", { request });
  return structuredClone(createPartitionMergeFixture(request));
}

/** Downloads a privacy-preserving JSON copy of a read-only preview report. */
export function downloadPartitionPreviewReport(preview: MergePreview): void {
  const payload = JSON.stringify(preview, null, 2);
  const url = URL.createObjectURL(
    new Blob([payload], { type: "application/json;charset=utf-8" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = `clarity-partition-preview-${preview.previewId.slice(-12)}.json`;
  link.click();
  URL.revokeObjectURL(url);
}
