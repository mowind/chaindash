use std::{
    env,
    sync::Arc,
};

use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{
        Line,
        Span,
    },
    widgets::Widget,
};

use crate::{
    collect::{
        SharedData,
        StatusLevel,
    },
    geo::{
        CountryCount,
        GeoViewSnapshot,
        PeerGeoStore,
    },
    sync::lock_or_panic,
    update::UpdatableWidget,
    widgets::block as panel,
};

const COUNTRIES_TITLE: &str = " Peer Countries ";
const UNKNOWN_COUNTRY_FLAG: &str = "🌐";
const UNKNOWN_COUNTRY_CODE: &str = "--";
const CONTENT_ROW_COUNT: usize = 4;
const SUMMARY_ROW_INDEX: usize = CONTENT_ROW_COUNT - 1;
const COUNTRY_COLUMN_GAP: usize = 2;
const MAX_COUNTRY_COLUMNS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CountryRowMode {
    FlagAndCode,
    CodeAndCount,
    SummaryOnly,
}

struct CountryRow<'a> {
    flag: &'a str,
    country_code: &'a str,
    peer_count: usize,
    mode: CountryRowMode,
    code_style: Style,
    count_style: Style,
}

/// Static Peer Country Distribution panel backed by an owned Geo View Snapshot.
///
/// The snapshot is loaded through the `PeerGeoStore` handle on `update`; drawing
/// never queries SQLite or external services.
pub struct PeerCountriesWidget {
    update_interval: Ratio<u64>,
    collect_data: SharedData,
    store: Arc<dyn PeerGeoStore>,
    snapshot: GeoViewSnapshot,
    snapshot_loaded: bool,
    show_country_flags: bool,
}

fn terminal_name_supports_country_flags(term: &str) -> bool {
    let term = term.to_ascii_lowercase();
    !term.starts_with("tmux") && !term.starts_with("screen")
}

fn terminal_supports_country_flags() -> bool {
    // tmux rewrites Regional Indicator graphemes with cursor corrections that some downstream
    // terminals interpret at a different width. Docker builds bake in the same fallback because
    // containers cannot observe the host's TMUX environment unless the launcher propagates it.
    if cfg!(feature = "ascii-countries")
        || env::var_os("TMUX").is_some()
        || env::var_os("CHAINDASH_ASCII_COUNTRIES").is_some()
    {
        return false;
    }

    env::var("TERM").map_or(true, |term| terminal_name_supports_country_flags(&term))
}

fn country_flag(country_code: &str) -> String {
    if country_code.len() != 2 || !country_code.bytes().all(|letter| letter.is_ascii_uppercase()) {
        return String::new();
    }

    country_code
        .bytes()
        .map(|letter| char::from_u32(0x1F1E6 + u32::from(letter - b'A')).unwrap_or('�'))
        .collect()
}

fn country_row_widths(
    flag: &str,
    country_code: &str,
    peer_count: usize,
) -> (usize, usize) {
    let count = peer_count.to_string();
    let full_width = Line::from(vec![
        Span::raw(flag),
        Span::raw(" "),
        Span::raw(country_code),
        Span::raw(" ["),
        Span::raw(count.as_str()),
        Span::raw("]"),
    ])
    .width();
    let code_only_width = Line::from(vec![
        Span::raw(country_code),
        Span::raw(" ["),
        Span::raw(count.as_str()),
        Span::raw("]"),
    ])
    .width();
    (full_width, code_only_width)
}

fn country_row_line<'a>(row: &CountryRow<'a>) -> Line<'a> {
    let count = row.peer_count.to_string();
    match row.mode {
        CountryRowMode::FlagAndCode => Line::from(vec![
            Span::styled(row.flag, row.code_style),
            Span::styled(" ", row.code_style),
            Span::styled(row.country_code, row.code_style),
            Span::styled(" [", row.count_style),
            Span::styled(count, row.count_style),
            Span::styled("]", row.count_style),
        ]),
        CountryRowMode::CodeAndCount => Line::from(vec![
            Span::styled(row.country_code, row.code_style),
            Span::styled(" [", row.count_style),
            Span::styled(count, row.count_style),
            Span::styled("]", row.count_style),
        ]),
        CountryRowMode::SummaryOnly => Line::default(),
    }
    .style(Style::default().bg(panel::PANEL_BG))
}

