use tui::{
    style::{
        Color,
        Style,
    },
    widgets::{
        Block,
        Borders,
    },
};

pub fn new<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Indexed(239_u8)))
        .title(title)
        .title_style(Style::default().fg(Color::Indexed(249_u8)))
}
