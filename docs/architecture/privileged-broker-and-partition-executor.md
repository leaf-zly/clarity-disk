# 管理员代理与实验性分区执行器

> 协议版本：1 · 分区计划版本：2 · 默认状态：维护可用、分区写入关闭

## 1. 交付边界

计划八新增独立二进制 `clarity-privileged-service`。它不是常驻 Windows 服务，而是一次 UAC 对应一次进程的一次性管理员代理：请求完成后立即退出，减少高权限常驻攻击面。普通 Tauri 进程继续以当前用户运行。

计划九在该代理内实现相邻普通 NTFS 数据分区的真实执行链：重新发现 → 校验身份和证据 → 迁移与 SHA-256 校验 → 持久化恢复检查点 → 删除右侧源分区 → 扩展左侧目标分区 → 后置校验。实现使用固定 PowerShell Storage cmdlet 与受信任的 `robocopy.exe`，协议中没有脚本、命令或路径字段。

分区执行器虽已实现，正式构建仍默认不可调用。必须同时满足：

- Release workflow 以 `partition-writes` 编译功能构建管理员代理；
- 管理员在 `%ProgramData%\ClarityDisk\enable-experimental-partition-writes.v1` 写入精确内容 `enable:v1`；
- 五分钟内的计划 v2 完整匹配磁盘唯一 ID、磁盘号、GUID、分区号、偏移、容量和剩余空间；
- 外接电源/桌面供电、无待重启、健康、BitLocker、VSS、NTFS、相邻关系和 5% 迁移余量全部通过；
- 独立备份提供程序只在管理员保护凭据证明另一物理磁盘上的实际恢复摘要时返回 `Verified`；凭据缺失的普通安装继续安全阻塞。

## 2. 协议与一次性握手

`clarity-privileged-protocol` 只包含枚举操作：休眠开关、Windows Update 下载缓存维护、系统还原点和摘要绑定的相邻数据分区合并。

请求 ID 为 128 位小写十六进制；请求最多两分钟有效。桌面端在内存中绑定随机确认令牌、精确中文确认词和操作；任意提交都会先消费挑战。请求文件只作为不受信任的同用户传输，UAC 启动参数只允许请求 ID 和 SHA-256 请求摘要。管理员代理原子认领文件，重复请求和被修改请求均拒绝。

## 3. 系统维护适配器

| 操作         | 固定实现                                                                                                | 失败处理                                         |
| ------------ | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------ |
| 休眠开关     | `%SystemRoot%\System32\powercfg.exe /hibernate on\|off`                                                 | 非零退出即安全停止                               |
| 更新下载缓存 | 固定停止 BITS/Windows Update，整树预检重解析点，仅清空 `SoftwareDistribution\Download` 子项，再恢复服务 | 无论清理是否成功均尝试恢复服务；部分完成不报成功 |
| 系统还原点   | 固定 `Checkpoint-Computer` 描述和类型                                                                   | 遵守 Windows 策略与频率限制                      |

## 4. 分区恢复边界

恢复日志位于 `%ProgramData%\ClarityDisk\partition-recovery\<plan_digest>.json`。每次状态变化使用临时文件、刷新和原子替换。

- `MigrationPrepared` 之前失败：可安全停止；
- `MutationStarted` 写入日志并刷新后，才允许调用删除/扩展适配器；
- `MutationStarted` 之后任何异常：`ManualRecoveryRequired`，禁止自动重试；
- 后置校验要求源分区消失、目标 GUID/偏移不变、容量至少增加源分区容量、迁移数据摘要一致。

## 5. 发布与验证

本地不生成管理员代理安装包。GitHub Release 先单独构建启用编译门禁的代理，再由 Tauri release config 作为 external binary 放入 NSIS；portable ZIP 同时包含两个可执行文件。常规 CI 对 workspace 运行 `--all-features` Clippy 和测试，确保默认关闭的高风险代码也持续编译。

自动化测试不会执行系统维护或磁盘写操作；真实故障注入只能在可销毁 Windows 虚拟机、测试磁盘、UPS 和可恢复备份条件下进行。
