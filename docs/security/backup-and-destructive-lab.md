# 独立备份证据与可销毁分区实验环境

> 版本：1.0 · 日期：2026-08-17

## 1. 目的

分区数据迁移成功不等于数据可恢复。澄盘只在独立备份提供程序证明“另一物理磁盘上的恢复点已经完成实际恢复抽检”后，把备份状态标记为 `Verified`。普通勾选、本机卷影副本、同盘副本、文件存在检查或仅有备份任务成功状态都不满足条件。

## 2. 凭据契约

固定凭据路径为 `%ProgramData%\ClarityDisk\backup-verification.v1.json`。Windows 发现适配器接受凭据前必须确认：

- schema 为 1，源磁盘 ID 与当前不可变分区计划完全一致；
- 目标磁盘 ID 存在且与源物理磁盘不同；
- 恢复点和备份清单 SHA-256 非空且格式正确；
- 原始样本与异地恢复样本的目录树摘要完全一致；
- 备份完成时间不晚于恢复验证时间，证据不来自未来；
- 证据未过期，且提供程序不能签发超过 30 天的有效期；
- 文件所有者为 SYSTEM、Administrators 或 TrustedInstaller，Users、Authenticated Users 和 Everyone 没有写/修改/完全控制权限。

任一字段未知、ACL 不安全、JSON 损坏或时间异常都会失败关闭。`scripts/backup/New-BackupVerificationReceipt.ps1` 是独立管理员运维工具，不由 Tauri 调用。它从挂载卷读取实际 Windows `UniqueId`，比较两个样本树并把凭据 ACL 收紧到 SYSTEM/Administrators。

## 3. 可销毁 VHDX 故障实验室

`.github/workflows/partition-fault-lab.yml` 只能手动触发，并要求：

- 输入精确确认文本 `RUN DISPOSABLE VHDX LAB`；
- Runner 同时带有 `self-hosted`、`windows`、`clarity-disk-destructive-lab` 标签；
- Runner 预置 `CLARITY_DISK_DESTRUCTIVE_LAB_ROOT`，目录中存在 `ALLOW-CLARITY-DISK-DESTRUCTIVE-LAB` 标记；
- 主机安装 Hyper-V VHD cmdlet，并且没有生产数据；
- 工作流只创建新的 8 GB 动态 VHDX，验证磁盘为 `File Backed Virtual` 且不是 Boot/System 后再初始化 GPT；
- 每次运行生成两个 NTFS 数据卷和确定性测试文件，保存磁盘号、分区号和 SHA-256 证据；
- 测试结束无论成功失败都按唯一 Run ID 卸载并移除该 VHDX 目录。

脚本不接受物理磁盘号，也不枚举后选择任意删除目标。实验记录保留 14 天，用于安全评审。实际硬件断电、USB 断开和 UPS 演练必须在另行批准的专用设备完成，不能在 GitHub 托管 Runner 或开发机模拟。

## 4. 上线判定

代码存在不代表分区写入可以公开启用。每个计划仍需同时满足：

1. Release 使用有效 Authenticode 发布者；
2. 当前源磁盘存在有效恢复凭据；
3. 对应 Windows/存储类型已完成可销毁实验并留存记录；
4. `partition-writes` 编译门禁和 ProgramData 管理员运行时门禁同时启用；
5. 当次供电、待重启、健康、BitLocker、VSS、身份和容量证据新鲜且全部通过。

缺少任一部署证据时，UI 仍只提供预演和阻塞建议。
