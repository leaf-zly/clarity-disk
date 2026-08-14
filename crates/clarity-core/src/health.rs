//! Read-only physical-disk health models and conservative aggregation rules.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One point-in-time health snapshot for all discovered physical disks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealthSnapshot {
    /// Unix timestamp in milliseconds when the provider data was captured.
    pub captured_at_unix_ms: u64,
    /// Evaluated physical disks ordered by Windows disk number.
    pub disks: Vec<PhysicalDiskHealth>,
    /// Aggregate counts used by the health dashboard.
    pub summary: DiskHealthSummary,
    /// Non-fatal provider limitations that remain visible to the user.
    pub discovery_warnings: Vec<String>,
    /// Always true because F11 has no repair or write command surface.
    pub read_only: bool,
}

impl DiskHealthSnapshot {
    /// Creates a validated snapshot and derives its aggregate counts.
    ///
    /// # Errors
    ///
    /// Returns an error when no disks exist or a physical-disk identity is
    /// empty or duplicated.
    pub fn try_new(
        captured_at_unix_ms: u64,
        mut disks: Vec<PhysicalDiskHealth>,
        discovery_warnings: Vec<String>,
    ) -> Result<Self, HealthModelError> {
        if disks.is_empty() {
            return Err(HealthModelError::NoPhysicalDisks);
        }
        let mut identities = HashSet::with_capacity(disks.len());
        for disk in &disks {
            if disk.id.trim().is_empty() {
                return Err(HealthModelError::EmptyDiskIdentity);
            }
            if !identities.insert(&disk.id) {
                return Err(HealthModelError::DuplicateDiskIdentity(disk.id.clone()));
            }
        }
        disks.sort_by_key(|disk| disk.number);
        let summary = DiskHealthSummary::from_disks(&disks);
        Ok(Self {
            captured_at_unix_ms,
            disks,
            summary,
            discovery_warnings,
            read_only: true,
        })
    }
}

/// Aggregate health counts for a point-in-time snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealthSummary {
    /// Number of physical disks in the snapshot.
    pub total_disks: u32,
    /// Disks with complete evidence and no warning signals.
    pub good_disks: u32,
    /// Disks with one or more warning signals.
    pub attention_disks: u32,
    /// Disks with a critical provider or reliability signal.
    pub critical_disks: u32,
    /// Disks whose evidence is insufficient for a health conclusion.
    pub unknown_disks: u32,
    /// Worst status across all disks.
    pub overall_status: DiskHealthStatus,
}

impl DiskHealthSummary {
    fn from_disks(disks: &[PhysicalDiskHealth]) -> Self {
        let count = |status| {
            disks
                .iter()
                .filter(|disk| disk.status == status)
                .count()
                .try_into()
                .unwrap_or(u32::MAX)
        };
        let critical_disks = count(DiskHealthStatus::Critical);
        let attention_disks = count(DiskHealthStatus::Attention);
        let unknown_disks = count(DiskHealthStatus::Unknown);
        let good_disks = count(DiskHealthStatus::Good);
        let overall_status = if critical_disks > 0 {
            DiskHealthStatus::Critical
        } else if attention_disks > 0 {
            DiskHealthStatus::Attention
        } else if unknown_disks > 0 {
            DiskHealthStatus::Unknown
        } else {
            DiskHealthStatus::Good
        };
        Self {
            total_disks: disks.len().try_into().unwrap_or(u32::MAX),
            good_disks,
            attention_disks,
            critical_disks,
            unknown_disks,
            overall_status,
        }
    }
}

