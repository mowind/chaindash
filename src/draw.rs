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
        StatusLevel,
        StatusMessage,
    },
    error::{
        ChaindashError,
        Result,
    },
    sync::lock_or_panic,
    widgets::block,
};

const AUXILIARY_ROW_HEIGHT: u16 = 6;

pub fn draw<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let status_message = {
        let data = lock_or_panic(&app.data);
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

            draw_widgets(frame, &mut app.widgets, layout[main_area_index])
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
    auxiliary_height: u16,
) -> (u16, u16) {
    let remaining = total_height.saturating_sub(auxiliary_height);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DashboardRows {
    auxiliary: Rect,
    charts: Rect,
    bottom: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardMode {
    #[cfg(target_family = "unix")]
    Unix,
    #[cfg(any(not(target_family = "unix"), test))]
    NonUnix,
}

fn split_dashboard_rows(
    area: Rect,
    auxiliary_height: u16,
) -> DashboardRows {
    let auxiliary_height = auxiliary_height.min(area.height);
    let (chart_height, bottom_height) = content_row_heights(area.height, auxiliary_height);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(auxiliary_height),
                Constraint::Length(chart_height),
                Constraint::Length(bottom_height),
            ]
            .as_ref(),
        )
        .split(area);

    DashboardRows {
        auxiliary: rows[0],
        charts: rows[1],
        bottom: rows[2],
    }
}

fn draw_widgets_for_mode(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
    mode: DashboardMode,
) {
    let rows = split_dashboard_rows(area, AUXILIARY_ROW_HEIGHT);

    match mode {
        #[cfg(target_family = "unix")]
        DashboardMode::Unix => draw_system_row_split(frame, widgets, rows.auxiliary),
        #[cfg(any(not(target_family = "unix"), test))]
        DashboardMode::NonUnix => frame.render_widget(&widgets.peer_countries, rows.auxiliary),
    }

    draw_top_row(frame, widgets, rows.charts);
    draw_bottom_section(frame, widgets, rows.bottom);
}

/// Draw the complete dashboard using the platform-appropriate auxiliary row.
pub fn draw_widgets(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
) {
    #[cfg(target_family = "unix")]
    let mode = DashboardMode::Unix;
    #[cfg(not(target_family = "unix"))]
    let mode = DashboardMode::NonUnix;

    draw_widgets_for_mode(frame, widgets, area, mode);
}

fn split_equal_columns(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
        .split(area)
}

/// Draw the Unix six-row System Stats, Disk Details, and Peer Countries strip.
#[cfg(target_family = "unix")]
pub fn draw_system_row_split(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
) {
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [Constraint::Percentage(50), Constraint::Percentage(25), Constraint::Percentage(25)]
                .as_ref(),
        )
        .split(area);

    frame.render_widget(&widgets.system_summary, horizontal_chunks[0]);
    frame.render_widget(&widgets.disk_list, horizontal_chunks[1]);
    frame.render_widget(&widgets.peer_countries, horizontal_chunks[2]);
}

/// Draw Block Time and Block Transactions side by side across the chart row.
pub fn draw_top_row(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
) {
    let columns = split_equal_columns(area);

    frame.render_widget(&widgets.time, columns[0]);
    frame.render_widget(&widgets.txs, columns[1]);
}

