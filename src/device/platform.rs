use crate::error::{PurgeError, Result};
use super::Device;
use std::fs::{File, OpenOptions};

/// 列出所有可移动存储设备
pub fn list_removable_devices() -> Result<Vec<Device>> {
    #[cfg(target_os = "macos")]
    { macos::list_devices() }

    #[cfg(target_os = "windows")]
    { windows::list_devices() }

    #[cfg(target_os = "linux")]
    { linux::list_devices() }
}

/// 卸载设备上的所有卷
pub fn unmount_device(path: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        // macOS: 将 /dev/rdisk4 转换为 disk4 用于 diskutil
        let disk_id = path.trim_start_matches("/dev/r").trim_start_matches("/dev/");
        let output = std::process::Command::new("diskutil")
            .args(["unmountDisk", "force", disk_id])
            .output()
            .map_err(|e| PurgeError::DeviceInfoError(format!("执行 unmountDisk 失败: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            // "Unmount of individual volumes failed" 在 force 模式下不一定致命
            if !stderr.contains("failed") && !stdout.contains("failed") {
                return Err(PurgeError::DeviceInfoError(format!(
                    "卸载设备失败: {}{}", stdout, stderr
                )));
            }
        }
        log::info!("设备 {} 已卸载", disk_id);
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: 卸载所有分区
        let disk_name = path.trim_start_matches("/dev/");
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                if line.starts_with(&format!("/dev/{}", disk_name)) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(mount_point) = parts.get(1) {
                        let _ = std::process::Command::new("umount")
                            .arg(mount_point)
                            .output();
                    }
                }
            }
        }
    }

    // Windows 不需要显式卸载

    Ok(())
}

/// 擦写完成后重新格式化设备（GPT + ExFAT）
pub fn reformat_device(path: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let disk_id = path.trim_start_matches("/dev/r").trim_start_matches("/dev/");
        // 使用 diskutil 重新分区为 GPT + ExFAT
        let output = std::process::Command::new("diskutil")
            .args([
                "partitionDisk",
                disk_id,
                "GPT",
                "ExFAT",
                "PurgeDisk",
                "R",
            ])
            .output()
            .map_err(|e| PurgeError::DeviceInfoError(format!("执行 partitionDisk 失败: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(PurgeError::DeviceInfoError(format!(
                "重新格式化失败: {}{}",
                stdout, stderr
            )));
        }
        log::info!("设备 {} 已重新格式化为 ExFAT", disk_id);
    }

    #[cfg(target_os = "linux")]
    {
        let disk_path = path.trim_start_matches("/dev/r"); // macOS raw device prefix
        let disk_path = if path.starts_with("/dev/r") {
            &path[3..] // /dev/rdisk4 -> /dev/disk4 (shouldn't happen on Linux, but be safe)
        } else {
            path
        };
        // 使用 wipefs 清除残留签名，然后用 parted 创建新分区
        let _ = std::process::Command::new("wipefs").args(["-a", disk_path]).output();
        let _ = std::process::Command::new("parted")
            .args(["-s", disk_path, "mklabel", "gpt", "mkpart", "primary", "exfat", "0%", "100%"])
            .output();
        // 等待内核识别分区
        std::thread::sleep(std::time::Duration::from_secs(1));
        // 格式化为 exFAT（需要 exfatprogs）
        let partition = format!("{}1", disk_path);
        let _ = std::process::Command::new("mkfs.exfat")
            .args(["-n", "PurgeDisk", &partition])
            .output();
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: 使用 diskpart 或 PowerShell 格式化
        // 这里简化处理，输出提示让用户手动格式化
        log::warn!("Windows 平台请手动在磁盘管理中初始化并格式化设备");
    }

    Ok(())
}

