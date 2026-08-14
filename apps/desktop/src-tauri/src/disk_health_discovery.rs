//! Windows read-only physical-disk health discovery.
//!
//! The adapter launches only the trusted in-box `PowerShell` executable and
//! supplies one compile-time script without caller-controlled parameters. The
//! script redacts serial numbers before JSON crosses the process boundary.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use clarity_core::{
    DiskEncryptionSummary, DiskHealthSnapshot, IdentityMappingConfidence, PhysicalDiskHealth,
    PhysicalDiskHealthInput, ProviderHealthStatus, SmartHealthStatus,
};
use serde::Deserialize;

// This immutable, input-free script is the adapter's shell-injection boundary.
// It performs only Storage-module discovery and never invokes a write command.
const HEALTH_DISCOVERY_SCRIPT: &str = r"
$ErrorActionPreference = 'Stop'
$utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$warnings = [System.Collections.Generic.List[string]]::new()

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

$physicalDisks = @()
try {
  $physicalDisks = @(Get-PhysicalDisk -ErrorAction Stop)
} catch {
  $warnings.Add('无法读取物理磁盘存储提供程序，可靠性数据将标记为不可用。')
}
$bitLockerCommand = Get-Command -Name Get-BitLockerVolume -ErrorAction SilentlyContinue

