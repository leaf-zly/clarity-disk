import { ref } from "vue";

import type { LanguagePreference } from "@/types/operations";

/** Supported stable UI copy identifiers. */
export type LocaleKey =
  | "brand"
  | "manage"
  | "overview"
  | "space"
  | "cleanup"
  | "largeFiles"
  | "disk"
  | "partitions"
  | "partitionSafety"
  | "health"
  | "maintenance"
  | "recovery"
  | "history"
  | "settings"
  | "localFirst"
  | "settingsTitle"
  | "saveChanges"
  | "appearance"
  | "appearanceDescription"
  | "theme"
  | "language"
  | "simplifiedChinese"
  | "english"
  | "launchAtLogin"
  | "notifications"
  | "privacyDiagnostics"
  | "automaticMaintenance"
  | "disabled"
  | "weekly"
  | "monthly"
  | "close"
  | "overviewTitle"
  | "overviewDescription";

/** Reactive language preference shared by all mounted Vue pages. */
export const currentLanguage = ref<LanguagePreference>("simplifiedChinese");

const copy: Record<LanguagePreference, Record<LocaleKey, string>> = {
  simplifiedChinese: {
    brand: "Clarity Disk",
    manage: "管理",
    overview: "概览",
    space: "空间分析",
    cleanup: "智能清理",
    largeFiles: "大文件",
    disk: "磁盘",
    partitions: "分区管理",
    partitionSafety: "安全基础",
    health: "磁盘健康",
    maintenance: "管理员维护",
    recovery: "恢复中心",
    history: "活动历史",
    settings: "设置",
    localFirst: "本地优先",
    settingsTitle: "设置与隐私",
    saveChanges: "保存更改",
    appearance: "外观与行为",
    appearanceDescription: "界面偏好和 Windows 启动行为。",
    theme: "主题",
    language: "语言",
    simplifiedChinese: "简体中文",
    english: "English",
    launchAtLogin: "登录 Windows 后启动 Clarity Disk",
    notifications: "允许维护完成通知",
    privacyDiagnostics: "隐私与诊断",
    automaticMaintenance: "自动维护",
    disabled: "关闭",
    weekly: "每周",
    monthly: "每月",
    close: "已关闭",
    overviewTitle: "下午好",
    overviewDescription: "查看磁盘容量、清理建议和维护状态。",
  },
  english: {
    brand: "Clarity Disk",
    manage: "Manage",
    overview: "Overview",
    space: "Space analysis",
    cleanup: "Smart cleanup",
    largeFiles: "Large files",
    disk: "Disk",
    partitions: "Partitions",
    partitionSafety: "Safety",
    health: "Disk health",
    maintenance: "Admin maintenance",
    recovery: "Recovery",
    history: "Activity history",
    settings: "Settings",
    localFirst: "Local-first",
    settingsTitle: "Settings & Privacy",
    saveChanges: "Save changes",
    appearance: "Appearance & behavior",
    appearanceDescription:
      "Interface preferences and Windows startup behavior.",
    theme: "Theme",
    language: "Language",
    simplifiedChinese: "Simplified Chinese",
    english: "English",
    launchAtLogin: "Launch Clarity Disk when Windows starts",
    notifications: "Allow maintenance notifications",
    privacyDiagnostics: "Privacy & diagnostics",
    automaticMaintenance: "Automatic maintenance",
    disabled: "Off",
    weekly: "Weekly",
    monthly: "Monthly",
    close: "Off",
    overviewTitle: "Good afternoon",
    overviewDescription:
      "Review disk capacity, cleanup suggestions, and maintenance status.",
  },
};

/** Applies a validated language preference and updates the document locale. */
export function applyLanguagePreference(language: LanguagePreference): void {
  currentLanguage.value = language;
  document.documentElement.lang = language === "english" ? "en" : "zh-CN";
}

/** Resolves a stable UI key to localized copy. */
export function translate(key: LocaleKey): string {
  return copy[currentLanguage.value][key];
}
