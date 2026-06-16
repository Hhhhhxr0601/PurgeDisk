use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// 擦写配置
#[derive(Debug, Clone)]
pub struct WipeConfig {
    /// 缓冲区大小（字节），默认 1MB
    pub buffer_size: usize,
    /// 是否在每轮结束后验证
    pub verify: bool,
}

impl Default for WipeConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1024 * 1024, // 1MB
            verify: false,
        }
    }
}

/// 覆写模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum WipePass {
    /// 全 0 填充
    Zero,
    /// 全 1 (0xFF) 填充
    One,
    /// 随机数据填充
    Random,
}

impl WipePass {
    pub fn description(&self) -> &str {
        match self {
            WipePass::Zero => "全 0x00 填充",
            WipePass::One => "全 0xFF 填充",
            WipePass::Random => "随机数据填充",
        }
    }

    pub fn pattern_byte(&self) -> Option<u8> {
        match self {
            WipePass::Zero => Some(0x00),
            WipePass::One => Some(0xFF),
            WipePass::Random => None,
        }
    }

    pub fn index(&self) -> u32 {
        match self {
            WipePass::Zero => 1,
            WipePass::One => 2,
            WipePass::Random => 3,
        }
    }
}

/// DoD 5220.22-M 标准的三轮覆写序列
pub const DOD_PASSES: [WipePass; 3] = [WipePass::Zero, WipePass::One, WipePass::Random];

/// 擦写进度（线程安全）
#[derive(Debug)]
pub struct WipeProgress {
    pub total_bytes: u64,
    pub written_bytes: AtomicU64,
    pub current_pass: AtomicU64,
    pub total_passes: u64,
    pub cancelled: AtomicBool,
    pub error_message: parking_lot::Mutex<Option<String>>,
}

impl WipeProgress {
    pub fn new(total_bytes: u64, total_passes: u64) -> Self {
        Self {
            total_bytes,
            written_bytes: AtomicU64::new(0),
            current_pass: AtomicU64::new(0),
            total_passes,
            cancelled: AtomicBool::new(false),
            error_message: parking_lot::Mutex::new(None),
        }
    }

    pub fn progress_percent(&self) -> f64 {
        let total = self.total_bytes * self.total_passes;
        if total == 0 {
            return 0.0;
        }
        let done = self.written_bytes.load(Ordering::Relaxed)
            + self.current_pass.load(Ordering::Relaxed) * self.total_bytes;
        (done as f64 / total as f64) * 100.0
    }

