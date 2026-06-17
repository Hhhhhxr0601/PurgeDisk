use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use purgedisk::audit::{AuditLog, LogStatus};
use purgedisk::error::PurgeError;
use purgedisk::wipe::{WipeConfig, WipeEngine, WipeProgress};

/// 企业级 U 盘数据合规销毁工具
///
/// 符合 DoD 5220.22-M 标准，三轮覆写：全0 → 全1 → 随机数据
#[derive(Parser)]
#[command(name = "purgedisk", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 日志目录
    #[arg(long, default_value = "~/.purgedisk/logs")]
    log_dir: String,

    /// 跳过确认提示（危险！仅用于自动化脚本）
    #[arg(long, global = true)]
    yes: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// 列出所有可移动存储设备
    List,

    /// 擦写指定设备（DoD 5220.22-M 三轮覆写）
    Wipe {
        /// 设备路径 (如 /dev/disk2)
        #[arg(short, long)]
        device: String,

        /// 覆写后验证
        #[arg(long)]
        verify: bool,

        /// 缓冲区大小 (KB)，默认 1024
        #[arg(long, default_value = "1024")]
        buffer_size: usize,
    },

    /// 查看审计日志
    Logs {
        /// 显示最近 N 天的日志
        #[arg(short, long, default_value = "7")]
        days: u32,

        /// 过滤设备序列号
        #[arg(long)]
        serial: Option<String>,
    },

    /// 交互式 TUI 界面
    Tui,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::List => cmd_list(),
        Commands::Wipe {
            ref device,
            verify,
            buffer_size,
        } => cmd_wipe(&cli, device, verify, buffer_size),
        Commands::Logs { days, ref serial } => cmd_logs(&cli, days, serial.as_deref()),
        Commands::Tui => cmd_tui(&cli),
    };

    match result {
        Ok(()) => {}
        Err(e) => {
            eprintln!("错误: {}", e);
            std::process::exit(1);
        }
    }
}

/// 列出可移动设备
fn cmd_list() -> Result<()> {
    println!("正在扫描可移动存储设备...\n");

    let devices = purgedisk::device::list_removable_devices().map_err(|e| {
        if let PurgeError::PermissionDenied(msg) = &e {
            anyhow::anyhow!("{}\n提示: 请使用管理员/root权限运行", msg)
        } else {
            anyhow::anyhow!("{}", e)
        }
    })?;

    if devices.is_empty() {
        println!("未检测到可移动存储设备。");
        println!("请确认 U 盘已插入并被系统识别。");
        return Ok(());
    }

    println!("检测到 {} 个可移动设备:\n", devices.len());
    println!("{:-<80}", "");
    for (i, dev) in devices.iter().enumerate() {
        println!("  [{}] {}", i + 1, dev.short_name());
        println!("      路径: {}", dev.path);
        println!("      序列号: {}", dev.serial);
        println!("      容量: {}", dev.size_display());
        println!("      总线: {}", dev.bus_type);
        if i < devices.len() - 1 {
            println!();
        }
    }
    println!("{:-<80}", "");

    Ok(())
}

