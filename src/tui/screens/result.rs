use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::tui::app::App;
use crate::tui::format::{format_bytes, format_duration};

/// 渲染结果屏幕
pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 状态标题
            Constraint::Min(5),   // 详情
            Constraint::Length(3), // 操作提示
        ])
        .split(area);

    // 如果有错误（设备打开失败等）
    if let Some(err) = &app.wipe_error {
        render_error(f, err, chunks[0]);
        let empty = Paragraph::new("").block(Block::default());
        f.render_widget(empty, chunks[1]);
        render_result_help(f, chunks[2]);
        return;
    }

    // 擦写结果
    if let Some(result) = &app.wipe_result {
        render_status(f, result.success, chunks[0]);
        render_details(f, result, chunks[1]);
    }

    render_result_help(f, chunks[2]);
}

fn render_error(f: &mut Frame, error: &str, area: Rect) {
    let lines = vec![
        Line::from(vec![
            Span::styled("  ✗ 操作失败", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(format!("  {}", error), Style::default().fg(Color::Red)),
        ]),
    ];
    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .title(" 错误 "),
    );
    f.render_widget(widget, area);
}

fn render_status(f: &mut Frame, success: bool, area: Rect) {
    let (icon, text, color) = if success {
        ("✓", "擦写完成！", Color::Green)
    } else {
        ("✗", "擦写未完成", Color::Red)
    };

    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("  {} {}", icon, text),
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color))
            .title(" 操作结果 "),
    );
    f.render_widget(status, area);
}

fn render_details(f: &mut Frame, result: &crate::wipe::WipeResult, area: Rect) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("  总写入: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(result.total_written),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("  设备容量: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(result.total_size),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("  总耗时: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_duration(result.elapsed_seconds),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("  平均速度: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if result.elapsed_seconds > 0.0 {
                    format!(
                        "{}/s",
                        format_bytes(
                            (result.total_written as f64 / result.elapsed_seconds) as u64
                        )
                    )
                } else {
                    "N/A".to_string()
                },
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  轮次详情:",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
    ];

    for pass in &result.passes {
        let (icon, color) = if pass.success {
            ("✓", Color::Green)
        } else {
            ("✗", Color::Red)
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("    {} 第 {} 轮: ", icon, pass.pass_index + 1),
                Style::default().fg(color),
            ),
            Span::styled(
                format!("{} - ", pass.pattern.description()),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format_bytes(pass.bytes_written),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!(" - {}", format_duration(pass.elapsed_seconds)),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        if let Some(err) = &pass.error {
            lines.push(Line::from(Span::styled(
                format!("      错误: {}", err),
                Style::default().fg(Color::Red),
            )));
        }
    }

    if let Some(err) = &result.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  错误信息: {}", err),
            Style::default().fg(Color::Red),
        )));
    }

    let details = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 详情 "),
    );
    f.render_widget(details, area);
}

fn render_result_help(f: &mut Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" Enter / q", Style::default().fg(Color::Yellow)),
        Span::raw(" 返回设备列表"),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 操作 "),
    )
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, area);
}
