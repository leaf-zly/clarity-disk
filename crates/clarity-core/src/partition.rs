//! Read-only partition topology and merge-preview safety rules.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// A point-in-time, read-only view of all discovered physical disks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionTopology {
    /// Unix timestamp in milliseconds when the platform snapshot was captured.
    pub captured_at_unix_ms: u64,
    /// Physical disks and their partitions in start-offset order.
    pub disks: Vec<PhysicalDisk>,
    /// Non-fatal discovery limitations that must remain visible to the user.
    pub discovery_warnings: Vec<String>,
    /// Always true for the P6 command surface; no write operation is exposed.
    pub read_only: bool,
}

impl PartitionTopology {
    /// Evaluates a requested source-to-target merge without changing disk state.
    ///
    /// The request contains backend identities only. Every safety signal is
    /// treated conservatively: unknown encryption, health, snapshot, identity,
    /// or disk-layout state blocks feasibility instead of being guessed safe.
    ///
    /// # Errors
    ///
    /// Returns an error when either partition identity is missing or duplicated.
    pub fn preview_merge(
        &self,
        request: &MergePreviewRequest,
    ) -> Result<MergePreview, PartitionPreviewError> {
        let source = self.unique_partition(&request.source_partition_id)?;
        let target = self.unique_partition(&request.target_partition_id)?;
        let mut checks = Vec::new();
        let mut blockers = Vec::new();

        record_check(
            &mut checks,
            &mut blockers,
            MergeCheckCode::DistinctPartitions,
            source.id != target.id,
            "源分区与目标分区不同",
            "请选择两个不同的分区",
            MergeBlockerCode::SamePartition,
            None,
        );

        let same_disk = source.disk_id == target.disk_id;
        record_check(
            &mut checks,
            &mut blockers,
            MergeCheckCode::SamePhysicalDisk,
            same_disk,
            "两个分区位于同一物理磁盘",
            "分区合并不能跨越物理磁盘",
            MergeBlockerCode::DifferentPhysicalDisk,
            None,
        );

        let disk = self.disks.iter().find(|disk| disk.id == target.disk_id);
        if let Some(disk) = disk {
            check_disk_safety(disk, &mut checks, &mut blockers);
        }

        let adjacent = same_disk
            && target
                .end_offset_bytes()
                .is_some_and(|end| end == source.start_offset_bytes);
        if same_disk {
            record_check(
                &mut checks,
                &mut blockers,
                MergeCheckCode::AdjacentAndOrdered,
                adjacent,
                "源分区紧邻目标分区右侧，可模拟向右扩展",
                "当前只支持将右侧相邻数据分区预演合并到左侧目标分区",
                MergeBlockerCode::NotAdjacentOrUnsupportedDirection,
                Some(source.id.clone()),
            );
        } else {
            checks.push(MergeCheck {
                code: MergeCheckCode::AdjacentAndOrdered,
                passed: false,
                message: "两个分区不在同一物理磁盘，未评估相邻关系".to_owned(),
            });
        }

        check_partition_safety(source, &mut checks, &mut blockers);
        check_partition_safety(target, &mut checks, &mut blockers);

        let migration_bytes = source.used_bytes;
        record_check(
            &mut checks,
            &mut blockers,
            MergeCheckCode::MigrationSizeKnown,
            migration_bytes.is_some(),
            "已读取需要迁移的数据量",
            "无法读取源分区已用空间，请刷新拓扑或先运行文件系统检查",
            MergeBlockerCode::MigrationSizeUnknown,
            Some(source.id.clone()),
        );

        deduplicate_blockers(&mut blockers);
        let feasible = blockers.is_empty();
        let simulated_layout = feasible
            .then(|| simulate_layout(disk, source, target))
            .transpose()?;
        let migration_bytes = migration_bytes.unwrap_or(0);
        let preview_id = preview_id(self, request);

        Ok(MergePreview {
            preview_id,
            topology_captured_at_unix_ms: self.captured_at_unix_ms,
            disk_id: target.disk_id.clone(),
            source_partition_id: source.id.clone(),
            target_partition_id: target.id.clone(),
            feasible,
            execution_authorized: false,
            risk_level: MergeRiskLevel::High,
            migration_bytes,
            estimated_duration_seconds: estimate_duration_seconds(migration_bytes),
            requires_restart: false,
            checks,
            blockers,
            simulated_layout,
            disclaimer: "此结果仅为只读预演，不会修改磁盘；执行前必须重新发现并重新确认。"
                .to_owned(),
        })
    }