/// 打开原始设备用于擦写（自动卸载）
pub fn open_raw_device(path: &str) -> Result<File> {
    let devices = list_removable_devices()?;
    if !devices.iter().any(|d| d.path == path) {
        return Err(PurgeError::DeviceNotFound(format!(
            "设备 {} 未在可移动设备列表中找到", path
        )));
    }

    let device = devices.iter().find(|d| d.path == path).unwrap();
    if device.is_system {
        return Err(PurgeError::SystemDiskForbidden(path.to_string()));
    }

    // 先卸载设备
    unmount_device(path)?;

    open_raw(path)
}

fn open_raw(path: &str) -> Result<File> {
    #[cfg(target_os = "macos")]
    { macos::open_raw(path) }

    #[cfg(target_os = "windows")]
    { windows::open_raw(path) }

    #[cfg(target_os = "linux")]
    { linux::open_raw(path) }
}

// ============================================================
// macOS 实现
// ============================================================
#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::process::Command;

    pub fn list_devices() -> Result<Vec<Device>> {
        // 使用 diskutil list external physical 获取外部物理磁盘（不含分区）
        let output = Command::new("diskutil")
            .args(["list", "external", "physical"])
            .output()
            .map_err(|e| PurgeError::DeviceInfoError(format!("执行 diskutil 失败: {}", e)))?;

        if !output.status.success() {
            // fallback: 使用 diskutil list -plist
            return list_devices_fallback();
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let disk_ids = extract_whole_disk_ids_from_text(&text);

        if disk_ids.is_empty() {
            return list_devices_fallback();
        }

        let mut devices = Vec::new();
        for disk_id in disk_ids {
            let info_output = Command::new("diskutil")
                .args(["info", "-plist", &disk_id])
                .output();

            match info_output {
                Ok(output) if output.status.success() => {
                    let info_xml = String::from_utf8_lossy(&output.stdout);
                    if let Some(device) = parse_single_disk(&disk_id, &info_xml) {
                        if device.removable && !device.is_system {
                            devices.push(device);
                        }
                    }
                }
                _ => continue,
            }
        }

        Ok(devices)
    }

    fn list_devices_fallback() -> Result<Vec<Device>> {
        let output = Command::new("diskutil")
            .args(["list", "-plist"])
            .output()
            .map_err(|e| PurgeError::DeviceInfoError(format!("执行 diskutil 失败: {}", e)))?;

        if !output.status.success() {
            return Err(PurgeError::DeviceInfoError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let plist_xml = String::from_utf8_lossy(&output.stdout);
        let disk_ids = extract_whole_disk_ids_from_plist(&plist_xml);
        let mut devices = Vec::new();

        for disk_id in disk_ids {
            let info_output = Command::new("diskutil")
                .args(["info", "-plist", &disk_id])
                .output();

            match info_output {
                Ok(output) if output.status.success() => {
                    let info_xml = String::from_utf8_lossy(&output.stdout);
                    if let Some(device) = parse_single_disk(&disk_id, &info_xml) {
                        if device.removable && !device.is_system {
                            devices.push(device);
                        }
                    }
                }
                _ => continue,
            }
        }

        Ok(devices)
    }

    /// 从 diskutil list 文本输出中提取整个磁盘 ID（过滤掉分区）
    /// 整个磁盘行格式: "/dev/disk4 (external, physical):"
    fn extract_whole_disk_ids_from_text(text: &str) -> Vec<String> {
        let mut ids = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for line in text.lines() {
            let trimmed = line.trim();
            // 只匹配以 "/dev/disk" 开头的行（整个磁盘头行）
            // 不匹配后面的 IDENTIFIER 列（如 disk4s1）
            if !trimmed.starts_with("/dev/disk") {
                continue;
            }

            // 提取 "/dev/disk4" 部分（到空格为止）
            let dev_part: String = trimmed.chars()
                .take_while(|c| *c != ' ')
                .collect();

            // 从 /dev/disk4 提取 disk4
            let disk_id = dev_part.trim_start_matches("/dev/");

            // 分区格式：diskNsM（s 后跟数字），跳过
            if is_partition_id(disk_id) {
                continue;
            }

            if disk_id.starts_with("disk") && seen.insert(disk_id.to_string()) {
                ids.push(disk_id.to_string());
            }
        }

        ids
    }

    /// 判断是否为分区 ID (如 disk4s1, disk4s2)
    fn is_partition_id(id: &str) -> bool {
        // 匹配 diskNsM 模式
        if let Some(s_pos) = id.find('s') {
            let after_s = &id[s_pos + 1..];
            return !after_s.is_empty() && after_s.chars().all(|c| c.is_ascii_digit());
        }
        false
    }

    /// 从 plist XML 中提取整个磁盘 ID
    fn extract_whole_disk_ids_from_plist(xml: &str) -> Vec<String> {
        let mut ids = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for line in xml.lines() {
            let trimmed = line.trim();
            if let Some(start) = trimmed.find("<string>disk") {
                if let Some(end) = trimmed.find("</string>") {
                    let disk = &trimmed[start + 8..end];
                    // 只要 diskN，不要 diskNsM（分区）
                    if disk.starts_with("disk") && !disk.contains('s') && seen.insert(disk.to_string()) {
                        ids.push(disk.to_string());
                    }
                }
            }
        }

        ids
    }


    fn parse_single_disk(disk_id: &str, xml: &str) -> Option<Device> {
        let is_removable = xml_contains(xml, "Removable", "true")
            || xml_contains(xml, "RemovableMedia", "true")
            || xml_contains(xml, "Ejectable", "true");

        let is_internal = xml_contains(xml, "Internal", "true")
            || xml_contains(xml, "DeviceLocation", "Internal");

        let is_system = xml_contains(xml, "Bootable", "true") && is_internal;

        let is_virtual = xml_contains(xml, "Virtual", "true")
            || xml_contains(xml, "Protocol", "Disk Image");

        if is_virtual {
            return None;
        }
        if !is_removable && is_internal {
            return None;
        }

        let model = extract_string(xml, "MediaName")
            .or_else(|| extract_string(xml, "IORegistryEntryName"))
            .unwrap_or_else(|| "未知设备".to_string());

        // 尝试多种方式获取序列号
        let serial = extract_string(xml, "SerialNumber")
            .or_else(|| extract_string(xml, "UUID"))
            .filter(|s| s != "Not Applicable" && s != "None" && !s.is_empty())
            .unwrap_or_else(|| {
                // 无硬件序列号时，用型号+容量生成合成标识
                let model_part = extract_string(xml, "MediaName")
                    .or_else(|| extract_string(xml, "IORegistryEntryName"))
                    .unwrap_or_else(|| "Unknown".to_string());
                let size = extract_u64(xml, "TotalSize").unwrap_or(0);
                format!("N/S-{}-{}GB", model_part.replace(' ', ""), size / 1_073_741_824)
            });

        let size = extract_u64(xml, "TotalSize").unwrap_or(0);

        let bus = extract_string(xml, "Protocol")
            .or_else(|| extract_string(xml, "BusProtocol"))
            .unwrap_or_else(|| "未知".to_string());

        let path = format!("/dev/r{}", disk_id);

        Some(Device {
            path,
            volume_paths: vec![],
            model,
            serial,
            size_bytes: size,
            removable: is_removable,
            bus_type: bus,
            is_system,
        })
    }

    fn xml_contains(xml: &str, key: &str, value: &str) -> bool {
        let pattern = format!("<key>{}</key>", key);
        if let Some(key_pos) = xml.find(&pattern) {
            // 只看紧跟着的下一个 XML 标签（防止跨 key 匹配）
            let after_key = &xml[key_pos + pattern.len()..];
            // 跳过空白
            let after_trimmed = after_key.trim_start();
            if value == "true" {
                return after_trimmed.starts_with("<true/>");
            }
            if value == "false" {
                return after_trimmed.starts_with("<false/>");
            }
            if let Some(start) = after_trimmed.find("<string>") {
                if let Some(end) = after_trimmed.find("</string>") {
                    return &after_trimmed[start + 8..end] == value;
                }
            }
        }
        false
    }

    fn extract_string(xml: &str, key: &str) -> Option<String> {
        let pattern = format!("<key>{}</key>", key);
        let pos = xml.find(&pattern)?;
        let after = &xml[pos + pattern.len()..];
        let start = after.find("<string>")?;
        let end = after.find("</string>")?;
        let val = after[start + 8..end].trim().to_string();
        if val.is_empty() { None } else { Some(val) }
    }

    fn extract_u64(xml: &str, key: &str) -> Option<u64> {
        let pattern = format!("<key>{}</key>", key);
        let pos = xml.find(&pattern)?;
        let after = &xml[pos + pattern.len()..];
        let start = after.find("<integer>")?;
        let end = after.find("</integer>")?;
        after[start + 9..end].parse().ok()
    }

    pub fn open_raw(path: &str) -> Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    PurgeError::PermissionDenied(format!(
                        "无法打开设备 {}. 请使用 sudo 运行", path
                    ))
                } else {
                    PurgeError::IoError(e)
                }
            })
    }
}

