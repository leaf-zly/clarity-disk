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

当前第一阶段仅通过安全 Rust 封装读取系统盘挂载点、总容量和可用容量。该模块不申请管理员权限，不遍历用户文件，也不具备删除或修改分区的接口。详细分类扫描、健康检测与清理规则将作为独立能力逐步接入。

### 特权服务

未来以独立管理员进程运行，只接受有限的版本化协议。服务必须重新验证路径、卷标识、操作前置条件和用户确认令牌。

## 依赖方向

依赖只能从外层指向内层。领域核心不能依赖 Tauri、Vue 或 Windows UI。这样可独立测试清理和分区规则，并降低高权限代码审计范围。
