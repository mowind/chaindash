use std::collections::BTreeSet;

use ratatui::text::Line;

pub(crate) type PriorityLine = (u8, Line<'static>);
pub(crate) type PriorityLines = Vec<PriorityLine>;

pub(crate) fn format_grouped_u64(value: u64) -> String {
    let digits = value.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    formatted.chars().rev().collect()
}

pub(crate) fn prefix_chars(
    value: &str,
    max_chars: usize,
) -> &str {
    if max_chars == 0 {
        return "";
    }

    match value.char_indices().nth(max_chars) {
        Some((index, _)) => &value[..index],
        None => value,
    }
}

pub(crate) fn suffix_chars(
    value: &str,
    max_chars: usize,
) -> &str {
    if max_chars == 0 {
        return "";
    }

    let char_count = value.chars().count();
    if max_chars >= char_count {
        return value;
    }

    let start = value
        .char_indices()
        .nth(char_count.saturating_sub(max_chars))
        .map(|(index, _)| index)
        .unwrap_or(0);
    &value[start..]
}

pub(crate) fn select_prioritized_lines(
    specs: PriorityLines,
    max_rows: u16,
) -> Vec<Line<'static>> {
    let max_rows = max_rows as usize;
    if max_rows == 0 {
        return Vec::new();
    }

    if specs.len() <= max_rows {
        return specs.into_iter().map(|(_, line)| line).collect();
    }

    let mut ranked: Vec<(usize, u8)> =
        specs.iter().enumerate().map(|(index, (priority, _))| (index, *priority)).collect();
    ranked.sort_by_key(|(index, priority)| (*priority, *index));

    let keep: BTreeSet<usize> = ranked.into_iter().take(max_rows).map(|(index, _)| index).collect();

    specs
        .into_iter()
        .enumerate()
        .filter_map(|(index, (_, line))| keep.contains(&index).then_some(line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        prefix_chars,
        suffix_chars,
    };

    #[test]
    fn test_prefix_chars_respects_utf8_boundaries() {
        assert_eq!(prefix_chars("节点-01", 2), "节点");
    }

    #[test]
    fn test_suffix_chars_respects_utf8_boundaries() {
        assert_eq!(suffix_chars("validator-节点", 2), "节点");
    }
}