/// Evaluated health information for one physical disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDiskHealth {
    /// Stable identity shared with the partition-topology provider.
    pub id: String,
    /// Windows disk number used for diagnostics and display.
    pub number: u32,
    /// Operating-system or vendor friendly name.
    pub friendly_name: String,
    /// Manufacturer when the provider exposes it.
    pub manufacturer: Option<String>,
    /// Model when the provider exposes it.
    pub model: Option<String>,
    /// Firmware version when the provider exposes it.
    pub firmware_version: Option<String>,
    /// Last four serial characters only; the complete serial is not retained.
    pub serial_suffix: Option<String>,
    /// Bus technology such as `NVMe`, `SATA`, `USB`, or Storage Spaces.
    pub bus_type: String,
    /// Media classification such as solid-state or rotational media.
    pub media_type: String,
    /// Total physical capacity in bytes.
    pub size_bytes: u64,
    /// Provider-reported operational states.
    pub operational_status: Vec<String>,
    /// Whether Windows reports this disk offline.
    pub is_offline: bool,
    /// Storage-provider health state before domain aggregation.
    pub provider_health: ProviderHealthStatus,
    /// Provider-backed self-monitoring conclusion and source.
    pub smart_status: SmartHealthStatus,
    /// How reliably storage-provider and partition identities were joined.
    pub identity_mapping: IdentityMappingConfidence,
    /// Current temperature in degrees Celsius, when available.
    pub temperature_celsius: Option<i16>,
    /// Highest provider-observed temperature in degrees Celsius.
    pub temperature_max_celsius: Option<i16>,
    /// Provider wear percentage, where 100 means fully consumed.
    pub wear_percent_used: Option<u8>,
    /// Derived remaining life percentage for solid-state media.
    pub estimated_life_remaining_percent: Option<u8>,
    /// Total power-on hours when available.
    pub power_on_hours: Option<u64>,
    /// Total corrected and uncorrected read errors.
    pub read_errors_total: Option<u64>,
    /// Uncorrected read errors.
    pub read_errors_uncorrected: Option<u64>,
    /// Total corrected and uncorrected write errors.
    pub write_errors_total: Option<u64>,
    /// Uncorrected write errors.
    pub write_errors_uncorrected: Option<u64>,
    /// Logical sector size in bytes.
    pub logical_sector_bytes: Option<u32>,
    /// Physical sector size in bytes.
    pub physical_sector_bytes: Option<u32>,
    /// Volume encryption counts associated with this disk.
    pub encryption: DiskEncryptionSummary,
    /// Completeness of reliability evidence, never inferred as healthy.
    pub data_completeness: HealthDataCompleteness,
    /// Conservative final health status.
    pub status: DiskHealthStatus,
    /// Evidence-bearing warnings, critical findings, or unknown states.
    pub signals: Vec<DiskHealthSignal>,
    /// Always true in F11 because physical-disk writes are not implemented.
    pub partition_writes_blocked: bool,
}

impl PhysicalDiskHealth {
    /// Validates provider input and evaluates a conservative health status.
    ///
    /// # Errors
    ///
    /// Returns an error for empty identities, zero capacity, impossible
    /// temperature values, or wear percentages above 100.
    pub fn try_from_input(input: PhysicalDiskHealthInput) -> Result<Self, HealthModelError> {
        validate_health_input(&input)?;
        let data_completeness = data_completeness(&input);
        let mut signals = evaluate_signals(&input, data_completeness);
        signals.sort_by_key(|signal| signal.severity.rank());
        let status = aggregate_status(&signals);
        let estimated_life_remaining_percent = input
            .wear_percent_used
            .map(|wear| 100_u8.saturating_sub(wear));
        Ok(Self {
            id: input.id,
            number: input.number,
            friendly_name: input.friendly_name,
            manufacturer: input.manufacturer,
            model: input.model,
            firmware_version: input.firmware_version,
            serial_suffix: input.serial_suffix,
            bus_type: input.bus_type,
            media_type: input.media_type,
            size_bytes: input.size_bytes,
            operational_status: input.operational_status,
            is_offline: input.is_offline,
            provider_health: input.provider_health,
            smart_status: input.smart_status,
            identity_mapping: input.identity_mapping,
            temperature_celsius: input.temperature_celsius,
            temperature_max_celsius: input.temperature_max_celsius,
            wear_percent_used: input.wear_percent_used,
            estimated_life_remaining_percent,
            power_on_hours: input.power_on_hours,
            read_errors_total: input.read_errors_total,
            read_errors_uncorrected: input.read_errors_uncorrected,
            write_errors_total: input.write_errors_total,
            write_errors_uncorrected: input.write_errors_uncorrected,
            logical_sector_bytes: input.logical_sector_bytes,
            physical_sector_bytes: input.physical_sector_bytes,
            encryption: input.encryption,
            data_completeness,
            status,
            signals,
            partition_writes_blocked: true,
        })
    }
}

