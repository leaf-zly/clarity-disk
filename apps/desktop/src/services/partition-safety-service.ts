import { invoke, isTauri } from "@tauri-apps/api/core";

import { createPartitionSafetyFixture } from "@/testing/partition-safety-fixture";
import type { PartitionSafetyAssessment } from "@/types/partition-safety";
import type { MergePreviewRequest } from "@/types/partition";

/**
 * Re-discovers topology and system evidence for a non-authorizing safety plan.
 * The request contains backend partition identities only.
 */
export async function assessPartitionMergeSafety(
  request: MergePreviewRequest,
): Promise<PartitionSafetyAssessment> {
  if (isTauri())
    return invoke<PartitionSafetyAssessment>("assess_partition_merge_safety", {
      request,
    });
  return structuredClone(createPartitionSafetyFixture(request));
}
