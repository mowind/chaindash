use tui::style::{Color, Style};
use tui::widgets::{Block, Borders};

pub fn new<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Indexed(239 as u8)))
        .title(title)
        .title_style(Style::default().fg(Color::Indexed(249 as u8)))
}
