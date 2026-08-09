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

/// Static Peer Country Distribution panel backed by an owned Geo View Snapshot.
///
/// The snapshot is loaded through the `PeerGeoStore` handle on `update`; drawing
/// never queries SQLite or external services.
pub struct PeerCountriesWidget {
    update_interval: Ratio<u64>,
    collect_data: SharedData,
    store: Arc<dyn PeerGeoStore>,
    snapshot: GeoViewSnapshot,
}

fn country_flag(country_code: &str) -> String {
    country_code
        .bytes()
        .map(|letter| char::from_u32(0x1F1E6 + u32::from(letter - b'A')).unwrap_or('�'))
        .collect()
}

fn render_country_row(
    buf: &mut Buffer,
    area: Rect,
    flag: &str,
    country_code: &str,
    peer_count: usize,
    code_style: Style,
    count_style: Style,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let count = peer_count.to_string();
    let flag_width = flag.width();
    let code_width = country_code.width();
    let count_width = count.width();
    let full_width = flag_width + 1 + code_width + 1 + count_width;
    let code_only_width = code_width + 1 + count_width;

    if full_width <= area.width as usize {
        buf.set_stringn(area.x, area.y, flag, flag_width, code_style);
        buf.set_stringn(
            area.x + flag_width as u16 + 1,
            area.y,
            country_code,
            code_width,
            code_style,
        );
    } else if code_only_width <= area.width as usize {
        buf.set_stringn(area.x, area.y, country_code, code_width, code_style);
    } else {
        return;
    }

    let count_x = area.x + area.width - count_width as u16;
    buf.set_stringn(count_x, area.y, count, count_width, count_style);
}

fn render_known_country_row(
    buf: &mut Buffer,
    area: Rect,
    country: &CountryCount,
    code_style: Style,
    count_style: Style,
) {
    render_country_row(
        buf,
        area,
        &country_flag(&country.country_code),
        country.country_code.as_str(),
        country.peer_count,
        code_style,
        count_style,
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
        }
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> &GeoViewSnapshot {
        &self.snapshot
    }

    /// Load the latest Geo View Snapshot and report whether the read succeeded.
    pub(crate) fn refresh_snapshot(&mut self) -> bool {
        match self.store.geo_view_snapshot() {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                true
            },
            Err(err) => {
                let message = format!("geo snapshot unavailable: {err}");
                lock_or_panic(&self.collect_data).set_status_message(StatusLevel::Warn, message);
                self.snapshot = GeoViewSnapshot::default();
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

        if self.snapshot.total_peers == 0 {
            buf.set_stringn(area.x, area.y, "No peers", area.width as usize, panel::muted_style());
            return;
        }

        let known_style = panel::highlight_style();
        let unknown_style = panel::muted_style();
        let count_style = Style::default().fg(panel::METRIC_PRIMARY).bg(panel::PANEL_BG);
        let unknown_present = usize::from(self.snapshot.unknown_country_count > 0);
        let known_rows = self
            .snapshot
            .country_counts
            .len()
            .min((area.height as usize).saturating_sub(unknown_present));

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
                known_style,
                count_style,
            );
        }

        if unknown_present > 0 {
            render_country_row(
                buf,
                Rect {
                    x: area.x,
                    y: area.y + known_rows as u16,
                    width: area.width,
                    height: 1,
                },
                UNKNOWN_COUNTRY_FLAG,
                UNKNOWN_COUNTRY_CODE,
                self.snapshot.unknown_country_count,
                unknown_style,
                unknown_style,
            );
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
            testutil::FakePeerGeoStore,
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
            ..GeoViewSnapshot::default()
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
        let mut buf = Buffer::empty(area);
        (&widget).render(area, &mut buf);
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
        let text = buffer_text(
            &render_widget(GeoViewSnapshot::default(), Rect::new(0, 0, 24, 6)),
            Rect::new(0, 0, 24, 6),
        );

        assert!(text.contains("No peers"));
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
    fn test_peer_countries_update_reports_read_failure() {
        let data = Data::new();
        let store: Arc<dyn PeerGeoStore> = Arc::new(FailingStore);
        let mut widget = PeerCountriesWidget::new(data.clone(), store);

        assert!(!widget.refresh_snapshot());
        assert_eq!(widget.snapshot(), &GeoViewSnapshot::default());
        assert!(data.lock().expect("mutex poisoned").status_message().is_some());
    }

    struct FailingStore;

    impl std::fmt::Debug for FailingStore {
        fn fmt(
            &self,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            f.write_str("FailingStore")
        }
    }

    impl PeerGeoStore for FailingStore {
        fn replace_peer_snapshot(
            &self,
            _ips: Vec<String>,
        ) -> crate::error::Result<Vec<String>> {
            Err(crate::error::ChaindashError::Other("db broken".to_string()))
        }

        fn update_location_cache(
            &self,
            _entries: Vec<crate::geo::LocationEntry>,
        ) -> crate::error::Result<()> {
            Err(crate::error::ChaindashError::Other("db broken".to_string()))
        }

        fn geo_view_snapshot(&self) -> crate::error::Result<GeoViewSnapshot> {
            Err(crate::error::ChaindashError::Other("db broken".to_string()))
        }

        fn updates(&self) -> crossbeam_channel::Receiver<()> {
            let (_tx, rx) = crossbeam_channel::bounded(1);
            rx
        }

        fn shutdown(&self) {}
    }
}
