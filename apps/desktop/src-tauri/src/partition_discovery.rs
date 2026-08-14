//! Windows read-only partition topology discovery.
//!
//! The adapter executes one compile-time PowerShell/CIM query because the
//! Windows Storage Management API is PowerShell's supported compatibility
//! surface across the Windows versions targeted by this desktop application.
//! No caller input, path, command text, or partition operation is interpolated.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use clarity_core::{
    DiskLayoutKind, EncryptionState, MediaErrorState, PartitionDescriptor, PartitionKind,
    PartitionOperationalState, PartitionTopology, PhysicalDisk, SnapshotState, TopologyHealth,
};
use serde::Deserialize;

const EFI_GPT_TYPE: &str = "{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}";
const MSR_GPT_TYPE: &str = "{e3c9e316-0b5c-4db8-817d-f92df00215ae}";
const RECOVERY_GPT_TYPE: &str = "{de94bba4-06d1-4d40-a16a-bfd50179d6ac}";

// This script is an immutable application resource. Keeping it input-free is
// the security boundary that prevents this adapter becoming an arbitrary shell.
const DISCOVERY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$warnings = [System.Collections.Generic.List[string]]::new()

$snapshotState = 'none'
try {
  $shadowCopies = @(Get-CimInstance -ClassName Win32_ShadowCopy -ErrorAction Stop)
  if ($shadowCopies.Count -gt 0) { $snapshotState = 'present' }
} catch {
  $snapshotState = 'unknown'
  $warnings.Add('无法完整读取卷影副本状态，相关分区将保持阻塞。')
}

$dynamicKnown = $true
$dynamicDiskNumbers = [System.Collections.Generic.HashSet[int]]::new()
try {
  Get-CimInstance -ClassName Win32_DiskPartition -ErrorAction Stop |
    Where-Object { $_.Type -match 'Logical Disk Manager|LDM' } |
    ForEach-Object { [void]$dynamicDiskNumbers.Add([int]$_.DiskIndex) }
} catch {
  $dynamicKnown = $false
  $warnings.Add('无法确认动态磁盘状态，磁盘布局将标记为未知。')
}

