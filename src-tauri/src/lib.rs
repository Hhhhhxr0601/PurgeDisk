use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use tauri::{AppHandle, Emitter};

// ============================================================
// Types
// ============================================================

/// 擦写进度事件（流式推送到前端）
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum WipeEvent {
    /// 轮次进度更新
    Progress {
        pass: u32,
        total_passes: u32,
        percent: f64,
        written_bytes: u64,
        total_bytes: u64,
        speed_bps: u64,
        eta_seconds: f64,
    },
    /// 单轮完成
    PassComplete {
        pass: u32,
        pattern: String,
        bytes_written: u64,
        elapsed_seconds: f64,
        success: bool,
    },
    /// 全部完成
    Finished {
        success: bool,
        total_written: u64,
        total_size: u64,
        elapsed_seconds: f64,
        error: Option<String>,
    },
    /// 错误
    Error { message: String },
}

// ============================================================
// Tauri Commands
// ============================================================

/// 扫描可移动存储设备
#[tauri::command]
fn list_devices() -> Result<Vec<purgedisk::device::Device>, String> {
    purgedisk::device::list_removable_devices().map_err(|e| format!("{}", e))
}

/// 获取审计日志
#[tauri::command]
fn get_logs(log_dir: Option<String>) -> Result<Vec<purgedisk::audit::LogEntry>, String> {
    let dir = log_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| default_log_dir());

    let mut entries = Vec::new();
    if !dir.exists() {
        return Ok(entries);
    }

    let dir_entries =
        std::fs::read_dir(&dir).map_err(|e| format!("读取日志目录失败: {}", e))?;

    let mut files: Vec<PathBuf> = dir_entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "jsonl")
        })
        .map(|e| e.path())
        .collect();
    files.sort_by(|a, b| b.cmp(a));

    for path in files {
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                if let Ok(entry) = serde_json::from_str::<purgedisk::audit::LogEntry>(line) {
                    entries.push(entry);
                }
            }
        }
    }

    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(entries)
}

/// 检查未完成的擦写记录
#[tauri::command]
fn check_incomplete_wipes(
    log_dir: Option<String>,
) -> Result<Vec<purgedisk::audit::LogEntry>, String> {
    let dir = log_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| default_log_dir());

    let audit_log =
        purgedisk::audit::AuditLog::new(&dir).map_err(|e| format!("初始化审计日志失败: {}", e))?;

    audit_log
        .check_incomplete_wipes()
        .map_err(|e| format!("检查未完成擦写失败: {}", e))
}