/// Provider values needed to evaluate one physical disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalDiskHealthInput {
    /// Stable identity shared with partition discovery.
    pub id: String,
    /// Windows disk number.
    pub number: u32,
    /// Friendly display name.
    pub friendly_name: String,
    /// Optional manufacturer.
    pub manufacturer: Option<String>,
    /// Optional hardware model.
    pub model: Option<String>,
    /// Optional firmware version.
    pub firmware_version: Option<String>,
    /// Redacted serial suffix containing at most four characters.
    pub serial_suffix: Option<String>,
    /// Bus technology.
    pub bus_type: String,
    /// Media classification.
    pub media_type: String,
    /// Physical capacity in bytes.
    pub size_bytes: u64,
    /// Provider operational states.
    pub operational_status: Vec<String>,
    /// Provider offline flag.
    pub is_offline: bool,
    /// Provider health state.
    pub provider_health: ProviderHealthStatus,
    /// Provider-backed self-monitoring state.
    pub smart_status: SmartHealthStatus,
    /// Confidence of the physical-disk identity mapping.
    pub identity_mapping: IdentityMappingConfidence,
    /// Current temperature in degrees Celsius.
    pub temperature_celsius: Option<i16>,
    /// Maximum observed temperature in degrees Celsius.
    pub temperature_max_celsius: Option<i16>,
    /// Percentage of solid-state wear already consumed.
    pub wear_percent_used: Option<u8>,
    /// Total power-on hours.
    pub power_on_hours: Option<u64>,
    /// Total read errors.
    pub read_errors_total: Option<u64>,
    /// Uncorrected read errors.
    pub read_errors_uncorrected: Option<u64>,
    /// Total write errors.
    pub write_errors_total: Option<u64>,
    /// Uncorrected write errors.
    pub write_errors_uncorrected: Option<u64>,
    /// Logical sector bytes.
    pub logical_sector_bytes: Option<u32>,
    /// Physical sector bytes.
    pub physical_sector_bytes: Option<u32>,
    /// Encryption counts for mounted volumes on the disk.
    pub encryption: DiskEncryptionSummary,
}

/// Final conservative health classifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiskHealthStatus {
    /// Complete evidence contains no warning or critical signal.
    Good,
    /// At least one warning signal requires review.
    Attention,
    /// At least one critical signal requires backup and diagnostics.
    Critical,
    /// Evidence is insufficient for a health conclusion.
    Unknown,
}

/// Health status reported directly by the Windows storage provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderHealthStatus {
    /// Provider explicitly reported healthy.
    Healthy,
    /// Provider reported a warning.
    Warning,
    /// Provider explicitly reported unhealthy.
    Unhealthy,
    /// Provider returned no trustworthy state.
    Unknown,
}

/// Provider-backed self-monitoring conclusion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SmartHealthStatus {
    /// Storage provider reported no self-monitoring failure.
    Passed,
    /// Provider or reliability counters reported a warning.
    Warning,
    /// Provider reported a failure or uncorrected media errors.
    Failed,
    /// Self-monitoring state was unavailable.
    Unavailable,
}

/// Confidence that a storage-provider record maps to the partition disk ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdentityMappingConfidence {
    /// Unique ID matched exactly across providers.
    Exact,
    /// Windows disk number matched when a unique ID was unavailable.
    DiskNumber,
    /// Provider records could not be mapped safely.
    Unknown,
}

/// Completeness of health and reliability evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthDataCompleteness {
    /// All expected metrics for the media type were returned.
    Complete,
    /// Some useful metrics exist, but one or more expected fields are absent.
    Partial,
    /// Reliability counters are unavailable or identity mapping is unknown.
    Limited,
}

/// `BitLocker` summary for mounted volumes on one physical disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskEncryptionSummary {
    /// Mounted volumes confirmed protected.
    pub protected_volumes: u32,
    /// Mounted volumes confirmed unprotected and fully decrypted.
    pub unprotected_volumes: u32,
    /// Mounted volumes whose encryption state could not be read.
    pub unknown_volumes: u32,
}

/// One evidence-bearing disk health signal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealthSignal {
    /// Stable signal identifier.
    pub code: DiskHealthSignalCode,
    /// Severity used for aggregation and UI ordering.
    pub severity: HealthSignalSeverity,
    /// Concise localized finding.
    pub title: String,
    /// Evidence or provider limitation behind the finding.
    pub detail: String,
    /// Safe next step that never claims to repair the disk.
    pub recommendation: String,
}

