import type { DashboardSnapshot } from "@/types/dashboard";

const GIB = 1024 ** 3;
const MIB = 1024 ** 2;

/**
 * Browser-safe fixture used for UI development and tests before Windows disk
 * discovery is connected. Callers receive a clone through the service layer.
 */
export const dashboardFixture: Readonly<DashboardSnapshot> = {
  disk: {
    id: "C:",
    label: "Windows · 本地磁盘 (C:)",
    totalBytes: 200 * GIB + 61 * MIB,
    usedBytes: 159 * GIB + 645 * MIB,
    categories: [
      { kind: "applications", label: "应用", bytes: 56 * GIB + 205 * MIB },
      { kind: "system", label: "系统", bytes: 44 * GIB + 819 * MIB },
      { kind: "files", label: "文件", bytes: 33 * GIB + 410 * MIB },
      { kind: "development", label: "开发", bytes: 25 * GIB + 205 * MIB },
    ],
  },
  health: {
    status: "良好",
    deviceType: "NVMe",
    temperatureCelsius: 42,
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