/// Draw Node and Node Details in their unchanged equal-width bottom layout.
pub fn draw_bottom_section(
    frame: &mut Frame,
    widgets: &mut Widgets,
    area: Rect,
) {
    let horizontal_chunks = split_equal_columns(area);

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
    fn test_draw_top_row_splits_block_charts_side_by_side() {
        use clap::Parser;
        use ratatui::backend::TestBackend;

        use crate::{
            app::setup_app,
            opts::Opts,
        };

        let opts = Opts::parse_from([
            "test",
            "--url",
            "test@ws://127.0.0.1:6789",
            "--db-path",
            ":memory:",
        ]);
        let mut app = setup_app(&opts);
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("terminal should create");
        terminal
            .draw(|frame| draw_top_row(frame, &mut app.widgets, Rect::new(0, 0, 80, 20)))
            .expect("draw should succeed");
        let buf = terminal.backend().buffer().clone();

        // Block Time and Block Transactions use equal-width, full-height columns.
        assert_eq!(buf.get(2, 0).symbol(), "B");
        assert_eq!(buf.get(8, 0).symbol(), "T");
        assert_eq!(buf.get(9, 0).symbol(), "i");
        assert_eq!(buf.get(42, 0).symbol(), "B");
        assert_eq!(buf.get(48, 0).symbol(), "T");
        assert_eq!(buf.get(49, 0).symbol(), "r");
        assert_eq!(buf.get(0, 19).symbol(), "└");
        assert_eq!(buf.get(40, 19).symbol(), "└");
        // Panel borders separate the two equal-width charts.
        assert_eq!(buf.get(39, 0).symbol(), "┐");
        assert_eq!(buf.get(40, 0).symbol(), "┌");
    }

    #[test]
    fn test_draw_non_unix_mode_keeps_peer_strip_above_charts_and_bottom() {
        use clap::Parser;
        use ratatui::backend::TestBackend;

        use crate::{
            app::setup_app,
            opts::Opts,
        };

        let opts = Opts::parse_from([
            "test",
            "--url",
            "test@ws://127.0.0.1:6789",
            "--db-path",
            ":memory:",
        ]);
        let mut app = setup_app(&opts);
        assert!(app.refresh_geo_snapshot());
        let area = Rect::new(0, 0, 120, 30);
        let rows = split_dashboard_rows(area, AUXILIARY_ROW_HEIGHT);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("terminal should create");
        terminal
            .draw(|frame| {
                draw_widgets_for_mode(frame, &mut app.widgets, area, DashboardMode::NonUnix)
            })
            .expect("draw should succeed");
        let buf = terminal.backend().buffer().clone();

        assert_eq!(rows.auxiliary, Rect::new(0, 0, 120, 6));
        assert_eq!(rows.charts, Rect::new(0, 6, 120, 14));
        assert_eq!(rows.bottom, Rect::new(0, 20, 120, 10));
        assert_eq!(buf.get(2, 0).symbol(), "P");
        assert_eq!(buf.get(1, 1).symbol(), "N");
        assert_eq!(buf.get(2, 6).symbol(), "B");
        assert_eq!(buf.get(9, 6).symbol(), "i");
        assert_eq!(buf.get(62, 6).symbol(), "B");
        assert_eq!(buf.get(69, 6).symbol(), "r");
        assert_eq!(buf.get(2, 20).symbol(), "N");
        assert_eq!(buf.get(62, 20).symbol(), "N");
    }

    #[test]
    fn test_content_row_heights_handles_small_layouts() {
        assert_eq!(content_row_heights(16, 0), (8, 8));
        assert_eq!(content_row_heights(15, 5), (5, 5));
        assert_eq!(content_row_heights(16, 6), (5, 5));
    }

    #[test]
    fn test_dashboard_rows_keep_auxiliary_strip_above_block_and_node_sections() {
        let rows = split_dashboard_rows(Rect::new(4, 7, 100, 30), AUXILIARY_ROW_HEIGHT);

        assert_eq!(rows.auxiliary, Rect::new(4, 7, 100, 6));
        assert_eq!(rows.charts, Rect::new(4, 13, 100, 14));
        assert_eq!(rows.bottom, Rect::new(4, 27, 100, 10));
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_draw_widgets_uses_final_unix_panel_geometry() {
        use clap::Parser;
        use ratatui::backend::TestBackend;

        use crate::{
            app::setup_app,
            opts::Opts,
        };

        let opts = Opts::parse_from([
            "test",
            "--url",
            "test@ws://127.0.0.1:6789",
            "--db-path",
            ":memory:",
        ]);
        let mut app = setup_app(&opts);
        let area = Rect::new(0, 0, 120, 40);
        let rows = split_dashboard_rows(area, AUXILIARY_ROW_HEIGHT);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("terminal should create");
        terminal
            .draw(|frame| draw_widgets(frame, &mut app.widgets, area))
            .expect("draw should succeed");
        let buf = terminal.backend().buffer().clone();

        assert_eq!(rows.auxiliary, Rect::new(0, 0, 120, 6));
        assert_eq!(rows.charts, Rect::new(0, 6, 120, 21));
        assert_eq!(rows.bottom, Rect::new(0, 27, 120, 13));

        // Unix system strip: System Stats 50%, Disk Details 25%, Peer Countries 25%.
        assert_eq!(buf.get(2, 0).symbol(), "S");
        assert_eq!(buf.get(62, 0).symbol(), "D");
        assert_eq!(buf.get(92, 0).symbol(), "P");
        assert_eq!(buf.get(59, 0).symbol(), "┐");
        assert_eq!(buf.get(60, 0).symbol(), "┌");
        assert_eq!(buf.get(89, 0).symbol(), "┐");
        assert_eq!(buf.get(90, 0).symbol(), "┌");

        // Block charts fill the row side by side, with the unchanged bottom split below them.
        assert_eq!(buf.get(2, 6).symbol(), "B");
        assert_eq!(buf.get(9, 6).symbol(), "i");
        assert_eq!(buf.get(62, 6).symbol(), "B");
        assert_eq!(buf.get(69, 6).symbol(), "r");
        assert_eq!(buf.get(0, 26).symbol(), "└");
        assert_eq!(buf.get(60, 26).symbol(), "└");
        assert_eq!(buf.get(2, 27).symbol(), "N");
        assert_eq!(buf.get(62, 27).symbol(), "N");
        assert_eq!(buf.get(59, 27).symbol(), "┐");
        assert_eq!(buf.get(60, 27).symbol(), "┌");
    }

    #[test]
    fn test_draw_keeps_status_panel_above_dashboard() {
        use clap::Parser;
        use ratatui::backend::TestBackend;

        use crate::{
            app::setup_app,
            opts::Opts,
        };

        let opts = Opts::parse_from([
            "test",
            "--url",
            "test@ws://127.0.0.1:6789",
            "--db-path",
            ":memory:",
        ]);
        let mut app = setup_app(&opts);
        app.data.lock().expect("mutex poisoned").set_status_message(StatusLevel::Info, "synced");
        let mut terminal =
            Terminal::new(TestBackend::new(120, 40)).expect("terminal should create");

        draw(&mut terminal, &mut app).expect("draw should succeed");
        let buf = terminal.backend().buffer().clone();

        assert_eq!(buf.get(0, 2).symbol(), "└");
        assert_eq!(buf.get(0, 3).symbol(), "┌");
        #[cfg(target_family = "unix")]
        assert_eq!(buf.get(2, 3).symbol(), "S");
        #[cfg(not(target_family = "unix"))]
        assert_eq!(buf.get(2, 3).symbol(), "P");
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