    pub fn current_pass_display(&self) -> String {
        let pass = self.current_pass.load(Ordering::Relaxed);
        format!("{}/{}", pass + 1, self.total_passes)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// 擦写结果
#[derive(Debug, Serialize, Deserialize)]
pub struct WipeResult {
    /// 是否成功完成
    pub success: bool,
    /// 总写入字节数
    pub total_written: u64,
    /// 设备总容量
    pub total_size: u64,
    /// 各轮次结果
    pub passes: Vec<PassResult>,
    /// 总耗时（秒）
    pub elapsed_seconds: f64,
    /// 错误信息（如有）
    pub error: Option<String>,
}

/// 单轮覆写结果
#[derive(Debug, Serialize, Deserialize)]
pub struct PassResult {
    pub pass_index: u32,
    pub pattern: WipePass,
    pub bytes_written: u64,
    pub elapsed_seconds: f64,
    pub success: bool,
    pub error: Option<String>,
}

/// 擦写引擎
pub struct WipeEngine {
    config: WipeConfig,
}

impl WipeEngine {
    pub fn new(config: WipeConfig) -> Self {
        Self { config }
    }

    /// 执行 DoD 5220.22-M 标准三轮覆写
    pub fn wipe(
        &self,
        file: &mut File,
        total_size: u64,
        progress: &WipeProgress,
        log_callback: Option<&dyn Fn(&str)>,
    ) -> WipeResult {
        let start = Instant::now();
        let mut pass_results = Vec::new();
        let mut total_written = 0u64;
        let mut success = true;
        let mut final_error = None;

        for (i, &pass_pattern) in DOD_PASSES.iter().enumerate() {
            progress.current_pass.store(i as u64, Ordering::Relaxed);
            progress.written_bytes.store(0, Ordering::Relaxed);

            if let Some(cb) = log_callback {
                cb(&format!(
                    "开始第 {} 轮覆写: {} ({})",
                    i + 1,
                    pass_pattern.description(),
                    format_pattern_name(pass_pattern)
                ));
            }

            let pass_result = self.execute_pass(file, total_size, pass_pattern, i as u32, progress);

            total_written += pass_result.bytes_written;

            if !pass_result.success {
                success = false;
                final_error = pass_result.error.clone();
                pass_results.push(pass_result);
                break;
            }

            pass_results.push(pass_result);

            // 检查是否被取消
            if progress.is_cancelled() {
                success = false;
                final_error = Some("用户取消操作".to_string());
                break;
            }

            // 回到文件开头准备下一轮
            if file.seek(SeekFrom::Start(0)).is_err() {
                success = false;
                final_error = Some("无法重置文件位置".to_string());
                break;
            }
        }

        WipeResult {
            success,
            total_written,
            total_size,
            passes: pass_results,
            elapsed_seconds: start.elapsed().as_secs_f64(),
            error: final_error,
        }
    }

    /// 执行单轮覆写
    fn execute_pass(
        &self,
        file: &mut File,
        total_size: u64,
        pass: WipePass,
        pass_index: u32,
        progress: &WipeProgress,
    ) -> PassResult {
        let start = Instant::now();
        let mut written = 0u64;
        let buf_size = self.config.buffer_size;

        // 准备写入缓冲区
        let mut buffer = vec![0u8; buf_size];
        match pass {
            WipePass::Zero => buffer.fill(0x00),
            WipePass::One => buffer.fill(0xFF),
            WipePass::Random => {
                // 随机数据在每次写入前填充
            }
        }

        // 回到开头
        if let Err(e) = file.seek(SeekFrom::Start(0)) {
            return PassResult {
                pass_index,
                pattern: pass,
                bytes_written: 0,
                elapsed_seconds: start.elapsed().as_secs_f64(),
                success: false,
                error: Some(format!("Seek 失败: {}", e)),
            };
        }

        while written < total_size {
            // 检查取消
            if progress.is_cancelled() {
                return PassResult {
                    pass_index,
                    pattern: pass,
                    bytes_written: written,
                    elapsed_seconds: start.elapsed().as_secs_f64(),
                    success: false,
                    error: Some("用户取消".to_string()),
                };
            }

            // 计算本次写入大小
            let remaining = total_size - written;
            let chunk_size = buf_size.min(remaining as usize);

            // 随机模式每次填充新随机数据
            if pass == WipePass::Random {
                rand::thread_rng().fill_bytes(&mut buffer[..chunk_size]);
            }

            // 写入
            match file.write_all(&buffer[..chunk_size]) {
                Ok(()) => {
                    written += chunk_size as u64;
                    progress.written_bytes.store(written, Ordering::Relaxed);
                }
                Err(e) => {
                    return PassResult {
                        pass_index,
                        pattern: pass,
                        bytes_written: written,
                        elapsed_seconds: start.elapsed().as_secs_f64(),
                        success: false,
                        error: Some(format!("写入失败: {}", e)),
                    };
                }
            }

            // 每写满一次缓冲区就 flush
            if written % (buf_size as u64 * 16) == 0 {
                let _ = file.flush();
            }
        }

        // 最终 flush
        if let Err(e) = file.flush() {
            return PassResult {
                pass_index,
                pattern: pass,
                bytes_written: written,
                elapsed_seconds: start.elapsed().as_secs_f64(),
                success: false,
                error: Some(format!("Flush 失败: {}", e)),
            };
        }

        PassResult {
            pass_index,
            pattern: pass,
            bytes_written: written,
            elapsed_seconds: start.elapsed().as_secs_f64(),
            success: true,
            error: None,
        }
    }
}

fn format_pattern_name(pass: WipePass) -> &'static str {
    match pass {
        WipePass::Zero => "0x00",
        WipePass::One => "0xFF",
        WipePass::Random => "RANDOM",
    }
}
