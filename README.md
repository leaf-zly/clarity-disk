# 澄盘 Clarity Disk

澄盘是一款以安全为第一原则的 Windows 磁盘空间分析、系统清理和分区管理工具。项目采用 Rust 领域核心、Tauri 桌面运行时与 Vue 3 界面。

> 当前状态：计划一至计划七及 F11 磁盘健康只读版的已评审范围已接入。应用支持只读空间扫描、版本化清理预览、四类缓存隔离与恢复、回收站官方 API、物理磁盘拓扑、分区合并可行性预演、磁盘健康检测，以及分区不可变安全计划与恢复协议；Windows Update 管理员维护、隔离区永久删除和任何分区写操作均未开放。

## 设计目标

- 快速：优先使用 NTFS 元数据扫描和增量索引。
- 安全：任何破坏性操作都必须经过预演、确认和审计。
- 清晰：普通模式只展示建议操作，专业模式提供完整细节。
- 本地优先：文件名、路径和磁盘信息默认不离开设备。
- 可验证：领域规则、平台适配和界面行为分别测试。

## 开发环境

- Windows 11
- Rust 1.97
- Node.js 24
- pnpm 10.20

安装依赖后可运行：

    pnpm install
    pnpm dev

本地质量检查：

    pnpm check

## 打包规则

不要在开发机生成安装包。正式 NSIS 安装包和便携版只通过 GitHub Actions 的 Release 工作流生成。详细说明见 [发布流程](docs/release/github-actions.md)。

## 目录

    apps/desktop/           Vue 3 + Tauri 桌面应用
    crates/clarity-core/    与平台无关的领域模型和安全规则
    docs/architecture/      架构说明
    docs/decisions/         架构决策记录
    docs/security/          权限与破坏性操作规范
    docs/release/           CI/CD 与发布规范

## 安全说明

当前版本只在固定允许根内执行可恢复隔离，并可通过独立确认调用 Windows 回收站官方 API。分区功能完全只读；计划七可以生成摘要绑定的安全计划并评估供电、待重启、备份和恢复协议，但始终不授权执行。详细边界见 [分区写操作威胁模型](docs/security/partition-write-threat-model.md)。所有未来高权限操作都必须遵循 [特权操作安全规范](docs/security/privileged-operations.md)。
