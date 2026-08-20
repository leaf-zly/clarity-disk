//! Read-only physical-disk identity discovery through native Windows IOCTLs.
//!
//! The provider intentionally reports only evidence that can be read safely
//! without a shell process. Reliability counters and encryption state remain
//! unknown until their dedicated native providers are implemented. This keeps
//! the health model fail-closed while removing PowerShell from the normal path.

#![allow(unsafe_code)]

use std::{mem::size_of, ptr::null_mut};

use windows_sys::Win32::{
    Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{
        BusType1394 as BUS_TYPE_1394, BusTypeAta as BUS_TYPE_ATA, BusTypeAtapi as BUS_TYPE_ATAPI,
        BusTypeFibre as BUS_TYPE_FIBRE, BusTypeFileBackedVirtual as BUS_TYPE_FILE_BACKED_VIRTUAL,
        BusTypeMmc as BUS_TYPE_MMC, BusTypeNvme as BUS_TYPE_NVME, BusTypeRAID as BUS_TYPE_RAID,
        BusTypeSCM as BUS_TYPE_SCM, BusTypeSas as BUS_TYPE_SAS, BusTypeSata as BUS_TYPE_SATA,
        BusTypeScsi as BUS_TYPE_SCSI, BusTypeSd as BUS_TYPE_SD, BusTypeSpaces as BUS_TYPE_SPACES,
        BusTypeSsa as BUS_TYPE_SSA, BusTypeUfs as BUS_TYPE_UFS, BusTypeUsb as BUS_TYPE_USB,
        BusTypeVirtual as BUS_TYPE_VIRTUAL, BusTypeiScsi as BUS_TYPE_ISCSI, CreateFileW,
        FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    },
    System::{
        IO::DeviceIoControl,
        Ioctl::{
            GET_LENGTH_INFORMATION, IOCTL_DISK_GET_LENGTH_INFO, IOCTL_STORAGE_QUERY_PROPERTY,
            PropertyStandardQuery, STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR, STORAGE_DEVICE_DESCRIPTOR,
            STORAGE_PROPERTY_QUERY, STORAGE_TEMPERATURE_DATA_DESCRIPTOR,
            StorageAccessAlignmentProperty, StorageDeviceProperty,
            StorageDeviceTemperatureProperty,
        },
    },
};

const MAX_DISK_NUMBER: u32 = 32;
const DESCRIPTOR_BUFFER_SIZE: usize = 4096;

/// A privacy-preserving physical-disk record read from native storage APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeDiskHealth {
    /// Windows disk number.
    pub(crate) number: u32,
    /// Stable identity shared with native partition discovery.
    pub(crate) id: String,
    /// Friendly name derived from the hardware model when available.
    pub(crate) friendly_name: String,
    /// Vendor string from the storage device descriptor.
    pub(crate) manufacturer: Option<String>,
    /// Product/model string from the storage device descriptor.
    pub(crate) model: Option<String>,
    /// Firmware revision from the storage device descriptor.
    pub(crate) firmware_version: Option<String>,
    /// Last four serial characters only; the full serial never leaves parsing.
    pub(crate) serial_suffix: Option<String>,
    /// Windows storage bus label.
    pub(crate) bus_type: String,
    /// Conservative media classification based on removable-media evidence.
    pub(crate) media_type: String,
    /// Physical capacity in bytes.
    pub(crate) size_bytes: u64,
    /// Logical sector size in bytes, when reported by Windows.
    pub(crate) logical_sector_bytes: Option<u32>,
    /// Physical sector size in bytes, when reported by Windows.
    pub(crate) physical_sector_bytes: Option<u32>,
    /// Current device temperature in Celsius, when reported by Windows.
    pub(crate) temperature_celsius: Option<i16>,
}

