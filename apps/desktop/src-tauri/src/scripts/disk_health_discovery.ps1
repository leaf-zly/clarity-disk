$ErrorActionPreference = 'Stop'
$utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$storageNamespace = 'root/Microsoft/Windows/Storage'
$warnings = [System.Collections.Generic.List[string]]::new()
$reliabilityUnavailable = [System.Collections.Generic.HashSet[string]]::new()
$bitLockerUnavailable = [System.Collections.Generic.HashSet[string]]::new()
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

function Convert-MediaType($value) {
  switch ([int]$value) {
    3 { return 'HDD' }
    4 { return 'SSD' }
    5 { return 'SCM' }
    default { return 'Unknown' }
  }
}

function Convert-OperationalStatus($value) {
  switch ([int]$value) {
    2 { return 'OK' }
    3 { return 'Degraded' }
    6 { return 'Error' }
    10 { return 'Stopped' }
    17 { return 'Completed' }
    default { return [string]$value }
  }
}

function Get-NormalizedDriveLetter($value) {
  $letter = ([string]$value).Trim().ToUpperInvariant()
  if ($letter -match '^[A-Z]$') { return $letter }
  return ''
}

$diskRecords = @(Get-CimInstance -Namespace $storageNamespace -ClassName MSFT_Disk -ErrorAction Stop)
$partitionRecords = @(Get-CimInstance -Namespace $storageNamespace -ClassName MSFT_Partition -ErrorAction Stop)
$physicalDisks = @()
try {
  $physicalDisks = @(Get-CimInstance -Namespace $storageNamespace -ClassName MSFT_PhysicalDisk -ErrorAction Stop)
} catch {
  $warnings.Add('物理介质扩展信息不可用；基础健康状态仍来自 Windows 磁盘提供程序。')
}
$bitLockerCommand = if ($isAdministrator) { Get-Command -Name Get-BitLockerVolume -ErrorAction SilentlyContinue } else { $null }

