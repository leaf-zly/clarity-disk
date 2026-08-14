//! Experimental adjacent data-partition migration and resize executor.

use clarity_privileged_protocol::{
    PartitionMergeOperation, PrivilegedExecutionReport, PrivilegedExecutionStatus,
};

/// Returns true only when compile-time support and the administrator-owned runtime flag agree.
pub(crate) fn runtime_gate_enabled() -> bool {
    #[cfg(all(windows, feature = "partition-writes"))]
    {
        runtime_flag_path().is_some_and(|path| {
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                return false;
            };
            metadata.is_file()
                && !is_reparse(&metadata)
                && std::fs::read_to_string(path).is_ok_and(|value| value.trim() == "enable:v1")
        })
    }
    #[cfg(not(all(windows, feature = "partition-writes")))]
    {
        false
    }
}

/// Executes an experimental merge or returns a terminal failure-closed report.
pub(crate) fn execute(
    operation: &PartitionMergeOperation,
    request_id: &str,
    now_unix_ms: u64,
) -> PrivilegedExecutionReport {
    #[cfg(all(windows, feature = "partition-writes"))]
    {
        if !runtime_gate_enabled() {
            return rejected(request_id, "实验性分区写入的管理员运行时开关未启用。");
        }
        execute_enabled(operation, request_id, now_unix_ms)
            .unwrap_or_else(|failure| failure.into_report(request_id))
    }
    #[cfg(not(all(windows, feature = "partition-writes")))]
    {
        let _ = (operation, now_unix_ms);
        rejected(request_id, "当前构建未编译实验性分区写入能力。")
    }
}

fn rejected(request_id: &str, message: &str) -> PrivilegedExecutionReport {
    PrivilegedExecutionReport {
        request_id: request_id.to_owned(),
        status: PrivilegedExecutionStatus::Rejected,
        message: message.to_owned(),
        completed_at_unix_ms: crate::unix_ms(),
        recovery_state: None,
    }
}

#[cfg(all(windows, feature = "partition-writes"))]
fn execute_enabled(
    operation: &PartitionMergeOperation,
    request_id: &str,
    now_unix_ms: u64,
) -> Result<PrivilegedExecutionReport, PartitionFailure> {
    use clarity_core::{PartitionRecoveryEvent, PartitionRecoveryJournal};

    let plan = &operation.plan;
    let mut journal = PartitionRecoveryJournal::try_new(plan, now_unix_ms)
        .map_err(|error| PartitionFailure::before_write(error.to_string()))?;
    persist_journal(&journal).map_err(PartitionFailure::before_write)?;

    let snapshot = rediscover(plan).map_err(PartitionFailure::before_write)?;
    validate_snapshot(plan, &snapshot).map_err(PartitionFailure::before_write)?;
    journal
        .apply(PartitionRecoveryEvent::PreflightPassed, crate::unix_ms())
        .map_err(|error| PartitionFailure::before_write(error.to_string()))?;
    persist_journal(&journal).map_err(PartitionFailure::before_write)?;

    let destination = migration_destination(plan, &snapshot)?;
    if destination.exists() {
        return Err(PartitionFailure::before_write(
            "目标卷已存在同名迁移目录，操作已停止。".to_owned(),
        ));
    }
    preflight_migration_tree(&snapshot.source_root).map_err(PartitionFailure::before_write)?;
    run_robocopy(&snapshot.source_root, &destination).map_err(PartitionFailure::before_write)?;
    let source_digest =
        digest_tree(&snapshot.source_root).map_err(PartitionFailure::before_write)?;
    let destination_digest = digest_tree(&destination).map_err(PartitionFailure::before_write)?;
    if source_digest != destination_digest {
        let _ = std::fs::remove_dir_all(&destination);
        return Err(PartitionFailure::before_write(
            "迁移数据校验失败，未开始分区元数据修改。".to_owned(),
        ));
    }
    journal
        .apply(PartitionRecoveryEvent::MigrationPrepared, crate::unix_ms())
        .map_err(|error| PartitionFailure::before_write(error.to_string()))?;
    persist_journal(&journal).map_err(PartitionFailure::before_write)?;

    // This durable checkpoint is the irreversible boundary. Any later error
    // is reported as manual recovery required and is never retried blindly.
    journal
        .apply(PartitionRecoveryEvent::MutationStarted, crate::unix_ms())
        .map_err(|error| PartitionFailure::before_write(error.to_string()))?;
    persist_journal(&journal).map_err(PartitionFailure::before_write)?;
    if let Err(error) = mutate_partition_layout(plan) {
        let _ = journal.apply(PartitionRecoveryEvent::Interrupted, crate::unix_ms());
        let _ = persist_journal(&journal);
        return Err(PartitionFailure::after_write(error));
    }
    journal
        .apply(PartitionRecoveryEvent::MutationCommitted, crate::unix_ms())
        .map_err(|error| PartitionFailure::after_write(error.to_string()))?;
    persist_journal(&journal).map_err(PartitionFailure::after_write)?;

    let verified = verify_postconditions(plan, &destination, &destination_digest).unwrap_or(false);
    let event = if verified {
        PartitionRecoveryEvent::VerificationPassed
    } else {
        PartitionRecoveryEvent::VerificationFailed
    };
    journal
        .apply(event, crate::unix_ms())
        .map_err(|error| PartitionFailure::after_write(error.to_string()))?;
    persist_journal(&journal).map_err(PartitionFailure::after_write)?;
    if !verified {
        return Err(PartitionFailure::after_write(
            "分区布局已修改，但后置校验未全部通过。请停止写入并按恢复日志人工检查。".to_owned(),
        ));
    }
    Ok(PrivilegedExecutionReport {
        request_id: request_id.to_owned(),
        status: PrivilegedExecutionStatus::Completed,
        message: "源卷数据已校验迁移，源分区已移除，目标 NTFS 分区已扩展。".to_owned(),
        completed_at_unix_ms: crate::unix_ms(),
        recovery_state: Some(journal.state),
    })
}

