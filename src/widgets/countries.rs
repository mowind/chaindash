use std::sync::Arc;

use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::Widget,
};
use unicode_width::UnicodeWidthStr;

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
const MAX_KNOWN_COUNTRIES_WITHOUT_UNKNOWN: usize = 3;
const MAX_KNOWN_COUNTRIES_WITH_UNKNOWN: usize = 2;

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
    let count_width = peer_count.to_string().width();
    let code_width = country_code.width();
    let full_width = flag.width() + 1 + code_width + 1 + count_width;
    let code_only_width = code_width + 1 + count_width;
    (full_width, code_only_width)
}

fn render_country_row(
    buf: &mut Buffer,
    area: Rect,
    row: CountryRow<'_>,
) {
    if area.width == 0 || area.height == 0 || row.mode == CountryRowMode::SummaryOnly {
        return;
    }

    let count = row.peer_count.to_string();
    let count_width = count.width();
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

    match row.mode {
        CountryRowMode::FlagAndCode => {
            let flag_width = row.flag.width();
            buf.set_string(area.x, area.y, row.flag, row.code_style);
            buf.set_string(
                area.x + flag_width as u16 + 1,
                area.y,
                row.country_code,
                row.code_style,
            );
        },
        CountryRowMode::CodeAndCount => {
            buf.set_string(area.x, area.y, row.country_code, row.code_style);
        },
        CountryRowMode::SummaryOnly => return,
    }

    let count_x = area.x + area.width - count_width as u16;
    buf.set_string(count_x, area.y, count, row.count_style);
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
    let text = if full.width() <= area.width as usize {
        full
    } else if compact.width() <= area.width as usize {
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

impl PeerCountriesWidget {
    /// Create a widget backed by `collect_data` for status reporting and
    /// `store` for Geo View Snapshot reads.
    pub fn new(
        collect_data: SharedData,
        store: Arc<dyn PeerGeoStore>,
    ) -> PeerCountriesWidget {
        PeerCountriesWidget {
            update_interval: Ratio::from_integer(0),
            collect_data,
            store,
            snapshot: GeoViewSnapshot::default(),
            snapshot_loaded: false,
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
        let unknown_present = usize::from(self.snapshot.unknown_country_count > 0);
        let max_known_rows = if unknown_present > 0 {
            MAX_KNOWN_COUNTRIES_WITH_UNKNOWN
        } else {
            MAX_KNOWN_COUNTRIES_WITHOUT_UNKNOWN
        };
        let known_rows_limit = max_known_rows.min(data_rows.saturating_sub(unknown_present));
        let row_mode = self.country_row_mode(area.width as usize, known_rows_limit);
        let known_rows = if row_mode == CountryRowMode::SummaryOnly {
            0
        } else {
            self.snapshot.country_counts.len().min(known_rows_limit)
        };

        for (row, country) in self.snapshot.country_counts.iter().take(known_rows).enumerate() {
            render_known_country_row(
                buf,
                Rect {
                    x: area.x,
                    y: area.y + row as u16,
                    width: area.width,
                    height: 1,
                },
                country,
                row_mode,
                known_style,
                count_style,
            );
        }

        if unknown_present > 0 && row_mode != CountryRowMode::SummaryOnly {
            render_country_row(
                buf,
                Rect {
                    x: area.x,
                    y: area.y + known_rows as u16,
                    width: area.width,
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

    fn country_row_mode(
        &self,
        area_width: usize,
        known_rows_limit: usize,
    ) -> CountryRowMode {
        let mut full_width = 0;
        let mut code_only_width = 0;

        for country in self.snapshot.country_counts.iter().take(known_rows_limit) {
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

        if full_width > 0 && area_width >= full_width {
            CountryRowMode::FlagAndCode
        } else if code_only_width > 0 && area_width >= code_only_width {
            CountryRowMode::CodeAndCount
        } else {
            CountryRowMode::SummaryOnly
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
        let data = Data::new();
        let store = Arc::new(FakePeerGeoStore::new(snapshot));
        let mut widget = PeerCountriesWidget::new(data, store);
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
                (area.x..area.x + area.width).map(|x| buf.get(x, y).symbol()).collect::<String>()
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
        (area.x..area.x + area.width).map(|x| buf.get(x, area.y + row as u16).symbol()).collect()
    }

    #[test]
    fn test_country_flag_is_generated_from_country_code() {
        assert_eq!(country_flag("CN"), "🇨🇳");
        assert_eq!(country_flag("US"), "🇺🇸");
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
    fn test_peer_countries_panel_summarizes_hidden_known_countries() {
        let area = Rect::new(0, 0, 32, 6);
        let buf = render_widget(
            snapshot_with_countries(vec![("CN", 4), ("US", 3), ("DE", 2), ("JP", 1)], 0),
            area,
        );

        let text = buffer_text(&buf, area);
        assert!(text.contains("CN"));
        assert!(text.contains("US"));
        assert!(text.contains("DE"));
        assert!(!text.contains("JP"));
        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "+1 country · 10 peers");
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

        assert_eq!(buf.get(content.x, content.y).symbol(), "C");
        assert_eq!(buf.get(content.x + 1, content.y).symbol(), "N");
        assert_eq!(buf.get(content.x + content.width - 1, content.y).symbol(), "5");
        assert!(!content_line(&buf, area, 0).contains("🇨🇳"));
    }

    #[test]
    fn test_peer_countries_panel_does_not_partially_render_at_extreme_width() {
        let area = Rect::new(0, 0, 5, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 5)], 0), area);
        let content = content_area(area);

        assert_eq!(content_line(&buf, area, 0).trim(), "");
        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "");
        assert_eq!(buf.get(area.right() - 1, content.y).symbol(), "│");
    }

    #[test]
    fn test_peer_countries_panel_keeps_unknown_country_after_two_known_rows() {
        let area = Rect::new(0, 0, 32, 6);
        let buf =
            render_widget(snapshot_with_countries(vec![("CN", 4), ("US", 3), ("DE", 2)], 1), area);

        let text = buffer_text(&buf, area);
        assert!(text.contains("CN"));
        assert!(text.contains("US"));
        assert!(text.contains("🌐"));
        assert!(!text.contains("DE"));
        assert_eq!(content_line(&buf, area, SUMMARY_ROW_INDEX).trim(), "+1 country · 10 peers");
    }

    #[test]
    fn test_peer_countries_panel_uses_display_width_without_overwriting_border() {
        let area = Rect::new(0, 0, 14, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 12)], 0), area);
        let content = content_area(area);

        assert_eq!(buf.get(content.x, content.y).symbol(), "🇨🇳");
        assert_eq!(buf.get(content.x + 3, content.y).symbol(), "C");
        assert_eq!(buf.get(content.x + content.width - 2, content.y).symbol(), "1");
        assert_eq!(buf.get(content.x + content.width - 1, content.y).symbol(), "2");
        assert_eq!(buf.get(area.right() - 1, content.y).symbol(), "│");
    }

    #[test]
    fn test_peer_countries_panel_uses_required_row_styles() {
        let area = Rect::new(0, 0, 32, 6);
        let buf = render_widget(snapshot_with_countries(vec![("CN", 2)], 3), area);
        let content = content_area(area);

        assert_eq!(buf.get(content.x + 3, content.y).fg, panel::CONTENT_HIGHLIGHT);
        assert_eq!(buf.get(content.x + content.width - 1, content.y).fg, panel::METRIC_PRIMARY);
        assert_eq!(buf.get(content.x + 3, content.y + 1).fg, panel::PANEL_MUTED);
        assert_eq!(buf.get(content.x + content.width - 1, content.y + 1).fg, panel::PANEL_MUTED);
        assert_eq!(buf.get(content.x, content.y + SUMMARY_ROW_INDEX as u16).fg, panel::PANEL_MUTED,);
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
