use tui::widgets::{Block, Borders};

pub fn new<'a>(title: &'a str) -> Block<'a> {
    Block::default().borders(Borders::ALL).title(title)
}