#[cfg(all(windows, feature = "partition-writes"))]
#[derive(Debug)]
struct PartitionFailure {
    message: String,
    write_started: bool,
}

#[cfg(all(windows, feature = "partition-writes"))]
impl PartitionFailure {
    fn before_write(message: String) -> Self {
        Self {
            message,
            write_started: false,
        }
    }

    fn after_write(message: String) -> Self {
        Self {
            message,
            write_started: true,
        }
    }

    fn into_report(self, request_id: &str) -> PrivilegedExecutionReport {
        let (status, recovery_state) = if self.write_started {
            (
                PrivilegedExecutionStatus::ManualRecoveryRequired,
                Some(clarity_core::PartitionRecoveryState::ManualRecoveryRequired),
            )
        } else {
            (
                PrivilegedExecutionStatus::SafeStopped,
                Some(clarity_core::PartitionRecoveryState::SafeStopped),
            )
        };
        PrivilegedExecutionReport {
            request_id: request_id.to_owned(),
            status,
            message: self.message,
            completed_at_unix_ms: crate::unix_ms(),
            recovery_state,
        }
    }
}

#[cfg(all(windows, feature = "partition-writes"))]
// This fixed provider DTO keeps each independently revalidated Windows signal
// explicit so no aggregate boolean can hide which safety property changed.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RediscoveredSnapshot {
    disk_id: String,
    disk_number: u32,
    disk_health: String,
    disk_offline: bool,
    source: RediscoveredPartition,
    target: RediscoveredPartition,
    source_root: std::path::PathBuf,
    target_root: std::path::PathBuf,
}

#[cfg(all(windows, feature = "partition-writes"))]
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RediscoveredPartition {
    guid: String,
    partition_number: u32,
    offset_bytes: u64,
    size_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
    file_system: String,
    health: String,
    is_system: bool,
    is_boot: bool,
    is_read_only: bool,
    encryption_off: bool,
    has_shadow_copy: bool,
    has_page_file: bool,
}