    fn unique_partition(&self, id: &str) -> Result<&PartitionDescriptor, PartitionPreviewError> {
        let mut matches = self
            .disks
            .iter()
            .flat_map(|disk| &disk.partitions)
            .filter(|partition| partition.id == id);
        let partition = matches
            .next()
            .ok_or_else(|| PartitionPreviewError::PartitionNotFound(id.to_owned()))?;
        if matches.next().is_some() {
            return Err(PartitionPreviewError::DuplicatePartitionIdentity(
                id.to_owned(),
            ));
        }
        Ok(partition)
    }
}

/// Read-only physical disk metadata used to group partition identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDisk {
    /// Stable backend identity, independent of a display name or drive letter.
    pub id: String,
    /// Windows disk number used only for diagnostics and display.
    pub number: u32,
    /// Vendor- or operating-system-provided friendly name.
    pub friendly_name: String,
    /// Bus technology such as NVMe, SATA, USB, or Spaces.
    pub bus_type: String,
    /// Partition table style such as GPT or MBR.
    pub partition_style: String,
    /// Total physical capacity in bytes.
    pub size_bytes: u64,
    /// Layout technology, which must be basic for a feasible P6 preview.
    pub layout_kind: DiskLayoutKind,
    /// Conservative health state reported by the storage provider.
    pub health: TopologyHealth,
    /// Uncorrected media-error signal; unknown is intentionally blocking.
    pub media_error_state: MediaErrorState,
    /// Whether the operating system reports the disk offline.
    pub is_offline: bool,
    /// Ordered real and synthetic unallocated regions on this disk.
    pub partitions: Vec<PartitionDescriptor>,
}

/// A real partition or a synthetic unallocated region in a disk topology.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionDescriptor {
    /// Stable backend identity submitted by the UI for preview requests.
    pub id: String,
    /// Stable identity of the containing physical disk.
    pub disk_id: String,
    /// Windows partition number, or zero for synthetic unallocated regions.
    pub partition_number: u32,
    /// Partition GUID where the platform provides one.
    pub guid: Option<String>,
    /// Byte offset from the beginning of the physical disk.
    pub start_offset_bytes: u64,
    /// Region capacity in bytes.
    pub size_bytes: u64,
    /// File-system name, if a mounted volume was resolved.
    pub file_system: Option<String>,
    /// User-visible volume label, if present.
    pub label: Option<String>,
    /// Backend-discovered mount points; never accepted as command input.
    pub mount_points: Vec<String>,
    /// Structural role of the partition.
    pub kind: PartitionKind,
    /// Whether Windows reports this as the running system partition.
    pub is_system: bool,
    /// Whether Windows reports this as a boot partition.
    pub is_boot: bool,
    /// Whether the partition or its backing disk is read-only.
    pub is_read_only: bool,
    /// Current online/offline state.
    pub operational_state: PartitionOperationalState,
    /// BitLocker state; unknown is intentionally blocking.
    pub encryption_state: EncryptionState,
    /// Volume Shadow Copy presence; unknown is intentionally blocking.
    pub snapshot_state: SnapshotState,
    /// File-system or storage-provider health state.
    pub health: TopologyHealth,
    /// Used bytes when a mounted volume provided reliable counters.
    pub used_bytes: Option<u64>,
    /// Free bytes when a mounted volume provided reliable counters.
    pub free_bytes: Option<u64>,
}

