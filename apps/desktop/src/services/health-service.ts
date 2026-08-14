import { invoke, isTauri } from "@tauri-apps/api/core";

import { diskHealthFixture } from "@/testing/health-fixture";
import type { DiskHealthSnapshot, PhysicalDiskHealth } from "@/types/health";

/** Loads a fresh privacy-preserving physical-disk health snapshot. */
export async function loadDiskHealthSnapshot(): Promise<DiskHealthSnapshot> {
  if (isTauri()) return invoke<DiskHealthSnapshot>("get_disk_health_snapshot");
  return structuredClone(diskHealthFixture);
}

/** Downloads an explicitly redacted JSON health report for user diagnostics. */
export function downloadDiskHealthReport(snapshot: DiskHealthSnapshot): void {
  // Explicit field selection prevents future provider-only identity fields from
  // being included in exports by an accidental object spread.
  const disks = snapshot.disks.map(redactedDiskReport);
  const payload = JSON.stringify(
    {
      schemaVersion: 1,
      capturedAtUnixMs: snapshot.capturedAtUnixMs,
      readOnly: snapshot.readOnly,
      summary: snapshot.summary,
      discoveryWarnings: snapshot.discoveryWarnings,
      disks,
    },
    null,
    2,
  );
  const url = URL.createObjectURL(
    new Blob([payload], { type: "application/json;charset=utf-8" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = `clarity-disk-health-${snapshot.capturedAtUnixMs}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

function redactedDiskReport(disk: PhysicalDiskHealth): PhysicalDiskHealth {
  return {
    id: disk.id,
    number: disk.number,
    friendlyName: disk.friendlyName,
    manufacturer: disk.manufacturer,
    model: disk.model,
    firmwareVersion: disk.firmwareVersion,
    serialSuffix: disk.serialSuffix,
    busType: disk.busType,
    mediaType: disk.mediaType,
    sizeBytes: disk.sizeBytes,
    operationalStatus: [...disk.operationalStatus],
    isOffline: disk.isOffline,
    providerHealth: disk.providerHealth,
    smartStatus: disk.smartStatus,
    identityMapping: disk.identityMapping,
    temperatureCelsius: disk.temperatureCelsius,
    temperatureMaxCelsius: disk.temperatureMaxCelsius,
    wearPercentUsed: disk.wearPercentUsed,
    estimatedLifeRemainingPercent: disk.estimatedLifeRemainingPercent,
    powerOnHours: disk.powerOnHours,
    readErrorsTotal: disk.readErrorsTotal,
    readErrorsUncorrected: disk.readErrorsUncorrected,
    writeErrorsTotal: disk.writeErrorsTotal,
    writeErrorsUncorrected: disk.writeErrorsUncorrected,
    logicalSectorBytes: disk.logicalSectorBytes,
    physicalSectorBytes: disk.physicalSectorBytes,
    encryption: { ...disk.encryption },
    dataCompleteness: disk.dataCompleteness,
    status: disk.status,
    signals: disk.signals.map((signal) => ({ ...signal })),
    partitionWritesBlocked: true,
  };
}