#[cfg(all(windows, feature = "partition-writes"))]
fn rediscover(plan: &clarity_core::ImmutablePartitionPlan) -> Result<RediscoveredSnapshot, String> {
    const SCRIPT: &str = r"
$ErrorActionPreference='Stop'
$utf8=[System.Text.UTF8Encoding]::new($false); [Console]::OutputEncoding=$utf8; $OutputEncoding=$utf8
$diskNumber=[Convert]::ToUInt32($env:CLARITY_DISK_NUMBER)
$sourceNumber=[Convert]::ToUInt32($env:CLARITY_SOURCE_PARTITION_NUMBER)
$targetNumber=[Convert]::ToUInt32($env:CLARITY_TARGET_PARTITION_NUMBER)
$disk=Get-Disk -Number $diskNumber -ErrorAction Stop
function Fingerprint([string]$serial) { if ([string]::IsNullOrWhiteSpace($serial)) { return 'unknown' }; $sha=[Security.Cryptography.SHA256]::Create(); try { $hash=$sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($serial.Trim())); return ([BitConverter]::ToString($hash)).Replace('-','').ToLowerInvariant().Substring(0,16) } finally { $sha.Dispose() } }
$diskId=if ([string]::IsNullOrWhiteSpace([string]$disk.UniqueId)) { 'disk-number:'+[string]$disk.Number+':serial-sha256:'+(Fingerprint ([string]$disk.SerialNumber)) } else { 'disk:'+([string]$disk.UniqueId).Trim() }
$shadows=@(Get-CimInstance Win32_ShadowCopy -ErrorAction Stop | ForEach-Object VolumeName)
$pages=@(Get-CimInstance Win32_PageFileUsage -ErrorAction Stop | ForEach-Object { ([string]$_.Name).Substring(0,1).ToUpperInvariant() })
function ReadPartition([uint32]$number) {
  $p=Get-Partition -DiskNumber $diskNumber -PartitionNumber $number -ErrorAction Stop
  if ($null -eq $p.DriveLetter) { throw 'partition has no drive letter' }
  $letter=([string]$p.DriveLetter).ToUpperInvariant(); $v=Get-Volume -DriveLetter $letter -ErrorAction Stop
  $bl=Get-BitLockerVolume -MountPoint ($letter+':') -ErrorAction Stop
  $access=[string]$p.AccessPaths[0]
  $hasShadow=$false; foreach ($shadow in $shadows) { if ($access -eq [string]$shadow) { $hasShadow=$true } }
  [ordered]@{ guid=([string]$p.Guid).Trim(); partitionNumber=[uint32]$p.PartitionNumber; offsetBytes=[uint64]$p.Offset; sizeBytes=[uint64]$p.Size; usedBytes=[uint64]$v.Size-[uint64]$v.SizeRemaining; freeBytes=[uint64]$v.SizeRemaining; fileSystem=[string]$v.FileSystem; driveLetter=$letter; health=[string]$v.HealthStatus; isSystem=[bool]$p.IsSystem; isBoot=[bool]$p.IsBoot; isReadOnly=[bool]$p.IsReadOnly -or [bool]$disk.IsReadOnly; encryptionOff=([int]$bl.EncryptionPercentage -eq 0); hasShadowCopy=$hasShadow; hasPageFile=($pages -contains $letter) }
}
$source=ReadPartition $sourceNumber; $target=ReadPartition $targetNumber
[ordered]@{ diskId=$diskId; diskNumber=[uint32]$disk.Number; diskHealth=[string]$disk.HealthStatus; diskOffline=[bool]$disk.IsOffline; source=$source; target=$target; sourceRoot=($source.driveLetter+':\'); targetRoot=($target.driveLetter+':\') } | ConvertTo-Json -Depth 6 -Compress
";
    let powershell = trusted_system_executable(&["WindowsPowerShell", "v1.0", "powershell.exe"])?;
    let output = std::process::Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .env("CLARITY_DISK_NUMBER", plan.disk_number.to_string())
        .env(
            "CLARITY_SOURCE_PARTITION_NUMBER",
            plan.source_identity.partition_number.to_string(),
        )
        .env(
            "CLARITY_TARGET_PARTITION_NUMBER",
            plan.target_identity.partition_number.to_string(),
        )
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("无法启动分区重新发现：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "分区重新发现失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("分区重新发现响应无效：{error}"))
}

#[cfg(all(windows, feature = "partition-writes"))]
fn validate_snapshot(
    plan: &clarity_core::ImmutablePartitionPlan,
    snapshot: &RediscoveredSnapshot,
) -> Result<(), String> {
    let matches_partition = |expected: &clarity_core::PartitionExecutionIdentity,
                             actual: &RediscoveredPartition| {
        expected
            .guid
            .as_deref()
            .is_some_and(|guid| guid.eq_ignore_ascii_case(&actual.guid))
            && expected.partition_number == actual.partition_number
            && expected.start_offset_bytes == actual.offset_bytes
            && expected.size_bytes == actual.size_bytes
            && expected.free_bytes == Some(actual.free_bytes)
            && actual.file_system.eq_ignore_ascii_case("NTFS")
            && actual.health.eq_ignore_ascii_case("Healthy")
            && !actual.is_system
            && !actual.is_boot
            && !actual.is_read_only
            && actual.encryption_off
            && !actual.has_shadow_copy
    };
    let exact_identity = snapshot.disk_id == plan.disk_id
        && snapshot.disk_number == plan.disk_number
        && snapshot.disk_health.eq_ignore_ascii_case("Healthy")
        && !snapshot.disk_offline
        && matches_partition(&plan.source_identity, &snapshot.source)
        && matches_partition(&plan.target_identity, &snapshot.target)
        && snapshot.source.used_bytes == plan.migration_bytes
        && !snapshot.source.has_page_file
        && snapshot.source.offset_bytes
            == snapshot
                .target
                .offset_bytes
                .saturating_add(snapshot.target.size_bytes)
        && snapshot.target.free_bytes
            >= plan
                .migration_bytes
                .saturating_add(plan.migration_bytes / 20);
    if !exact_identity {
        return Err("磁盘身份、容量、安全状态或迁移空间已变化，旧计划已失效。".to_owned());
    }
    Ok(())
}

#[cfg(all(windows, feature = "partition-writes"))]
fn migration_destination(
    plan: &clarity_core::ImmutablePartitionPlan,
    snapshot: &RediscoveredSnapshot,
) -> Result<std::path::PathBuf, PartitionFailure> {
    if !is_drive_root(&snapshot.source_root) || !is_drive_root(&snapshot.target_root) {
        return Err(PartitionFailure::before_write(
            "重新发现的卷访问路径不符合固定盘符根格式。".to_owned(),
        ));
    }
    Ok(snapshot
        .target_root
        .join(format!("Clarity-Merged-{}", &plan.plan_digest[..12])))
}

#[cfg(all(windows, feature = "partition-writes"))]
fn is_drive_root(path: &std::path::Path) -> bool {
    let value = path.to_string_lossy();
    let bytes = value.as_bytes();
    bytes.len() == 3
        && bytes[0].is_ascii_uppercase()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

#[cfg(all(windows, feature = "partition-writes"))]
fn preflight_migration_tree(root: &std::path::Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || is_reparse(&metadata) {
        return Err("源卷根包含不安全的重解析边界。".to_owned());
    }
    walk_migration_tree(root, root, &mut |_, _| Ok(()))
}

#[cfg(all(windows, feature = "partition-writes"))]
fn run_robocopy(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    let executable = trusted_system_executable(&["robocopy.exe"])?;
    let output = std::process::Command::new(executable)
        .arg(source)
        .arg(destination)
        .args([
            "/E",
            "/COPYALL",
            "/DCOPY:DAT",
            "/XJ",
            "/B",
            "/R:0",
            "/W:0",
            "/XD",
            "System Volume Information",
            "$RECYCLE.BIN",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("无法启动受信任的迁移工具：{error}"))?;
    let code = output.status.code().unwrap_or(i32::MAX);
    if code > 7 {
        return Err(format!(
            "数据迁移未完整完成（robocopy {code}）：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(all(windows, feature = "partition-writes"))]
fn digest_tree(root: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    use std::io::Read;

    let mut hasher = Sha256::new();
    walk_migration_tree(root, root, &mut |path, metadata| {
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        hasher.update(relative.to_string_lossy().to_lowercase().as_bytes());
        if metadata.is_file() {
            hasher.update(metadata.len().to_le_bytes());
            let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
            let mut buffer = vec![0_u8; 1024 * 1024];
            loop {
                let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
        }
        Ok(())
    })?;
    let mut output = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(output)
}

#[cfg(all(windows, feature = "partition-writes"))]
fn walk_migration_tree(
    root: &std::path::Path,
    path: &std::path::Path,
    visitor: &mut impl FnMut(&std::path::Path, &std::fs::Metadata) -> Result<(), String>,
) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if is_reparse(&metadata) {
        return Err(format!("迁移树包含重解析点：{}", path.display()));
    }
    visitor(path, &metadata)?;
    if !metadata.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if path == root {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case("System Volume Information")
                || name.eq_ignore_ascii_case("$RECYCLE.BIN")
            {
                continue;
            }
        }
        walk_migration_tree(root, &entry.path(), visitor)?;
    }
    Ok(())
}

#[cfg(all(windows, feature = "partition-writes"))]
fn mutate_partition_layout(plan: &clarity_core::ImmutablePartitionPlan) -> Result<(), String> {
    const SCRIPT: &str = r"
$ErrorActionPreference='Stop'
$d=[Convert]::ToUInt32($env:CLARITY_DISK_NUMBER); $s=[Convert]::ToUInt32($env:CLARITY_SOURCE_PARTITION_NUMBER); $t=[Convert]::ToUInt32($env:CLARITY_TARGET_PARTITION_NUMBER)
$expectedSourceGuid=$env:CLARITY_SOURCE_GUID; $expectedTargetGuid=$env:CLARITY_TARGET_GUID
$expectedSourceOffset=[Convert]::ToUInt64($env:CLARITY_SOURCE_OFFSET); $expectedSourceSize=[Convert]::ToUInt64($env:CLARITY_SOURCE_SIZE); $expectedTargetOffset=[Convert]::ToUInt64($env:CLARITY_TARGET_OFFSET); $expectedTargetSize=[Convert]::ToUInt64($env:CLARITY_TARGET_SIZE)
$source=Get-Partition -DiskNumber $d -PartitionNumber $s -ErrorAction Stop; $target=Get-Partition -DiskNumber $d -PartitionNumber $t -ErrorAction Stop
if (([string]$source.Guid -ine $expectedSourceGuid) -or ([uint64]$source.Offset -ne $expectedSourceOffset) -or ([uint64]$source.Size -ne $expectedSourceSize) -or ([string]$target.Guid -ine $expectedTargetGuid) -or ([uint64]$target.Offset -ne $expectedTargetOffset) -or ([uint64]$target.Size -ne $expectedTargetSize) -or ([uint64]$source.Offset -ne ([uint64]$target.Offset+[uint64]$target.Size))) { throw 'partition identity changed at mutation boundary' }
Remove-Partition -InputObject $source -Confirm:$false -ErrorAction Stop
$target=Get-Partition -DiskNumber $d -PartitionNumber $t -ErrorAction Stop; $supported=Get-PartitionSupportedSize -InputObject $target -ErrorAction Stop
Resize-Partition -InputObject $target -Size $supported.SizeMax -ErrorAction Stop
";
    let source_guid = plan
        .source_identity
        .guid
        .as_deref()
        .ok_or_else(|| "源分区 GUID 缺失。".to_owned())?;
    let target_guid = plan
        .target_identity
        .guid
        .as_deref()
        .ok_or_else(|| "目标分区 GUID 缺失。".to_owned())?;
    let output = std::process::Command::new(trusted_system_executable(&[
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
    ])?)
    .args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        SCRIPT,
    ])
    .env("CLARITY_DISK_NUMBER", plan.disk_number.to_string())
    .env(
        "CLARITY_SOURCE_PARTITION_NUMBER",
        plan.source_identity.partition_number.to_string(),
    )
    .env(
        "CLARITY_TARGET_PARTITION_NUMBER",
        plan.target_identity.partition_number.to_string(),
    )
    .env("CLARITY_SOURCE_GUID", source_guid)
    .env("CLARITY_TARGET_GUID", target_guid)
    .env(
        "CLARITY_SOURCE_OFFSET",
        plan.source_identity.start_offset_bytes.to_string(),
    )
    .env(
        "CLARITY_SOURCE_SIZE",
        plan.source_identity.size_bytes.to_string(),
    )
    .env(
        "CLARITY_TARGET_OFFSET",
        plan.target_identity.start_offset_bytes.to_string(),
    )
    .env(
        "CLARITY_TARGET_SIZE",
        plan.target_identity.size_bytes.to_string(),
    )
    .stdin(std::process::Stdio::null())
    .output()
    .map_err(|error| format!("无法启动固定分区调整适配器：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "固定分区调整适配器失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(all(windows, feature = "partition-writes"))]
fn verify_postconditions(
    plan: &clarity_core::ImmutablePartitionPlan,
    destination: &std::path::Path,
    expected_digest: &str,
) -> Result<bool, String> {
    const SCRIPT: &str = "$ErrorActionPreference='Stop'; $d=[Convert]::ToUInt32($env:CLARITY_DISK_NUMBER); $s=[Convert]::ToUInt32($env:CLARITY_SOURCE_PARTITION_NUMBER); $t=[Convert]::ToUInt32($env:CLARITY_TARGET_PARTITION_NUMBER); $source=@(Get-Partition -DiskNumber $d -PartitionNumber $s -ErrorAction SilentlyContinue); $target=Get-Partition -DiskNumber $d -PartitionNumber $t -ErrorAction Stop; [ordered]@{ sourceAbsent=($source.Count -eq 0); targetOffset=[uint64]$target.Offset; targetSize=[uint64]$target.Size; targetGuid=[string]$target.Guid } | ConvertTo-Json -Compress";
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LayoutResult {
        source_absent: bool,
        target_offset: u64,
        target_size: u64,
        target_guid: String,
    }
    let output = std::process::Command::new(trusted_system_executable(&[
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe",
    ])?)
    .args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        SCRIPT,
    ])
    .env("CLARITY_DISK_NUMBER", plan.disk_number.to_string())
    .env(
        "CLARITY_SOURCE_PARTITION_NUMBER",
        plan.source_identity.partition_number.to_string(),
    )
    .env(
        "CLARITY_TARGET_PARTITION_NUMBER",
        plan.target_identity.partition_number.to_string(),
    )
    .stdin(std::process::Stdio::null())
    .output()
    .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Ok(false);
    }
    let result: LayoutResult =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    let expected_target_guid = plan.target_identity.guid.as_deref().unwrap_or_default();
    Ok(result.source_absent
        && result.target_offset == plan.target_identity.start_offset_bytes
        && result.target_size
            >= plan
                .target_identity
                .size_bytes
                .saturating_add(plan.source_identity.size_bytes)
        && result
            .target_guid
            .eq_ignore_ascii_case(expected_target_guid)
        && digest_tree(destination)? == expected_digest)
}

#[cfg(all(windows, feature = "partition-writes"))]
fn persist_journal(journal: &clarity_core::PartitionRecoveryJournal) -> Result<(), String> {
    use std::io::Write;

    let root = program_data_root()
        .ok_or_else(|| "ProgramData 不可用，无法持久化恢复日志。".to_owned())?
        .join("ClarityDisk")
        .join("partition-recovery");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let final_path = root.join(format!("{}.json", journal.plan_digest));
    let temporary = root.join(format!("{}.tmp", journal.plan_digest));
    let bytes = serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &final_path).map_err(|error| error.to_string())
}

#[cfg(all(windows, feature = "partition-writes"))]
fn trusted_system_executable(components: &[&str]) -> Result<std::path::PathBuf, String> {
    let root = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "受信任的 Windows 系统目录不可用。".to_owned())?;
    let path = components
        .iter()
        .fold(root.join("System32"), |path, item| path.join(item));
    if path.is_file() {
        Ok(path)
    } else {
        Err("受信任的 Windows 系统组件不可用。".to_owned())
    }
}

#[cfg(all(windows, feature = "partition-writes"))]
fn runtime_flag_path() -> Option<std::path::PathBuf> {
    program_data_root().map(|root| {
        root.join("ClarityDisk")
            .join("enable-experimental-partition-writes.v1")
    })
}

#[cfg(all(windows, feature = "partition-writes"))]
fn program_data_root() -> Option<std::path::PathBuf> {
    std::env::var_os("ProgramData")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
}

#[cfg(all(windows, feature = "partition-writes"))]
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0400 != 0
}
