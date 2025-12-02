use tui::{
    backend::Backend,
    layout::{
        Constraint,
        Direction,
        Layout,
        Rect,
    },
    Frame,
    Terminal,
};

use crate::{
    app::{
        App,
        Widgets,
    },
    collect::SharedData,
    widgets::{
        DiskListWidget,
        SystemSummaryWidget,
    },
};

pub fn draw<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) {
    terminal
        .draw(|mut frame| {
            let chunks = Layout::default()
                .constraints(vec![Constraint::Percentage(100)])
                .split(frame.size());
            draw_widgets(&mut frame, &mut app.widgets, app.data.clone(), chunks[0])
        })
        .unwrap();
}

pub fn draw_widgets<B: Backend>(
    frame: &mut Frame<B>,
    widgets: &mut Widgets,
    data: SharedData,
    area: Rect,
) {
    #[cfg(target_family = "unix")]
    {
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Percentage(20),
                    Constraint::Percentage(25),
                    Constraint::Percentage(55),
                ]
                .as_ref(),
            )
            .split(area);
        // 使用新的左右分屏布局
        draw_system_row_split(frame, widgets, data, vertical_chunks[0]);
        draw_top_row(frame, widgets, vertical_chunks[1]);
        draw_bottom_row(frame, widgets, vertical_chunks[2]);
    }

    #[cfg(not(target_family = "unix"))]
    {
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
            .split(area);
        draw_top_row(frame, widgets, vertical_chunks[0]);
        draw_bottom_row(frame, widgets, vertical_chunks[1]);
    }
}

#[cfg(target_family = "unix")]
pub fn draw_system_row_split<B: Backend>(
    frame: &mut Frame<B>,
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

pub fn draw_top_row<B: Backend>(
    frame: &mut Frame<B>,
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

pub fn draw_bottom_row<B: Backend>(
    frame: &mut Frame<B>,
    widgets: &mut Widgets,
    area: Rect,
) {
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(area);

    frame.render_widget(&widgets.node, horizontal_chunks[0]);
}
