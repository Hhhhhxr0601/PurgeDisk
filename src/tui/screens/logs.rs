use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::audit::LogStatus;
use crate::tui::app::App;

/// 渲染审计日志查看屏幕
pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // 日志表格
            Constraint::Length(3), // 底部提示
        ])
        .split(area);

    render_log_table(f, app, chunks[0]);
    render_log_help(f, app, chunks[1]);
}

fn render_log_table(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(
        " 审计日志 (共 {} 条) ",
        app.log_entries.len()
    );

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    if !app.logs_loaded {
        let loading = Paragraph::new("正在加载审计日志...")
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading, area);
        return;
    }

    if app.log_entries.is_empty() {
        let empty = Paragraph::new("  暂无审计日志记录。")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 表头
    let header = Row::new(vec![
        Cell::from("时间"),
        Cell::from("操作人"),
        Cell::from("设备序列号"),
        Cell::from("算法"),
        Cell::from("状态"),
        Cell::from("详情"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .height(1);

    // 数据行
    let rows: Vec<Row> = app
        .log_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let style = if i == app.log_scroll {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let status_color = match entry.status {
                LogStatus::Completed => Color::Green,
                LogStatus::Error | LogStatus::Interrupted => Color::Red,
                LogStatus::Cancelled => Color::Yellow,
                LogStatus::Started => Color::Cyan,
                LogStatus::PassComplete => Color::Blue,
            };

            let short_ts = if entry.timestamp.len() >= 19 {
                &entry.timestamp[..19]
            } else {
                &entry.timestamp
            };

            let detail = entry.error_detail.as_deref().unwrap_or("");

            Row::new(vec![
                Cell::from(short_ts.to_string()),
                Cell::from(truncate(&entry.operator, 12)),
                Cell::from(truncate(&entry.device_serial, 20)),
                Cell::from(truncate(&entry.algorithm, 15)),
                Cell::from(Span::styled(
                    entry.status.as_str().to_string(),
                    Style::default().fg(status_color),
                )),
                Cell::from(truncate(detail, 30)),
            ])
            .style(style)
            .height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Length(12),
            Constraint::Percentage(25),
        ],
    )
    .header(header)
    .block(Block::default());

    f.render_widget(table, inner);
}

fn render_log_help(f: &mut Frame, app: &App, area: Rect) {
    let pos_info = if app.log_entries.is_empty() {
        String::new()
    } else {
        format!(
            "  {}/{}  ",
            app.log_scroll + 1,
            app.log_entries.len()
        )
    };

    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(" 滚动  "),
        Span::styled(pos_info, Style::default().fg(Color::DarkGray)),
        Span::styled("Esc / q", Style::default().fg(Color::Yellow)),
        Span::raw(" 返回"),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 操作 "),
    )
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, area);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
