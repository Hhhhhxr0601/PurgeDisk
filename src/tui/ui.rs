use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::app::{App, Screen};
use crate::tui::screens::{confirm, home, logs, progress, result};

/// 顶层渲染函数
pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    // 整体布局：标题栏 + 内容区
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 标题栏
            Constraint::Min(5),   // 内容区
        ])
        .split(area);

    render_title_bar(f, chunks[0]);

    // 根据当前屏幕渲染内容
    match app.screen {
        Screen::Home => home::render(f, app, chunks[1]),
        Screen::Confirm => confirm::render(f, app, chunks[1]),
        Screen::Progress => progress::render(f, app, chunks[1]),
        Screen::Result => result::render(f, app, chunks[1]),
        Screen::Logs => logs::render(f, app, chunks[1]),
    }
}

/// 渲染顶部标题栏
fn render_title_bar(f: &mut Frame, area: Rect) {
    let title = Line::from(vec![
        Span::styled(
            " PurgeDisk",
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            " — 企业级 U 盘数据合规销毁工具",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let bar = Paragraph::new(title).style(Style::default().bg(Color::Rgb(30, 30, 46)));
    f.render_widget(bar, area);
}
