use ratatui::{
    backend::Backend,
    layout::{
        Constraint,
        Direction,
        Layout,
        Rect,
    },
    style::Color,
    text::{
        Line,
        Span,
    },
    widgets::Paragraph,
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

#[cfg(target_family = "unix")]
const SYSTEM_ROW_HEIGHT: u16 = 6;

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

const STATUS_TITLE: &str = " Status ";

fn status_level_label(level: StatusLevel) -> (&'static str, Color) {
    match level {
        StatusLevel::Info => ("INFO", block::ACCENT_INFO),
        StatusLevel::Warn => ("WARN", block::ACCENT_WARN),
        StatusLevel::Error => ("ERROR", block::ACCENT_ERROR),
    }
}

fn status_paragraph<'a>(message: &'a StatusMessage) -> Paragraph<'a> {
    let (label, color) = status_level_label(message.level);
    let content = Line::from(vec![
        Span::styled(format!(" {label} "), block::badge_style(color)),
        Span::styled(" ", block::content_style()),
        Span::styled("• ", block::accent_style(color)),
        Span::styled(message.text.as_str(), block::content_style()),
    ]);

    Paragraph::new(content).style(block::content_style()).block(block::new(STATUS_TITLE))
}

fn draw_status_bar(
    frame: &mut Frame,
    area: Rect,
    message: &StatusMessage,
) {
    frame.render_widget(status_paragraph(message), area);
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
        let (chart_height, bottom_height) = content_row_heights(area.height, SYSTEM_ROW_HEIGHT);
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(SYSTEM_ROW_HEIGHT),
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

fn split_aligned_columns(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
        .split(area)
}

#[cfg(target_family = "unix")]
pub fn draw_system_row_split(
    frame: &mut Frame,
    widgets: &mut Widgets,
    _data: SharedData,
    area: Rect,
) {
    let horizontal_chunks = split_aligned_columns(area);

    frame.render_widget(&widgets.system_summary, horizontal_chunks[0]);
    frame.render_widget(&widgets.disk_list, horizontal_chunks[1]);
}

pub fn draw_top_row(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
) {
    let horizontal_chunks = split_aligned_columns(area);

    frame.render_widget(&widgets.time, horizontal_chunks[0]);
    frame.render_widget(&widgets.txs, horizontal_chunks[1]);
}

pub fn draw_bottom_section(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
) {
    let horizontal_chunks = split_aligned_columns(area);

    frame.render_widget(&widgets.node, horizontal_chunks[0]);
    frame.render_widget(&widgets.node_details, horizontal_chunks[1]);
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::Buffer,
        widgets::Widget,
    };

    use super::*;
    use crate::collect::Data;

    fn render_status_bar_buffer(
        level: StatusLevel,
        text: &str,
    ) -> Buffer {
        let area = Rect::new(0, 0, 48, 3);
        let mut buf = Buffer::empty(area);
        let mut data = Data::default();
        data.set_status_message(level, text);
        let message = data.status_message().expect("status message should exist");

        status_paragraph(&message).render(area, &mut buf);
        buf
    }

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

    #[test]
    fn test_status_bar_uses_standard_panel_border_and_title() {
        let buf = render_status_bar_buffer(StatusLevel::Info, "synced");

        assert_eq!(buf.get(0, 0).symbol(), "┌");
        assert_eq!(buf.get(47, 2).symbol(), "┘");
        assert_eq!(buf.get(0, 0).fg, block::PANEL_BORDER);
        assert_eq!(buf.get(2, 0).symbol(), "S");
        assert_eq!(buf.get(2, 0).fg, block::PANEL_TITLE);
    }

    #[test]
    fn test_status_bar_highlights_badge_without_tinting_message_body() {
        let buf = render_status_bar_buffer(StatusLevel::Warn, "disk alert");

        assert_eq!(buf.get(2, 1).symbol(), "W");
        assert_eq!(buf.get(2, 1).fg, block::PANEL_BG);
        assert_eq!(buf.get(2, 1).bg, block::ACCENT_WARN);
        assert_eq!(buf.get(8, 1).symbol(), "•");
        assert_eq!(buf.get(8, 1).fg, block::ACCENT_WARN);
        assert_eq!(buf.get(10, 1).symbol(), "d");
        assert_eq!(buf.get(10, 1).fg, block::PANEL_TEXT);
        assert_eq!(buf.get(10, 1).bg, block::PANEL_BG);
    }
}