fn render_country_row(
    buf: &mut Buffer,
    area: Rect,
    row: CountryRow<'_>,
) {
    if area.width == 0 || area.height == 0 || row.mode == CountryRowMode::SummaryOnly {
        return;
    }

    let (full_width, code_only_width) =
        country_row_widths(row.flag, row.country_code, row.peer_count);
    let row_width = match row.mode {
        CountryRowMode::FlagAndCode => full_width,
        CountryRowMode::CodeAndCount => code_only_width,
        CountryRowMode::SummaryOnly => return,
    };

    if row_width > area.width as usize {
        return;
    }

    country_row_line(&row).render(area, buf);
}

fn summary_texts(
    known_country_count: usize,
    hidden_known_country_count: usize,
    total_peer_count: usize,
) -> (String, String) {
    let displayed_country_count = if hidden_known_country_count > 0 {
        hidden_known_country_count
    } else {
        known_country_count
    };
    let prefix = if hidden_known_country_count > 0 {
        "+"
    } else {
        ""
    };
    let country_label = if displayed_country_count == 1 {
        "country"
    } else {
        "countries"
    };
    let peer_label = if total_peer_count == 1 {
        "peer"
    } else {
        "peers"
    };

    (
        format!(
            "{prefix}{displayed_country_count} {country_label} · {total_peer_count} {peer_label}"
        ),
        format!("{prefix}{displayed_country_count}c · {total_peer_count}p"),
    )
}

fn render_summary(
    buf: &mut Buffer,
    area: Rect,
    known_country_count: usize,
    hidden_known_country_count: usize,
    total_peer_count: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (full, compact) =
        summary_texts(known_country_count, hidden_known_country_count, total_peer_count);
    let text = if Line::from(full.as_str()).width() <= area.width as usize {
        full
    } else if Line::from(compact.as_str()).width() <= area.width as usize {
        compact
    } else {
        return;
    };

    buf.set_string(area.x, area.y, text, panel::muted_style());
}

fn render_known_country_row(
    buf: &mut Buffer,
    area: Rect,
    country: &CountryCount,
    mode: CountryRowMode,
    code_style: Style,
    count_style: Style,
) {
    let flag = country_flag(&country.country_code);
    render_country_row(
        buf,
        area,
        CountryRow {
            flag: &flag,
            country_code: country.country_code.as_str(),
            peer_count: country.peer_count,
            mode,
            code_style,
            count_style,
        },
    );
}

fn country_column_count(
    area_width: usize,
    column_width: usize,
) -> usize {
    if column_width == 0 {
        return 1;
    }

    ((area_width + COUNTRY_COLUMN_GAP) / (column_width + COUNTRY_COLUMN_GAP))
        .clamp(1, MAX_COUNTRY_COLUMNS)
}

impl PeerCountriesWidget {
    /// Create a widget backed by `collect_data` for status reporting and
    /// `store` for Geo View Snapshot reads.
    pub fn new(
        collect_data: SharedData,
        store: Arc<dyn PeerGeoStore>,
    ) -> PeerCountriesWidget {
        Self::with_country_flags(collect_data, store, terminal_supports_country_flags())
    }

    fn with_country_flags(
        collect_data: SharedData,
        store: Arc<dyn PeerGeoStore>,
        show_country_flags: bool,
    ) -> PeerCountriesWidget {
        PeerCountriesWidget {
            update_interval: Ratio::from_integer(0),
            collect_data,
            store,
            snapshot: GeoViewSnapshot::default(),
            snapshot_loaded: false,
            show_country_flags,
        }
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> &GeoViewSnapshot {
        &self.snapshot
    }

    /// Load the latest Geo View Snapshot and report whether the read succeeded.
    ///
    /// A failed read leaves the last successfully loaded snapshot intact so a
    /// transient database error cannot clear useful Peer Country Distribution
    /// data from the panel.
    pub(crate) fn refresh_snapshot(&mut self) -> bool {
        match self.store.geo_view_snapshot() {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                self.snapshot_loaded = true;
                true
            },
            Err(err) => {
                let message = format!("geo snapshot unavailable: {err}");
                lock_or_panic(&self.collect_data).set_status_message(StatusLevel::Warn, message);
                false
            },
        }
    }

