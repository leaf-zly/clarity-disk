import type { DashboardSnapshot, DiskSummary } from "@/types/dashboard";

const GIB = 1024 ** 3;
const MIB = 1024 ** 2;

/** Browser-safe fixture used for UI development and tests. */
const systemDisk: DiskSummary = {
  id: "C:",
  label: "Windows · 本地磁盘 (C:)",
  totalBytes: 200 * GIB + 61 * MIB,
  usedBytes: 159 * GIB + 645 * MIB,
  metadata: {
    mountPoint: "C:\\",
    fileSystem: "NTFS",
    deviceType: "SSD",
    isSystemVolume: true,
    isRemovable: false,
    isReadOnly: false,
    healthStatus: "healthy",
    healthNote: null,
  },
  categories: [
    { kind: "applications", label: "应用", bytes: 56 * GIB + 205 * MIB },
    { kind: "system", label: "系统", bytes: 44 * GIB + 819 * MIB },
    { kind: "files", label: "文件", bytes: 33 * GIB + 410 * MIB },
    { kind: "development", label: "开发", bytes: 25 * GIB + 205 * MIB },
  ],
};

const dataDisk: DiskSummary = {
  id: "D:",
  label: "资料 · 本地磁盘 (D:)",
  totalBytes: 512 * GIB,
  usedBytes: 278 * GIB,
  metadata: {
    mountPoint: "D:\\",
    fileSystem: "NTFS",
    deviceType: "SSD",
    isSystemVolume: false,
    isRemovable: false,
    isReadOnly: false,
    healthStatus: "healthy",
    healthNote: null,
  },
  categories: [],
};

export const dashboardFixture: Readonly<DashboardSnapshot> = {
  disk: systemDisk,
  disks: [systemDisk, dataDisk],
  health: {
    status: "良好",
    deviceType: "SSD",
    temperatureCelsius: null,
    hasWarning: false,
  },
  cleanup: {
    reclaimableBytes: GIB + 860 * MIB,
    categoryCount: 3,
  },
  suggestions: [
    {
      id: "hibernation",
      title: "休眠文件占用较大",
      description: "若不使用休眠，可释放 12.74 GB",
      risk: "confirmationRequired",
      reclaimableBytes: 12 * GIB + 758 * MIB,
    },
    {
      id: "browser-cache",
      title: "浏览器缓存",
      description: "Chrome 与 Edge 共 454 MB",
      risk: "safe",
      reclaimableBytes: 454 * MIB,
    },
    {
      id: "recycle-bin",
      title: "回收站",
      description: "3,700 个文件，共 389 MB",
      risk: "review",
      reclaimableBytes: 389 * MIB,
    },
  ],
};
