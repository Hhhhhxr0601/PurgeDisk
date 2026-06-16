use serde::{Deserialize, Serialize};
use std::fmt;

/// 可移动存储设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// 设备路径 (e.g. /dev/disk2, \\\\.\\PhysicalDrive1)
    pub path: String,
    /// 逻辑卷路径 (e.g. /dev/disk2s1, E:)
    pub volume_paths: Vec<String>,
    /// 设备型号
    pub model: String,
    /// 硬件序列号
    pub serial: String,
    /// 总容量（字节）
    pub size_bytes: u64,
    /// 是否为可移动设备
    pub removable: bool,
    /// 总线类型 (USB, SD, etc.)
    pub bus_type: String,
    /// 是否为系统盘
    pub is_system: bool,
}

impl Device {
    /// 格式化容量显示
    pub fn size_display(&self) -> String {
        let bytes = self.size_bytes as f64;
        if bytes >= 1_099_511_627_776.0 {
            format!("{:.2} TB", bytes / 1_099_511_627_776.0)
        } else if bytes >= 1_073_741_824.0 {
            format!("{:.2} GB", bytes / 1_073_741_824.0)
        } else if bytes >= 1_048_576.0 {
            format!("{:.2} MB", bytes / 1_048_576.0)
        } else {
            format!("{:.0} B", bytes)
        }
    }

    /// 获取设备短名称（用于显示）
    pub fn short_name(&self) -> String {
        let model = if self.model.is_empty() {
            "未知设备".to_string()
        } else {
            self.model.clone()
        };
        format!("{} [{}] ({})", model, self.serial, self.size_display())
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "设备: {}\n  路径: {}\n  序列号: {}\n  容量: {}\n  总线: {}\n  可移动: {}",
            self.model,
            self.path,
            self.serial,
            self.size_display(),
            self.bus_type,
            if self.removable { "是" } else { "否" }
        )
    }
}
