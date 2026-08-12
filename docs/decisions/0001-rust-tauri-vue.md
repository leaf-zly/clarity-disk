# ADR-0001：采用 Rust、Tauri 和 Vue 3

- 状态：Accepted
- 日期：2026-08-12

## 背景

产品需要原生 Windows 能力、较小安装体积、可靠的类型边界和精致的桌面界面，同时要求 GitHub Actions 在几分钟内完成增量打包。

## 决策

- Rust 负责领域规则、扫描、清理计划和 Windows 平台适配。
- Tauri 负责桌面窗口与受控命令边界。
- Vue 3、Composition API 和 TypeScript 负责界面。
- pnpm 管理前端依赖。
- GitHub Actions 是唯一发布打包环境。

## 结果

优点：

- 核心规则可独立测试。
- UI 和高权限能力隔离。
- 相比完整 Chromium 打包，分发体积更小。
- Rust 与 TypeScript 都提供强类型边界。

代价：

- Windows WebView2 是运行时依赖。
- 冷启动的 Rust CI 构建仍需要缓存优化。
- Tauri 命令协议需要维护 Rust 与 TypeScript 两侧类型一致性。