impl PartitionDescriptor {
    /// Returns the exclusive end offset, or `None` when platform values overflow.
    #[must_use]
    pub fn end_offset_bytes(&self) -> Option<u64> {
        self.start_offset_bytes.checked_add(self.size_bytes)
    }
}

/// Storage layout technology relevant to merge feasibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiskLayoutKind {
    /// Standard basic disk managed by the Windows storage provider.
    Basic,
    /// Legacy dynamic disk, unsupported by the P6 preview engine.
    Dynamic,
    /// Storage Spaces virtual disk, unsupported by the P6 preview engine.
    StorageSpaces,
    /// Provider could not establish the layout technology.
    Unknown,
}

/// Structural partition roles used by protection rules and the topology UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionKind {
    /// Ordinary user or application data partition.
    Data,
    /// EFI System Partition required for UEFI boot.
    EfiSystem,
    /// Microsoft Reserved Partition on GPT disks.
    MicrosoftReserved,
    /// Windows or OEM recovery partition.
    Recovery,
    /// A provider-identified system partition not covered by EFI.
    System,
    /// Synthetic free extent not backed by a partition.
    Unallocated,
    /// Provider could not classify the partition role.
    Unknown,
}

/// BitLocker signal used by conservative feasibility checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EncryptionState {
    /// BitLocker is confirmed disabled for the mounted data volume.
    Off,
    /// BitLocker protection is enabled.
    On,
    /// BitLocker protection is suspended but encrypted metadata still exists.
    Suspended,
    /// Encryption does not apply to a synthetic unallocated region.
    NotApplicable,
    /// Provider could not establish encryption state.
    Unknown,
}

/// Volume Shadow Copy signal used by conservative feasibility checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotState {
    /// No shadow copy was reported for the volume.
    None,
    /// One or more shadow copies may depend on the volume layout.
    Present,
    /// Provider could not establish snapshot state.
    Unknown,
    /// Snapshots do not apply to a synthetic unallocated region.
    NotApplicable,
}

/// Health state shared by disks and partitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TopologyHealth {
    /// Provider explicitly reported a healthy state.
    Healthy,
    /// Provider reported a condition requiring attention.
    Warning,
    /// Provider explicitly reported an unhealthy state.
    Unhealthy,
    /// Provider could not establish health.
    Unknown,
    /// Health does not apply to a synthetic unallocated region.
    NotApplicable,
}

/// Uncorrected storage-media error signal used by disk safety checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaErrorState {
    /// Reliability counters explicitly reported no uncorrected errors.
    None,
    /// One or more uncorrected read or write errors were reported.
    Present,
    /// Reliability counters were unavailable or could not be mapped safely.
    Unknown,
}

/// Online state for a partition or volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionOperationalState {
    /// Partition and backing disk are online.
    Online,
    /// Partition or backing disk is offline.
    Offline,
    /// Provider could not establish online state.
    Unknown,
    /// State does not apply to a synthetic unallocated region.
    NotApplicable,
}

/// Backend-identity-only request for a read-only merge preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreviewRequest {
    /// Data partition whose used bytes would need migration.
    pub source_partition_id: String,
    /// Left-side data partition that would receive the simulated capacity.
    pub target_partition_id: String,
}