// ============================================================
// Linux 实现 (兼容麒麟系统)
// ============================================================
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::fs;
    use std::path::Path;

    pub fn list_devices() -> Result<Vec<Device>> {
        let block_dir = Path::new("/sys/block");
        let mut devices = Vec::new();

        let entries = fs::read_dir(block_dir).map_err(|e| {
            PurgeError::DeviceInfoError(format!("读取 /sys/block 失败: {}", e))
        })?;

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("sd") && !name.starts_with("nvme") {
                continue;
            }

            match parse_sysfs_device(&name) {
                Ok(Some(device)) => {
                    if device.removable && !device.is_system {
                        devices.push(device);
                    }
                }
                _ => continue,
            }
        }

        Ok(devices)
    }

    fn parse_sysfs_device(name: &str) -> Result<Option<Device>> {
        let base = format!("/sys/block/{}", name);

        let removable = match fs::read_to_string(format!("{}/removable", base)) {
            Ok(v) => v.trim() == "1",
            Err(_) => false,
        };

        if !removable {
            return Ok(None);
        }

        let size_sectors = fs::read_to_string(format!("{}/size", base))
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let size_bytes = size_sectors * 512;

        if size_bytes < 64 * 1024 * 1024 {
            return Ok(None);
        }

        let device_path = format!("/sys/block/{}/device", name);
        let model = fs::read_to_string(format!("{}/model", device_path))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "未知设备".to_string());

        let serial = fs::read_to_string(format!("{}/serial", device_path))
            .or_else(|_| fs::read_to_string(format!("{}/vpd_pg80", device_path)))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "无序列号".to_string());

        let vendor = fs::read_to_string(format!("{}/vendor", device_path))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let bus = if vendor.contains("USB") || name.starts_with("sd") {
            "USB".to_string()
        } else {
            "未知".to_string()
        };

        let is_system = check_is_system_disk(name);
        let path = format!("/dev/{}", name);

        Ok(Some(Device {
            path,
            volume_paths: vec![],
            model,
            serial,
            size_bytes,
            removable,
            bus_type: bus,
            is_system,
        }))
    }

    fn check_is_system_disk(name: &str) -> bool {
        if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
            let prefix = format!("/dev/{}", name);
            for line in mounts.lines() {
                if line.starts_with(&prefix) && line.contains(" / ") {
                    return true;
                }
            }
        }
        false
    }

    pub fn open_raw(path: &str) -> Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    PurgeError::PermissionDenied(format!(
                        "无法打开设备 {}. 请使用 sudo 运行", path
                    ))
                } else {
                    PurgeError::IoError(e)
                }
            })
    }
}