/// Stable health-signal identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiskHealthSignalCode {
    /// Disk is offline.
    DiskOffline,
    /// Provider explicitly reported unhealthy.
    ProviderUnhealthy,
    /// Provider reported a warning.
    ProviderWarning,
    /// Provider health was unavailable.
    ProviderUnknown,
    /// Provider-backed self-monitoring reported failure.
    SmartFailure,
    /// Provider-backed self-monitoring reported warning.
    SmartWarning,
    /// Self-monitoring state was unavailable.
    SmartUnavailable,
    /// Current temperature reached the warning threshold.
    TemperatureHigh,
    /// Current temperature reached the critical threshold.
    TemperatureCritical,
    /// Solid-state wear reached the warning threshold.
    WearHigh,
    /// Solid-state wear reached the critical threshold.
    WearCritical,
    /// Uncorrected read or write errors were reported.
    UncorrectedErrors,
    /// Corrected or retryable read/write errors were reported.
    CorrectedErrors,
    /// Reliability evidence is incomplete.
    DataIncomplete,
    /// Provider-to-disk identity mapping is unknown.
    IdentityMappingUnknown,
}

/// Severity of a health signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthSignalSeverity {
    /// Critical evidence requiring backup and vendor diagnostics.
    Critical,
    /// Evidence requiring review and monitoring.
    Warning,
    /// Evidence is unavailable and must not be inferred healthy.
    Unknown,
}

impl HealthSignalSeverity {
    const fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::Warning => 1,
            Self::Unknown => 2,
        }
    }
}

/// Validation failures for disk-health provider data.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum HealthModelError {
    /// Provider returned no physical disks.
    #[error("no physical disks were returned")]
    NoPhysicalDisks,
    /// A disk had no stable identity.
    #[error("physical disk identity is empty")]
    EmptyDiskIdentity,
    /// The same stable identity occurred more than once.
    #[error("physical disk identity is duplicated: {0}")]
    DuplicateDiskIdentity(String),
    /// Physical capacity was zero.
    #[error("physical disk capacity must be greater than zero")]
    ZeroDiskCapacity,
    /// Provider temperature was outside the supported diagnostic range.
    #[error("temperature {0}°C is outside the supported range")]
    InvalidTemperature(i16),
    /// Provider wear percentage exceeded 100.
    #[error("wear percentage {0} exceeds 100")]
    InvalidWearPercentage(u8),
    /// Serial suffix contained more than four characters.
    #[error("serial suffix exceeds four characters")]
    SerialSuffixTooLong,
}

fn validate_health_input(input: &PhysicalDiskHealthInput) -> Result<(), HealthModelError> {
    if input.id.trim().is_empty() {
        return Err(HealthModelError::EmptyDiskIdentity);
    }
    if input.size_bytes == 0 {
        return Err(HealthModelError::ZeroDiskCapacity);
    }
    for temperature in [input.temperature_celsius, input.temperature_max_celsius]
        .into_iter()
        .flatten()
    {
        if !(-50..=150).contains(&temperature) {
            return Err(HealthModelError::InvalidTemperature(temperature));
        }
    }
    if let Some(wear @ 101..) = input.wear_percent_used {
        return Err(HealthModelError::InvalidWearPercentage(wear));
    }
    if input
        .serial_suffix
        .as_ref()
        .is_some_and(|suffix| suffix.chars().count() > 4)
    {
        return Err(HealthModelError::SerialSuffixTooLong);
    }
    Ok(())
}

fn data_completeness(input: &PhysicalDiskHealthInput) -> HealthDataCompleteness {
    if input.identity_mapping == IdentityMappingConfidence::Unknown {
        return HealthDataCompleteness::Limited;
    }
    let mut expected = vec![
        input.temperature_celsius.is_some(),
        input.power_on_hours.is_some(),
        input.read_errors_uncorrected.is_some(),
        input.write_errors_uncorrected.is_some(),
    ];
    let solid_state = input.media_type.to_ascii_lowercase().contains("ssd")
        || input.bus_type.eq_ignore_ascii_case("nvme");
    if solid_state {
        expected.push(input.wear_percent_used.is_some());
    }
    let available = expected.iter().filter(|available| **available).count();
    if available == expected.len() {
        HealthDataCompleteness::Complete
    } else if available == 0 {
        HealthDataCompleteness::Limited
    } else {
        HealthDataCompleteness::Partial
    }
}