    fn render_content(
        &self,
        buf: &mut Buffer,
        area: Rect,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        if !self.snapshot_loaded {
            buf.set_stringn(
                area.x,
                area.y,
                "Geo data unavailable",
                area.width as usize,
                panel::muted_style(),
            );
            return;
        }

        if self.snapshot.total_peers == 0 {
            buf.set_stringn(area.x, area.y, "No peers", area.width as usize, panel::muted_style());
            if area.height as usize > SUMMARY_ROW_INDEX {
                render_summary(
                    buf,
                    Rect {
                        x: area.x,
                        y: area.y + SUMMARY_ROW_INDEX as u16,
                        width: area.width,
                        height: 1,
                    },
                    0,
                    0,
                    0,
                );
            }
            return;
        }

        let known_style = panel::highlight_style();
        let unknown_style = panel::muted_style();
        let count_style = Style::default().fg(panel::METRIC_PRIMARY).bg(panel::PANEL_BG);
        let content_rows = (area.height as usize).min(CONTENT_ROW_COUNT);
        let summary_visible = content_rows > SUMMARY_ROW_INDEX;
        let data_rows = if summary_visible {
            SUMMARY_ROW_INDEX
        } else {
            content_rows
        };
        let row_mode = self.country_row_mode(area.width as usize);
        let column_width = self.country_row_width(row_mode);
        let column_count = country_column_count(area.width as usize, column_width);
        let data_capacity = data_rows.saturating_mul(column_count);
        let unknown_present = usize::from(self.snapshot.unknown_country_count > 0);
        let known_rows_limit = data_capacity.saturating_sub(unknown_present);
        let known_rows = if row_mode == CountryRowMode::SummaryOnly {
            0
        } else {
            self.snapshot.country_counts.len().min(known_rows_limit)
        };

        for (index, country) in self.snapshot.country_counts.iter().take(known_rows).enumerate() {
            let row = index / column_count;
            let column = index % column_count;
            let x = area.x + (column * (column_width + COUNTRY_COLUMN_GAP)) as u16;
            render_known_country_row(
                buf,
                Rect {
                    x,
                    y: area.y + row as u16,
                    width: (area.right() - x).min(column_width as u16),
                    height: 1,
                },
                country,
                row_mode,
                known_style,
                count_style,
            );
        }

        if unknown_present > 0 && row_mode != CountryRowMode::SummaryOnly {
            let index = known_rows;
            let row = index / column_count;
            let column = index % column_count;
            let x = area.x + (column * (column_width + COUNTRY_COLUMN_GAP)) as u16;
            render_country_row(
                buf,
                Rect {
                    x,
                    y: area.y + row as u16,
                    width: (area.right() - x).min(column_width as u16),
                    height: 1,
                },
                CountryRow {
                    flag: UNKNOWN_COUNTRY_FLAG,
                    country_code: UNKNOWN_COUNTRY_CODE,
                    peer_count: self.snapshot.unknown_country_count,
                    mode: row_mode,
                    code_style: unknown_style,
                    count_style: unknown_style,
                },
            );
        }

        if summary_visible {
            let hidden_known_country_count = self.snapshot.country_counts.len() - known_rows;
            render_summary(
                buf,
                Rect {
                    x: area.x,
                    y: area.y + SUMMARY_ROW_INDEX as u16,
                    width: area.width,
                    height: 1,
                },
                self.snapshot.country_counts.len(),
                hidden_known_country_count,
                self.snapshot.total_peers,
            );
        }
    }

