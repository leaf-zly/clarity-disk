$ErrorActionPreference = 'Stop'
$utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$storageNamespace = 'root/Microsoft/Windows/Storage'
$warnings = [System.Collections.Generic.List[string]]::new()
$bitLockerUnavailable = [System.Collections.Generic.HashSet[string]]::new()
$reliabilityUnavailable = [System.Collections.Generic.HashSet[string]]::new()
$volumeUnavailable = [System.Collections.Generic.HashSet[string]]::new()
$windowsIdentity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$windowsPrincipal = [System.Security.Principal.WindowsPrincipal]::new($windowsIdentity)
$isAdministrator = $windowsPrincipal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)

function Get-SerialFingerprint([string]$serialNumber) {
  if ([string]::IsNullOrWhiteSpace($serialNumber)) { return 'unknown' }
  $sha256 = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($serialNumber.Trim())
    $hash = $sha256.ComputeHash($bytes)
    return ([System.BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant().Substring(0, 16)
  } finally {
    $sha256.Dispose()
  }
}

function Convert-HealthStatus($value) {
  switch ([int]$value) {
    0 { return 'Healthy' }
    1 { return 'Warning' }
    2 { return 'Unhealthy' }
    default { return 'Unknown' }
  }
}

function Convert-BusType($value) {
  switch ([int]$value) {
    1 { return 'SCSI' }
    2 { return 'ATAPI' }
    3 { return 'ATA' }
    4 { return 'IEEE 1394' }
    6 { return 'Fibre Channel' }
    7 { return 'USB' }
    8 { return 'RAID' }
    9 { return 'iSCSI' }
    10 { return 'SAS' }
    11 { return 'SATA' }
    12 { return 'SD' }
    13 { return 'MMC' }
    15 { return 'Virtual' }
    16 { return 'Spaces' }
    17 { return 'NVMe' }
    18 { return 'SCM' }
    19 { return 'UFS' }
    default { return 'Unknown' }
  }
}

function Convert-PartitionStyle($value) {
  switch ([int]$value) {
    1 { return 'MBR' }
    2 { return 'GPT' }
    default { return 'Unknown' }
  }
}

function Get-NormalizedDriveLetter($value) {
  $letter = ([string]$value).Trim().ToUpperInvariant()
  if ($letter -match '^[A-Z]$') { return $letter }
  return ''
}

$snapshotState = 'none'
if (-not $isAdministrator) {
  $snapshotState = 'unknown'
  $warnings.Add('当前权限无法核验卷影副本；基础拓扑可用，但分区写入保持阻塞。')
} else {
  try {
    $shadowCopies = @(Get-CimInstance -ClassName Win32_ShadowCopy -ErrorAction Stop)
    if ($shadowCopies.Count -gt 0) { $snapshotState = 'present' }
  } catch {
    $snapshotState = 'unknown'
    $warnings.Add('无法核验卷影副本；基础拓扑可用，但分区写入保持阻塞。')
  }
}

$dynamicKnown = $true
$dynamicDiskNumbers = [System.Collections.Generic.HashSet[int]]::new()
try {
  Get-CimInstance -ClassName Win32_DiskPartition -ErrorAction Stop |
    Where-Object { $_.Type -match 'Logical Disk Manager|LDM' } |
    ForEach-Object { [void]$dynamicDiskNumbers.Add([int]$_.DiskIndex) }
} catch {
  $dynamicKnown = $false
  $warnings.Add('无法确认动态磁盘状态；相关磁盘的写入操作保持阻塞。')
}

$diskRecords = @(Get-CimInstance -Namespace $storageNamespace -ClassName MSFT_Disk -ErrorAction Stop)
$partitionRecords = @(Get-CimInstance -Namespace $storageNamespace -ClassName MSFT_Partition -ErrorAction Stop)
$volumeRecords = @(Get-CimInstance -Namespace $storageNamespace -ClassName MSFT_Volume -ErrorAction Stop)
$physicalDisks = @()
try {
  $physicalDisks = @(Get-CimInstance -Namespace $storageNamespace -ClassName MSFT_PhysicalDisk -ErrorAction Stop)
} catch {
  $warnings.Add('物理介质扩展信息不可用；基础磁盘与分区信息仍可读取。')
}

$volumesByDrive = @{}
foreach ($volume in $volumeRecords) {
  $driveLetter = Get-NormalizedDriveLetter $volume.DriveLetter
  if (-not [string]::IsNullOrWhiteSpace($driveLetter)) { $volumesByDrive[$driveLetter] = $volume }
}
$logicalDisksByDrive = @{}
try {
  foreach ($logicalDisk in @(Get-CimInstance -ClassName Win32_LogicalDisk -ErrorAction Stop)) {
    $deviceId = ([string]$logicalDisk.DeviceID).TrimEnd(':').ToUpperInvariant()
    if (-not [string]::IsNullOrWhiteSpace($deviceId)) { $logicalDisksByDrive[$deviceId] = $logicalDisk }
  }
} catch {}
$physicalDisksByDeviceId = @{}
foreach ($physicalDisk in $physicalDisks) {
  $deviceId = [string]$physicalDisk.DeviceId
  if (-not [string]::IsNullOrWhiteSpace($deviceId)) { $physicalDisksByDeviceId[$deviceId] = $physicalDisk }
}
$bitLockerCommand = if ($isAdministrator) { Get-Command -Name Get-BitLockerVolume -ErrorAction SilentlyContinue } else { $null }

$disks = @(
  $diskRecords | Sort-Object Number | ForEach-Object {
    $disk = $_
    $diskIdentity = if ([string]::IsNullOrWhiteSpace([string]$disk.UniqueId)) {
      'disk-number:' + [string]$disk.Number + ':serial-sha256:' + (Get-SerialFingerprint ([string]$disk.SerialNumber))
    } else {
      'disk:' + ([string]$disk.UniqueId).Trim()
    }
    $busType = Convert-BusType $disk.BusType
    $layoutKind = if (-not $dynamicKnown) {
      'unknown'
    } elseif ($dynamicDiskNumbers.Contains([int]$disk.Number)) {
      'dynamic'
    } elseif ($busType -eq 'Spaces') {
      'storageSpaces'
    } else {
      'basic'
    }

    $mediaErrorState = 'unknown'
    $physicalDisk = $physicalDisksByDeviceId[[string]$disk.Number]
    if ($null -ne $physicalDisk) {
      try {
        $reliability = @(Get-CimAssociatedInstance -InputObject $physicalDisk -ResultClassName MSFT_StorageReliabilityCounter -ErrorAction Stop) | Select-Object -First 1
        if ($null -ne $reliability -and $null -ne $reliability.ReadErrorsUncorrected -and $null -ne $reliability.WriteErrorsUncorrected) {
          $uncorrected = [uint64]$reliability.ReadErrorsUncorrected + [uint64]$reliability.WriteErrorsUncorrected
          $mediaErrorState = if ($uncorrected -gt 0) { 'present' } else { 'none' }
        } else {
          [void]$reliabilityUnavailable.Add([string]$disk.Number)
        }
      } catch {
        [void]$reliabilityUnavailable.Add([string]$disk.Number)
      }
    } else {
      [void]$reliabilityUnavailable.Add([string]$disk.Number)
    }

    $partitions = @(
      $partitionRecords |
        Where-Object { [uint32]$_.DiskNumber -eq [uint32]$disk.Number } |
        Sort-Object Offset |
        ForEach-Object {
          $partition = $_
          $mountPoints = @($partition.AccessPaths | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) })
          $driveLetter = Get-NormalizedDriveLetter $partition.DriveLetter
          $driveMount = if ([string]::IsNullOrWhiteSpace($driveLetter)) { $null } else { $driveLetter + ':' }
          $volume = if ($null -eq $driveMount) { $null } else { $volumesByDrive[$driveLetter] }
          $logicalDisk = if ($null -eq $driveMount) { $null } else { $logicalDisksByDrive[$driveLetter] }
          if ($null -ne $driveMount -and $null -eq $volume -and $null -eq $logicalDisk) {
            [void]$volumeUnavailable.Add($driveMount)
          }

          $encryptionState = 'unknown'
          if ($null -ne $driveMount) {
            if ($null -ne $bitLockerCommand) {
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
                [void]$bitLockerUnavailable.Add($driveMount)
              }
            } else {
              [void]$bitLockerUnavailable.Add($driveMount)
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
          $volumeSize = if ($null -ne $volume) { $volume.Size } elseif ($null -ne $logicalDisk) { $logicalDisk.Size } else { $null }
          $freeBytes = if ($null -ne $volume) { $volume.SizeRemaining } elseif ($null -ne $logicalDisk) { $logicalDisk.FreeSpace } else { $null }
          $usedBytes = if ($null -ne $volumeSize -and $null -ne $freeBytes) { [uint64]$volumeSize - [uint64]$freeBytes } else { $null }

          [ordered]@{
            id = $partitionIdentity
            diskId = $diskIdentity
            partitionNumber = [uint32]$partition.PartitionNumber
            guid = $partitionGuid
            offsetBytes = [uint64]$partition.Offset
            sizeBytes = [uint64]$partition.Size
            gptType = if ($null -eq $partition.GptType) { $null } else { [string]$partition.GptType }
            mbrType = if ($null -eq $partition.MbrType) { $null } else { [string]$partition.MbrType }
            fileSystem = if ($null -ne $volume) { [string]$volume.FileSystem } elseif ($null -ne $logicalDisk) { [string]$logicalDisk.FileSystem } else { $null }
            label = if ($null -ne $volume) { [string]$volume.FileSystemLabel } elseif ($null -ne $logicalDisk) { [string]$logicalDisk.VolumeName } else { $null }
            mountPoints = $mountPoints
            isSystem = [bool]$partition.IsSystem
            isBoot = [bool]$partition.IsBoot
            isReadOnly = [bool]$partition.IsReadOnly -or [bool]$disk.IsReadOnly
            isOffline = [bool]$partition.IsOffline -or [bool]$disk.IsOffline
            encryptionState = $encryptionState
            snapshotState = $snapshotState
            health = if ($null -ne $volume) { Convert-HealthStatus $volume.HealthStatus } else { 'Unknown' }
            usedBytes = $usedBytes
            freeBytes = if ($null -eq $freeBytes) { $null } else { [uint64]$freeBytes }
          }
        }
    )

    [ordered]@{
      id = $diskIdentity
      number = [uint32]$disk.Number
      friendlyName = if ([string]::IsNullOrWhiteSpace([string]$disk.FriendlyName)) { '物理磁盘 ' + [string]$disk.Number } else { [string]$disk.FriendlyName }
      busType = $busType
      partitionStyle = Convert-PartitionStyle $disk.PartitionStyle
      sizeBytes = [uint64]$disk.Size
      layoutKind = $layoutKind
      health = Convert-HealthStatus $disk.HealthStatus
      mediaErrorState = $mediaErrorState
      isOffline = [bool]$disk.IsOffline
      partitions = $partitions
    }
  }
)

if ($volumeUnavailable.Count -gt 0) {
  $warnings.Add([string]$volumeUnavailable.Count + ' 个挂载卷的文件系统信息不可用；相关分区保持阻塞。')
}
if ($bitLockerUnavailable.Count -gt 0) {
  $warnings.Add('当前权限无法核验 ' + [string]$bitLockerUnavailable.Count + ' 个卷的 BitLocker 状态；基础拓扑可用，但分区写入保持阻塞。')
}
if ($reliabilityUnavailable.Count -gt 0) {
  $warnings.Add([string]$reliabilityUnavailable.Count + ' 块磁盘未提供介质可靠性计数；基础拓扑可用，磁盘健康结论标记为受限。')
}

[ordered]@{ disks = $disks; warnings = @($warnings) } | ConvertTo-Json -Depth 8 -Compress
