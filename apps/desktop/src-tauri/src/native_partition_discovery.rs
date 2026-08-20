//! Native Windows partition-topology provider.
//!
//! This module intentionally covers the read-only, layout-critical fields
//! first. Advanced provider signals such as BitLocker, dynamic-disk state,
//! and reliability counters remain explicit `Unknown` values until their
//! corresponding native APIs are added. The PowerShell provider is retained
//! as a compatibility fallback when a device or IOCTL cannot be read.

use std::{collections::HashMap, mem::size_of, ptr::null_mut};

use windows_sys::Win32::{
    Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW,
        GetLogicalDrives, GetVolumeInformationW, OPEN_EXISTING,
    },
    System::{
        IO::DeviceIoControl,
        Ioctl::{
            DRIVE_LAYOUT_INFORMATION_EX, GET_LENGTH_INFORMATION, IOCTL_DISK_GET_DRIVE_LAYOUT_EX,
            IOCTL_DISK_GET_LENGTH_INFO, IOCTL_STORAGE_GET_DEVICE_NUMBER, PARTITION_INFORMATION_EX,
            PARTITION_STYLE_GPT, PARTITION_STYLE_MBR, PARTITION_STYLE_RAW, STORAGE_DEVICE_NUMBER,
        },
    },
};

use crate::partition_discovery::{
    PartitionDiscoveryError, PowerShellDisk, PowerShellPartition, PowerShellTopologyEnvelope,
};

const MAX_DISK_NUMBER: u32 = 32;
const INITIAL_IOCTL_BUFFER_SIZE: usize = 128 * 1024;
const MAX_IOCTL_BUFFER_SIZE: usize = 16 * 1024 * 1024;
const ERROR_INSUFFICIENT_BUFFER: i32 = 122;

/// Discovers the basic disk layout without starting a shell process.
pub(crate) fn discover() -> Result<PowerShellTopologyEnvelope, PartitionDiscoveryError> {
    let volumes = enumerate_volumes()?;
    let mut disks = Vec::new();
    let mut warnings = vec![
        "原生拓扑已读取；动态磁盘、Storage Spaces、BitLocker 和卷影副本状态将在后续能力中补充。"
            .to_owned(),
    ];

    for number in 0..MAX_DISK_NUMBER {
        let Some(handle) = open_device(&format!(r"\\.\PhysicalDrive{number}")) else {
            continue;
        };
        let layout = query_layout(handle).map_err(|error| {
            PartitionDiscoveryError::NativeProviderFailed(format!("PhysicalDrive{number}: {error}"))
        });
        let size = query_disk_size(handle).ok();
        unsafe { CloseHandle(handle) };

        let layout = layout?;
        let disk_id = format!("disk-number:{number}:native-layout");
        let mut partitions = Vec::new();
        for entry in &layout.entries {
            if entry.PartitionNumber == 0 || entry.PartitionLength <= 0 {
                continue;
            }
            let offset = u64::try_from(entry.StartingOffset).map_err(|_| {
                PartitionDiscoveryError::NativeProviderFailed(
                    "分区起始偏移为负数，原生布局被拒绝。".to_owned(),
                )
            })?;
            let length = u64::try_from(entry.PartitionLength).map_err(|_| {
                PartitionDiscoveryError::NativeProviderFailed(
                    "分区容量为负数，原生布局被拒绝。".to_owned(),
                )
            })?;
            let guid = partition_guid(&entry);
            let id = guid.clone().map_or_else(
                || format!("{disk_id}:offset:{offset}:size:{length}"),
                |value| format!("partition:{value}"),
            );
            let volume = volumes.get(&(number, entry.PartitionNumber));
            partitions.push(PowerShellPartition {
                id,
                disk_id: disk_id.clone(),
                partition_number: entry.PartitionNumber,
                guid,
                offset_bytes: offset,
                size_bytes: length,
                gpt_type: gpt_type(&entry),
                mbr_type: mbr_type(&entry),
                file_system: volume.and_then(|value| value.file_system.clone()),
                label: volume.and_then(|value| value.label.clone()),
                mount_points: volume
                    .map(|value| value.mount_points.clone())
                    .unwrap_or_default(),
                is_system: false,
                is_boot: mbr_boot_indicator(&entry),
                is_read_only: false,
                is_offline: false,
                encryption_state: "unknown".to_owned(),
                snapshot_state: "unknown".to_owned(),
                health: "unknown".to_owned(),
                used_bytes: volume.and_then(|value| value.used_bytes),
                free_bytes: volume.and_then(|value| value.free_bytes),
            });
        }

        let disk_size = size.unwrap_or_else(|| {
            partitions
                .iter()
                .filter_map(|partition| partition.offset_bytes.checked_add(partition.size_bytes))
                .max()
                .unwrap_or_default()
        });
        if disk_size == 0 {
            warnings.push(format!("磁盘 {number} 的容量不可用，已保持阻塞。"));
        }
        disks.push(PowerShellDisk {
            id: disk_id,
            number,
            friendly_name: format!("物理磁盘 {number}"),
            bus_type: "Unknown".to_owned(),
            partition_style: partition_style(layout.partition_style).to_owned(),
            size_bytes: disk_size,
            layout_kind: "basic".to_owned(),
            health: "unknown".to_owned(),
            media_error_state: "unknown".to_owned(),
            is_offline: false,
            partitions,
        });
    }

    if disks.is_empty() {
        return Err(PartitionDiscoveryError::NativeProviderFailed(
            "没有可读取的物理磁盘布局。".to_owned(),
        ));
    }
    Ok(PowerShellTopologyEnvelope { disks, warnings })
}