/// Discovers physical disks using `CreateFileW` and storage IOCTLs.
///
/// # Errors
///
/// Returns an error when no physical disk can be opened or a mandatory disk
/// capacity query fails for every candidate.
pub(crate) fn discover() -> Result<Vec<NativeDiskHealth>, std::io::Error> {
    let mut disks = Vec::new();
    for number in 0..MAX_DISK_NUMBER {
        let Some(handle) = open_device(number) else {
            continue;
        };
        let size = query_disk_size(handle);
        let descriptor = query_descriptor(handle).ok();
        let alignment = query_alignment(handle).ok();
        let temperature = query_temperature(handle).ok().flatten();
        unsafe { CloseHandle(handle) };
        let size_bytes = match size {
            Ok(value) if value > 0 => value,
            Ok(_) | Err(_) => continue,
        };
        disks.push(build_disk(
            number,
            size_bytes,
            descriptor,
            alignment,
            temperature,
        ));
    }
    if disks.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no physical disks were available through native storage IOCTLs",
        ));
    }
    Ok(disks)
}

#[derive(Clone, Debug)]
struct NativeDescriptor {
    vendor: Option<String>,
    product: Option<String>,
    revision: Option<String>,
    serial_suffix: Option<String>,
    bus_type: String,
    removable: bool,
}

fn build_disk(
    number: u32,
    size_bytes: u64,
    descriptor: Option<NativeDescriptor>,
    alignment: Option<(u32, u32)>,
    temperature_celsius: Option<i16>,
) -> NativeDiskHealth {
    let descriptor = descriptor.unwrap_or_else(|| NativeDescriptor {
        vendor: None,
        product: None,
        revision: None,
        serial_suffix: None,
        bus_type: "Unknown".to_owned(),
        removable: false,
    });
    let model = descriptor.product.filter(|value| !value.is_empty());
    let friendly_name = model
        .clone()
        .unwrap_or_else(|| format!("Physical Disk {number}"));
    NativeDiskHealth {
        number,
        id: format!("disk-number:{number}:native-layout"),
        friendly_name,
        manufacturer: descriptor.vendor,
        model,
        firmware_version: descriptor.revision,
        serial_suffix: descriptor.serial_suffix,
        bus_type: descriptor.bus_type,
        media_type: if descriptor.removable {
            "Removable".to_owned()
        } else {
            "Unknown".to_owned()
        },
        size_bytes,
        logical_sector_bytes: alignment.map(|value| value.0),
        physical_sector_bytes: alignment.map(|value| value.1),
        temperature_celsius,
    }
}

fn open_device(number: u32) -> Option<HANDLE> {
    let path = wide(&format!(r"\\.\PhysicalDrive{number}"));
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    (handle != INVALID_HANDLE_VALUE).then_some(handle)
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
            (&raw mut length).cast(),
            u32::try_from(size_of::<GET_LENGTH_INFORMATION>())
                .expect("disk length structure size fits u32"),
            &raw mut returned,
            null_mut(),
        ) != 0
    };
    if success && length.Length > 0 {
        Ok(length.Length.cast_unsigned())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn query_descriptor(handle: HANDLE) -> Result<NativeDescriptor, std::io::Error> {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        ..Default::default()
    };
    let mut buffer = vec![0_u8; DESCRIPTOR_BUFFER_SIZE];
    let mut returned = 0_u32;
    let success = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&raw const query).cast(),
            u32::try_from(size_of::<STORAGE_PROPERTY_QUERY>())
                .expect("storage property query size fits u32"),
            buffer.as_mut_ptr().cast(),
            u32::try_from(buffer.len()).expect("descriptor buffer length fits u32"),
            &raw mut returned,
            null_mut(),
        ) != 0
    };
    if !success {
        return Err(std::io::Error::last_os_error());
    }
    let header_size = size_of::<STORAGE_DEVICE_DESCRIPTOR>()
        .checked_sub(1)
        .expect("storage descriptor has a flexible trailing field");
    let returned = usize::try_from(returned).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "storage descriptor response length is invalid",
        )
    })?;
    if returned < header_size || returned > buffer.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "storage descriptor response is truncated",
        ));
    }
    let descriptor: STORAGE_DEVICE_DESCRIPTOR =
        unsafe { std::ptr::read_unaligned(buffer.as_ptr().cast()) };
    Ok(NativeDescriptor {
        vendor: descriptor_string(&buffer, returned, descriptor.VendorIdOffset),
        product: descriptor_string(&buffer, returned, descriptor.ProductIdOffset),
        revision: descriptor_string(&buffer, returned, descriptor.ProductRevisionOffset),
        serial_suffix: descriptor_string(&buffer, returned, descriptor.SerialNumberOffset)
            .map(|value| serial_suffix(&value)),
        bus_type: bus_type_name(descriptor.BusType),
        removable: descriptor.RemovableMedia,
    })
}

