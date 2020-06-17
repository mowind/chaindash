use tui::backend::Backend;
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::widgets::{Block, Borders};
use tui::{Frame, Terminal};

pub fn draw<B: Backend>(terminal: &mut Terminal<B>) {
    terminal
        .draw(|mut frame| {
            let chunks = Layout::default()
                .constraints(vec![Constraint::Percentage(100)])
                .split(frame.size());
            draw_widgets(&mut frame, chunks[0])
        })
        .unwrap();
}

pub fn draw_widgets<B: Backend>(frame: &mut Frame<B>, area: Rect) {
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(area);
    draw_top_row(frame, vertical_chunks[0]);
    draw_bottom_row(frame, vertical_chunks[1]);
}

pub fn draw_top_row<B: Backend>(frame: &mut Frame<B>, area: Rect) {
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)].as_ref())
        .split(area);

    let block = Block::default()
        .title("Block Interval")
        .borders(Borders::ALL);
    frame.render_widget(block, horizontal_chunks[0]);

    let block = Block::default()
        .title("Block Transaction Count")
        .borders(Borders::ALL);
    frame.render_widget(block, horizontal_chunks[1]);
}

pub fn draw_bottom_row<B: Backend>(frame: &mut Frame<B>, area: Rect) {
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(area);
    let block = Block::default().title("Node Status").borders(Borders::ALL);
    frame.render_widget(block, horizontal_chunks[0]);
}