$disks = @(
  $diskRecords | Sort-Object Number | ForEach-Object {
    $disk = $_
    $diskUniqueId = if ([string]::IsNullOrWhiteSpace([string]$disk.UniqueId)) { $null } else { ([string]$disk.UniqueId).Trim() }
    $diskIdentity = if ($null -eq $diskUniqueId) {
      'disk-number:' + [string]$disk.Number + ':serial-sha256:' + (Get-SerialFingerprint ([string]$disk.SerialNumber))
    } else {
      'disk:' + $diskUniqueId
    }

    $physicalDisk = $null
    $mappingConfidence = 'unknown'
    if ($null -ne $diskUniqueId) {
      $physicalDisk = $physicalDisks |
        Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_.UniqueId) -and ([string]$_.UniqueId).Trim() -eq $diskUniqueId } |
        Select-Object -First 1
      if ($null -ne $physicalDisk) { $mappingConfidence = 'exact' }
    }
    if ($null -eq $physicalDisk) {
      $physicalDisk = $physicalDisks |
        Where-Object { [string]$_.DeviceId -eq [string]$disk.Number } |
        Select-Object -First 1
      if ($null -ne $physicalDisk) { $mappingConfidence = 'diskNumber' }
    }

    $reliability = $null
    if ($null -ne $physicalDisk) {
      try {
        $reliability = @(Get-CimAssociatedInstance -InputObject $physicalDisk -ResultClassName MSFT_StorageReliabilityCounter -ErrorAction Stop) | Select-Object -First 1
        if ($null -eq $reliability) { [void]$reliabilityUnavailable.Add([string]$disk.Number) }
      } catch {
        [void]$reliabilityUnavailable.Add([string]$disk.Number)
      }
    } else {
      [void]$reliabilityUnavailable.Add([string]$disk.Number)
    }

    $providerHealth = if ($null -ne $physicalDisk) { Convert-HealthStatus $physicalDisk.HealthStatus } else { Convert-HealthStatus $disk.HealthStatus }
    $readUncorrected = if ($null -eq $reliability -or $null -eq $reliability.ReadErrorsUncorrected) { $null } else { [uint64]$reliability.ReadErrorsUncorrected }
    $writeUncorrected = if ($null -eq $reliability -or $null -eq $reliability.WriteErrorsUncorrected) { $null } else { [uint64]$reliability.WriteErrorsUncorrected }
    $uncorrected = if ($null -ne $readUncorrected -and $null -ne $writeUncorrected) { $readUncorrected + $writeUncorrected } else { $null }
    $smartStatus = if ($providerHealth -eq 'Unhealthy' -or ($null -ne $uncorrected -and $uncorrected -gt 0)) {
      'failed'
    } elseif ($providerHealth -eq 'Warning') {
      'warning'
    } elseif ($providerHealth -eq 'Healthy' -and $null -ne $reliability) {
      'passed'
    } else {
      'unavailable'
    }

    $protectedVolumes = [uint32]0
    $unprotectedVolumes = [uint32]0
    $unknownVolumes = [uint32]0
    $mountedPartitions = @(
      $partitionRecords |
        Where-Object { [uint32]$_.DiskNumber -eq [uint32]$disk.Number } |
        Where-Object { -not [string]::IsNullOrWhiteSpace((Get-NormalizedDriveLetter $_.DriveLetter)) }
    )
    foreach ($partition in $mountedPartitions) {
      $driveLetter = Get-NormalizedDriveLetter $partition.DriveLetter
      $mountPoint = $driveLetter + ':'
      if ($null -eq $bitLockerCommand) {
        $unknownVolumes++
        [void]$bitLockerUnavailable.Add($mountPoint)
        continue
      }
      try {
        $bitLocker = Get-BitLockerVolume -MountPoint $mountPoint -ErrorAction Stop
        if ([int]$bitLocker.EncryptionPercentage -eq 0) {
          $unprotectedVolumes++
        } elseif ([string]$bitLocker.ProtectionStatus -eq 'On') {
          $protectedVolumes++
        } else {
          $unknownVolumes++
        }
      } catch {
        $unknownVolumes++
        [void]$bitLockerUnavailable.Add($mountPoint)
      }
    }

    $serial = if ($null -ne $physicalDisk -and -not [string]::IsNullOrWhiteSpace([string]$physicalDisk.SerialNumber)) {
      ([string]$physicalDisk.SerialNumber).Trim()
    } elseif (-not [string]::IsNullOrWhiteSpace([string]$disk.SerialNumber)) {
      ([string]$disk.SerialNumber).Trim()
    } else { $null }
    $serialSuffix = if ($null -eq $serial) { $null } elseif ($serial.Length -le 4) { $serial } else { $serial.Substring($serial.Length - 4) }
    $operationalStatus = if ($null -ne $physicalDisk) { @($physicalDisk.OperationalStatus) } else { @($disk.OperationalStatus) }

    [ordered]@{
      id = $diskIdentity
      number = [uint32]$disk.Number
      friendlyName = if ([string]::IsNullOrWhiteSpace([string]$disk.FriendlyName)) { '物理磁盘 ' + [string]$disk.Number } else { [string]$disk.FriendlyName }
      manufacturer = if ($null -ne $physicalDisk -and -not [string]::IsNullOrWhiteSpace([string]$physicalDisk.Manufacturer)) { [string]$physicalDisk.Manufacturer } elseif (-not [string]::IsNullOrWhiteSpace([string]$disk.Manufacturer)) { [string]$disk.Manufacturer } else { $null }
      model = if ($null -ne $physicalDisk -and -not [string]::IsNullOrWhiteSpace([string]$physicalDisk.Model)) { [string]$physicalDisk.Model } elseif (-not [string]::IsNullOrWhiteSpace([string]$disk.Model)) { [string]$disk.Model } else { $null }
      firmwareVersion = if ($null -ne $physicalDisk -and -not [string]::IsNullOrWhiteSpace([string]$physicalDisk.FirmwareVersion)) { [string]$physicalDisk.FirmwareVersion } elseif (-not [string]::IsNullOrWhiteSpace([string]$disk.FirmwareVersion)) { [string]$disk.FirmwareVersion } else { $null }
      serialSuffix = $serialSuffix
      busType = Convert-BusType $disk.BusType
      mediaType = if ($null -eq $physicalDisk) { 'Unknown' } else { Convert-MediaType $physicalDisk.MediaType }
      sizeBytes = [uint64]$disk.Size
      operationalStatus = @($operationalStatus | ForEach-Object { Convert-OperationalStatus $_ })
      isOffline = [bool]$disk.IsOffline
      providerHealth = $providerHealth
      smartStatus = $smartStatus
      identityMapping = $mappingConfidence
      temperatureCelsius = if ($null -eq $reliability -or $null -eq $reliability.Temperature) { $null } else { [int16]$reliability.Temperature }
      temperatureMaxCelsius = if ($null -eq $reliability -or $null -eq $reliability.TemperatureMax) { $null } else { [int16]$reliability.TemperatureMax }
      wearPercentUsed = if ($null -eq $reliability -or $null -eq $reliability.Wear) { $null } else { [uint8]$reliability.Wear }
      powerOnHours = if ($null -eq $reliability -or $null -eq $reliability.PowerOnHours) { $null } else { [uint64]$reliability.PowerOnHours }
      readErrorsTotal = if ($null -eq $reliability -or $null -eq $reliability.ReadErrorsTotal) { $null } else { [uint64]$reliability.ReadErrorsTotal }
      readErrorsUncorrected = $readUncorrected
      writeErrorsTotal = if ($null -eq $reliability -or $null -eq $reliability.WriteErrorsTotal) { $null } else { [uint64]$reliability.WriteErrorsTotal }
      writeErrorsUncorrected = $writeUncorrected
      logicalSectorBytes = if ($null -eq $disk.LogicalSectorSize) { $null } else { [uint32]$disk.LogicalSectorSize }
      physicalSectorBytes = if ($null -eq $disk.PhysicalSectorSize) { $null } else { [uint32]$disk.PhysicalSectorSize }
      encryption = [ordered]@{
        protectedVolumes = $protectedVolumes
        unprotectedVolumes = $unprotectedVolumes
        unknownVolumes = $unknownVolumes
      }
    }
  }
)

if ($bitLockerUnavailable.Count -gt 0) {
  $warnings.Add('当前权限无法核验 ' + [string]$bitLockerUnavailable.Count + ' 个卷的 BitLocker 状态；加密汇总标记为受限。')
}
if ($reliabilityUnavailable.Count -gt 0) {
  $warnings.Add([string]$reliabilityUnavailable.Count + ' 块磁盘未提供温度、寿命或错误计数；基础健康状态仍可用。')
}

[ordered]@{ disks = $disks; warnings = @($warnings) } | ConvertTo-Json -Depth 7 -Compress