fn evaluate_signals(
    input: &PhysicalDiskHealthInput,
    completeness: HealthDataCompleteness,
) -> Vec<DiskHealthSignal> {
    let mut signals = Vec::new();
    append_offline_signal(input, &mut signals);
    append_provider_signal(input, &mut signals);
    append_smart_signal(input, &mut signals);
    append_temperature_signal(input, &mut signals);
    append_wear_signal(input, &mut signals);
    append_error_signal(input, &mut signals);
    append_completeness_signals(input, completeness, &mut signals);
    signals
}

fn append_offline_signal(input: &PhysicalDiskHealthInput, signals: &mut Vec<DiskHealthSignal>) {
    if input.is_offline {
        signals.push(signal(
            DiskHealthSignalCode::DiskOffline,
            HealthSignalSeverity::Critical,
            "磁盘当前离线",
            "Windows 存储提供程序报告此物理磁盘离线。",
            "保持停止写入，检查连接并先备份可访问的数据。",
        ));
    }
}

fn append_provider_signal(input: &PhysicalDiskHealthInput, signals: &mut Vec<DiskHealthSignal>) {
    match input.provider_health {
        ProviderHealthStatus::Unhealthy => signals.push(signal(
            DiskHealthSignalCode::ProviderUnhealthy,
            HealthSignalSeverity::Critical,
            "存储提供程序报告异常",
            "Windows 明确返回 Unhealthy 状态。",
            "立即备份重要数据，并使用设备厂商诊断工具复核。",
        )),
        ProviderHealthStatus::Warning => signals.push(signal(
            DiskHealthSignalCode::ProviderWarning,
            HealthSignalSeverity::Warning,
            "存储提供程序需要注意",
            "Windows 返回 Warning 状态。",
            "尽快备份并观察健康状态是否继续变化。",
        )),
        ProviderHealthStatus::Unknown => signals.push(signal(
            DiskHealthSignalCode::ProviderUnknown,
            HealthSignalSeverity::Unknown,
            "提供程序健康状态不可用",
            "系统没有返回可验证的物理磁盘健康状态。",
            "不要据此判断磁盘健康；可使用厂商工具进一步检查。",
        )),
        ProviderHealthStatus::Healthy => {}
    }
}

fn append_smart_signal(input: &PhysicalDiskHealthInput, signals: &mut Vec<DiskHealthSignal>) {
    match input.smart_status {
        SmartHealthStatus::Failed => signals.push(signal(
            DiskHealthSignalCode::SmartFailure,
            HealthSignalSeverity::Critical,
            "自监测状态失败",
            "存储提供程序或可靠性计数报告明确故障。",
            "立即备份重要数据，并停止计划中的高风险磁盘操作。",
        )),
        SmartHealthStatus::Warning => signals.push(signal(
            DiskHealthSignalCode::SmartWarning,
            HealthSignalSeverity::Warning,
            "自监测状态需要注意",
            "存储提供程序报告潜在可靠性问题。",
            "备份重要数据并运行设备厂商的完整诊断。",
        )),
        SmartHealthStatus::Unavailable => signals.push(signal(
            DiskHealthSignalCode::SmartUnavailable,
            HealthSignalSeverity::Unknown,
            "自监测状态不可用",
            "当前设备或驱动未提供可读取的自监测结论。",
            "未知不代表健康；需要时使用厂商工具补充检查。",
        )),
        SmartHealthStatus::Passed => {}
    }
}

fn append_temperature_signal(input: &PhysicalDiskHealthInput, signals: &mut Vec<DiskHealthSignal>) {
    if let Some(temperature) = input.temperature_celsius {
        if temperature >= 70 {
            signals.push(signal(
                DiskHealthSignalCode::TemperatureCritical,
                HealthSignalSeverity::Critical,
                "磁盘温度过高",
                format!("当前温度为 {temperature}°C，达到 70°C 严重阈值。"),
                "暂停高负载任务，检查散热并等待温度下降。",
            ));
        } else if temperature >= 60 {
            signals.push(signal(
                DiskHealthSignalCode::TemperatureHigh,
                HealthSignalSeverity::Warning,
                "磁盘温度偏高",
                format!("当前温度为 {temperature}°C，达到 60°C 提醒阈值。"),
                "检查风道和持续负载，并继续观察温度。",
            ));
        }
    }
}

