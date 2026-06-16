use chrono::Local;
use ring::hmac;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{PurgeError, Result};
use crate::wipe::WipePass;

/// 日志条目状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogStatus {
    /// 擦写开始
    Started,
    /// 单轮完成
    PassComplete,
    /// 全部完成
    Completed,
    /// 异常中断（断电/崩溃）
    Interrupted,
    /// 错误
    Error,
    /// 用户取消
    Cancelled,
}

impl LogStatus {
    pub fn as_str(&self) -> &str {
        match self {
            LogStatus::Started => "started",
            LogStatus::PassComplete => "pass_complete",
            LogStatus::Completed => "completed",
            LogStatus::Interrupted => "interrupted",
            LogStatus::Error => "error",
            LogStatus::Cancelled => "cancelled",
        }
    }
}

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// ISO 8601 时间戳
    pub timestamp: String,
    /// 操作人（系统用户名）
    pub operator: String,
    /// 电脑主机名
    pub hostname: String,
    /// U 盘硬件序列号
    pub device_serial: String,
    /// 设备路径
    pub device_path: String,
    /// 设备总容量（字节）
    pub device_size_bytes: u64,
    /// 覆写算法
    pub algorithm: String,
    /// 当前轮次 (1-3)
    pub pass_number: u32,
    /// 覆写模式描述
    pub pass_pattern: String,
    /// 操作状态
    pub status: LogStatus,
    /// 错误详情
    pub error_detail: Option<String>,
    /// HMAC-SHA256 签名（十六进制）
    pub hmac_signature: String,
}

/// 审计日志管理器
pub struct AuditLog {
    log_dir: PathBuf,
    hmac_key: hmac::Key,
}

impl AuditLog {
    /// 创建审计日志管理器
    pub fn new(log_dir: &Path) -> Result<Self> {
        // 确保日志目录存在
        fs::create_dir_all(log_dir).map_err(|e| {
            PurgeError::LogError(format!("创建日志目录失败 {}: {}", log_dir.display(), e))
        })?;

        // 生成或加载 HMAC 密钥
        let key_path = log_dir.join(".hmac_key");
        let key_bytes = if key_path.exists() {
            fs::read(&key_path).map_err(|e| {
                PurgeError::LogError(format!("读取 HMAC 密钥失败: {}", e))
            })?
        } else {
            // 使用确定性方式生成密钥（基于机器信息）
            let seed = generate_machine_seed();
            let new_key = hmac::Key::new(hmac::HMAC_SHA256, &seed);
            let tag = hmac::sign(&new_key, b"purgedisk-keygen");
            fs::write(&key_path, tag.as_ref()).map_err(|e| {
                PurgeError::LogError(format!("保存 HMAC 密钥失败: {}", e))
            })?;
            tag.as_ref().to_vec()
        };

        let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, &key_bytes);

