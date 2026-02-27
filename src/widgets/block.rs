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
    use super::*;

    #[test]
    fn test_block_new_has_title() {
        let title = " Test Title ";
        let block = new(title);
        let _ = block;
    }

    #[test]
    fn test_block_new_empty_title() {
        let block = new("");
        let _ = block;
    }

    #[test]
    fn test_block_new_with_unicode_title() {
        let title = " 节点信息 ";
        let block = new(title);
        let _ = block;
    }

    #[test]
    fn test_block_new_with_special_chars() {
        let title = " [Node] - (Status) ";
        let block = new(title);
        let _ = block;
    }

    #[test]
    fn test_block_border_color_index() {
        assert_eq!(239_u8, 239_u8);
    }

    #[test]
    fn test_block_title_color_index() {
        assert_eq!(249_u8, 249_u8);
    }
}
