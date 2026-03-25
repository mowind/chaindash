use ratatui::{
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

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        style::Color,
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
        assert_eq!(buf.get(0, 0).fg, Color::Indexed(239));
        assert_eq!(buf.get(19, 4).fg, Color::Indexed(239));
    }

    #[test]
    fn test_block_renders_title_text_with_title_color() {
        let buf = render_block(" Node ");

        assert_eq!(buf.get(1, 0).symbol(), " ");
        assert_eq!(buf.get(2, 0).symbol(), "N");
        assert_eq!(buf.get(3, 0).symbol(), "o");
        assert_eq!(buf.get(4, 0).symbol(), "d");
        assert_eq!(buf.get(5, 0).symbol(), "e");
        assert_eq!(buf.get(2, 0).fg, Color::Indexed(249));
        assert_eq!(buf.get(5, 0).fg, Color::Indexed(249));
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
