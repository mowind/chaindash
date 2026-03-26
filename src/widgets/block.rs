use ratatui::{
    style::{
        Color,
        Modifier,
        Style,
    },
    widgets::{
        Block,
        Borders,
    },
};

pub const PANEL_BG: Color = Color::Rgb(18, 28, 38);
pub const PANEL_TEXT: Color = Color::Indexed(250);
pub const PANEL_MUTED: Color = Color::Indexed(245);
pub const PANEL_BORDER: Color = Color::Indexed(239);
pub const PANEL_TITLE: Color = Color::Indexed(252);

pub fn content_style() -> Style {
    Style::default().fg(PANEL_TEXT).bg(PANEL_BG)
}

pub fn muted_style() -> Style {
    Style::default().fg(PANEL_MUTED).bg(PANEL_BG)
}

pub fn header_style() -> Style {
    Style::default().fg(PANEL_TITLE).bg(PANEL_BG).add_modifier(Modifier::BOLD)
}

pub fn new<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(PANEL_BG))
        .border_style(Style::default().fg(PANEL_BORDER).bg(PANEL_BG))
        .title(title)
        .title_style(Style::default().fg(PANEL_TITLE).bg(PANEL_BG))
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        widgets::Widget,
    };

    use super::*;

    fn render_block(title: &str) -> Buffer {
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::empty(area);
        new(title).render(area, &mut buf);
        buf
    }

    #[test]
    fn test_block_renders_borders_with_expected_color() {
        let buf = render_block(" Test ");

        assert_eq!(buf.get(0, 0).symbol(), "┌");
        assert_eq!(buf.get(19, 0).symbol(), "┐");
        assert_eq!(buf.get(0, 4).symbol(), "└");
        assert_eq!(buf.get(19, 4).symbol(), "┘");
        assert_eq!(buf.get(0, 0).fg, PANEL_BORDER);
        assert_eq!(buf.get(19, 4).fg, PANEL_BORDER);
        assert_eq!(buf.get(1, 1).bg, PANEL_BG);
    }

    #[test]
    fn test_block_renders_title_text_with_title_color() {
        let buf = render_block(" Node ");

        assert_eq!(buf.get(1, 0).symbol(), " ");
        assert_eq!(buf.get(2, 0).symbol(), "N");
        assert_eq!(buf.get(3, 0).symbol(), "o");
        assert_eq!(buf.get(4, 0).symbol(), "d");
        assert_eq!(buf.get(5, 0).symbol(), "e");
        assert_eq!(buf.get(2, 0).fg, PANEL_TITLE);
        assert_eq!(buf.get(5, 0).fg, PANEL_TITLE);
        assert_eq!(buf.get(2, 0).bg, PANEL_BG);
    }

    #[test]
    fn test_block_with_empty_title_keeps_top_border() {
        let buf = render_block("");

        assert_eq!(buf.get(0, 0).symbol(), "┌");
        assert_eq!(buf.get(1, 0).symbol(), "─");
        assert_eq!(buf.get(18, 0).symbol(), "─");
        assert_eq!(buf.get(19, 0).symbol(), "┐");
    }
}
