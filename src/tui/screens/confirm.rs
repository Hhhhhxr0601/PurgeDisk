use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::format::format_bytes;

/// 渲染确认屏幕
pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // 标题警告
            Constraint::Length(10), // 设备信息
            Constraint::Length(5),  // 序列号输入
            Constraint::Length(3),  // 底部提示
        ])
        .split(area);

    render_warning(f, chunks[0]);
    render_device_info(f, app, chunks[1]);
    render_serial_input(f, app, chunks[2]);
    render_confirm_help(f, chunks[3]);
}

fn render_warning(f: &mut Frame, area: Rect) {
    let warning = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            "  ⚠ 警告: 即将销毁设备上的所有数据，此操作不可逆！",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
    ])])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .title(" 确认操作 "),
    );
    f.render_widget(warning, area);
}

fn render_device_info(f: &mut Frame, app: &App, area: Rect) {
    let Some(device) = app.selected_device() else {
        return;
    };

    let info_lines = vec![
        Line::from(vec![
            Span::styled("  设备型号: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &device.model,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  设备路径: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&device.path, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  序 列 号: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &device.serial,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  容    量: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_bytes(device.size_bytes), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  总    线: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&device.bus_type, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  算法: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "DoD 5220.22-M (三轮覆写: 0x00 → 0xFF → 随机)",
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let info = Paragraph::new(info_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 设备信息 "),
    );
    f.render_widget(info, area);
}

fn render_serial_input(f: &mut Frame, app: &App, area: Rect) {
    let Some(device) = app.selected_device() else {
        return;
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(
            "  请输入设备序列号以确认操作:",
            Style::default().fg(Color::White),
        ),
    ])];

    // 输入框
    let input_display = if app.confirm_input.is_empty() {
        "_".to_string()
    } else {
        app.confirm_input.clone()
    };

    let input_style = if app.confirm_error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Green)
    };

    lines.push(Line::from(vec![
        Span::styled(format!("  > {}  ", input_display), input_style),
        Span::styled(
            format!("(预期: {})", device.serial),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    // 错误提示
    if let Some(err) = &app.confirm_error {
        lines.push(Line::from(Span::styled(
            format!("  ✗ {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    let border_color = if app.confirm_error.is_some() {
        Color::Red
    } else {
        Color::Yellow
    };

    let input_widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(" 序列号验证 "),
    );
    f.render_widget(input_widget, area);
}

fn render_confirm_help(f: &mut Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" 确认  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(" 取消返回"),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 操作 "),
    )
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, area);
}