/// Complete, non-authorizing merge feasibility result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreview {
    /// Deterministic digest binding the request to one topology snapshot.
    pub preview_id: String,
    /// Capture time copied from the evaluated topology.
    pub topology_captured_at_unix_ms: u64,
    /// Physical disk identity involved in the request.
    pub disk_id: String,
    /// Evaluated source partition identity.
    pub source_partition_id: String,
    /// Evaluated target partition identity.
    pub target_partition_id: String,
    /// Whether all conservative P6 checks passed.
    pub feasible: bool,
    /// Always false; P6 exposes no partition writer or authorization token.
    pub execution_authorized: bool,
    /// Inherent risk of a future partition merge even when preflight passes.
    pub risk_level: MergeRiskLevel,
    /// Source used bytes that a future implementation would need to migrate.
    pub migration_bytes: u64,
    /// Conservative transfer-only estimate, excluding unknown offline work.
    pub estimated_duration_seconds: u64,
    /// Whether this P6 simulation predicts an offline restart step.
    pub requires_restart: bool,
    /// Every evaluated check, including passing evidence.
    pub checks: Vec<MergeCheck>,
    /// Actionable reasons that make the request infeasible.
    pub blockers: Vec<MergeBlocker>,
    /// Simulated post-merge layout only when every check passes.
    pub simulated_layout: Option<SimulatedPartitionLayout>,
    /// User-visible statement that the result is not an execution approval.
    pub disclaimer: String,
}

/// Risk levels presented by the read-only preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeRiskLevel {
    /// Partition changes are inherently high risk even when simulation passes.
    High,
}

/// One passed or failed feasibility check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeCheck {
    /// Stable code used for rendering and analytics.
    pub code: MergeCheckCode,
    /// Whether this check passed.
    pub passed: bool,
    /// Localized evidence or explanation.
    pub message: String,
}

/// Stable feasibility-check identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeCheckCode {
    /// Source and target must differ.
    DistinctPartitions,
    /// Source and target must share one physical disk.
    SamePhysicalDisk,
    /// Source must be directly to the right of the target.
    AdjacentAndOrdered,
    /// Disk must use a supported basic layout.
    SupportedDiskLayout,
    /// Disk health must be known healthy and online.
    DiskHealthyAndOnline,
    /// Reliability counters must confirm no uncorrected media errors.
    NoMediaErrors,
    /// Partition role and identity must be safe for preview.
    PartitionIdentityAndRole,
    /// Partition file system must be NTFS.
    SupportedFileSystem,
    /// Encryption must be confirmed disabled.
    EncryptionDisabled,
    /// No dependent snapshots may be present.
    NoSnapshots,
    /// Partition must be healthy, online, and writable.
    PartitionHealthyAndOnline,
    /// Source used bytes must be known.
    MigrationSizeKnown,
}

/// An actionable blocker with a recovery recommendation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeBlocker {
    /// Stable blocker identifier.
    pub code: MergeBlockerCode,
    /// Partition responsible for the blocker, when applicable.
    pub partition_id: Option<String>,
    /// Concise reason for the blocked preview.
    pub message: String,
    /// Safe next step; never an instruction to bypass protection.
    pub recovery_suggestion: String,
}

/// Stable blocker identifiers returned by the preview engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeBlockerCode {
    /// Source and target identities are equal.
    SamePartition,
    /// Partitions belong to different physical disks.
    DifferentPhysicalDisk,
    /// Partitions are not in the only supported adjacent order.
    NotAdjacentOrUnsupportedDirection,
    /// Dynamic, Storage Spaces, or unknown layouts are unsupported.
    UnsupportedDiskLayout,
    /// Disk is offline, unhealthy, or has unknown health.
    DiskNotHealthyOrOnline,
    /// Media errors are present or reliability counters are unknown.
    MediaErrorsUnsafe,
    /// EFI, recovery, system, boot, reserved, unallocated, or unknown roles are protected.
    ProtectedPartition,
    /// Stable partition GUID was not available.
    PartitionIdentityUnknown,
    /// File system is not confirmed NTFS.
    UnsupportedFileSystem,
    /// BitLocker is enabled, suspended, or unknown.
    EncryptionNotConfirmedOff,
    /// Shadow copies are present or could not be checked.
    SnapshotStateUnsafe,
    /// Partition is offline, read-only, unhealthy, or unknown.
    PartitionNotHealthyOrOnline,
    /// Source used-space counters were unavailable.
    MigrationSizeUnknown,
}