    fn country_widths(&self) -> (usize, usize) {
        let mut full_width = 0;
        let mut code_only_width = 0;

        for country in &self.snapshot.country_counts {
            let (country_full_width, country_code_only_width) = country_row_widths(
                &country_flag(&country.country_code),
                &country.country_code,
                country.peer_count,
            );
            full_width = full_width.max(country_full_width);
            code_only_width = code_only_width.max(country_code_only_width);
        }

        if self.snapshot.unknown_country_count > 0 {
            let (unknown_full_width, unknown_code_only_width) = country_row_widths(
                UNKNOWN_COUNTRY_FLAG,
                UNKNOWN_COUNTRY_CODE,
                self.snapshot.unknown_country_count,
            );
            full_width = full_width.max(unknown_full_width);
            code_only_width = code_only_width.max(unknown_code_only_width);
        }

        (full_width, code_only_width)
    }

    fn country_row_mode(
        &self,
        area_width: usize,
    ) -> CountryRowMode {
        let (full_width, code_only_width) = self.country_widths();

        if self.show_country_flags && full_width > 0 && area_width >= full_width {
            CountryRowMode::FlagAndCode
        } else if code_only_width > 0 && area_width >= code_only_width {
            CountryRowMode::CodeAndCount
        } else {
            CountryRowMode::SummaryOnly
        }
    }

    fn country_row_width(
        &self,
        mode: CountryRowMode,
    ) -> usize {
        let (full_width, code_only_width) = self.country_widths();
        match mode {
            CountryRowMode::FlagAndCode => full_width,
            CountryRowMode::CodeAndCount => code_only_width,
            CountryRowMode::SummaryOnly => 0,
        }
    }
}

impl UpdatableWidget for PeerCountriesWidget {
    fn update(&mut self) {
        self.refresh_snapshot();
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl Widget for &PeerCountriesWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let block = panel::new(COUNTRIES_TITLE);
        let inner = block.inner(area);
        block.render(area, buf);
        self.render_content(buf, inner);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        widgets::Widget,
    };

    use super::*;
    use crate::{
        collect::Data,
        geo::{
            testutil::{
                FakePeerGeoStore,
                ScriptedPeerGeoStore,
            },
            CountryCount,
        },
    };

    fn snapshot_with_countries(
        countries: Vec<(&str, usize)>,
        unknown_country_count: usize,
    ) -> GeoViewSnapshot {
        let country_counts = countries
            .into_iter()
            .map(|(country_code, peer_count)| CountryCount {
                country_code: country_code.to_string(),
                peer_count,
            })
            .collect::<Vec<_>>();
        let total_peers = country_counts.iter().map(|country| country.peer_count).sum::<usize>()
            + unknown_country_count;

        GeoViewSnapshot {
            total_peers,
            unique_countries: country_counts.len(),
            country_counts,
            unknown_country_count,
        }
    }

    fn render_widget(
        snapshot: GeoViewSnapshot,
        area: Rect,
    ) -> Buffer {
        render_widget_with_country_flags(snapshot, area, true)
    }

    fn render_widget_with_country_flags(
        snapshot: GeoViewSnapshot,
        area: Rect,
        show_country_flags: bool,
    ) -> Buffer {
        let data = Data::new();
        let store = Arc::new(FakePeerGeoStore::new(snapshot));
        let mut widget = PeerCountriesWidget::with_country_flags(data, store, show_country_flags);
        widget.update();
        render_current_widget(&widget, area)
    }

    fn render_current_widget(
        widget: &PeerCountriesWidget,
        area: Rect,
    ) -> Buffer {
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        buf
    }

