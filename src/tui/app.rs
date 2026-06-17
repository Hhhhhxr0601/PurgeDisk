use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use crate::audit::{AuditLog, LogEntry, LogStatus};
use crate::device::Device;
use crate::error::PurgeError;
use crate::wipe::{WipeConfig, WipeEngine, WipeProgress, WipeResult};

/// 当前屏幕
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    /// 主页：设备列表
    Home,
    /// 擦写确认
    Confirm,
    /// 擦写进行中
    Progress,
    /// 擦写结果
    Result,
    /// 审计日志查看
    Logs,
}

/// 擦写任务句柄
pub struct WipeTask {
    /// 进度追踪（线程安全，TUI 轮询此对象）
    pub progress: Arc<WipeProgress>,
    /// 开始时间
    pub start_time: Instant,
    /// 结果接收通道
    pub result_rx: mpsc::Receiver<WipeResult>,
}

/// App 核心状态
pub struct App {
    /// 当前屏幕
    pub screen: Screen,
    /// 是否应退出
    pub should_quit: bool,
    /// 设备列表
    pub devices: Vec<Device>,
    /// 当前选中的设备索引
    pub selected_device: usize,
    /// 设备扫描是否完成
    pub devices_loaded: bool,
    /// 设备扫描错误信息
    pub device_error: Option<String>,

    /// 确认屏幕：用户输入的序列号
    pub confirm_input: String,
    /// 确认屏幕：错误提示
    pub confirm_error: Option<String>,

    /// 擦写后台任务
    pub wipe_task: Option<WipeTask>,
    /// 擦写结果
    pub wipe_result: Option<WipeResult>,
    /// 擦写错误（设备打开失败等）
    pub wipe_error: Option<String>,

    /// 审计日志
    pub log_entries: Vec<LogEntry>,
    /// 日志滚动位置
    pub log_scroll: usize,
    /// 日志是否已加载
    pub logs_loaded: bool,

    /// 日志目录路径
    pub log_dir: std::path::PathBuf,
    /// 审计日志实例
    pub audit_log: Option<AuditLog>,
    /// 日志加载通道
    pub log_rx: Option<mpsc::Receiver<Vec<LogEntry>>>,
}

impl App {
    pub fn new(log_dir: std::path::PathBuf) -> Self {
        Self {
            screen: Screen::Home,
            should_quit: false,
            devices: Vec::new(),
            selected_device: 0,
            devices_loaded: false,
            device_error: None,

            confirm_input: String::new(),
            confirm_error: None,

            wipe_task: None,
            wipe_result: None,
            wipe_error: None,

            log_entries: Vec::new(),
            log_scroll: 0,
            logs_loaded: false,

            log_dir,
            audit_log: None,
            log_rx: None,
        }
    }

    /// 设置设备列表（由后台扫描线程调用）
    pub fn set_devices(&mut self, devices: Vec<Device>) {
        self.devices = devices;
        self.devices_loaded = true;
        self.device_error = None;
    }

    /// 设置设备扫描错误
    pub fn set_device_error(&mut self, err: String) {
        self.device_error = Some(err);
        self.devices_loaded = true;
    }

    /// 获取当前选中的设备
    pub fn selected_device(&self) -> Option<&Device> {
        self.devices.get(self.selected_device)
    }