/// Simulated disk layout after a feasible source-to-target merge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulatedPartitionLayout {
    /// Physical disk identity represented by the simulation.
    pub disk_id: String,
    /// Total disk capacity, unchanged by the simulation.
    pub disk_size_bytes: u64,
    /// Regions after removing the source and expanding the target.
    pub partitions: Vec<SimulatedPartition>,
}

/// One region in a simulated post-merge disk layout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulatedPartition {
    /// Original partition identity, retained for UI diffing.
    pub id: String,
    /// Start offset in bytes.
    pub start_offset_bytes: u64,
    /// Simulated capacity in bytes.
    pub size_bytes: u64,
    /// Structural role copied from the current topology.
    pub kind: PartitionKind,
    /// True only for the target region expanded by the simulation.
    pub is_expanded_target: bool,
}

/// Validation failures that prevent a trustworthy preview result.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PartitionPreviewError {
    /// A request referred to an identity absent from the captured topology.
    #[error("partition identity was not found: {0}")]
    PartitionNotFound(String),
    /// Discovery returned the same identity more than once.
    #[error("partition identity is duplicated: {0}")]
    DuplicatePartitionIdentity(String),
    /// Platform capacity values overflowed while creating a simulation.
    #[error("partition capacity overflowed during simulation")]
    CapacityOverflow,
}

fn check_disk_safety(
    disk: &PhysicalDisk,
    checks: &mut Vec<MergeCheck>,
    blockers: &mut Vec<MergeBlocker>,
) {
    record_check(
        checks,
        blockers,
        MergeCheckCode::SupportedDiskLayout,
        disk.layout_kind == DiskLayoutKind::Basic,
        "物理磁盘使用受支持的基本磁盘布局",
        "动态磁盘、存储空间或未知布局暂不支持预演",
        MergeBlockerCode::UnsupportedDiskLayout,
        None,
    );
    record_check(
        checks,
        blockers,
        MergeCheckCode::DiskHealthyAndOnline,
        disk.health == TopologyHealth::Healthy && !disk.is_offline,
        "物理磁盘在线且健康状态良好",
        "请先处理磁盘离线或健康告警，再刷新拓扑",
        MergeBlockerCode::DiskNotHealthyOrOnline,
        None,
    );
    record_check(
        checks,
        blockers,
        MergeCheckCode::NoMediaErrors,
        disk.media_error_state == MediaErrorState::None,
        "未发现未纠正的介质读写错误",
        "磁盘介质错误计数存在或不可用，分区预演保持阻塞",
        MergeBlockerCode::MediaErrorsUnsafe,
        None,
    );
}

fn check_partition_safety(
    partition: &PartitionDescriptor,
    checks: &mut Vec<MergeCheck>,
    blockers: &mut Vec<MergeBlocker>,
) {
    let identity_and_role_safe = partition.guid.is_some()
        && partition.kind == PartitionKind::Data
        && !partition.is_system
        && !partition.is_boot;
    record_check(
        checks,
        blockers,
        MergeCheckCode::PartitionIdentityAndRole,
        identity_and_role_safe,
        "分区身份稳定且属于普通数据分区",
        "系统、启动、EFI、恢复、保留、未分配或身份未知的区域默认受保护",
        if partition.guid.is_none() {
            MergeBlockerCode::PartitionIdentityUnknown
        } else {
            MergeBlockerCode::ProtectedPartition
        },
        Some(partition.id.clone()),
    );

    let supported_file_system = partition
        .file_system
        .as_deref()
        .is_some_and(|file_system| file_system.eq_ignore_ascii_case("NTFS"));
    record_check(
        checks,
        blockers,
        MergeCheckCode::SupportedFileSystem,
        supported_file_system,
        "文件系统为 NTFS",
        "当前预演仅支持已识别的 NTFS 数据分区",
        MergeBlockerCode::UnsupportedFileSystem,
        Some(partition.id.clone()),
    );

    record_check(
        checks,
        blockers,
        MergeCheckCode::EncryptionDisabled,
        partition.encryption_state == EncryptionState::Off,
        "BitLocker 已确认关闭",
        "请确认 BitLocker 状态；加密开启、暂停或未知时不允许继续",
        MergeBlockerCode::EncryptionNotConfirmedOff,
        Some(partition.id.clone()),
    );

    record_check(
        checks,
        blockers,
        MergeCheckCode::NoSnapshots,
        partition.snapshot_state == SnapshotState::None,
        "未发现依赖此卷的快照",
        "请检查并妥善处理卷影副本；状态未知时保持阻塞",
        MergeBlockerCode::SnapshotStateUnsafe,
        Some(partition.id.clone()),
    );

    let healthy_and_online = partition.health == TopologyHealth::Healthy
        && partition.operational_state == PartitionOperationalState::Online
        && !partition.is_read_only;
    record_check(
        checks,
        blockers,
        MergeCheckCode::PartitionHealthyAndOnline,
        healthy_and_online,
        "分区在线、可写且健康状态良好",
        "请先处理只读、离线、健康告警或未知状态，再刷新拓扑",
        MergeBlockerCode::PartitionNotHealthyOrOnline,
        Some(partition.id.clone()),
    );
}

