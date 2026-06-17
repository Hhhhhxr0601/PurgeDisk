use crossterm::event::{self, Event, KeyEvent};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// TUI 事件
#[derive(Debug, Clone)]
pub enum TuiEvent {
    /// 键盘事件
    Key(KeyEvent),
    /// 定时 tick（用于刷新进度等）
    Tick,
}

/// 事件处理器
///
/// 在后台线程中读取终端事件，通过 channel 发送给主循环。
pub struct EventHandler {
    rx: mpsc::Receiver<TuiEvent>,
    _handle: thread::JoinHandle<()>,
}

impl EventHandler {
    /// 创建事件处理器
    ///
    /// - `tick_rate`: tick 间隔，用于定时刷新 UI
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            loop {
                // 检查是否有键盘事件
                if event::poll(tick_rate).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = event::read() {
                        if tx.send(TuiEvent::Key(key)).is_err() {
                            return;
                        }
                    }
                }
                // 无论是否有键盘事件，都发送 tick
                if tx.send(TuiEvent::Tick).is_err() {
                    return;
                }
            }
        });

        Self {
            rx,
            _handle: handle,
        }
    }

    /// 接收下一个事件（阻塞）
    pub fn next(&self) -> Result<TuiEvent, mpsc::RecvError> {
        self.rx.recv()
    }
}