        Ok(Self { log_dir: log_dir.to_path_buf(), hmac_key })
    }

    /// 写入日志条目
    pub fn write_entry(&self, mut entry: LogEntry) -> Result<()> {
        // 计算 HMAC 签名（签名前先清空签名字段）
        entry.hmac_signature = String::new();
        let payload = serde_json::to_string(&entry).map_err(|e| {
            PurgeError::LogError(format!("序列化日志条目失败: {}", e))
        })?;

        let signature = hmac::sign(&self.hmac_key, payload.as_bytes());
        entry.hmac_signature = hex_encode(signature.as_ref());

        // 确定日志文件路径（按日期）
        let date = Local::now().format("%Y-%m-%d").to_string();
        let log_file = self.log_dir.join(format!("audit-{}.jsonl", date));

        // 追加写入
        let json_line = serde_json::to_string(&entry).map_err(|e| {
            PurgeError::LogError(format!("序列化日志条目失败: {}", e))
        })?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .map_err(|e| {
                PurgeError::LogError(format!("打开日志文件失败 {}: {}", log_file.display(), e))
            })?;

        writeln!(file, "{}", json_line).map_err(|e| {
            PurgeError::LogError(format!("写入日志失败: {}", e))
        })?;

        file.flush().map_err(|e| {
            PurgeError::LogError(format!("刷新日志文件失败: {}", e))
        })?;

        Ok(())
    }

    /// 创建一个新的开始条目
    pub fn create_start_entry(
        &self,
        device_path: &str,
        device_serial: &str,
        device_size: u64,
    ) -> LogEntry {
        LogEntry {
            timestamp: now_iso8601(),
            operator: get_operator(),
            hostname: get_hostname(),
            device_serial: device_serial.to_string(),
            device_path: device_path.to_string(),
            device_size_bytes: device_size,
            algorithm: "DoD 5220.22-M (3 轮)".to_string(),
            pass_number: 0,
            pass_pattern: String::new(),
            status: LogStatus::Started,
            error_detail: None,
            hmac_signature: String::new(),
        }
    }

    /// 创建单轮完成条目
    pub fn create_pass_entry(
        &self,
        device_path: &str,
        device_serial: &str,
        device_size: u64,
        pass: WipePass,
    ) -> LogEntry {
        LogEntry {
            timestamp: now_iso8601(),
            operator: get_operator(),
            hostname: get_hostname(),
            device_serial: device_serial.to_string(),
            device_path: device_path.to_string(),
            device_size_bytes: device_size,
            algorithm: "DoD 5220.22-M (3 轮)".to_string(),
            pass_number: pass.index(),
            pass_pattern: match pass {
                WipePass::Zero => "0x00".to_string(),
                WipePass::One => "0xFF".to_string(),
                WipePass::Random => "RANDOM".to_string(),
            },
            status: LogStatus::PassComplete,
            error_detail: None,
            hmac_signature: String::new(),
        }
    }

    /// 创建完成/错误/取消条目
    pub fn create_end_entry(
        &self,
        device_path: &str,
        device_serial: &str,
        device_size: u64,
        status: LogStatus,
        error: Option<String>,
    ) -> LogEntry {
        LogEntry {
            timestamp: now_iso8601(),
            operator: get_operator(),
            hostname: get_hostname(),
            device_serial: device_serial.to_string(),
            device_path: device_path.to_string(),
            device_size_bytes: device_size,
            algorithm: "DoD 5220.22-M (3 轮)".to_string(),
            pass_number: 3,
            pass_pattern: String::new(),
            status,
            error_detail: error,
            hmac_signature: String::new(),
        }
    }

    /// 检查是否存在未完成的擦写记录
    pub fn check_incomplete_wipes(&self) -> Result<Vec<LogEntry>> {
        let mut incomplete = Vec::new();

        let entries = fs::read_dir(&self.log_dir).map_err(|e| {
            PurgeError::LogError(format!("读取日志目录失败: {}", e))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "jsonl") {
                continue;
            }

            if let Ok(content) = fs::read_to_string(&path) {
                let lines: Vec<&str> = content.lines().collect();
                if lines.is_empty() {
                    continue;
                }

                // 检查最后一个有效条目
                if let Some(last_line) = lines.last() {
                    if let Ok(last_entry) = serde_json::from_str::<LogEntry>(last_line) {
                        if last_entry.status == LogStatus::Started {
                            incomplete.push(last_entry);
                        }
                    }
                }
            }
        }

        Ok(incomplete)
    }

    /// 获取日志目录路径
    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }
}

fn now_iso8601() -> String {
    Local::now().to_rfc3339()
}

fn get_operator() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERNAME").unwrap_or_else(|_| "未知用户".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("USER").unwrap_or_else(|_| "未知用户".to_string())
    }
}

fn get_hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "未知主机".to_string())
}

fn generate_machine_seed() -> Vec<u8> {
    let mut seed = Vec::new();
    seed.extend_from_slice(get_hostname().as_bytes());
    seed.extend_from_slice(get_operator().as_bytes());
    seed.extend_from_slice(b"purgedisk-salt-2024");
    // 使用 SHA256 生成固定长度的种子
    let digest = ring::digest::digest(&ring::digest::SHA256, &seed);
    digest.as_ref().to_vec()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