fn query_alignment(handle: HANDLE) -> Result<(u32, u32), std::io::Error> {
    let mut alignment = STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR::default();
    if query_fixed_property(handle, StorageAccessAlignmentProperty, &mut alignment) {
        if alignment.BytesPerLogicalSector > 0 && alignment.BytesPerPhysicalSector > 0 {
            return Ok((
                alignment.BytesPerLogicalSector,
                alignment.BytesPerPhysicalSector,
            ));
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "storage alignment descriptor is unavailable",
    ))
}

fn query_temperature(handle: HANDLE) -> Result<Option<i16>, std::io::Error> {
    let mut temperature = STORAGE_TEMPERATURE_DATA_DESCRIPTOR::default();
    if !query_fixed_property(handle, StorageDeviceTemperatureProperty, &mut temperature) {
        return Err(std::io::Error::last_os_error());
    }
    let value = temperature.TemperatureInfo[0].Temperature;
    Ok((-50..=150).contains(&value).then_some(value))
}

fn query_fixed_property<T>(handle: HANDLE, property: i32, output: &mut T) -> bool {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: property,
        QueryType: PropertyStandardQuery,
        ..Default::default()
    };
    let mut returned = 0_u32;
    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&raw const query).cast(),
            u32::try_from(size_of::<STORAGE_PROPERTY_QUERY>())
                .expect("storage property query size fits u32"),
            std::ptr::addr_of_mut!(*output).cast(),
            u32::try_from(size_of::<T>()).expect("storage property output size fits u32"),
            &raw mut returned,
            null_mut(),
        ) != 0
    }
}

fn descriptor_string(buffer: &[u8], returned: usize, offset: u32) -> Option<String> {
    let start = usize::try_from(offset).ok()?;
    if start == 0 || start >= returned {
        return None;
    }
    let end = buffer[start..returned]
        .iter()
        .position(|byte| *byte == 0)
        .map_or(returned, |index| start + index);
    let value = String::from_utf8_lossy(&buffer[start..end])
        .trim()
        .to_owned();
    (!value.is_empty()).then_some(value)
}

fn serial_suffix(value: &str) -> String {
    value
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn bus_type_name(value: i32) -> String {
    match value {
        BUS_TYPE_1394 => "IEEE 1394",
        BUS_TYPE_ATA => "ATA",
        BUS_TYPE_ATAPI => "ATAPI",
        BUS_TYPE_FIBRE => "Fibre Channel",
        BUS_TYPE_FILE_BACKED_VIRTUAL => "File-backed Virtual",
        BUS_TYPE_MMC => "MMC",
        BUS_TYPE_NVME => "NVMe",
        BUS_TYPE_RAID => "RAID",
        BUS_TYPE_SCM => "SCM",
        BUS_TYPE_SAS => "SAS",
        BUS_TYPE_SATA => "SATA",
        BUS_TYPE_SCSI => "SCSI",
        BUS_TYPE_SD => "SD",
        BUS_TYPE_SPACES => "Storage Spaces",
        BUS_TYPE_SSA => "SSA",
        BUS_TYPE_UFS => "UFS",
        BUS_TYPE_USB => "USB",
        BUS_TYPE_VIRTUAL => "Virtual",
        BUS_TYPE_ISCSI => "iSCSI",
        _ => "Unknown",
    }
    .to_owned()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_suffix_never_exposes_more_than_four_characters() {
        assert_eq!(serial_suffix("ABCD1234"), "1234");
        assert_eq!(serial_suffix("XY"), "XY");
    }

    #[test]
    fn descriptor_string_respects_returned_length() {
        let mut buffer = vec![0_u8; 16];
        buffer[4..9].copy_from_slice(b"model");
        buffer[9] = 0;
        assert_eq!(descriptor_string(&buffer, 10, 4).as_deref(), Some("model"));
        assert!(descriptor_string(&buffer, 8, 4).is_some());
        assert!(descriptor_string(&buffer, 10, 15).is_none());
    }

    #[test]
    fn unknown_bus_types_fail_closed() {
        assert_eq!(bus_type_name(-1), "Unknown");
    }
}
