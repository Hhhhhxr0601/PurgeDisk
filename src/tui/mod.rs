//! TUI 模块 - 基于 ratatui 的终端交互界面
//!
//! 提供多屏幕交互体验：
//! - Home: 设备列表浏览与选择
//! - Confirm: 擦写确认（输入序列号验证）
//! - Progress: 实时擦写进度监控（三轮覆写进度条）
//! - Result: 擦写结果摘要
//! - Logs: 审计日志查看器

mod app;
mod event;
mod format;
mod screens;
mod ui;

use std::io;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::device::Device;
use app::App;
use event::{EventHandler, TuiEvent};

/// TUI 入口函数
///
/// 初始化终端、启动事件循环、处理清理退出。
pub fn run(log_dir: std::path::PathBuf) -> anyhow::Result<()> {
    // 设置 panic hook，确保异常时恢复终端
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    // 初始化终端
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 创建 App
    let mut app = App::new(log_dir);

    // 创建事件处理器（200ms tick）
    let events = EventHandler::new(Duration::from_millis(200));

    // 启动设备扫描（后台线程）
    let (dev_tx, dev_rx) = mpsc::channel::<Result<Vec<Device>, String>>();
    std::thread::spawn(move || {
        let result = crate::device::list_removable_devices().map_err(|e| match &e {
            crate::error::PurgeError::PermissionDenied(msg) => {
                format!("{}\n提示: 请使用管理员/root权限运行", msg)
            }
            _ => format!("{}", e),
        });
        let _ = dev_tx.send(result);
    });

    // 主事件循环
    loop {
        // 渲染
        terminal.draw(|f| ui::render(f, &app))?;

        // 检查设备扫描结果
        if !app.devices_loaded {
            if let Ok(result) = dev_rx.try_recv() {
                match result {
                    Ok(devices) => app.set_devices(devices),
                    Err(err) => app.set_device_error(err),
                }
            }
        }

        // 检查日志加载是否完成
        app.poll_log_result();

        // 检查擦写是否完成
        app.poll_wipe_result();

        // 处理事件
        match events.next()? {
            TuiEvent::Key(key) => {
                app.handle_key(key);
            }
            TuiEvent::Tick => {
                // tick 用于刷新进度等，无需额外处理
            }
        }

        // 检查是否退出
        if app.should_quit {
            break;
        }
    }

    // 恢复终端
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
