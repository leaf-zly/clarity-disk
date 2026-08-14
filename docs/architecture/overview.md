# 架构概览

## 分层

    Vue UI
      ↓ typed invoke contracts
    Tauri command adapter
      ↓ validated domain requests
    clarity-core domain rules
      ↓ explicit platform ports
    Windows platform implementation
      ↓ constrained privileged protocol
    Privileged Windows service

## 模块职责

### Vue UI

只负责展示、用户输入和任务状态，不直接访问磁盘设备或执行系统命令。

### Tauri 适配层

把前端请求转换成领域输入，负责序列化、命令注册和取消信号传播，不承载业务判断。

### clarity-core

保存跨平台领域模型、风险等级、容量规则、操作计划和状态机，不依赖 UI 或 Windows API。

### Windows 平台层

封装 Windows Storage API、卷管理、SMART、VSS、BitLocker 和文件系统能力。平台错误必须转换成稳定的领域错误。

当前平台层包括逻辑卷发现、受限清理适配器、计划六的物理分区发现和 F11 磁盘健康发现。分区适配器只从受信任的 `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe` 运行编译期固定的 Storage/CIM 查询，不插入调用方参数；它读取 `Get-Disk`、`Get-Partition`、`Get-Volume`、BitLocker、卷影副本、动态磁盘和介质可靠性状态，并把未知状态保守映射为阻塞。该模块不申请管理员权限，也没有 `diskpart`、格式化、缩放、删除、迁移或重启接口。

`disk_health_discovery.rs` 复用计划六的物理磁盘身份规则，优先按 `UniqueId` 关联 `Get-Disk` 与 `Get-PhysicalDisk`，再退化为磁盘号，无法关联则标记未知。适配器读取 Storage Reliability Counter、固件、介质、扇区和 BitLocker 汇总，完整序列号在脚本输出前被截断。`clarity-core::health` 统一验证范围、计算证据完整性并聚合状态；未知提供程序、自监测、身份或可靠性字段不会得到“良好”结论。Tauri 的 `get_disk_health_snapshot` 不接受参数，浏览器服务只使用静态 Fixture。

`clarity-core::partition` 保存平台无关的拓扑身份、保护分区分类、失败关闭检查和模拟布局。Tauri 的 `get_partition_topology` 不接受参数，`preview_partition_merge` 只接受源/目标分区 ID，并在每次预演前重新发现；Vue 不提交盘符、路径、GUID、偏移、命令文本或执行选项。浏览器模式使用独立 Fixture，因此前端开发不会访问真实磁盘。

`clarity-core::partition_safety` 保存计划七不可变计划、前置条件、摘要和版本化恢复状态机。计划 v2 额外绑定磁盘号、GUID、分区号、偏移、容量、剩余空间、供电、待重启和独立备份结论。普通评估依旧不授权执行；计划八/九的一次性工作流只有在所有证据与双重功能门禁通过时才签发挑战。

### 特权服务

`apps/privileged-service` 是独立一次性管理员代理，不作为常驻服务扩大攻击面。`clarity-privileged-protocol` v1 只允许休眠开关、Windows Update 下载缓存维护、系统还原点和摘要绑定的相邻数据分区合并。请求不能携带脚本、命令或路径；请求首次认领即消费，管理员进程重新发现所有系统和磁盘身份。

实验性分区执行器采用编译/管理员运行时双门禁。它先把源卷数据复制到目标卷固定目录并重新计算摘要，再刷新版本化恢复日志，最后通过固定 Storage cmdlet 删除源分区和扩展目标分区。写入边界后的失败只进入人工恢复。当前独立备份证据仍不可用，所以默认产品流程保持阻塞。

## 依赖方向

依赖只能从外层指向内层。领域核心不能依赖 Tauri、Vue 或 Windows UI。这样可独立测试清理和分区规则，并降低高权限代码审计范围。