fn append_wear_signal(input: &PhysicalDiskHealthInput, signals: &mut Vec<DiskHealthSignal>) {
    if let Some(wear) = input.wear_percent_used {
        if wear >= 90 {
            signals.push(signal(
                DiskHealthSignalCode::WearCritical,
                HealthSignalSeverity::Critical,
                "固态介质寿命接近上限",
                format!("提供程序报告已使用 {wear}% 设计寿命。"),
                "立即备份关键数据并安排更换设备。",
            ));
        } else if wear >= 80 {
            signals.push(signal(
                DiskHealthSignalCode::WearHigh,
                HealthSignalSeverity::Warning,
                "固态介质磨损较高",
                format!("提供程序报告已使用 {wear}% 设计寿命。"),
                "保持可靠备份并规划设备更换。",
            ));
        }
    }
}

fn append_error_signal(input: &PhysicalDiskHealthInput, signals: &mut Vec<DiskHealthSignal>) {
    let uncorrected = input
        .read_errors_uncorrected
        .unwrap_or(0)
        .saturating_add(input.write_errors_uncorrected.unwrap_or(0));
    if uncorrected > 0 {
        signals.push(signal(
            DiskHealthSignalCode::UncorrectedErrors,
            HealthSignalSeverity::Critical,
            "发现未纠正读写错误",
            format!("可靠性计数报告 {uncorrected} 次未纠正错误。"),
            "立即备份重要数据，并使用厂商工具执行介质诊断。",
        ));
    } else {
        let total = input
            .read_errors_total
            .unwrap_or(0)
            .saturating_add(input.write_errors_total.unwrap_or(0));
        if total > 0 {
            signals.push(signal(
                DiskHealthSignalCode::CorrectedErrors,
                HealthSignalSeverity::Warning,
                "存在已纠正或重试错误",
                format!("可靠性计数累计报告 {total} 次读写错误。"),
                "继续监控错误计数变化，并确保备份可用。",
            ));
        }
    }
}

fn append_completeness_signals(
    input: &PhysicalDiskHealthInput,
    completeness: HealthDataCompleteness,
    signals: &mut Vec<DiskHealthSignal>,
) {
    if input.identity_mapping == IdentityMappingConfidence::Unknown {
        signals.push(signal(
            DiskHealthSignalCode::IdentityMappingUnknown,
            HealthSignalSeverity::Unknown,
            "物理磁盘身份映射不可确认",
            "分区提供程序与可靠性提供程序无法安全关联。",
            "不要将当前指标用于分区写操作判断。",
        ));
    }
    if completeness != HealthDataCompleteness::Complete {
        signals.push(signal(
            DiskHealthSignalCode::DataIncomplete,
            HealthSignalSeverity::Unknown,
            "健康数据不完整",
            "部分温度、磨损、通电时间或错误计数字段不可用。",
            "不可用字段不会被推断为正常；可使用厂商工具补充检查。",
        ));
    }
}

fn signal(
    code: DiskHealthSignalCode,
    severity: HealthSignalSeverity,
    title: impl Into<String>,
    detail: impl Into<String>,
    recommendation: impl Into<String>,
) -> DiskHealthSignal {
    DiskHealthSignal {
        code,
        severity,
        title: title.into(),
        detail: detail.into(),
        recommendation: recommendation.into(),
    }
}