    fn buffer_text(
        buf: &Buffer,
        area: Rect,
    ) -> String {
        (area.y..area.y + area.height)
            .map(|y| {
                (area.x..area.x + area.width).map(|x| buf[(x, y)].symbol()).collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn content_area(area: Rect) -> Rect {
        panel::new(COUNTRIES_TITLE).inner(area)
    }

    fn content_line(
        buf: &Buffer,
        outer_area: Rect,
        row: usize,
    ) -> String {
        let area = content_area(outer_area);
        (area.x..area.x + area.width).map(|x| buf[(x, area.y + row as u16)].symbol()).collect()
    }

    #[test]
    fn test_country_flag_is_generated_from_country_code() {
        assert_eq!(country_flag("CN"), "🇨🇳");
        assert_eq!(country_flag("US"), "🇺🇸");
    }

    #[test]
    fn test_widget_uses_terminal_country_flag_support() {
        let data = Data::new();
        let store = Arc::new(FakePeerGeoStore::new(GeoViewSnapshot::default()));
        let widget = PeerCountriesWidget::new(data, store);

        assert_eq!(widget.show_country_flags, terminal_supports_country_flags());
    }

    #[test]
    fn test_tmux_terminal_names_disable_country_flags() {
        assert!(!terminal_name_supports_country_flags("tmux-256color"));
        assert!(!terminal_name_supports_country_flags("screen"));
        assert!(!terminal_name_supports_country_flags("screen-256color"));
        assert!(terminal_name_supports_country_flags("xterm-256color"));
    }

    #[cfg(feature = "ascii-countries")]
    #[test]
    fn test_ascii_country_build_disables_country_flags() {
        let area = Rect::new(0, 0, 40, 8);
        let data = Data::new();
        let store = Arc::new(FakePeerGeoStore::new(snapshot_with_countries(vec![("SG", 6)], 2)));
        let mut widget = PeerCountriesWidget::new(data, store);
        widget.update();
        let text = buffer_text(&render_current_widget(&widget, area), area);

        assert!(!terminal_supports_country_flags());
        assert!(text.contains("SG [6]"));
        assert!(text.contains("-- [2]"));
        assert!(!text.contains("🇸🇬"));
        assert!(!text.contains(UNKNOWN_COUNTRY_FLAG));
    }

    #[test]
    fn test_country_flags_are_omitted_in_tmux_compatible_mode() {
        let area = Rect::new(0, 0, 40, 8);
        let buf = render_widget_with_country_flags(
            snapshot_with_countries(vec![("SG", 6)], 2),
            area,
            false,
        );
        let text = buffer_text(&buf, area);
        let content = content_area(area);

        assert!(text.contains("SG [6]"));
        assert!(text.contains("-- [2]"));
        assert!(!text.contains("🇸🇬"));
        assert!(!text.contains(UNKNOWN_COUNTRY_FLAG));
        assert_eq!(buf[(area.x, content.y)].symbol(), "│");
        assert_eq!(buf[(area.right() - 1, content.y)].symbol(), "│");
        assert_eq!(buf[(area.x, content.y + 1)].symbol(), "│");
        assert_eq!(buf[(area.right() - 1, content.y + 1)].symbol(), "│");
    }

    #[test]
    fn test_country_flag_diff_handles_trailing_cell_updates() {
        let area = Rect::new(0, 0, 3, 1);
        let flag = country_flag("GB");

        let mut styled_blanks = Buffer::empty(area);
        styled_blanks.set_style(area, Style::default().fg(panel::PANEL_MUTED));
        let mut flag_buffer = Buffer::empty(area);
        flag_buffer.set_string(0, 0, &flag, Style::default().bg(panel::PANEL_BG));

        let draw_updates = styled_blanks.diff_iter(&flag_buffer).collect::<Vec<_>>();
        assert!(draw_updates.iter().any(|(x, y, _)| (*x, *y) == (0, 0)));
        assert!(
            !draw_updates.iter().any(|(x, y, _)| (*x, *y) == (1, 0)),
            "a hidden trailing-cell update can shift the terminal cursor",
        );

        let mut code_buffer = Buffer::empty(area);
        code_buffer.set_string(0, 0, "G", Style::default());
        let clear_updates = flag_buffer.diff_iter(&code_buffer).collect::<Vec<_>>();
        assert!(
            clear_updates.iter().any(|(x, y, _)| (*x, *y) == (1, 0)),
            "an uncovered trailing cell must be refreshed",
        );
    }

    #[test]
    fn test_peer_countries_panel_renders_title_and_known_rows() {
        let area = Rect::new(0, 0, 32, 8);
        let text = buffer_text(
            &render_widget(snapshot_with_countries(vec![("CN", 2), ("US", 1)], 0), area),
            area,
        );
        assert!(text.contains("Peer Countries"));
        assert!(text.contains("🇨🇳"));
        assert!(text.contains("CN"));
        assert!(text.contains("🇺🇸"));
        assert!(text.contains("US"));
        assert!(text.contains("2"));
        assert!(text.contains("1"));
    }

    #[test]
    fn test_peer_countries_panel_renders_unknown_country_after_known_rows() {
        let area = Rect::new(0, 0, 32, 8);
        let text =
            buffer_text(&render_widget(snapshot_with_countries(vec![("CN", 2)], 3), area), area);
        assert!(
            text.find("CN").expect("known row should render")
                < text.find("--").expect("unknown row should render")
        );
        assert!(text.contains("🌐"));
        assert!(text.contains("--"));
    }

    #[test]
    fn test_peer_countries_panel_uses_fourth_line_for_complete_summary() {
        let area = Rect::new(0, 0, 32, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 2), ("US", 1)], 0), area);

        let content = content_area(area);
        assert_eq!(content.height, CONTENT_ROW_COUNT as u16);
        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "2 countries · 3 peers");
    }

    #[test]
    fn test_peer_countries_panel_uses_multiple_columns_for_known_countries() {
        let area = Rect::new(0, 0, 32, 6);
        let buf = render_widget(
            snapshot_with_countries(vec![("CN", 4), ("US", 3), ("DE", 2), ("JP", 1)], 0),
            area,
        );

        let content = content_area(area);
        let column_width = country_row_widths("🇨🇳", "CN", 4).0;
        let second_column_x = content.x + (column_width + COUNTRY_COLUMN_GAP) as u16;
        assert_eq!(buf[(second_column_x, content.y)].symbol(), "🇺🇸");
        assert_eq!(buf[(content.x, content.y + 1)].symbol(), "🇩🇪");
        assert_eq!(buf[(second_column_x, content.y + 1)].symbol(), "🇯🇵");
        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "4 countries · 10 peers");
    }

    #[test]
    fn test_peer_countries_panel_caps_columns_at_five() {
        let area = Rect::new(0, 0, 100, 6);
        let buf = render_widget(
            snapshot_with_countries(
                vec![("CN", 6), ("US", 5), ("DE", 4), ("JP", 3), ("FR", 2), ("FI", 1)],
                0,
            ),
            area,
        );

        let content = content_area(area);
        let column_width = country_row_widths("🇨🇳", "CN", 6).0;
        let column_step = (column_width + COUNTRY_COLUMN_GAP) as u16;
        assert_eq!(buf[(content.x + 4 * column_step, content.y)].symbol(), "🇫🇷");
        assert_eq!(buf[(content.x, content.y + 1)].symbol(), "🇫🇮");
        assert_eq!(buf[(content.x + 5 * column_step, content.y)].symbol(), " ");
    }

    #[test]
    fn test_peer_countries_panel_pluralizes_singular_summary() {
        let area = Rect::new(0, 0, 32, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 1)], 0), area);

        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "1 country · 1 peer");
    }

    #[test]
    fn test_peer_countries_panel_uses_compact_summary_when_full_text_does_not_fit() {
        let area = Rect::new(0, 0, 12, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 5), ("US", 4)], 0), area);

        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "2c · 9p");
    }

    #[test]
    fn test_peer_countries_panel_degrades_to_country_code_and_exact_count() {
        let area = Rect::new(0, 0, 8, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 5)], 0), area);
        let content = content_area(area);

        assert_eq!(buf[(content.x, content.y)].symbol(), "C");
        assert_eq!(buf[(content.x + 1, content.y)].symbol(), "N");
        assert_eq!(buf[(content.x + 3, content.y)].symbol(), "[");
        assert_eq!(buf[(content.x + 4, content.y)].symbol(), "5");
        assert_eq!(buf[(content.x + 5, content.y)].symbol(), "]");
        assert!(!content_line(&buf, area, 0).contains("🇨🇳"));
    }

    #[test]
    fn test_peer_countries_panel_does_not_partially_render_at_extreme_width() {
        let area = Rect::new(0, 0, 5, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 5)], 0), area);
        let content = content_area(area);

        assert_eq!(content_line(&buf, area, 0).trim(), "");
        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "");
        assert_eq!(buf[(area.right() - 1, content.y)].symbol(), "│");
    }

    #[test]
    fn test_peer_countries_panel_uses_multiple_columns_with_unknown_peers() {
        let area = Rect::new(0, 0, 32, 6);
        let buf =
            render_widget(snapshot_with_countries(vec![("CN", 4), ("US", 3), ("DE", 2)], 1), area);

        let content = content_area(area);
        let column_width = country_row_widths("🇨🇳", "CN", 4).0;
        let second_column_x = content.x + (column_width + COUNTRY_COLUMN_GAP) as u16;
        assert_eq!(buf[(second_column_x, content.y)].symbol(), "🇺🇸");
        assert_eq!(buf[(content.x, content.y + 1)].symbol(), "🇩🇪");
        assert_eq!(buf[(second_column_x, content.y + 1)].symbol(), "🌐");
        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "3 countries · 10 peers");
    }

    #[test]
    fn test_peer_countries_panel_uses_display_width_without_overwriting_border() {
        let area = Rect::new(0, 0, 14, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 12)], 0), area);
        let content = content_area(area);

        assert_eq!(buf[(content.x, content.y)].symbol(), "🇨🇳");
        assert_eq!(buf[(content.x + 3, content.y)].symbol(), "C");
        assert_eq!(buf[(content.x + 4, content.y)].symbol(), "N");
        assert_eq!(buf[(content.x + 6, content.y)].symbol(), "[");
        assert_eq!(buf[(content.x + 7, content.y)].symbol(), "1");
        assert_eq!(buf[(content.x + 8, content.y)].symbol(), "2");
        assert_eq!(buf[(content.x + 9, content.y)].symbol(), "]");
        assert_eq!(buf[(area.right() - 1, content.y)].symbol(), "│");
    }

    #[test]
    fn test_peer_countries_panel_aligns_codes_after_flag_spans() {
        let area = Rect::new(0, 0, 12, 6);
        let buf =
            render_widget(snapshot_with_countries(vec![("DE", 4), ("FI", 3), ("FR", 2)], 0), area);
        let content = content_area(area);

        for (row, (code, count)) in [("DE", "4"), ("FI", "3"), ("FR", "2")].iter().enumerate() {
            let y = content.y + row as u16;
            assert_eq!(buf[(content.x + 2, y)].symbol(), " ");
            assert_eq!(buf[(content.x + 3, y)].symbol(), &code[0..1]);
            assert_eq!(buf[(content.x + 4, y)].symbol(), &code[1..2]);
            assert_eq!(buf[(content.x + 6, y)].symbol(), "[");
            assert_eq!(buf[(content.x + 7, y)].symbol(), *count);
            assert_eq!(buf[(content.x + 8, y)].symbol(), "]");
        }
    }

    #[test]
    fn test_peer_countries_panel_uses_required_row_styles() {
        let area = Rect::new(0, 0, 32, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 2)], 3), area);
        let content = content_area(area);
        let column_width = country_row_widths("🇨🇳", "CN", 2).0;
        let second_column_x = content.x + (column_width + COUNTRY_COLUMN_GAP) as u16;

        assert_eq!(buf[(content.x + 3, content.y)].fg, panel::CONTENT_HIGHLIGHT);
        assert_eq!(buf[(content.x + 7, content.y)].fg, panel::METRIC_PRIMARY);
        assert_eq!(buf[(second_column_x + 3, content.y)].fg, panel::PANEL_MUTED);
        assert_eq!(buf[(second_column_x + 7, content.y)].fg, panel::PANEL_MUTED);
        assert_eq!(buf[(content.x, content.y + SUMMARY_ROW_INDEX as u16)].fg, panel::PANEL_MUTED,);
    }

    #[test]
    fn test_peer_countries_panel_renders_all_unknown_peers_as_unknown_country() {
        let area = Rect::new(0, 0, 32, 6);
        let text = buffer_text(&render_widget(snapshot_with_countries(vec![], 3), area), area);

        assert!(!text.contains("No peers"));
        assert!(text.contains("🌐"));
        assert!(text.contains("--"));
        assert!(text.contains("3"));
    }

    #[test]
    fn test_peer_countries_panel_reserves_space_for_unknown_row() {
        let area = Rect::new(0, 0, 32, 4);
        let text = buffer_text(
            &render_widget(snapshot_with_countries(vec![("CN", 2), ("US", 1)], 3), area),
            area,
        );

        assert!(text.contains("CN"));
        assert!(text.contains("--"));
    }

    #[test]
    fn test_peer_countries_panel_renders_successful_empty_snapshot_as_no_peers() {
        let area = Rect::new(0, 0, 24, 6);
        let buf = render_widget(GeoViewSnapshot::default(), area);

        assert!(buffer_text(&buf, area).contains("No peers"));
        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "0 countries · 0 peers");
    }

    #[test]
    fn test_peer_countries_update_loads_owned_snapshot() {
        let data = Data::new();
        let snapshot = snapshot_with_countries(vec![("CN", 1)], 0);
        let store = Arc::new(FakePeerGeoStore::new(snapshot.clone()));
        let mut widget = PeerCountriesWidget::new(data, store);

        assert!(widget.refresh_snapshot());
        assert_eq!(widget.snapshot(), &snapshot);
    }

    #[test]
    fn test_peer_countries_initial_read_failure_renders_unavailable() {
        let data = Data::new();
        let store = ScriptedPeerGeoStore::new(vec![Err("db broken".to_string())]);
        let mut widget = PeerCountriesWidget::new(data.clone(), store);

        assert!(!widget.refresh_snapshot());
        assert_eq!(widget.snapshot(), &GeoViewSnapshot::default());

        let area = Rect::new(0, 0, 32, 6);
        let text = buffer_text(&render_current_widget(&widget, area), area);
        assert!(text.contains("Geo data unavailable"));
        assert!(!text.contains("No peers"));

        let status = data
            .lock()
            .expect("mutex poisoned")
            .status_message()
            .expect("read failure should set a status message");
        assert_eq!(status.level, StatusLevel::Warn);
    }

    #[test]
    fn test_peer_countries_read_failure_preserves_last_successful_snapshot() {
        let data = Data::new();
        let first = snapshot_with_countries(vec![("CN", 2)], 1);
        let store = ScriptedPeerGeoStore::new(vec![
            Ok(first.clone()),
            Err("transient read failure".to_string()),
        ]);
        let mut widget = PeerCountriesWidget::new(data.clone(), store);

        assert!(widget.refresh_snapshot());
        assert!(!widget.refresh_snapshot());
        assert_eq!(widget.snapshot(), &first);

        let area = Rect::new(0, 0, 32, 6);
        let text = buffer_text(&render_current_widget(&widget, area), area);
        assert!(text.contains("CN"));
        assert!(text.contains("--"));
        assert!(!text.contains("Geo data unavailable"));

        let status = data
            .lock()
            .expect("mutex poisoned")
            .status_message()
            .expect("read failure should set a status message");
        assert_eq!(status.level, StatusLevel::Warn);
    }

    #[test]
    fn test_peer_countries_retry_replaces_retained_snapshot_after_recovery() {
        let data = Data::new();
        let first = snapshot_with_countries(vec![("CN", 1)], 0);
        let recovered = snapshot_with_countries(vec![("US", 3)], 0);
        let store = ScriptedPeerGeoStore::new(vec![
            Ok(first.clone()),
            Err("transient read failure".to_string()),
            Ok(recovered.clone()),
        ]);
        let mut widget = PeerCountriesWidget::new(data, store.clone());

        assert!(widget.refresh_snapshot());
        assert!(!widget.refresh_snapshot());
        assert_eq!(widget.snapshot(), &first);
        assert!(widget.refresh_snapshot());
        assert_eq!(widget.snapshot(), &recovered);
        assert_eq!(store.read_count(), 3);
    }
}