#[allow(clippy::too_many_arguments)]
fn record_check(
    checks: &mut Vec<MergeCheck>,
    blockers: &mut Vec<MergeBlocker>,
    code: MergeCheckCode,
    passed: bool,
    pass_message: &str,
    failure_message: &str,
    blocker_code: MergeBlockerCode,
    partition_id: Option<String>,
) {
    checks.push(MergeCheck {
        code,
        passed,
        message: if passed {
            pass_message
        } else {
            failure_message
        }
        .to_owned(),
    });
    if !passed {
        blockers.push(MergeBlocker {
            code: blocker_code,
            partition_id,
            message: failure_message.to_owned(),
            recovery_suggestion: recovery_suggestion(blocker_code).to_owned(),
        });
    }
}

fn recovery_suggestion(code: MergeBlockerCode) -> &'static str {
    match code {
        MergeBlockerCode::SamePartition => "重新选择一个源分区和一个目标分区。",
        MergeBlockerCode::DifferentPhysicalDisk => "选择同一物理磁盘上的两个数据分区。",
        MergeBlockerCode::NotAdjacentOrUnsupportedDirection => {
            "选择左侧目标分区及其紧邻右侧的源分区。"
        }
        MergeBlockerCode::UnsupportedDiskLayout => "改用 Windows 对应的专用存储管理工具。",
        MergeBlockerCode::DiskNotHealthyOrOnline => "备份重要数据并先完成磁盘健康检查。",
        MergeBlockerCode::MediaErrorsUnsafe => "备份重要数据并使用厂商工具完成介质健康检查。",
        MergeBlockerCode::ProtectedPartition => "不要尝试合并受保护分区，请选择普通数据分区。",
        MergeBlockerCode::PartitionIdentityUnknown => "刷新拓扑；身份仍不可用时停止操作。",
        MergeBlockerCode::UnsupportedFileSystem => "选择两个已识别的 NTFS 数据分区。",
        MergeBlockerCode::EncryptionNotConfirmedOff => "在 Windows 中核验 BitLocker 状态后刷新。",
        MergeBlockerCode::SnapshotStateUnsafe => "在确认备份策略后核验卷影副本状态。",
        MergeBlockerCode::PartitionNotHealthyOrOnline => "先修复卷状态并确认磁盘健康。",
        MergeBlockerCode::MigrationSizeUnknown => "挂载并检查源卷，然后重新读取拓扑。",
    }
}

fn deduplicate_blockers(blockers: &mut Vec<MergeBlocker>) {
    blockers
        .dedup_by(|left, right| left.code == right.code && left.partition_id == right.partition_id);
}