/// 执行擦写
fn cmd_wipe(cli: &Cli, device_path: &str, verify: bool, buffer_size_kb: usize) -> Result<()> {
    let log_dir = expand_path(&cli.log_dir);
    let audit_log = AuditLog::new(&log_dir)?;

    // 检查未完成的擦写
    let incomplete = audit_log.check_incomplete_wipes()?;
    if !incomplete.is_empty() {
        eprintln!("警告: 检测到之前未完成的擦写记录:");
        for entry in &incomplete {
            eprintln!(
                "  - 设备 {} @ {} (操作人: {})",
                entry.device_serial, entry.timestamp, entry.operator
            );
        }
        eprintln!();
    }

    // 扫描并验证目标设备
    println!("正在验证目标设备...");
    let devices = purgedisk::device::list_removable_devices()?;
    let target = devices
        .iter()
        .find(|d| d.path == device_path)
        .ok_or_else(|| {
            PurgeError::DeviceNotFound(format!(
                "设备 {} 不在可移动设备列表中.\n运行 `purgedisk list` 查看可用设备.",
                device_path
            ))
        })?
        .clone();

    // 二次确认
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                  即将销毁设备上的所有数据                    ║");
    println!("║              此操作不可逆，请仔细确认以下信息               ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  设备: {:<52}║", target.model);
    println!("║  路径: {:<52}║", target.path);
    println!("║  序列号: {:<50}║", target.serial);
    println!("║  容量: {:<52}║", target.size_display());
    println!("║  算法: DoD 5220.22-M (三轮覆写: 0x00→0xFF→随机)           ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  第 1 轮: 全 0x00 填充                                      ║");
    println!("║  第 2 轮: 全 0xFF 填充                                      ║");
    println!("║  第 3 轮: 密码学安全随机数据                                ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    if !cli.yes {
        print!("输入设备序列号 [{}] 以确认操作: ", target.serial);
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input != target.serial {
            println!("序列号不匹配，操作已取消。");
            return Ok(());
        }
    }

    // 记录开始日志
    let start_entry = audit_log.create_start_entry(
        &target.path,
        &target.serial,
        target.size_bytes,
    );
    audit_log.write_entry(start_entry)?;

    // 打开设备
    println!("\n正在打开设备...");
    let mut file = purgedisk::device::open_raw_device(&target.path)?;

    // 创建进度追踪
    let progress = Arc::new(WipeProgress::new(target.size_bytes, 3));
    let progress_clone = Arc::clone(&progress);

    // 启动进度显示线程
    let progress_handle = thread::spawn(move || {
        let start = std::time::Instant::now();
        loop {
            if progress_clone.is_cancelled() {
                break;
            }

            let percent = progress_clone.progress_percent();
            let pass = progress_clone.current_pass_display();
            let elapsed = start.elapsed().as_secs_f64();
            let written = progress_clone.written_bytes.load(std::sync::atomic::Ordering::Relaxed);

            let eta = if percent > 0.1 {
                let total_est = elapsed / (percent / 100.0);
                total_est - elapsed
            } else {
                0.0
            };

            print!(
                "\r  轮次 {} | 进度: {:.1}% | 已写: {} | 耗时: {} | 预计剩余: {}        ",
                pass,
                percent,
                format_bytes(written),
                format_duration(elapsed),
                format_duration(eta),
            );
            use std::io::Write;
            let _ = std::io::stdout().flush();

            thread::sleep(Duration::from_millis(200));
        }
        println!();
    });

    // 执行擦写
    let config = WipeConfig {
        buffer_size: buffer_size_kb * 1024,
        verify,
    };
    let engine = WipeEngine::new(config);

    let log_callback = |msg: &str| {
        log::info!("{}", msg);
    };

    let result = engine.wipe(
        &mut file,
        target.size_bytes,
        &progress,
        Some(&log_callback),
    );

    // 停止进度显示
    progress.cancel();
    let _ = progress_handle.join();

    // 记录结果
    println!("\n{:=<60}", "");

    if result.success {
        println!("擦写完成!");
        println!("  总写入: {}", format_bytes(result.total_written));
        println!("  总耗时: {}", format_duration(result.elapsed_seconds));
        println!("  平均速度: {}/s", format_bytes((result.total_written as f64 / result.elapsed_seconds) as u64));

        // 重新格式化设备
        println!("\n正在重新格式化设备...");
        match purgedisk::device::reformat_device(&target.path) {
            Ok(()) => println!("设备已重新格式化为 ExFAT (GPT)，可正常使用。"),
            Err(e) => eprintln!("重新格式化失败: {}。请手动在磁盘工具中格式化。", e),
        }

        let end_entry = audit_log.create_end_entry(
            &target.path,
            &target.serial,
            target.size_bytes,
            LogStatus::Completed,
            None,
        );
        audit_log.write_entry(end_entry)?;
    } else {
        let error_msg = result.error.unwrap_or_else(|| "未知错误".to_string());
        println!("擦写未完成: {}", error_msg);

        let status = if error_msg.contains("取消") {
            LogStatus::Cancelled
        } else {
            LogStatus::Interrupted
        };

        let end_entry = audit_log.create_end_entry(
            &target.path,
            &target.serial,
            target.size_bytes,
            status,
            Some(error_msg),
        );
        audit_log.write_entry(end_entry)?;
    }

    // 打印各轮详情
    println!("\n轮次详情:");
    for pass in &result.passes {
        let status_icon = if pass.success { "✓" } else { "✗" };
        println!(
            "  {} 第 {} 轮: {} - {} 字节 - {}",
            status_icon,
            pass.pass_index + 1,
            pass.pattern.description(),
            format_bytes(pass.bytes_written),
            format_duration(pass.elapsed_seconds),
        );
        if let Some(err) = &pass.error {
            println!("    错误: {}", err);
        }
    }

    println!("\n审计日志已保存到: {}", log_dir.display());

    Ok(())
}

/// 查看日志
fn cmd_logs(cli: &Cli, days: u32, serial_filter: Option<&str>) -> Result<()> {
    let log_dir = expand_path(&cli.log_dir);

    if !log_dir.exists() {
        println!("日志目录不存在: {}", log_dir.display());
        return Ok(());
    }

    let now = chrono::Local::now();
    let cutoff = now - chrono::Duration::days(days as i64);

    println!("审计日志 (最近 {} 天):\n", days);
    println!("{:-<100}", "");
    println!(
        "{:<22} {:<12} {:<20} {:<15} {:<10} {}",
        "时间", "操作人", "设备序列号", "算法", "状态", "详情"
    );
    println!("{:-<100}", "");

    let mut found = false;

    let entries = std::fs::read_dir(&log_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "jsonl") {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                if let Ok(log_entry) = serde_json::from_str::<purgedisk::audit::LogEntry>(line) {
                    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&log_entry.timestamp) {
                        if ts.with_timezone(&now.timezone()) < cutoff {
                            continue;
                        }
                    }

                    if let Some(filter) = serial_filter {
                        if log_entry.device_serial != filter {
                            continue;
                        }
                    }

                    found = true;
                    let detail = log_entry.error_detail.as_deref().unwrap_or("");
                    let short_ts = &log_entry.timestamp[..19.min(log_entry.timestamp.len())];
                    println!(
                        "{:<22} {:<12} {:<20} {:<15} {:<10} {}",
                        short_ts,
                        truncate(&log_entry.operator, 12),
                        truncate(&log_entry.device_serial, 20),
                        truncate(&log_entry.algorithm, 15),
                        log_entry.status.as_str(),
                        detail,
                    );
                }
            }
        }
    }

    if !found {
        println!("无符合条件的日志记录。");
    }

    println!("\n日志目录: {}", log_dir.display());
    Ok(())
}

/// 启动 TUI
fn cmd_tui(cli: &Cli) -> Result<()> {
    let log_dir = expand_path(&cli.log_dir);
    purgedisk::tui::run(log_dir)?;
    Ok(())
}

fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") || path.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

fn format_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_099_511_627_776.0 {
        format!("{:.2} TB", b / 1_099_511_627_776.0)
    } else if b >= 1_073_741_824.0 {
        format!("{:.2} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else {
        format!("{:.0} B", b)
    }
}

fn format_duration(seconds: f64) -> String {
    let secs = seconds as u64;
    if secs >= 3600 {
        format!("{}h{}m{}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