    /// 分发按键事件到当前屏幕
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        // Progress 屏幕只响应取消键
        if self.screen == Screen::Progress {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('c')) {
                if let Some(task) = &self.wipe_task {
                    task.progress.cancel();
                }
            }
            return;
        }

        match self.screen {
            Screen::Home => self.handle_home_key(key.code),
            Screen::Confirm => self.handle_confirm_key(key.code),
            Screen::Result => self.handle_result_key(key.code),
            Screen::Logs => self.handle_logs_key(key.code),
            Screen::Progress => {} // handled above
        }
    }

    /// 处理 Home 屏幕的按键
    fn handle_home_key(&mut self, code: crossterm::event::KeyCode) {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_device > 0 {
                    self.selected_device -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_device < self.devices.len().saturating_sub(1) {
                    self.selected_device += 1;
                }
            }
            KeyCode::Enter => {
                if self.devices_loaded && !self.devices.is_empty() {
                    self.screen = Screen::Confirm;
                    self.confirm_input.clear();
                    self.confirm_error = None;
                }
            }
            KeyCode::Char('l') => {
                self.screen = Screen::Logs;
                self.log_scroll = 0;
                if !self.logs_loaded {
                    self.start_log_loading();
                }
            }
            KeyCode::Char('r') => {
                // 刷新设备列表（标记为未加载，由主循环触发扫描）
                self.devices_loaded = false;
                self.devices.clear();
                self.device_error = None;
                self.selected_device = 0;
            }
            _ => {}
        }
    }

    /// 处理 Confirm 屏幕的按键
    fn handle_confirm_key(&mut self, code: crossterm::event::KeyCode) {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Esc => {
                self.screen = Screen::Home;
                self.confirm_input.clear();
                self.confirm_error = None;
            }
            KeyCode::Char(c) => {
                self.confirm_input.push(c);
                self.confirm_error = None;
            }
            KeyCode::Backspace => {
                self.confirm_input.pop();
                self.confirm_error = None;
            }
            KeyCode::Enter => {
                self.try_start_wipe();
            }
            _ => {}
        }
    }

    /// 处理 Result 屏幕的按键
    fn handle_result_key(&mut self, code: crossterm::event::KeyCode) {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                self.screen = Screen::Home;
                self.wipe_result = None;
                self.wipe_error = None;
            }
            _ => {}
        }
    }

    /// 处理 Logs 屏幕的按键
    fn handle_logs_key(&mut self, code: crossterm::event::KeyCode) {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.screen = Screen::Home;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.log_scroll = self.log_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.log_entries.len().saturating_sub(1);
                if self.log_scroll < max {
                    self.log_scroll += 1;
                }
            }
            KeyCode::Home => {
                self.log_scroll = 0;
            }
            KeyCode::End => {
                self.log_scroll = self.log_entries.len().saturating_sub(1);
            }
            _ => {}
        }
    }

    /// 验证序列号并启动擦写
    fn try_start_wipe(&mut self) {
        let Some(device) = self.selected_device().cloned() else {
            return;
        };

        if self.confirm_input.trim() != device.serial {
            self.confirm_error = Some("序列号不匹配，请重新输入。".to_string());
            return;
        }

        // 初始化审计日志
        let audit_log = match AuditLog::new(&self.log_dir) {
            Ok(log) => log,
            Err(e) => {
                self.wipe_error = Some(format!("初始化审计日志失败: {}", e));
                self.screen = Screen::Result;
                return;
            }
        };

        // 写入开始日志
        let start_entry =
            audit_log.create_start_entry(&device.path, &device.serial, device.size_bytes);
        if let Err(e) = audit_log.write_entry(start_entry) {
            self.wipe_error = Some(format!("写入审计日志失败: {}", e));
            self.screen = Screen::Result;
            return;
        }
        self.audit_log = Some(audit_log);

        // 打开设备
        let mut file = match crate::device::open_raw_device(&device.path) {
            Ok(f) => f,
            Err(e) => {
                let err_msg = match &e {
                    PurgeError::PermissionDenied(_) => {
                        format!("{}\n提示: 请使用 sudo 运行此程序", e)
                    }
                    _ => format!("打开设备失败: {}", e),
                };
                self.wipe_error = Some(err_msg);
                self.screen = Screen::Result;
                self.write_error_log(&device, &e.to_string());
                return;
            }
        };

        // 创建进度追踪和结果通道
        let progress = Arc::new(WipeProgress::new(device.size_bytes, 3));
        let (tx, rx) = mpsc::channel();

        // 在后台线程执行擦写
        let progress_clone = Arc::clone(&progress);
        std::thread::spawn(move || {
            let config = WipeConfig::default();
            let engine = WipeEngine::new(config);
            let result = engine.wipe(&mut file, device.size_bytes, &progress_clone, None);
            let _ = tx.send(result);
        });

        self.wipe_task = Some(WipeTask {
            progress,
            start_time: Instant::now(),
            result_rx: rx,
        });
        self.screen = Screen::Progress;
    }

    /// 写入错误审计日志
    fn write_error_log(&self, device: &Device, error: &str) {
        if let Some(al) = &self.audit_log {
            let entry = al.create_end_entry(
                &device.path,
                &device.serial,
                device.size_bytes,
                LogStatus::Error,
                Some(error.to_string()),
            );
            let _ = al.write_entry(entry);
        }
    }

    /// 启动日志加载（后台线程）
    fn start_log_loading(&mut self) {
        let log_dir = self.log_dir.clone();
        let (tx, rx) = mpsc::channel();
        self.log_rx = Some(rx);

        std::thread::spawn(move || {
            let entries = load_log_entries(&log_dir);
            let _ = tx.send(entries);
        });
    }

    /// 在 tick 中检查日志加载是否完成
    pub fn poll_log_result(&mut self) {
        let Some(rx) = &self.log_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(entries) => {
                self.log_entries = entries;
                self.logs_loaded = true;
                self.log_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.logs_loaded = true;
                self.log_rx = None;
            }
        }
    }

    /// 在 tick 中检查擦写是否完成
    pub fn poll_wipe_result(&mut self) {
        let Some(task) = &self.wipe_task else {
            return;
        };

        // 非阻塞检查结果
        match task.result_rx.try_recv() {
            Ok(result) => {
                let device = self.selected_device().cloned();

                // 写入结束审计日志
                if let Some(device) = &device {
                    if let Some(al) = &self.audit_log {
                        let status = if result.success {
                            LogStatus::Completed
                        } else if task.progress.is_cancelled() {
                            LogStatus::Cancelled
                        } else {
                            LogStatus::Error
                        };
                        let entry = al.create_end_entry(
                            &device.path,
                            &device.serial,
                            device.size_bytes,
                            status,
                            result.error.clone(),
                        );
                        let _ = al.write_entry(entry);
                    }
                }

                // 成功后重新格式化设备
                if result.success {
                    if let Some(device) = &device {
                        let _ = crate::device::reformat_device(&device.path);
                    }
                }

                self.wipe_result = Some(result);
                self.wipe_task = None;
                self.screen = Screen::Result;
            }
            Err(mpsc::TryRecvError::Empty) => {
                // 还在运行，继续等待
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // 线程断开（不应该发生）
                self.wipe_error = Some("擦写线程异常断开".to_string());
                self.wipe_task = None;
                self.screen = Screen::Result;
            }
        }
    }
}

/// 加载审计日志条目（在后台线程中调用）
fn load_log_entries(log_dir: &std::path::Path) -> Vec<LogEntry> {
    let mut entries = Vec::new();

    if !log_dir.exists() {
        return entries;
    }

    let dir_entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return entries,
    };

    // 收集所有 jsonl 文件并按名称排序（最新的在前）
    let mut files: Vec<std::path::PathBuf> = dir_entries
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
                if let Ok(entry) = serde_json::from_str::<LogEntry>(line) {
                    entries.push(entry);
                }
            }
        }
    }

    // 按时间倒序排列（最新的在前）
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    entries
}
