# GitHub Actions 构建与发布

## 原则

- 开发机不生成安装包。
- Pull Request 只执行格式化、静态检查、测试和前端构建。
- Tag 或手动触发时才在 windows-latest 生成 NSIS 安装包与便携版。
- CI 使用 pnpm Store 与 Rust Target 缓存。
- 同一分支的新运行会取消旧运行，减少排队和资源浪费。

## 发布步骤

1. 合并所有目标变更并确保 CI 通过。
2. 更新 CHANGELOG.md 和版本号。
3. 创建 vX.Y.Z Tag。
4. GitHub Actions 在托管 Windows Runner 构建。
5. 工作流创建 Draft Release 并上传安装包、便携版和 SHA-256。
6. 人工验证签名、校验和、安装与卸载后发布。

## 预期耗时

- 前端质量检查：缓存命中后约 1–3 分钟。
- Rust 检查：缓存命中后约 2–4 分钟。
- Windows Release：缓存命中后目标为 3–6 分钟。

GitHub Hosted Runner 的冷缓存构建可能超过目标时间，不能把冷缓存耗时伪装成稳定指标。工作流应保留各阶段耗时数据用于持续优化。

## 代码签名

GitHub 构建不能自动消除杀毒软件或 SmartScreen 对未签名程序的提示。正式公开发布前应配置可信 Authenticode 证书，并将证书以 GitHub Actions Secret 管理；不得把 PFX 或密码提交到仓库。

## 管理员代理

Release workflow 单独以 `partition-writes` 功能编译 `clarity-privileged-service.exe`，再通过仅发布使用的 `tauri.release.conf.json` 作为 external binary 打进 NSIS。portable ZIP 必须同时包含普通桌面程序和管理员代理。日常 CI 使用 `--all-features` 检查默认关闭的实验代码，但不运行任何真实系统或磁盘写操作。

本地仍不得运行 `tauri build`，也不得手工生成可分发管理员二进制。
