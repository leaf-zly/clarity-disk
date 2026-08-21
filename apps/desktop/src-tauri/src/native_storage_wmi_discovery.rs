//! In-process Windows Storage and CIM evidence for partition discovery.
//!
//! WMI is accessed through COM in the current process. No command interpreter,
//! script host, or child process is started. Each query is independent so one
//! unavailable provider leaves only its own fields unknown.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use wmi::WMIConnection;

/// Read-only storage evidence keyed by stable Windows disk identities.
#[derive(Debug, Default)]
pub(crate) struct StorageEvidence {
    /// Physical-disk provider properties keyed by Windows disk number.
    pub(crate) disks: HashMap<u32, DiskEvidence>,
    /// Partition provider properties keyed by disk and partition number.
    pub(crate) partitions: HashMap<(u32, u32), PartitionEvidence>,
    /// Mounted-volume health keyed by uppercase drive letter.
    pub(crate) volume_health: HashMap<char, String>,
    /// Legacy dynamic disks, when the CIM query completed.
    pub(crate) dynamic_disks: Option<HashSet<u32>>,
    /// Whether any VSS snapshot exists, when the CIM query completed.
    pub(crate) shadow_copy_present: Option<bool>,
    /// Non-fatal query failures kept visible to callers.
    pub(crate) warnings: Vec<String>,
}

/// Windows Storage provider fields for one physical disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DiskEvidence {
    /// Provider-friendly device name.
    pub(crate) friendly_name: Option<String>,
    /// Normalized storage bus name.
    pub(crate) bus_type: Option<String>,
    /// Normalized provider health.
    pub(crate) health: Option<String>,
    /// Explicit offline state.
    pub(crate) is_offline: Option<bool>,
    /// Explicit read-only state inherited by its partitions.
    pub(crate) is_read_only: Option<bool>,
    /// Storage Spaces virtual-disk identity derived from the bus type.
    pub(crate) is_storage_spaces: bool,
}

/// Windows Storage provider fields for one partition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PartitionEvidence {
    /// Whether Windows identifies the partition as the running system volume.
    pub(crate) is_system: Option<bool>,
    /// Whether Windows identifies the partition as a boot partition.
    pub(crate) is_boot: Option<bool>,
    /// Explicit partition-level read-only state.
    pub(crate) is_read_only: Option<bool>,
    /// Explicit partition-level offline state.
    pub(crate) is_offline: Option<bool>,
}

/// Queries Windows Storage and CIM providers without starting a child process.
pub(crate) fn discover() -> StorageEvidence {
    let mut evidence = StorageEvidence::default();
    match WMIConnection::with_namespace_path("ROOT\\Microsoft\\Windows\\Storage") {
        Ok(connection) => discover_storage(&connection, &mut evidence),
        Err(error) => evidence.warnings.push(format!(
            "Windows Storage WMI 提供程序不可用，磁盘管理状态保持未知：{error}"
        )),
    }
    match WMIConnection::new() {
        Ok(connection) => discover_cim(&connection, &mut evidence),
        Err(error) => evidence.warnings.push(format!(
            "Windows CIM 提供程序不可用，动态磁盘和卷影副本状态保持未知：{error}"
        )),
    }
    evidence
}

fn discover_storage(connection: &WMIConnection, evidence: &mut StorageEvidence) {
    let disks: wmi::WMIResult<Vec<StorageDiskRow>> = connection.raw_query(
        "SELECT Number,FriendlyName,BusType,HealthStatus,IsOffline,IsReadOnly FROM MSFT_Disk",
    );
    match disks {
        Ok(rows) => {
            for row in rows {
                evidence.disks.insert(
                    row.number,
                    DiskEvidence {
                        friendly_name: row.friendly_name.filter(|value| !value.trim().is_empty()),
                        bus_type: row.bus_type.map(bus_type_name),
                        health: row.health_status.and_then(health_name),
                        is_offline: row.is_offline,
                        is_read_only: row.is_read_only,
                        is_storage_spaces: row.bus_type == Some(16),
                    },
                );
            }
        }
        Err(error) => evidence
            .warnings
            .push(format!("MSFT_Disk 查询失败，磁盘管理状态保持未知：{error}")),
    }

    let partitions: wmi::WMIResult<Vec<StoragePartitionRow>> = connection.raw_query(
        "SELECT DiskNumber,PartitionNumber,IsSystem,IsBoot,IsReadOnly,IsOffline FROM MSFT_Partition",
    );
    match partitions {
        Ok(rows) => {
            for row in rows {
                evidence.partitions.insert(
                    (row.disk_number, row.partition_number),
                    PartitionEvidence {
                        is_system: row.is_system,
                        is_boot: row.is_boot,
                        is_read_only: row.is_read_only,
                        is_offline: row.is_offline,
                    },
                );
            }
        }
        Err(error) => evidence.warnings.push(format!(
            "MSFT_Partition 查询失败，分区管理状态保持未知：{error}"
        )),
    }

    let volumes: wmi::WMIResult<Vec<StorageVolumeRow>> = connection.raw_query(
        "SELECT DriveLetter,HealthStatus FROM MSFT_Volume WHERE DriveLetter IS NOT NULL",
    );
    match volumes {
        Ok(rows) => {
            for row in rows {
                let Some(letter) = drive_letter(row.drive_letter.as_deref()) else {
                    continue;
                };
                if let Some(health) = row.health_status.and_then(health_name) {
                    evidence.volume_health.insert(letter, health);
                }
            }
        }
        Err(error) => evidence
            .warnings
            .push(format!("MSFT_Volume 查询失败，卷健康状态保持未知：{error}")),
    }
}

