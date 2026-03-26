use ratatui::{
    backend::Backend,
    layout::{
        Constraint,
        Direction,
        Layout,
        Rect,
    },
    style::{
        Color,
        Modifier,
        Style,
    },
    text::{
        Line,
        Span,
    },
    widgets::{
        Block,
        Borders,
        Paragraph,
    },
    Frame,
    Terminal,
};

use crate::{
    app::{
        App,
        Widgets,
    },
    collect::{
        SharedData,
        StatusLevel,
        StatusMessage,
    },
    error::{
        ChaindashError,
        Result,
    },
    widgets::block,
};

pub fn draw<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let status_message = {
        let mut data = app.data.lock().expect("mutex poisoned - recovering");
        data.status_message()
    };

    terminal
        .draw(|frame| {
            let mut constraints = Vec::new();
            if status_message.is_some() {
                constraints.push(Constraint::Length(3));
            }
            constraints.push(Constraint::Min(1));

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(frame.size());

            let mut main_area_index = 0;
            if let Some(ref message) = status_message {
                draw_status_bar(frame, layout[0], message);
                main_area_index = 1;
            }

            draw_widgets(frame, &mut app.widgets, app.data.clone(), layout[main_area_index])
        })
        .map_err(|err| ChaindashError::Terminal(err.to_string()))?;

    Ok(())
}

fn draw_status_bar(
    frame: &mut Frame,
    area: Rect,
    message: &StatusMessage,
) {
    let (label, color) = match message.level {
        StatusLevel::Info => ("INFO", Color::Cyan),
        StatusLevel::Warn => ("WARN", Color::Yellow),
        StatusLevel::Error => ("ERROR", Color::Red),
    };

    let content = Line::from(vec![
        Span::styled(
            format!("[{label}] "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(message.text.as_str(), Style::default().fg(color)),
    ]);

    let paragraph = Paragraph::new(content).style(Style::default().bg(block::PANEL_BG)).block(
        Block::default()
            .title("Status")
            .borders(Borders::ALL)
            .style(Style::default().bg(block::PANEL_BG))
            .border_style(Style::default().fg(color).bg(block::PANEL_BG)),
    );

    frame.render_widget(paragraph, area);
}

fn content_row_heights(
    total_height: u16,
    system_height: u16,
) -> (u16, u16) {
    let remaining = total_height.saturating_sub(system_height);
    if remaining <= 16 {
        let bottom = remaining / 2;
        let chart = remaining.saturating_sub(bottom);
        return (chart, bottom);
    }

    let min_chart = 8;
    let preferred_bottom = if remaining >= 28 {
        remaining * 2 / 5
    } else if remaining >= 22 {
        10
    } else {
        8
    };
    let max_bottom = remaining.saturating_sub(min_chart);
    let bottom = preferred_bottom.min(max_bottom).max(8);
    let chart = remaining.saturating_sub(bottom);

    (chart, bottom)
}

pub fn draw_widgets(
    frame: &mut Frame,
    widgets: &mut Widgets,
    data: SharedData,
    area: Rect,
) {
    #[cfg(target_family = "unix")]
    {
        let system_height = 5;
        let (chart_height, bottom_height) = content_row_heights(area.height, system_height);
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(system_height),
                    Constraint::Length(chart_height),
                    Constraint::Length(bottom_height),
                ]
                .as_ref(),
            )
            .split(area);
        draw_system_row_split(frame, widgets, data, vertical_chunks[0]);
        draw_top_row(frame, widgets, vertical_chunks[1]);
        draw_bottom_section(frame, widgets, vertical_chunks[2]);
    }

    #[cfg(not(target_family = "unix"))]
    {
        let (chart_height, bottom_height) = content_row_heights(area.height, 0);
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [Constraint::Length(chart_height), Constraint::Length(bottom_height)].as_ref(),
            )
            .split(area);
        draw_top_row(frame, widgets, vertical_chunks[0]);
        draw_bottom_section(frame, widgets, vertical_chunks[1]);
    }
}

#[cfg(target_family = "unix")]
pub fn draw_system_row_split(
    frame: &mut Frame,
    widgets: &mut Widgets,
    _data: SharedData,
    area: Rect,
) {
    // 左右分屏布局：左侧70%显示系统摘要，右侧50%显示磁盘列表
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)].as_ref())
        .split(area);

    // 使用widgets中已有的实例
    frame.render_widget(&widgets.system_summary, horizontal_chunks[0]);
    frame.render_widget(&widgets.disk_list, horizontal_chunks[1]);
}

pub fn draw_top_row(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
) {
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
        .split(area);

    frame.render_widget(&widgets.time, horizontal_chunks[0]);
    frame.render_widget(&widgets.txs, horizontal_chunks[1]);
}

pub fn draw_bottom_section(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
) {
    let detail_percentage = if area.width >= 180 { 60 } else { 58 };
    let node_percentage = 100 - detail_percentage;
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [Constraint::Percentage(node_percentage), Constraint::Percentage(detail_percentage)]
                .as_ref(),
        )
        .split(area);

    frame.render_widget(&widgets.node, horizontal_chunks[0]);
    frame.render_widget(&widgets.node_details, horizontal_chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_row_heights_balances_tall_layouts() {
        assert_eq!(content_row_heights(40, 5), (21, 14));
        assert_eq!(content_row_heights(30, 5), (15, 10));
    }

    #[test]
    fn test_content_row_heights_handles_small_layouts() {
        assert_eq!(content_row_heights(16, 0), (8, 8));
        assert_eq!(content_row_heights(15, 5), (5, 5));
    }
}