// ============================================================
// Windows 实现
// ============================================================
#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::*;
    use windows_sys::Win32::System::Ioctl::*;
    use windows_sys::Win32::System::IO::DeviceIoControl;

    pub fn list_devices() -> Result<Vec<Device>> {
        let mut devices = Vec::new();

        for i in 0..16u32 {
            let path = format!("\\\\.\\PhysicalDrive{}", i);
            match get_disk_info(&path, i) {
                Ok(Some(device)) => {
                    if device.removable && !device.is_system {
                        devices.push(device);
                    }
                }
                _ => continue,
            }
        }

        Ok(devices)
    }

    fn get_disk_info(path: &str, index: u32) -> Result<Option<Device>> {
        unsafe {
            let handle = CreateFileW(
                to_wide(&path).as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                0,
            );

            if handle == INVALID_HANDLE_VALUE {
                return Ok(None);
            }

            let mut property: STORAGE_PROPERTY_QUERY = std::mem::zeroed();
            property.PropertyId = StorageDeviceProperty;
            property.QueryType = PropertyStandardQuery;

            let mut descriptor_buf = vec![0u8; 1024];
            let mut bytes_returned = 0u32;

            let success = DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                &property as *const _ as *const _,
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                descriptor_buf.as_mut_ptr() as *mut _,
                descriptor_buf.len() as u32,
                &mut bytes_returned,
                std::ptr::null_mut(),
            );

            if success == 0 {
                CloseHandle(handle);
                return Ok(None);
            }

            let descriptor = &*(descriptor_buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR);

            let removable = descriptor.RemovableMedia != 0;
            let bus = match descriptor.BusType {
                BusTypeUsb => "USB".to_string(),
                BusTypeSd => "SD".to_string(),
                BusTypeSata => "SATA".to_string(),
                _ => "未知".to_string(),
            };

            let model = extract_string_from_buf(
                &descriptor_buf,
                descriptor.VendorIdOffset,
                descriptor.ProductIdOffset,
            );

            let serial = extract_string_from_buf_single(
                &descriptor_buf,
                descriptor.SerialNumberOffset,
            );

            let mut geometry: DISK_GEOMETRY_EX = std::mem::zeroed();
            let mut geo_bytes = 0u32;
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
                std::ptr::null(),
                0,
                &mut geometry as *mut _ as *mut _,
                std::mem::size_of::<DISK_GEOMETRY_EX>() as u32,
                &mut geo_bytes,
                std::ptr::null_mut(),
            );
            let size = geometry.DiskSize.QuadPart as u64;

            let is_system = check_windows_system_disk(index);
            CloseHandle(handle);

            if !removable && bus != "USB" && bus != "SD" {
                return Ok(None);
            }

            Ok(Some(Device {
                path: path.to_string(),
                volume_paths: vec![format!("\\\\.\\PhysicalDrive{}", index)],
                model,
                serial,
                size_bytes: size,
                removable: removable || bus == "USB",
                bus_type: bus,
                is_system,
            }))
        }
    }

    fn extract_string_from_buf(buf: &[u8], offset1: u32, offset2: u32) -> String {
        let mut parts = Vec::new();
        for offset in [offset1, offset2] {
            if offset > 0 && (offset as usize) < buf.len() {
                let start = offset as usize;
                let end = buf[start..].iter().position(|&b| b == 0).unwrap_or(32) + start;
                if end > start {
                    parts.push(String::from_utf8_lossy(&buf[start..end]).trim().to_string());
                }
            }
        }
        parts.join(" ")
    }

    fn extract_string_from_buf_single(buf: &[u8], offset: u32) -> String {
        if offset == 0 || offset as usize >= buf.len() {
            return "无序列号".to_string();
        }
        let start = offset as usize;
        let end = buf[start..].iter().position(|&b| b == 0).unwrap_or(32) + start;
        String::from_utf8_lossy(&buf[start..end]).trim().to_string()
    }

    fn check_windows_system_disk(index: u32) -> bool {
        index == 0
    }

    fn to_wide(s: &str) -> Vec<u16> {
        OsString::from(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn open_raw(path: &str) -> Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    PurgeError::PermissionDenied(
                        "无法打开设备. 请以管理员身份运行".to_string()
                    )
                } else {
                    PurgeError::IoError(e)
                }
            })
    }
}
