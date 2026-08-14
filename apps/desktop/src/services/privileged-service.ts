import { invoke, isTauri } from "@tauri-apps/api/core";

import { createPartitionSafetyFixture } from "@/testing/partition-safety-fixture";
import type { MergePreviewRequest } from "@/types/partition";
import type {
  ExecutePrivilegedRequest,
  MaintenanceOperation,
  PartitionExecutionPreparation,
  PrivilegedAuditEvent,
  PrivilegedCapabilities,
  PrivilegedExecutionChallenge,
  PrivilegedExecutionReport,
} from "@/types/privileged";

/** Returns the non-elevated capability handshake for the adjacent broker. */
export async function getPrivilegedCapabilities(): Promise<PrivilegedCapabilities> {
  if (isTauri())
    return invoke<PrivilegedCapabilities>("get_privileged_capabilities");
  return {
    schemaVersion: 1,
    serviceVersion: "browser-preview",
    serviceAvailable: false,
    maintenanceOperations: [
      "hibernation",
      "windowsUpdateDownloadCache",
      "systemRestorePoint",
    ],
    partitionWriterCompiled: false,
    partitionWriterRuntimeEnabled: false,
  };
}

/** Prepares one fixed maintenance operation without requesting UAC yet. */
export async function prepareMaintenanceExecution(
  operation: MaintenanceOperation,
): Promise<PrivilegedExecutionChallenge> {
  if (!isTauri()) throw new Error("浏览器预览不会请求管理员权限或修改系统。");
  return invoke<PrivilegedExecutionChallenge>("prepare_maintenance_execution", {
    request: { operation },
  });
}

/** Re-discovers all partition evidence and returns a challenge only when fully ready. */
export async function preparePartitionExecution(
  request: MergePreviewRequest,
): Promise<PartitionExecutionPreparation> {
  if (!isTauri())
    return {
      assessment: structuredClone(createPartitionSafetyFixture(request)),
      challenge: null,
    };
  return invoke<PartitionExecutionPreparation>("prepare_partition_execution", {
    request,
  });
}

/** Consumes one challenge and requests the Windows UAC broker. */
export async function executePrivilegedOperation(
  request: ExecutePrivilegedRequest,
): Promise<PrivilegedExecutionReport> {
  if (!isTauri()) throw new Error("浏览器预览不会执行管理员操作。");
  return invoke<PrivilegedExecutionReport>("execute_privileged_operation", {
    request,
  });
}

/** Returns newest local privileged audit events without file paths. */
export async function getPrivilegedAuditEvents(): Promise<
  PrivilegedAuditEvent[]
> {
  if (!isTauri()) return [];
  return invoke<PrivilegedAuditEvent[]>("get_privileged_audit_events");
}