$disks = @(
  Get-Disk -ErrorAction Stop | Sort-Object Number | ForEach-Object {
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
        $reliability = $physicalDisk | Get-StorageReliabilityCounter -ErrorAction Stop
      } catch {
        $warnings.Add('磁盘 ' + [string]$disk.Number + ' 的存储可靠性计数不可用。')
      }
    } else {
      $warnings.Add('磁盘 ' + [string]$disk.Number + ' 无法安全映射到物理介质记录。')
    }

    $providerHealth = if ($null -ne $physicalDisk) { [string]$physicalDisk.HealthStatus } else { [string]$disk.HealthStatus }
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
    $mountedPartitions = @(Get-Partition -DiskNumber $disk.Number -ErrorAction Stop | Where-Object { $null -ne $_.DriveLetter })
    foreach ($partition in $mountedPartitions) {
      $mountPoint = [string]$partition.DriveLetter + ':'
      if ($null -eq $bitLockerCommand) {
        $unknownVolumes++
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
        $warnings.Add('卷 ' + $mountPoint + ' 的 BitLocker 状态不可用。')
      }
    }

    $serial = if ($null -ne $physicalDisk -and -not [string]::IsNullOrWhiteSpace([string]$physicalDisk.SerialNumber)) {
      ([string]$physicalDisk.SerialNumber).Trim()
    } elseif (-not [string]::IsNullOrWhiteSpace([string]$disk.SerialNumber)) {
      ([string]$disk.SerialNumber).Trim()
    } else { $null }
    $serialSuffix = if ($null -eq $serial) { $null } elseif ($serial.Length -le 4) { $serial } else { $serial.Substring($serial.Length - 4) }

    [ordered]@{
      id = $diskIdentity
      number = [uint32]$disk.Number
      friendlyName = if ([string]::IsNullOrWhiteSpace([string]$disk.FriendlyName)) { '物理磁盘 ' + [string]$disk.Number } else { [string]$disk.FriendlyName }
      manufacturer = if ($null -eq $physicalDisk -or [string]::IsNullOrWhiteSpace([string]$physicalDisk.Manufacturer)) { $null } else { [string]$physicalDisk.Manufacturer }
      model = if ($null -eq $physicalDisk -or [string]::IsNullOrWhiteSpace([string]$physicalDisk.Model)) { $null } else { [string]$physicalDisk.Model }
      firmwareVersion = if ($null -eq $physicalDisk -or [string]::IsNullOrWhiteSpace([string]$physicalDisk.FirmwareVersion)) { $null } else { [string]$physicalDisk.FirmwareVersion }
      serialSuffix = $serialSuffix
      busType = [string]$disk.BusType
      mediaType = if ($null -eq $physicalDisk) { 'Unknown' } else { [string]$physicalDisk.MediaType }
      sizeBytes = [uint64]$disk.Size
      operationalStatus = if ($null -eq $physicalDisk) { @() } else { @($physicalDisk.OperationalStatus | ForEach-Object { [string]$_ }) }
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

[ordered]@{ disks = $disks; warnings = @($warnings) } | ConvertTo-Json -Depth 7 -Compress
";

/// Discovers and conservatively evaluates physical-disk health on Windows.
///
/// The command accepts no caller input, runs only the fixed read-only script,
/// and never retains a complete device serial number.
///
/// # Errors
///
/// Returns an error when the platform is unsupported, trusted `PowerShell`
/// cannot be located or launched, provider JSON is invalid, or domain
/// invariants reject the returned data.
pub fn discover_disk_health() -> Result<DiskHealthSnapshot, DiskHealthDiscoveryError> {
    #[cfg(not(windows))]
    {
        Err(DiskHealthDiscoveryError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    {
        discover_windows_disk_health()
    }
}

#[cfg(windows)]
fn discover_windows_disk_health() -> Result<DiskHealthSnapshot, DiskHealthDiscoveryError> {
    let output = Command::new(powershell_path()?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            HEALTH_DISCOVERY_SCRIPT,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(DiskHealthDiscoveryError::Launch)?;

    if !output.status.success() {
        return Err(DiskHealthDiscoveryError::ProviderFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }

    let envelope: PowerShellHealthEnvelope = serde_json::from_slice(&output.stdout)
        .map_err(DiskHealthDiscoveryError::InvalidProviderResponse)?;
    convert_health_snapshot(envelope)
}

#[cfg(windows)]
fn powershell_path() -> Result<PathBuf, DiskHealthDiscoveryError> {
    let system_root = std::env::var_os("SystemRoot")
        .filter(|value| !value.is_empty())
        .ok_or(DiskHealthDiscoveryError::SystemRootUnavailable)?;
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !executable.is_file() {
        return Err(DiskHealthDiscoveryError::PowerShellUnavailable(executable));
    }
    Ok(executable)
}

fn convert_health_snapshot(
    envelope: PowerShellHealthEnvelope,
) -> Result<DiskHealthSnapshot, DiskHealthDiscoveryError> {
    let disks = envelope
        .disks
        .into_iter()
        .map(convert_health_disk)
        .collect::<Result<Vec<_>, _>>()?;
    DiskHealthSnapshot::try_new(captured_at_unix_ms(), disks, envelope.warnings)
        .map_err(DiskHealthDiscoveryError::InvalidHealthData)
}

fn convert_health_disk(
    raw: PowerShellHealthDisk,
) -> Result<PhysicalDiskHealth, DiskHealthDiscoveryError> {
    PhysicalDiskHealth::try_from_input(PhysicalDiskHealthInput {
        id: raw.id,
        number: raw.number,
        friendly_name: raw.friendly_name,
        manufacturer: non_empty(raw.manufacturer),
        model: non_empty(raw.model),
        firmware_version: non_empty(raw.firmware_version),
        serial_suffix: non_empty(raw.serial_suffix),
        bus_type: raw.bus_type,
        media_type: raw.media_type,
        size_bytes: raw.size_bytes,
        operational_status: raw.operational_status,
        is_offline: raw.is_offline,
        provider_health: parse_provider_health(&raw.provider_health),
        smart_status: parse_smart_status(&raw.smart_status),
        identity_mapping: parse_mapping_confidence(&raw.identity_mapping),
        temperature_celsius: raw.temperature_celsius,
        temperature_max_celsius: raw.temperature_max_celsius,
        wear_percent_used: raw.wear_percent_used,
        power_on_hours: raw.power_on_hours,
        read_errors_total: raw.read_errors_total,
        read_errors_uncorrected: raw.read_errors_uncorrected,
        write_errors_total: raw.write_errors_total,
        write_errors_uncorrected: raw.write_errors_uncorrected,
        logical_sector_bytes: raw.logical_sector_bytes,
        physical_sector_bytes: raw.physical_sector_bytes,
        encryption: raw.encryption,
    })
    .map_err(DiskHealthDiscoveryError::InvalidHealthData)
}

fn captured_at_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn parse_provider_health(value: &str) -> ProviderHealthStatus {
    match value.to_ascii_lowercase().as_str() {
        "healthy" => ProviderHealthStatus::Healthy,
        "warning" => ProviderHealthStatus::Warning,
        "unhealthy" => ProviderHealthStatus::Unhealthy,
        _ => ProviderHealthStatus::Unknown,
    }
}

fn parse_smart_status(value: &str) -> SmartHealthStatus {
    match value {
        "passed" => SmartHealthStatus::Passed,
        "warning" => SmartHealthStatus::Warning,
        "failed" => SmartHealthStatus::Failed,
        _ => SmartHealthStatus::Unavailable,
    }
}

fn parse_mapping_confidence(value: &str) -> IdentityMappingConfidence {
    match value {
        "exact" => IdentityMappingConfidence::Exact,
        "diskNumber" => IdentityMappingConfidence::DiskNumber,
        _ => IdentityMappingConfidence::Unknown,
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellHealthEnvelope {
    disks: Vec<PowerShellHealthDisk>,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PowerShellHealthDisk {
    id: String,
    number: u32,
    friendly_name: String,
    manufacturer: Option<String>,
    model: Option<String>,
    firmware_version: Option<String>,
    serial_suffix: Option<String>,
    bus_type: String,
    media_type: String,
    size_bytes: u64,
    #[serde(default)]
    operational_status: Vec<String>,
    is_offline: bool,
    provider_health: String,
    smart_status: String,
    identity_mapping: String,
    temperature_celsius: Option<i16>,
    temperature_max_celsius: Option<i16>,
    wear_percent_used: Option<u8>,
    power_on_hours: Option<u64>,
    read_errors_total: Option<u64>,
    read_errors_uncorrected: Option<u64>,
    write_errors_total: Option<u64>,
    write_errors_uncorrected: Option<u64>,
    logical_sector_bytes: Option<u32>,
    physical_sector_bytes: Option<u32>,
    encryption: DiskEncryptionSummary,
}

/// Errors produced by the Windows read-only health provider adapter.
#[derive(Debug, thiserror::Error)]
pub enum DiskHealthDiscoveryError {
    /// Disk-health discovery is available only on Windows.
    #[cfg(not(windows))]
    #[error("disk health discovery is supported only on Windows")]
    UnsupportedPlatform,
    /// Windows system root was unavailable, so no trusted executable exists.
    #[error("Windows SystemRoot is unavailable")]
    SystemRootUnavailable,
    /// The trusted in-box `PowerShell` executable was not present.
    #[error("Windows PowerShell is unavailable at {0}")]
    PowerShellUnavailable(PathBuf),
    /// The fixed read-only provider could not be launched.
    #[error("failed to launch the read-only disk health provider: {0}")]
    Launch(#[source] std::io::Error),
    /// The storage provider rejected or failed the fixed query.
    #[error("read-only disk health provider failed: {0}")]
    ProviderFailed(String),
    /// Provider output did not match the adapter contract.
    #[error("invalid disk health provider response: {0}")]
    InvalidProviderResponse(#[source] serde_json::Error),
    /// Provider data violated conservative domain invariants.
    #[error("invalid disk health data: {0}")]
    InvalidHealthData(#[source] clarity_core::HealthModelError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_disk() -> PowerShellHealthDisk {
        PowerShellHealthDisk {
            id: "disk:fixture-unique-id".to_owned(),
            number: 0,
            friendly_name: "Fixture NVMe".to_owned(),
            manufacturer: Some("Fixture".to_owned()),
            model: Some("Fast Disk".to_owned()),
            firmware_version: Some("1.0".to_owned()),
            serial_suffix: Some("7788".to_owned()),
            bus_type: "NVMe".to_owned(),
            media_type: "SSD".to_owned(),
            size_bytes: 1_000_000,
            operational_status: vec!["OK".to_owned()],
            is_offline: false,
            provider_health: "Healthy".to_owned(),
            smart_status: "passed".to_owned(),
            identity_mapping: "exact".to_owned(),
            temperature_celsius: Some(38),
            temperature_max_celsius: Some(51),
            wear_percent_used: Some(7),
            power_on_hours: Some(2_400),
            read_errors_total: Some(0),
            read_errors_uncorrected: Some(0),
            write_errors_total: Some(0),
            write_errors_uncorrected: Some(0),
            logical_sector_bytes: Some(512),
            physical_sector_bytes: Some(4_096),
            encryption: DiskEncryptionSummary {
                protected_volumes: 1,
                unprotected_volumes: 0,
                unknown_volumes: 0,
            },
        }
    }

    #[test]
    fn converts_complete_provider_evidence_to_good() {
        let disk = convert_health_disk(raw_disk()).expect("fixture should convert");
        assert_eq!(disk.status, clarity_core::DiskHealthStatus::Good);
        assert_eq!(disk.serial_suffix.as_deref(), Some("7788"));
        assert!(disk.partition_writes_blocked);
    }

    #[test]
    fn unknown_mapping_and_counters_fail_closed() {
        let mut raw = raw_disk();
        raw.identity_mapping = "unknown".to_owned();
        raw.smart_status = "unavailable".to_owned();
        raw.temperature_celsius = None;
        raw.power_on_hours = None;
        raw.read_errors_uncorrected = None;
        raw.write_errors_uncorrected = None;
        let disk = convert_health_disk(raw).expect("partial evidence should convert");
        assert_eq!(disk.status, clarity_core::DiskHealthStatus::Unknown);
        assert_eq!(
            disk.data_completeness,
            clarity_core::HealthDataCompleteness::Limited
        );
    }

    #[test]
    fn rejects_unredacted_serials_before_the_ui_boundary() {
        let mut raw = raw_disk();
        raw.serial_suffix = Some("full-serial".to_owned());
        let error = convert_health_disk(raw).expect_err("serial must be redacted");
        assert!(matches!(
            error,
            DiskHealthDiscoveryError::InvalidHealthData(
                clarity_core::HealthModelError::SerialSuffixTooLong
            )
        ));
    }
}