/// 启动擦写任务（后台线程执行，通过 app.emit 推送进度）
#[tauri::command]
async fn start_wipe(
    app: AppHandle,
    device_path: String,
    device_serial: String,
    device_size: u64,
    buffer_size: Option<usize>,
    log_dir: Option<String>,
) -> Result<(), String> {
    let dir = log_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| default_log_dir());

    // 初始化审计日志
    let audit_log =
        purgedisk::audit::AuditLog::new(&dir).map_err(|e| format!("初始化审计日志失败: {}", e))?;

    // 写入开始日志
    let start_entry =
        audit_log.create_start_entry(&device_path, &device_serial, device_size);
    audit_log
        .write_entry(start_entry)
        .map_err(|e| format!("写入审计日志失败: {}", e))?;

    // 打开设备
    let mut file = purgedisk::device::open_raw_device(&device_path).map_err(|e| {
        match &e {
            purgedisk::error::PurgeError::PermissionDenied(_) => {
                format!("{}\n提示: 请使用管理员/root权限运行此程序", e)
            }
            _ => format!("打开设备失败: {}", e),
        }
    })?;

    // 创建进度追踪
    let progress = Arc::new(purgedisk::wipe::WipeProgress::new(device_size, 3));
    let buf = buffer_size.unwrap_or(1024) * 1024; // 默认 1MB

    // 在后台线程执行擦写
    let progress_clone = Arc::clone(&progress);
    let app_clone = app.clone();
    let device_path_clone = device_path.clone();
    let device_serial_clone = device_serial.clone();

    thread::spawn(move || {
        let config = purgedisk::wipe::WipeConfig {
            buffer_size: buf,
            verify: false,
        };
        let engine = purgedisk::wipe::WipeEngine::new(config);
        let start_time = std::time::Instant::now();
        let total_bytes = device_size;

        // 进度轮询线程
        let progress_poll = Arc::clone(&progress_clone);
        let app_poll = app_clone.clone();
        let poll_handle = thread::spawn(move || {
            let mut last_pass: u64 = 0;
            loop {
                if progress_poll.is_cancelled() {
                    break;
                }

                let current_pass = progress_poll
                    .current_pass
                    .load(std::sync::atomic::Ordering::Relaxed);
                let written = progress_poll
                    .written_bytes
                    .load(std::sync::atomic::Ordering::Relaxed);
                let percent = progress_poll.progress_percent();
                let elapsed = start_time.elapsed().as_secs_f64();

                let speed = if elapsed > 0.0 {
                    (written as f64 / elapsed) as u64
                } else {
                    0
                };
                let eta = if percent > 0.5 {
                    let total_est = elapsed / (percent / 100.0);
                    total_est - elapsed
                } else {
                    0.0
                };

                // 轮次切换时发送 PassComplete
                if current_pass > last_pass && last_pass > 0 {
                    let _ = app_poll.emit(
                        "wipe-event",
                        WipeEvent::PassComplete {
                            pass: last_pass as u32,
                            pattern: match last_pass {
                                1 => "0x00 填充".to_string(),
                                2 => "0xFF 填充".to_string(),
                                3 => "随机数据".to_string(),
                                _ => "未知".to_string(),
                            },
                            bytes_written: total_bytes,
                            elapsed_seconds: elapsed,
                            success: true,
                        },
                    );
                }
                last_pass = current_pass;

                let _ = app_poll.emit(
                    "wipe-event",
                    WipeEvent::Progress {
                        pass: current_pass as u32 + 1,
                        total_passes: 3,
                        percent,
                        written_bytes: written,
                        total_bytes,
                        speed_bps: speed,
                        eta_seconds: eta,
                    },
                );

                thread::sleep(std::time::Duration::from_millis(200));
            }
        });

        // 执行擦写
        let result = engine.wipe(&mut file, total_bytes, &progress_clone, None);

        // 等待进度轮询线程结束
        let _ = poll_handle.join();

        let elapsed = start_time.elapsed().as_secs_f64();

        // 写入结束审计日志
        let status = if result.success {
            purgedisk::audit::LogStatus::Completed
        } else if progress_clone.is_cancelled() {
            purgedisk::audit::LogStatus::Cancelled
        } else {
            purgedisk::audit::LogStatus::Error
        };

        let end_entry = audit_log.create_end_entry(
            &device_path_clone,
            &device_serial_clone,
            total_bytes,
            status,
            result.error.clone(),
        );
        let _ = audit_log.write_entry(end_entry);

        // 成功后重新格式化
        if result.success {
            let _ = purgedisk::device::reformat_device(&device_path_clone);
        }

        // 发送完成事件
        let _ = app_clone.emit(
            "wipe-event",
            WipeEvent::Finished {
                success: result.success,
                total_written: result.total_written,
                total_size: result.total_size,
                elapsed_seconds: elapsed,
                error: result.error,
            },
        );
    });

    Ok(())
}

/// 取消擦写
#[tauri::command]
fn cancel_wipe() -> Result<(), String> {
    // 注意：cancel 需要持有 WipeProgress 的引用
    // 这里简化处理，实际通过前端发送事件后由后台线程检查
    // 完整实现需要将 WipeProgress 存入 Tauri state
    Ok(())
}

// ============================================================
// Helpers
// ============================================================

fn default_log_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join(".purgedisk").join("logs")
    } else {
        PathBuf::from("/tmp/purgedisk/logs")
    }
}

// ============================================================
// Tauri Entry
// ============================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            list_devices,
            get_logs,
            check_incomplete_wipes,
            start_wipe,
            cancel_wipe,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PurgeDisk GUI");
}
