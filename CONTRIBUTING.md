# 贡献指南

## 开发流程

1. 从 main 创建 feature/* 或 fix/* 分支。
2. 在本地完成格式化、静态检查、测试和 Rust 检查。
3. 使用 Conventional Commits 提交。
4. 创建 Pull Request，并等待全部 GitHub Actions 检查通过。
5. 至少完成一次代码审查后合并。

## 提交类型

- feat：新功能
- fix：缺陷修复
- refactor：不改变行为的重构
- test：测试
- docs：文档
- build：构建系统
- ci：CI/CD
- chore：维护任务

## 质量要求

- TypeScript 必须通过严格类型检查。
- Vue 组件使用 Composition API 和 script setup TypeScript。
- 导出的 TypeScript 类型、函数、Props、Emits 和 composable 必须包含 JSDoc。
- Rust 公共 API 必须包含 Rustdoc。
- 新增行为必须提供覆盖其公共行为的测试。
- 不得提交二进制安装包、缓存、密钥或本地环境配置。

## 安全变更

涉及删除文件、管理员权限、分区表、启动配置或 BitLocker 的 Pull Request 必须：

- 包含威胁分析。
- 明确列出允许和拒绝的输入。
- 提供失败与恢复路径。
- 包含至少一名安全审查者的批准。