fn simulate_layout(
    disk: Option<&PhysicalDisk>,
    source: &PartitionDescriptor,
    target: &PartitionDescriptor,
) -> Result<SimulatedPartitionLayout, PartitionPreviewError> {
    let disk = disk.ok_or_else(|| PartitionPreviewError::PartitionNotFound(target.id.clone()))?;
    let expanded_size = target
        .size_bytes
        .checked_add(source.size_bytes)
        .ok_or(PartitionPreviewError::CapacityOverflow)?;
    let partitions = disk
        .partitions
        .iter()
        .filter(|partition| partition.id != source.id)
        .map(|partition| SimulatedPartition {
            id: partition.id.clone(),
            start_offset_bytes: partition.start_offset_bytes,
            size_bytes: if partition.id == target.id {
                expanded_size
            } else {
                partition.size_bytes
            },
            kind: partition.kind,
            is_expanded_target: partition.id == target.id,
        })
        .collect();

    Ok(SimulatedPartitionLayout {
        disk_id: disk.id.clone(),
        disk_size_bytes: disk.size_bytes,
        partitions,
    })
}

fn estimate_duration_seconds(migration_bytes: u64) -> u64 {
    const CONSERVATIVE_BYTES_PER_SECOND: u64 = 100 * 1024 * 1024;
    migration_bytes
        .div_ceil(CONSERVATIVE_BYTES_PER_SECOND)
        .max(u64::from(migration_bytes > 0))
}