$bitLockerCommand = Get-Command -Name Get-BitLockerVolume -ErrorAction SilentlyContinue
$physicalDisks = @()
$physicalDisksKnown = $true
try { $physicalDisks = @(Get-PhysicalDisk -ErrorAction Stop) } catch {
  $physicalDisksKnown = $false
  $warnings.Add('无法读取物理介质可靠性计数，相关磁盘将保持阻塞。')
}
$disks = @(
  Get-Disk -ErrorAction Stop | Sort-Object Number | ForEach-Object {
    $disk = $_
    $diskIdentity = if ([string]::IsNullOrWhiteSpace([string]$disk.UniqueId)) {
      'disk-number:' + [string]$disk.Number + ':' + [string]$disk.SerialNumber
    } else {
      'disk:' + ([string]$disk.UniqueId).Trim()
    }
    $layoutKind = if (-not $dynamicKnown) {
      'unknown'
    } elseif ($dynamicDiskNumbers.Contains([int]$disk.Number)) {
      'dynamic'
    } elseif ([string]$disk.BusType -eq 'Spaces') {
      'storageSpaces'
    } else {
      'basic'
    }
    $mediaErrorState = 'unknown'
    if ($physicalDisksKnown) {
      $physicalDisk = $physicalDisks | Where-Object { [string]$_.DeviceId -eq [string]$disk.Number } | Select-Object -First 1
      if ($null -ne $physicalDisk) {
        try {
          $reliability = $physicalDisk | Get-StorageReliabilityCounter -ErrorAction Stop
          $uncorrected = [uint64]$reliability.ReadErrorsUncorrected + [uint64]$reliability.WriteErrorsUncorrected
          $mediaErrorState = if ($uncorrected -gt 0) { 'present' } else { 'none' }
        } catch {
          $warnings.Add('磁盘 ' + [string]$disk.Number + ' 的介质可靠性计数不可用。')
        }
      } else {
        $warnings.Add('磁盘 ' + [string]$disk.Number + ' 无法映射到物理介质可靠性计数。')
      }
    }

    $partitions = @(
      Get-Partition -DiskNumber $disk.Number -ErrorAction Stop |
        Sort-Object Offset | ForEach-Object {
          $partition = $_
          $mountPoints = @($partition.AccessPaths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
          $driveMount = if ($null -ne $partition.DriveLetter) { [string]$partition.DriveLetter + ':' } else { $null }
          $volume = $null
          if ($null -ne $partition.DriveLetter) {
            try { $volume = Get-Volume -DriveLetter $partition.DriveLetter -ErrorAction Stop } catch {
              $warnings.Add('卷 ' + $driveMount + ' 的文件系统信息不可用。')
            }
          }

          $encryptionState = 'unknown'
          if ($null -ne $driveMount -and $null -ne $bitLockerCommand) {
            try {
              $bitLocker = Get-BitLockerVolume -MountPoint $driveMount -ErrorAction Stop
              if ([int]$bitLocker.EncryptionPercentage -eq 0) {
                $encryptionState = 'off'
              } elseif ([string]$bitLocker.ProtectionStatus -eq 'On') {
                $encryptionState = 'on'
              } else {
                $encryptionState = 'suspended'
              }
            } catch {
              $warnings.Add('卷 ' + $driveMount + ' 的 BitLocker 状态不可用。')
            }
          }

          $partitionGuid = if ($null -eq $partition.Guid -or [string]::IsNullOrWhiteSpace([string]$partition.Guid)) {
            $null
          } else {
            ([string]$partition.Guid).Trim()
          }
          $partitionIdentity = if ($null -ne $partitionGuid) {
            'partition:' + $partitionGuid
          } else {
            $diskIdentity + ':offset:' + [string]$partition.Offset + ':size:' + [string]$partition.Size
          }
          $usedBytes = if ($null -ne $volume -and $null -ne $volume.Size -and $null -ne $volume.SizeRemaining) {
            [uint64]$volume.Size - [uint64]$volume.SizeRemaining
          } else { $null }
          $freeBytes = if ($null -ne $volume -and $null -ne $volume.SizeRemaining) { [uint64]$volume.SizeRemaining } else { $null }

          [ordered]@{
            id = $partitionIdentity
            diskId = $diskIdentity
            partitionNumber = [uint32]$partition.PartitionNumber
            guid = $partitionGuid
            offsetBytes = [uint64]$partition.Offset
            sizeBytes = [uint64]$partition.Size
            gptType = if ($null -eq $partition.GptType) { $null } else { [string]$partition.GptType }
            mbrType = if ($null -eq $partition.MbrType) { $null } else { [string]$partition.MbrType }
            fileSystem = if ($null -eq $volume) { $null } else { [string]$volume.FileSystem }
            label = if ($null -eq $volume) { $null } else { [string]$volume.FileSystemLabel }
            mountPoints = $mountPoints
            isSystem = [bool]$partition.IsSystem
            isBoot = [bool]$partition.IsBoot
            isReadOnly = [bool]$partition.IsReadOnly -or [bool]$disk.IsReadOnly
            isOffline = [bool]$disk.IsOffline
            encryptionState = $encryptionState
            snapshotState = $snapshotState
            health = if ($null -eq $volume) { 'unknown' } else { [string]$volume.HealthStatus }
            usedBytes = $usedBytes
            freeBytes = $freeBytes
          }
        }
    )

    [ordered]@{
      id = $diskIdentity
      number = [uint32]$disk.Number
      friendlyName = if ([string]::IsNullOrWhiteSpace([string]$disk.FriendlyName)) { '物理磁盘 ' + [string]$disk.Number } else { [string]$disk.FriendlyName }
      busType = [string]$disk.BusType
      partitionStyle = [string]$disk.PartitionStyle
      sizeBytes = [uint64]$disk.Size
      layoutKind = $layoutKind
      health = [string]$disk.HealthStatus
      mediaErrorState = $mediaErrorState
      isOffline = [bool]$disk.IsOffline
      partitions = $partitions
    }
  }
)

[ordered]@{ disks = $disks; warnings = @($warnings) } | ConvertTo-Json -Depth 8 -Compress
"#;

/// Discovers a read-only physical-disk and partition topology on Windows.
///
/// The child process receives only the compile-time script above. This
/// function exposes no command parameters and performs no partition writes.
///
/// # Errors
///
/// Returns an error when PowerShell cannot be located, the storage provider
/// fails, JSON is malformed, no disks are returned, or platform values violate
/// capacity invariants.
pub fn discover_partition_topology() -> Result<PartitionTopology, PartitionDiscoveryError> {
    #[cfg(not(windows))]
    {
        Err(PartitionDiscoveryError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    {
        discover_windows_partition_topology()
    }
}

#[cfg(windows)]
fn discover_windows_partition_topology() -> Result<PartitionTopology, PartitionDiscoveryError> {
    let output = Command::new(powershell_path()?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            DISCOVERY_SCRIPT,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(PartitionDiscoveryError::Launch)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(PartitionDiscoveryError::ProviderFailed(stderr));
    }

    let envelope: PowerShellTopologyEnvelope = serde_json::from_slice(&output.stdout)
        .map_err(PartitionDiscoveryError::InvalidProviderResponse)?;
    convert_topology(envelope)
}

#[cfg(windows)]
fn powershell_path() -> Result<PathBuf, PartitionDiscoveryError> {
    let system_root = std::env::var_os("SystemRoot")
        .filter(|value| !value.is_empty())
        .ok_or(PartitionDiscoveryError::SystemRootUnavailable)?;
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !executable.is_file() {
        return Err(PartitionDiscoveryError::PowerShellUnavailable(executable));
    }
    Ok(executable)
}

fn convert_topology(
    envelope: PowerShellTopologyEnvelope,
) -> Result<PartitionTopology, PartitionDiscoveryError> {
    if envelope.disks.is_empty() {
        return Err(PartitionDiscoveryError::NoPhysicalDisks);
    }

    let mut disks = envelope
        .disks
        .into_iter()
        .map(convert_disk)
        .collect::<Result<Vec<_>, _>>()?;
    disks.sort_by_key(|disk| disk.number);

    Ok(PartitionTopology {
        captured_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
        disks,
        discovery_warnings: envelope.warnings,
        read_only: true,
    })
}

fn convert_disk(raw: PowerShellDisk) -> Result<PhysicalDisk, PartitionDiscoveryError> {
    let mut partitions = raw
        .partitions
        .into_iter()
        .map(convert_partition)
        .collect::<Result<Vec<_>, _>>()?;
    partitions.sort_by_key(|partition| partition.start_offset_bytes);
    insert_unallocated_regions(&raw.id, raw.size_bytes, &mut partitions)?;

    Ok(PhysicalDisk {
        id: raw.id,
        number: raw.number,
        friendly_name: raw.friendly_name,
        bus_type: raw.bus_type,
        partition_style: raw.partition_style,
        size_bytes: raw.size_bytes,
        layout_kind: parse_layout_kind(&raw.layout_kind),
        health: parse_health(&raw.health),
        media_error_state: parse_media_error_state(&raw.media_error_state),
        is_offline: raw.is_offline,
        partitions,
    })
}

fn convert_partition(
    raw: PowerShellPartition,
) -> Result<PartitionDescriptor, PartitionDiscoveryError> {
    raw.offset_bytes
        .checked_add(raw.size_bytes)
        .ok_or(PartitionDiscoveryError::CapacityOverflow)?;
    let kind = classify_partition(&raw);
    Ok(PartitionDescriptor {
        id: raw.id,
        disk_id: raw.disk_id,
        partition_number: raw.partition_number,
        guid: raw.guid,
        start_offset_bytes: raw.offset_bytes,
        size_bytes: raw.size_bytes,
        file_system: non_empty(raw.file_system),
        label: non_empty(raw.label),
        mount_points: raw.mount_points,
        kind,
        is_system: raw.is_system,
        is_boot: raw.is_boot,
        is_read_only: raw.is_read_only,
        operational_state: if raw.is_offline {
            PartitionOperationalState::Offline
        } else {
            PartitionOperationalState::Online
        },
        encryption_state: parse_encryption_state(&raw.encryption_state),
        snapshot_state: parse_snapshot_state(&raw.snapshot_state),
        health: parse_health(&raw.health),
        used_bytes: raw.used_bytes,
        free_bytes: raw.free_bytes,
    })
}

fn insert_unallocated_regions(
    disk_id: &str,
    disk_size_bytes: u64,
    partitions: &mut Vec<PartitionDescriptor>,
) -> Result<(), PartitionDiscoveryError> {
    let mut cursor = 0_u64;
    let mut with_gaps = Vec::with_capacity(partitions.len().saturating_mul(2).saturating_add(1));
    for partition in partitions.drain(..) {
        if partition.start_offset_bytes > cursor {
            with_gaps.push(unallocated_region(
                disk_id,
                cursor,
                partition.start_offset_bytes - cursor,
            ));
        }
        cursor = partition
            .end_offset_bytes()
            .ok_or(PartitionDiscoveryError::CapacityOverflow)?;
        if cursor > disk_size_bytes {
            return Err(PartitionDiscoveryError::PartitionExceedsDisk {
                disk_size_bytes,
                partition_end_bytes: cursor,
            });
        }
        with_gaps.push(partition);
    }
    if cursor < disk_size_bytes {
        with_gaps.push(unallocated_region(
            disk_id,
            cursor,
            disk_size_bytes - cursor,
        ));
    }
    *partitions = with_gaps;
    Ok(())
}

fn unallocated_region(disk_id: &str, start: u64, size: u64) -> PartitionDescriptor {
    PartitionDescriptor {
        id: format!("unallocated:{disk_id}:{start}"),
        disk_id: disk_id.to_owned(),
        partition_number: 0,
        guid: None,
        start_offset_bytes: start,
        size_bytes: size,
        file_system: None,
        label: None,
        mount_points: vec![],
        kind: PartitionKind::Unallocated,
        is_system: false,
        is_boot: false,
        is_read_only: false,
        operational_state: PartitionOperationalState::NotApplicable,
        encryption_state: EncryptionState::NotApplicable,
        snapshot_state: SnapshotState::NotApplicable,
        health: TopologyHealth::NotApplicable,
        used_bytes: None,
        free_bytes: Some(size),
    }
}

fn classify_partition(raw: &PowerShellPartition) -> PartitionKind {
    let gpt_type = raw.gpt_type.as_deref().map(str::to_ascii_lowercase);
    match gpt_type.as_deref() {
        Some(EFI_GPT_TYPE) => PartitionKind::EfiSystem,
        Some(MSR_GPT_TYPE) => PartitionKind::MicrosoftReserved,
        Some(RECOVERY_GPT_TYPE) => PartitionKind::Recovery,
        _ if raw.is_system || raw.is_boot => PartitionKind::System,
        _ => match raw.mbr_type.as_deref().map(str::trim) {
            Some("39" | "0x27") => PartitionKind::Recovery,
            Some("239" | "0xEF" | "0xef") => PartitionKind::EfiSystem,
            _ if raw
                .file_system
                .as_deref()
                .is_some_and(|value| !value.is_empty()) =>
            {
                PartitionKind::Data
            }
            _ => PartitionKind::Unknown,
        },
    }
}

fn parse_layout_kind(value: &str) -> DiskLayoutKind {
    match value {
        "basic" => DiskLayoutKind::Basic,
        "dynamic" => DiskLayoutKind::Dynamic,
        "storageSpaces" => DiskLayoutKind::StorageSpaces,
        _ => DiskLayoutKind::Unknown,
    }
}

fn parse_health(value: &str) -> TopologyHealth {
    match value.to_ascii_lowercase().as_str() {
        "healthy" => TopologyHealth::Healthy,
        "warning" => TopologyHealth::Warning,
        "unhealthy" => TopologyHealth::Unhealthy,
        _ => TopologyHealth::Unknown,
    }
}

fn parse_encryption_state(value: &str) -> EncryptionState {
    match value {
        "off" => EncryptionState::Off,
        "on" => EncryptionState::On,
        "suspended" => EncryptionState::Suspended,
        _ => EncryptionState::Unknown,
    }
}

fn parse_media_error_state(value: &str) -> MediaErrorState {
    match value {
        "none" => MediaErrorState::None,
        "present" => MediaErrorState::Present,
        _ => MediaErrorState::Unknown,
    }
}

fn parse_snapshot_state(value: &str) -> SnapshotState {
    match value {
        "none" => SnapshotState::None,
        "present" => SnapshotState::Present,
        _ => SnapshotState::Unknown,
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellTopologyEnvelope {
    disks: Vec<PowerShellDisk>,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellDisk {
    id: String,
    number: u32,
    friendly_name: String,
    bus_type: String,
    partition_style: String,
    size_bytes: u64,
    layout_kind: String,
    health: String,
    media_error_state: String,
    is_offline: bool,
    #[serde(default)]
    partitions: Vec<PowerShellPartition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellPartition {
    id: String,
    disk_id: String,
    partition_number: u32,
    guid: Option<String>,
    offset_bytes: u64,
    size_bytes: u64,
    gpt_type: Option<String>,
    mbr_type: Option<String>,
    file_system: Option<String>,
    label: Option<String>,
    #[serde(default)]
    mount_points: Vec<String>,
    is_system: bool,
    is_boot: bool,
    is_read_only: bool,
    is_offline: bool,
    encryption_state: String,
    snapshot_state: String,
    health: String,
    used_bytes: Option<u64>,
    free_bytes: Option<u64>,
}

/// Errors produced by the read-only Windows storage provider adapter.
#[derive(Debug, thiserror::Error)]
pub enum PartitionDiscoveryError {
    /// Partition topology is available only on Windows.
    #[error("partition topology discovery is supported only on Windows")]
    UnsupportedPlatform,
    /// Windows system root was unavailable, so no trusted executable path exists.
    #[error("Windows SystemRoot is unavailable")]
    SystemRootUnavailable,
    /// The trusted in-box PowerShell executable was not present.
    #[error("Windows PowerShell is unavailable at {0}")]
    PowerShellUnavailable(PathBuf),
    /// The fixed storage query could not be launched.
    #[error("failed to launch the read-only storage provider: {0}")]
    Launch(#[source] std::io::Error),
    /// The storage provider rejected or failed the fixed query.
    #[error("read-only storage provider failed: {0}")]
    ProviderFailed(String),
    /// Provider output did not match the versioned adapter contract.
    #[error("invalid storage provider response: {0}")]
    InvalidProviderResponse(#[source] serde_json::Error),
    /// Provider returned no physical disks.
    #[error("no physical disks were returned by the storage provider")]
    NoPhysicalDisks,
    /// Partition offset plus capacity overflowed.
    #[error("partition capacity overflowed")]
    CapacityOverflow,
    /// A discovered partition exceeded its containing disk.
    #[error("partition end {partition_end_bytes} exceeds disk capacity {disk_size_bytes}")]
    PartitionExceedsDisk {
        /// Physical disk capacity.
        disk_size_bytes: u64,
        /// Exclusive end offset reported for the partition.
        partition_end_bytes: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_partition(offset: u64, size: u64) -> PowerShellPartition {
        PowerShellPartition {
            id: format!("partition-{offset}"),
            disk_id: "disk-0".to_owned(),
            partition_number: 1,
            guid: Some(format!("guid-{offset}")),
            offset_bytes: offset,
            size_bytes: size,
            gpt_type: Some("{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}".to_owned()),
            mbr_type: None,
            file_system: Some("NTFS".to_owned()),
            label: Some("Data".to_owned()),
            mount_points: vec!["D:\\".to_owned()],
            is_system: false,
            is_boot: false,
            is_read_only: false,
            is_offline: false,
            encryption_state: "off".to_owned(),
            snapshot_state: "none".to_owned(),
            health: "Healthy".to_owned(),
            used_bytes: Some(10),
            free_bytes: Some(size.saturating_sub(10)),
        }
    }

    #[test]
    fn inserts_leading_internal_and_trailing_unallocated_regions() {
        let mut partitions = vec![
            convert_partition(raw_partition(100, 100)).expect("partition should convert"),
            convert_partition(raw_partition(300, 100)).expect("partition should convert"),
        ];
        insert_unallocated_regions("disk-0", 500, &mut partitions)
            .expect("gaps should be representable");

        assert_eq!(partitions.len(), 5);
        assert_eq!(partitions[0].kind, PartitionKind::Unallocated);
        assert_eq!(partitions[2].start_offset_bytes, 200);
        assert_eq!(partitions[4].size_bytes, 100);
    }

    #[test]
    fn classifies_protected_gpt_partition_types() {
        let mut raw = raw_partition(100, 100);
        raw.gpt_type = Some(EFI_GPT_TYPE.to_owned());
        assert_eq!(classify_partition(&raw), PartitionKind::EfiSystem);
        raw.gpt_type = Some(MSR_GPT_TYPE.to_owned());
        assert_eq!(classify_partition(&raw), PartitionKind::MicrosoftReserved);
        raw.gpt_type = Some(RECOVERY_GPT_TYPE.to_owned());
        assert_eq!(classify_partition(&raw), PartitionKind::Recovery);
    }

    #[test]
    fn rejects_partitions_beyond_disk_capacity() {
        let mut partitions =
            vec![convert_partition(raw_partition(900, 200)).expect("partition should convert")];
        let error = insert_unallocated_regions("disk-0", 1_000, &mut partitions)
            .expect_err("invalid provider values must fail closed");
        assert!(matches!(
            error,
            PartitionDiscoveryError::PartitionExceedsDisk { .. }
        ));
    }
}