fn discover_cim(connection: &WMIConnection, evidence: &mut StorageEvidence) {
    let dynamic: wmi::WMIResult<Vec<DynamicPartitionRow>> = connection.raw_query(
        "SELECT DiskIndex FROM Win32_DiskPartition WHERE Type LIKE '%Logical Disk Manager%'",
    );
    match dynamic {
        Ok(rows) => {
            evidence.dynamic_disks = Some(rows.into_iter().map(|row| row.disk_index).collect());
        }
        Err(error) => evidence
            .warnings
            .push(format!("动态磁盘查询失败，布局类型保持未知：{error}")),
    }

    let shadows: wmi::WMIResult<Vec<ShadowCopyRow>> =
        connection.raw_query("SELECT ID FROM Win32_ShadowCopy");
    match shadows {
        Ok(rows) => {
            // The current safety model is deliberately global: an unmapped
            // snapshot blocks every merge rather than risking broken VSS data.
            evidence.shadow_copy_present = Some(!rows.is_empty());
        }
        Err(error) => evidence
            .warnings
            .push(format!("卷影副本查询失败，快照状态保持未知：{error}")),
    }
}

fn drive_letter(value: Option<&str>) -> Option<char> {
    value?
        .trim()
        .chars()
        .next()
        .filter(char::is_ascii_alphabetic)
        .map(|letter| letter.to_ascii_uppercase())
}

fn health_name(value: u16) -> Option<String> {
    match value {
        0 => Some("healthy".to_owned()),
        1 => Some("warning".to_owned()),
        2 => Some("unhealthy".to_owned()),
        _ => None,
    }
}

fn bus_type_name(value: u16) -> String {
    match value {
        1 => "SCSI",
        2 => "ATAPI",
        3 => "ATA",
        4 => "IEEE 1394",
        6 => "Fibre Channel",
        7 => "USB",
        8 => "RAID",
        9 => "iSCSI",
        10 => "SAS",
        11 => "SATA",
        12 => "SD",
        13 => "MMC",
        14 => "Virtual",
        15 => "File-backed Virtual",
        16 => "Storage Spaces",
        17 => "NVMe",
        18 => "SCM",
        19 => "UFS",
        20 => "NVMe-oF",
        _ => "Unknown",
    }
    .to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StorageDiskRow {
    number: u32,
    friendly_name: Option<String>,
    bus_type: Option<u16>,
    health_status: Option<u16>,
    is_offline: Option<bool>,
    is_read_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StoragePartitionRow {
    disk_number: u32,
    partition_number: u32,
    is_system: Option<bool>,
    is_boot: Option<bool>,
    is_read_only: Option<bool>,
    is_offline: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StorageVolumeRow {
    drive_letter: Option<String>,
    health_status: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DynamicPartitionRow {
    disk_index: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ShadowCopyRow {
    #[serde(rename = "ID")]
    _id: String,
}

#[cfg(test)]
mod tests {
    use super::{bus_type_name, drive_letter, health_name};

    #[test]
    fn normalizes_provider_enums() {
        assert_eq!(bus_type_name(17), "NVMe");
        assert_eq!(bus_type_name(16), "Storage Spaces");
        assert_eq!(health_name(0).as_deref(), Some("healthy"));
        assert_eq!(health_name(5), None);
    }

    #[test]
    fn normalizes_drive_letters() {
        assert_eq!(drive_letter(Some("c")), Some('C'));
        assert_eq!(drive_letter(Some(" D: ")), Some('D'));
        assert_eq!(drive_letter(Some("")), None);
        assert_eq!(drive_letter(None), None);
    }
}
