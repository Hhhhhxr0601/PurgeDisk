use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui::Frame;
use std::sync::atomic::Ordering;

use crate::tui::app::App;
use crate::tui::format::{format_bytes, format_duration};
use crate::wipe::DOD_PASSES;

/// 渲染进度屏幕
pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let Some(task) = &app.wipe_task else {
        return;
    };

    let progress = &task.progress;
    let elapsed = task.start_time.elapsed().as_secs_f64();
    let current_pass = progress.current_pass.load(Ordering::Relaxed) as usize;
    let written = progress.written_bytes.load(Ordering::Relaxed);
    let total = progress.total_bytes;
    let overall_percent = progress.progress_percent();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 标题
            Constraint::Length(3), // 第1轮
            Constraint::Length(3), // 第2轮
            Constraint::Length(3), // 第3轮
            Constraint::Length(3), // 总体进度
            Constraint::Length(5), // 统计信息
            Constraint::Length(3), // 操作提示
        ])
        .split(area);

    // 标题
    let title = Paragraph::new("  正在执行 DoD 5220.22-M 三轮覆写...")
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" 擦写进行中 "),
        )
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(title, chunks[0]);

    // 三轮进度条
    for (i, pass) in DOD_PASSES.iter().enumerate() {
        render_pass_bar(f, chunks[i + 1], i, pass.description(), current_pass, total, written);
    }

    // 总体进度
    let overall_label = format!("{:.1}%", overall_percent);
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 总体进度 "),
        )
        .gauge_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
        .label(overall_label)
        .ratio((overall_percent / 100.0).min(1.0));
    f.render_widget(gauge, chunks[4]);

    // 统计信息
    let speed = if elapsed > 0.0 {
        written as f64 / elapsed
    } else {
        0.0
    };
    let eta = if overall_percent > 0.5 {
        let total_est = elapsed / (overall_percent / 100.0);
        total_est - elapsed
    } else {
        0.0
    };

    let stats_lines = vec![
        Line::from(vec![
            Span::styled("  速度: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}/s", format_bytes(speed as u64)),
                Style::default().fg(Color::Green),
            ),
            Span::styled("    已写入: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(written),
                Style::default().fg(Color::White),
            ),
            Span::styled(" / ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_bytes(total), Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("  耗时: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_duration(elapsed), Style::default().fg(Color::White)),
            Span::styled("    预计剩余: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_duration(eta), Style::default().fg(Color::Cyan)),
        ]),
    ];
    let stats = Paragraph::new(stats_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 统计信息 "),
    );
    f.render_widget(stats, chunks[5]);

    // 操作提示
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" c / Esc", Style::default().fg(Color::Red)),
        Span::styled(" 取消擦写", Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 操作 "),
    );
    f.render_widget(help, chunks[6]);
}

/// 渲染单轮进度条
fn render_pass_bar(
    f: &mut Frame,
    area: Rect,
    pass_index: usize,
    description: &str,
    current_pass: usize,
    total_bytes: u64,
    written_bytes: u64,
) {
    let (percent, style, status_icon) = if pass_index < current_pass {
        // 已完成
        (100.0, Style::default().fg(Color::Green), "✓")
    } else if pass_index == current_pass {
        // 当前轮次
        let pass_written = written_bytes; // written_bytes 是当前轮次的
        let pct = if total_bytes > 0 {
            (pass_written as f64 / total_bytes as f64 * 100.0).min(100.0)
        } else {
            0.0
        };
        (
            pct,
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            "▶",
        )
    } else {
        // 未开始
        (0.0, Style::default().fg(Color::DarkGray), "○")
    };

    let patterns = ["0x00 填充", "0xFF 填充", "随机数据"];
    let label = format!(
        "{} 第 {} 轮: {}  {:.1}%",
        status_icon,
        pass_index + 1,
        patterns.get(pass_index).unwrap_or(&description),
        percent
    );

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(format!(
            " 第 {} 轮 ",
            pass_index + 1
        )))
        .gauge_style(style)
        .label(label)
        .ratio((percent / 100.0).min(1.0));
    f.render_widget(gauge, area);
}