#[derive(Clone, Debug, Default)]
struct NativeVolume {
    file_system: Option<String>,
    label: Option<String>,
    mount_points: Vec<String>,
    used_bytes: Option<u64>,
    free_bytes: Option<u64>,
}

fn enumerate_volumes() -> Result<HashMap<(u32, u32), NativeVolume>, PartitionDiscoveryError> {
    let mask = unsafe { GetLogicalDrives() };
    if mask == 0 {
        return Err(PartitionDiscoveryError::NativeProviderFailed(
            "GetLogicalDrives 返回空结果。".to_owned(),
        ));
    }
    let mut volumes = HashMap::new();
    for index in 0..26_u32 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let letter = char::from_u32(u32::from(b'A') + index).unwrap_or('C');
        let root = format!(r"{letter}:\");
        let device_path = format!(r"\\.\{letter}:");
        let Some(handle) = open_device(&device_path) else {
            continue;
        };
        let device_number = query_volume_device_number(handle);
        unsafe { CloseHandle(handle) };
        let Some((disk_number, partition_number)) = device_number else {
            continue;
        };
        let info = read_volume_info(&root);
        volumes
            .entry((disk_number, partition_number))
            .and_modify(|current: &mut NativeVolume| {
                current.mount_points.push(root.clone());
                if current.file_system.is_none() {
                    current.file_system = info.file_system.clone();
                }
                if current.label.is_none() {
                    current.label = info.label.clone();
                }
                if current.used_bytes.is_none() {
                    current.used_bytes = info.used_bytes;
                    current.free_bytes = info.free_bytes;
                }
            })
            .or_insert_with(|| NativeVolume {
                mount_points: vec![root],
                ..info
            });
    }
    Ok(volumes)
}

fn read_volume_info(root: &str) -> NativeVolume {
    let root_wide = wide(root);
    let mut label = [0_u16; 260];
    let mut file_system = [0_u16; 64];
    let mut serial = 0_u32;
    let mut max_component = 0_u32;
    let mut flags = 0_u32;
    let volume_ok = unsafe {
        GetVolumeInformationW(
            root_wide.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            &mut max_component,
            &mut flags,
            file_system.as_mut_ptr(),
            file_system.len() as u32,
        ) != 0
    };
    let mut available = 0_u64;
    let mut total = 0_u64;
    let mut free = 0_u64;
    let space_ok = unsafe {
        GetDiskFreeSpaceExW(root_wide.as_ptr(), &mut available, &mut total, &mut free) != 0
    };
    NativeVolume {
        file_system: volume_ok.then(|| from_wide(&file_system)),
        label: volume_ok.then(|| from_wide(&label)),
        mount_points: Vec::new(),
        used_bytes: space_ok.then(|| total.saturating_sub(free)),
        free_bytes: space_ok.then_some(free),
    }
}

fn query_volume_device_number(handle: HANDLE) -> Option<(u32, u32)> {
    let mut number = STORAGE_DEVICE_NUMBER::default();
    let mut returned = 0_u32;
    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_GET_DEVICE_NUMBER,
            std::ptr::null(),
            0,
            (&mut number as *mut STORAGE_DEVICE_NUMBER).cast(),
            size_of::<STORAGE_DEVICE_NUMBER>() as u32,
            &mut returned,
            null_mut(),
        ) != 0
    };
    success.then_some((number.DeviceNumber, number.PartitionNumber))
}

