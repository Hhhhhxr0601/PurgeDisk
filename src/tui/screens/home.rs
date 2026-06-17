use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::format::format_bytes;

/// 渲染 Home 屏幕
pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // 设备列表
            Constraint::Length(3), // 底部提示
        ])
        .split(area);

    render_device_list(f, app, chunks[0]);
    render_help_bar(f, chunks[1]);
}

/// 渲染设备列表
fn render_device_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" ⚡ PurgeDisk — 可移动存储设备 ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    if !app.devices_loaded {
        let loading = Paragraph::new("正在扫描可移动存储设备...")
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading, area);
        return;
    }

    if let Some(err) = &app.device_error {
        let error_msg = Paragraph::new(format!("扫描失败: {}", err))
            .block(block)
            .style(Style::default().fg(Color::Red));
        f.render_widget(error_msg, area);
        return;
    }

    if app.devices.is_empty() {
        let empty_msg = Paragraph::new(vec![
            Line::from(""),
            Line::from("  未检测到可移动存储设备。"),
            Line::from("  请确认 U 盘已插入并被系统识别。"),
            Line::from(""),
            Line::from(vec![
                Span::styled("  按 ", Style::default()),
                Span::styled("r", Style::default().fg(Color::Yellow)),
                Span::styled(" 刷新设备列表", Style::default()),
            ]),
        ])
        .block(block);
        f.render_widget(empty_msg, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 表头
    let header = Row::new(vec![
        Cell::from("#"),
        Cell::from("设备型号"),
        Cell::from("路径"),
        Cell::from("序列号"),
        Cell::from("容量"),
        Cell::from("总线"),
    ])
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .height(1);

    // 数据行
    let rows: Vec<Row> = app
        .devices
        .iter()
        .enumerate()
        .map(|(i, dev)| {
            let style = if i == app.selected_device {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(dev.model.clone()),
                Cell::from(dev.path.clone()),
                Cell::from(dev.serial.clone()),
                Cell::from(format_bytes(dev.size_bytes)),
                Cell::from(dev.bus_type.clone()),
            ])
            .style(style)
            .height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
        ],
    )
    .header(header)
    .block(Block::default());

    f.render_widget(table, inner);
}

/// 渲染底部帮助栏
fn render_help_bar(f: &mut Frame, area: Rect) {
    let help_text = Line::from(vec![
        Span::styled(" ↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(" 选择  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" 擦写  "),
        Span::styled("l", Style::default().fg(Color::Yellow)),
        Span::raw(" 日志  "),
        Span::styled("r", Style::default().fg(Color::Yellow)),
        Span::raw(" 刷新  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" 退出"),
    ]);

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 操作 "),
        )
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(help, area);
}