fn preview_id(topology: &PartitionTopology, request: &MergePreviewRequest) -> String {
    let mut digest = Sha256::new();
    digest.update(topology.captured_at_unix_ms.to_le_bytes());
    digest.update(request.source_partition_id.as_bytes());
    digest.update([0]);
    digest.update(request.target_partition_id.as_bytes());
    format!("partition-preview-{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_partition(id: &str, disk_id: &str, start: u64, size: u64) -> PartitionDescriptor {
        PartitionDescriptor {
            id: id.to_owned(),
            disk_id: disk_id.to_owned(),
            partition_number: 1,
            guid: Some(format!("guid-{id}")),
            start_offset_bytes: start,
            size_bytes: size,
            file_system: Some("NTFS".to_owned()),
            label: Some(id.to_owned()),
            mount_points: vec![format!("{id}:\\")],
            kind: PartitionKind::Data,
            is_system: false,
            is_boot: false,
            is_read_only: false,
            operational_state: PartitionOperationalState::Online,
            encryption_state: EncryptionState::Off,
            snapshot_state: SnapshotState::None,
            health: TopologyHealth::Healthy,
            used_bytes: Some(20),
            free_bytes: Some(size.saturating_sub(20)),
        }
    }

    fn disk(id: &str, partitions: Vec<PartitionDescriptor>) -> PhysicalDisk {
        PhysicalDisk {
            id: id.to_owned(),
            number: 0,
            friendly_name: "Test SSD".to_owned(),
            bus_type: "NVMe".to_owned(),
            partition_style: "GPT".to_owned(),
            size_bytes: 1_000,
            layout_kind: DiskLayoutKind::Basic,
            health: TopologyHealth::Healthy,
            media_error_state: MediaErrorState::None,
            is_offline: false,
            partitions,
        }
    }

    fn topology(disks: Vec<PhysicalDisk>) -> PartitionTopology {
        PartitionTopology {
            captured_at_unix_ms: 42,
            disks,
            discovery_warnings: vec![],
            read_only: true,
        }
    }

    fn request(source: &str, target: &str) -> MergePreviewRequest {
        MergePreviewRequest {
            source_partition_id: source.to_owned(),
            target_partition_id: target.to_owned(),
        }
    }

    #[test]
    fn previews_adjacent_ntfs_data_partitions_without_authorizing_execution() {
        let target = data_partition("target", "disk-0", 100, 200);
        let source = data_partition("source", "disk-0", 300, 100);
        let preview = topology(vec![disk("disk-0", vec![target, source])])
            .preview_merge(&request("source", "target"))
            .expect("safe topology should produce a preview");

        assert!(preview.feasible);
        assert!(!preview.execution_authorized);
        assert_eq!(preview.migration_bytes, 20);
        let simulated = preview.simulated_layout.expect("layout should exist");
        assert_eq!(simulated.partitions.len(), 1);
        assert_eq!(simulated.partitions[0].size_bytes, 300);
        assert!(simulated.partitions[0].is_expanded_target);
    }

    #[test]
    fn blocks_non_adjacent_and_reverse_direction_requests() {
        let left = data_partition("left", "disk-0", 100, 100);
        let right = data_partition("right", "disk-0", 300, 100);
        let topology = topology(vec![disk("disk-0", vec![left, right])]);

        let preview = topology
            .preview_merge(&request("right", "left"))
            .expect("invalid layout still returns explained preview");
        assert!(!preview.feasible);
        assert!(preview.blockers.iter().any(|blocker| {
            blocker.code == MergeBlockerCode::NotAdjacentOrUnsupportedDirection
        }));
        assert!(preview.simulated_layout.is_none());
    }

    #[test]
    fn blocks_cross_disk_requests() {
        let source = data_partition("source", "disk-1", 100, 100);
        let target = data_partition("target", "disk-0", 100, 100);
        let topology = topology(vec![
            disk("disk-0", vec![target]),
            disk("disk-1", vec![source]),
        ]);

        let preview = topology
            .preview_merge(&request("source", "target"))
            .expect("cross-disk selection should be explained");
        assert!(
            preview
                .blockers
                .iter()
                .any(|blocker| blocker.code == MergeBlockerCode::DifferentPhysicalDisk)
        );
    }

    #[test]
    fn blocks_efi_recovery_and_running_system_partitions() {
        for kind in [
            PartitionKind::EfiSystem,
            PartitionKind::Recovery,
            PartitionKind::System,
        ] {
            let target = data_partition("target", "disk-0", 100, 100);
            let mut source = data_partition("source", "disk-0", 200, 100);
            source.kind = kind;
            source.is_system = kind == PartitionKind::System;
            let preview = topology(vec![disk("disk-0", vec![target, source])])
                .preview_merge(&request("source", "target"))
                .expect("protected selection should be explained");
            assert!(
                preview
                    .blockers
                    .iter()
                    .any(|blocker| blocker.code == MergeBlockerCode::ProtectedPartition)
            );
        }
    }

    #[test]
    fn blocks_bitlocker_dynamic_snapshot_and_unknown_health() {
        let target = data_partition("target", "disk-0", 100, 100);
        let mut source = data_partition("source", "disk-0", 200, 100);
        source.encryption_state = EncryptionState::On;
        source.snapshot_state = SnapshotState::Present;
        source.health = TopologyHealth::Unknown;
        let mut physical = disk("disk-0", vec![target, source]);
        physical.layout_kind = DiskLayoutKind::Dynamic;
        physical.media_error_state = MediaErrorState::Present;
        let preview = topology(vec![physical])
            .preview_merge(&request("source", "target"))
            .expect("unsafe signals should be explained");

        for expected in [
            MergeBlockerCode::UnsupportedDiskLayout,
            MergeBlockerCode::MediaErrorsUnsafe,
            MergeBlockerCode::EncryptionNotConfirmedOff,
            MergeBlockerCode::SnapshotStateUnsafe,
            MergeBlockerCode::PartitionNotHealthyOrOnline,
        ] {
            assert!(
                preview
                    .blockers
                    .iter()
                    .any(|blocker| blocker.code == expected)
            );
        }
    }

    #[test]
    fn rejects_unknown_partition_identity() {
        let topology = topology(vec![disk("disk-0", vec![])]);
        let error = topology
            .preview_merge(&request("missing", "also-missing"))
            .expect_err("unknown identity must fail closed");
        assert_eq!(
            error,
            PartitionPreviewError::PartitionNotFound("missing".to_owned())
        );
    }
}