struct NativeLayout {
    partition_style: u32,
    entries: Vec<PARTITION_INFORMATION_EX>,
}

fn query_layout(handle: HANDLE) -> Result<NativeLayout, std::io::Error> {
    let mut buffer = vec![0_u8; INITIAL_IOCTL_BUFFER_SIZE];
    loop {
        let mut returned = 0_u32;
        let success = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_DRIVE_LAYOUT_EX,
                std::ptr::null(),
                0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut returned,
                null_mut(),
            ) != 0
        };
        if success {
            let header_size = size_of::<DRIVE_LAYOUT_INFORMATION_EX>()
                .checked_sub(size_of::<PARTITION_INFORMATION_EX>())
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "layout header overflow")
                })?;
            let header_end = header_size;
            if returned < header_end as u32 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "layout response was truncated",
                ));
            }
            let header: DRIVE_LAYOUT_INFORMATION_EX =
                unsafe { std::ptr::read_unaligned(buffer.as_ptr().cast()) };
            let count = usize::try_from(header.PartitionCount).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid partition count")
            })?;
            if count > 256 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "partition count exceeds safety limit",
                ));
            }
            let entries_size = count
                .checked_mul(size_of::<PARTITION_INFORMATION_EX>())
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "layout entries overflow")
                })?;
            let required = header_end.checked_add(entries_size).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "layout response overflow")
            })?;
            if returned < required as u32 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "layout entries were truncated",
                ));
            }
            let mut entries = Vec::with_capacity(count);
            for index in 0..count {
                let offset = header_end + index * size_of::<PARTITION_INFORMATION_EX>();
                entries
                    .push(unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(offset).cast()) });
            }
            return Ok(NativeLayout {
                partition_style: header.PartitionStyle,
                entries,
            });
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INSUFFICIENT_BUFFER)
            && buffer.len() < MAX_IOCTL_BUFFER_SIZE
        {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        return Err(error);
    }
}

fn query_disk_size(handle: HANDLE) -> Result<u64, std::io::Error> {
    let mut length = GET_LENGTH_INFORMATION::default();
    let mut returned = 0_u32;
    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            std::ptr::null(),
            0,
            (&mut length as *mut GET_LENGTH_INFORMATION).cast(),
            size_of::<GET_LENGTH_INFORMATION>() as u32,
            &mut returned,
            null_mut(),
        ) != 0
    };
    if success && length.Length >= 0 {
        Ok(length.Length as u64)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn open_device(path: &str) -> Option<HANDLE> {
    let path_wide = wide(path);
    let handle = unsafe {
        CreateFileW(
            path_wide.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            0,
        )
    };
    (handle != INVALID_HANDLE_VALUE).then_some(handle)
}

fn partition_style(style: u32) -> &'static str {
    match style {
        value if value == PARTITION_STYLE_GPT as u32 => "GPT",
        value if value == PARTITION_STYLE_MBR as u32 => "MBR",
        value if value == PARTITION_STYLE_RAW as u32 => "RAW",
        _ => "Unknown",
    }
}

fn partition_guid(entry: &PARTITION_INFORMATION_EX) -> Option<String> {
    if entry.PartitionStyle != PARTITION_STYLE_GPT {
        return None;
    }
    let guid = unsafe { entry.Anonymous.Gpt.PartitionId };
    (guid.data1 != 0 || guid.data2 != 0 || guid.data3 != 0 || guid.data4 != [0; 8]).then(|| {
        format!(
            "{{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
            guid.data1,
            guid.data2,
            guid.data3,
            guid.data4[0],
            guid.data4[1],
            guid.data4[2],
            guid.data4[3],
            guid.data4[4],
            guid.data4[5],
            guid.data4[6],
            guid.data4[7]
        )
    })
}

fn gpt_type(entry: &PARTITION_INFORMATION_EX) -> Option<String> {
    if entry.PartitionStyle != PARTITION_STYLE_GPT {
        return None;
    }
    let guid = unsafe { entry.Anonymous.Gpt.PartitionType };
    Some(format!(
        "{{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    ))
}

fn mbr_type(entry: &PARTITION_INFORMATION_EX) -> Option<String> {
    (entry.PartitionStyle == PARTITION_STYLE_MBR)
        .then(|| unsafe { entry.Anonymous.Mbr.PartitionType.to_string() })
}

fn mbr_boot_indicator(entry: &PARTITION_INFORMATION_EX) -> bool {
    entry.PartitionStyle == PARTITION_STYLE_MBR && unsafe { entry.Anonymous.Mbr.BootIndicator }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|item| *item == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}