fn aggregate_status(signals: &[DiskHealthSignal]) -> DiskHealthStatus {
    if signals
        .iter()
        .any(|signal| signal.severity == HealthSignalSeverity::Critical)
    {
        DiskHealthStatus::Critical
    } else if signals
        .iter()
        .any(|signal| signal.severity == HealthSignalSeverity::Warning)
    {
        DiskHealthStatus::Attention
    } else if signals
        .iter()
        .any(|signal| signal.severity == HealthSignalSeverity::Unknown)
    {
        DiskHealthStatus::Unknown
    } else {
        DiskHealthStatus::Good
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_input(id: &str, number: u32) -> PhysicalDiskHealthInput {
        PhysicalDiskHealthInput {
            id: id.to_owned(),
            number,
            friendly_name: "Test NVMe".to_owned(),
            manufacturer: Some("Test".to_owned()),
            model: Some("Model".to_owned()),
            firmware_version: Some("1.0".to_owned()),
            serial_suffix: Some("1234".to_owned()),
            bus_type: "NVMe".to_owned(),
            media_type: "SSD".to_owned(),
            size_bytes: 1_000,
            operational_status: vec!["OK".to_owned()],
            is_offline: false,
            provider_health: ProviderHealthStatus::Healthy,
            smart_status: SmartHealthStatus::Passed,
            identity_mapping: IdentityMappingConfidence::Exact,
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
    fn reports_good_only_with_complete_positive_evidence() {
        let disk = PhysicalDiskHealth::try_from_input(healthy_input("disk-a", 0))
            .expect("healthy input should evaluate");
        assert_eq!(disk.status, DiskHealthStatus::Good);
        assert_eq!(disk.data_completeness, HealthDataCompleteness::Complete);
        assert_eq!(disk.estimated_life_remaining_percent, Some(93));
        assert!(disk.signals.is_empty());
        assert!(disk.partition_writes_blocked);
    }

    #[test]
    fn treats_unavailable_evidence_as_unknown_not_healthy() {
        let mut input = healthy_input("disk-a", 0);
        input.smart_status = SmartHealthStatus::Unavailable;
        input.temperature_celsius = None;
        input.power_on_hours = None;
        input.wear_percent_used = None;
        input.read_errors_uncorrected = None;
        input.write_errors_uncorrected = None;
        let disk = PhysicalDiskHealth::try_from_input(input)
            .expect("missing evidence should still return a health record");
        assert_eq!(disk.status, DiskHealthStatus::Unknown);
        assert_eq!(disk.data_completeness, HealthDataCompleteness::Limited);
        assert!(
            disk.signals
                .iter()
                .any(|signal| signal.code == DiskHealthSignalCode::DataIncomplete)
        );
    }

    #[test]
    fn escalates_temperature_and_wear_thresholds() {
        let mut warning = healthy_input("warning", 0);
        warning.temperature_celsius = Some(60);
        warning.wear_percent_used = Some(80);
        let warning =
            PhysicalDiskHealth::try_from_input(warning).expect("threshold values should be valid");
        assert_eq!(warning.status, DiskHealthStatus::Attention);

        let mut critical = healthy_input("critical", 1);
        critical.temperature_celsius = Some(70);
        critical.wear_percent_used = Some(90);
        let critical =
            PhysicalDiskHealth::try_from_input(critical).expect("threshold values should be valid");
        assert_eq!(critical.status, DiskHealthStatus::Critical);
    }

    #[test]
    fn uncorrected_errors_are_critical() {
        let mut input = healthy_input("disk-a", 0);
        input.read_errors_total = Some(2);
        input.read_errors_uncorrected = Some(1);
        let disk =
            PhysicalDiskHealth::try_from_input(input).expect("error counters should evaluate");
        assert_eq!(disk.status, DiskHealthStatus::Critical);
        assert!(
            disk.signals
                .iter()
                .any(|signal| signal.code == DiskHealthSignalCode::UncorrectedErrors)
        );
    }

    #[test]
    fn aggregates_worst_status_and_rejects_duplicate_identities() {
        let good = PhysicalDiskHealth::try_from_input(healthy_input("good", 1))
            .expect("healthy input should evaluate");
        let mut warning_input = healthy_input("warning", 0);
        warning_input.provider_health = ProviderHealthStatus::Warning;
        let warning = PhysicalDiskHealth::try_from_input(warning_input)
            .expect("warning input should evaluate");
        let snapshot = DiskHealthSnapshot::try_new(42, vec![good.clone(), warning], vec![])
            .expect("unique disks should construct");
        assert_eq!(snapshot.summary.overall_status, DiskHealthStatus::Attention);
        assert_eq!(snapshot.disks[0].id, "warning");

        let error = DiskHealthSnapshot::try_new(42, vec![good.clone(), good], vec![])
            .expect_err("duplicate identities must fail closed");
        assert_eq!(
            error,
            HealthModelError::DuplicateDiskIdentity("good".to_owned())
        );
    }

    #[test]
    fn rejects_impossible_provider_values() {
        let mut temperature = healthy_input("disk-a", 0);
        temperature.temperature_celsius = Some(151);
        assert_eq!(
            PhysicalDiskHealth::try_from_input(temperature)
                .expect_err("impossible temperature should fail"),
            HealthModelError::InvalidTemperature(151)
        );

        let mut serial = healthy_input("disk-b", 1);
        serial.serial_suffix = Some("12345".to_owned());
        assert_eq!(
            PhysicalDiskHealth::try_from_input(serial).expect_err("long serial suffix should fail"),
            HealthModelError::SerialSuffixTooLong
        );
    }
}
