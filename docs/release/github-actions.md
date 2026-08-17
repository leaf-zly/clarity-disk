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
5. 工作流要求并导入临时 Authenticode PFX，签名管理员代理、桌面程序和 NSIS。
6. 验证签名发布者，执行静默安装/卸载冒烟测试。
7. 创建 Draft Release，上传安装包、便携版、SHA-256 和 GitHub 来源证明。
8. 人工核对变更说明、SmartScreen/杀毒软件结果和回滚版本后发布。

## 无签名测试包

当代码签名证书尚未配置但需要短期功能验收时，可手动运行
`Windows unsigned preview package`。该工作流仅在 GitHub 托管 Windows Runner
构建 NSIS，将安装包、SHA-256 和明确的未签名说明保存为保留 7 天的 Artifact；
它不会创建 Tag、GitHub Release 或可长期分发的资产。

测试工作流与正式发布严格分离，不会放宽正式 Release 的证书要求，也不会移除
备份验证、一次性确认或分区写入双门禁。由于 Artifact 没有 Authenticode 发布者，
Windows SmartScreen 或杀毒软件仍可能显示未签名警告。

## 预期耗时

- 前端质量检查：缓存命中后约 1–3 分钟。
- Rust 检查：缓存命中后约 2–4 分钟。
- Windows Release：缓存命中后目标为 3–6 分钟。

GitHub Hosted Runner 的冷缓存构建可能超过目标时间，不能把冷缓存耗时伪装成稳定指标。工作流应保留各阶段耗时数据用于持续优化。

## 代码签名

tag 或手动 Release 缺少 `WINDOWS_CERTIFICATE_PFX_BASE64`、`WINDOWS_CERTIFICATE_PASSWORD` 时直接失败。PFX 只写入 Runner 临时目录，导入 CurrentUser 证书库后立即移除文件；仓库、日志和发布资产不得包含 PFX 或密码。工作流使用 SHA-256 文件摘要与 RFC 3161 时间戳，并逐个比较签名证书 thumbprint。证书可以降低未签名警告，但不能承诺消除 SmartScreen 或杀毒软件信誉提示。

安装冒烟测试只在 GitHub Windows Runner 执行：验证 NSIS 签名、静默安装、安装目录边界、已安装主程序签名和静默卸载。失败保持 Draft 且 job 红色。

## 管理员代理

Release workflow 单独以 `partition-writes` 功能编译 `clarity-privileged-service.exe`，再通过仅发布使用的 `tauri.release.conf.json` 作为 external binary 打进 NSIS。portable ZIP 必须同时包含普通桌面程序和管理员代理。日常 CI 使用 `--all-features` 检查默认关闭的实验代码，但不运行任何真实系统或磁盘写操作。

本地仍不得运行 `tauri build`，也不得手工生成可分发管理员二进制。`partition-fault-lab.yml` 是独立手动自托管实验工作流，不生成发布资产。
