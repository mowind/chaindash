use std::{
    collections::BTreeMap,
    env,
    sync::Arc,
};

use num_rational::Ratio;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{
        Color,
        Style,
    },
    widgets::Widget,
};

use crate::{
    collect::{
        SharedData,
        StatusLevel,
    },
    geo::{
        GeoViewSnapshot,
        PeerGeoStore,
    },
    sync::lock_or_panic,
    update::UpdatableWidget,
    widgets::block as panel,
};

const MAP_TITLE: &str = " Peer Map ";
const LAND_TOP: &str = "▀";
const LAND_BOTTOM: &str = "▄";
const LAND_FULL: &str = "█";
const PEER_DOT: &str = "●";
const PEER_DIAMOND: &str = "◆";
const PEER_SQUARE: &str = "■";
const LEGEND_TEXT: &str = "●1 / ◆2-4 / ■5+";
const HALF_BLOCK_MIN_WIDTH: u16 = 24;
const HALF_BLOCK_MIN_HEIGHT: u16 = 7;
const LEGEND_MIN_HEIGHT: u16 = 11;
const MAP_CELL_ASPECT_RATIO: u16 = 4;

const LAND_MASK_WIDTH: usize = 360;
const LAND_MASK_HEIGHT: usize = 180;
const LAND_MASK_BYTES: usize = LAND_MASK_WIDTH * LAND_MASK_HEIGHT / 8;
const MAX_MAP_WIDTH: u16 = LAND_MASK_WIDTH as u16;
const MAX_MAP_HEIGHT: u16 = MAX_MAP_WIDTH / MAP_CELL_ASPECT_RATIO;

/// Natural Earth 1:110m land rasterized to one bit per one-degree cell.
///
/// Rows run from 89.5°N to 89.5°S and columns run from 179.5°W to 179.5°E.
/// The generator and source geometry live under `data/natural-earth` and
/// `src/bin/generate_land_mask.rs`.
static LAND_MASK: &[u8; LAND_MASK_BYTES] = include_bytes!("../../data/land_mask.bin");

fn land_mask_cell(
    row: usize,
    column: usize,
) -> bool {
    if row >= LAND_MASK_HEIGHT || column >= LAND_MASK_WIDTH {
        return false;
    }
    let bit = row * LAND_MASK_WIDTH + column;
    LAND_MASK[bit / 8] & (1 << (7 - bit % 8)) != 0
}

fn land_cell_coordinates(
    row: usize,
    column: usize,
) -> (f64, f64) {
    (90.0 - row as f64 - 0.5, -180.0 + column as f64 + 0.5)
}

/// Equirectangular projection of `(lat, lng)` onto normalized map coordinates.
///
/// The map is centered on the prime meridian and spans the full longitude and
/// latitude ranges. Longitudes outside the normal range wrap once so a peer is
/// still rendered exactly once at the date-line boundary.
fn project(
    lat: f64,
    lng: f64,
) -> Option<(f64, f64)> {
    if !lat.is_finite() || !lng.is_finite() || !(-90.0..=90.0).contains(&lat) {
        return None;
    }

    let longitude = normalize_longitude(lng);
    Some(((longitude + 180.0) / 360.0, (90.0 - lat) / 180.0))
}

fn normalize_longitude(lng: f64) -> f64 {
    let wrapped = (lng + 180.0).rem_euclid(360.0) - 180.0;
    if wrapped == -180.0 && lng > 0.0 {
        180.0
    } else {
        wrapped
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    TrueColor,
    Ansi256,
    Ansi16,
    Monochrome,
}

#[derive(Debug, Clone, Copy)]
struct MapPalette {
    land: Color,
    peers: [Color; 4],
    background: Color,
    border: Color,
    title: Color,
    stats: Color,
}

fn color_mode() -> ColorMode {
    let term = env::var("TERM").unwrap_or_default();
    let color_term = env::var("COLORTERM").unwrap_or_default();
    color_mode_from_values(env::var_os("NO_COLOR").is_some(), &term, &color_term)
}

fn color_mode_from_values(
    no_color: bool,
    term: &str,
    color_term: &str,
) -> ColorMode {
    if no_color {
        return ColorMode::Monochrome;
    }

    let term = term.to_ascii_lowercase();
    if term == "dumb" {
        return ColorMode::Monochrome;
    }

    let color_term = color_term.to_ascii_lowercase();
    if color_term.contains("truecolor")
        || color_term.contains("24bit")
        || term.contains("direct")
        || term.contains("truecolor")
    {
        ColorMode::TrueColor
    } else if term.contains("256color") {
        ColorMode::Ansi256
    } else {
        ColorMode::Ansi16
    }
}

fn palette(mode: ColorMode) -> MapPalette {
    match mode {
        ColorMode::TrueColor => MapPalette {
            land: Color::Rgb(92, 111, 125),
            peers: [
                Color::Rgb(71, 220, 214),
                Color::Rgb(255, 191, 71),
                Color::Rgb(255, 112, 166),
                Color::Rgb(151, 122, 255),
            ],
            background: panel::PANEL_BG,
            border: panel::PANEL_BORDER,
            title: panel::PANEL_TITLE,
            stats: panel::PANEL_MUTED,
        },
        ColorMode::Ansi256 => MapPalette {
            land: Color::Indexed(245),
            peers: [
                Color::Indexed(80),
                Color::Indexed(220),
                Color::Indexed(205),
                Color::Indexed(141),
            ],
            background: Color::Indexed(236),
            border: Color::Indexed(239),
            title: Color::Indexed(252),
            stats: Color::Indexed(245),
        },
        ColorMode::Ansi16 => MapPalette {
            land: Color::DarkGray,
            peers: [Color::Cyan, Color::Yellow, Color::Magenta, Color::Green],
            background: Color::Black,
            border: Color::DarkGray,
            title: Color::White,
            stats: Color::Gray,
        },
        ColorMode::Monochrome => MapPalette {
            land: Color::Reset,
            peers: [Color::Reset; 4],
            background: Color::Reset,
            border: Color::Reset,
            title: Color::Reset,
            stats: Color::Reset,
        },
    }
}

fn peer_color(
    peer: &crate::geo::snapshot::LocatedPeer,
    palette: MapPalette,
) -> Color {
    let key = if peer.country.is_empty() {
        peer.ip.as_bytes()
    } else {
        peer.country.as_bytes()
    };
    let hash =
        key.iter().fold(0usize, |hash, byte| hash.wrapping_mul(31).wrapping_add(*byte as usize));
    palette.peers[hash % palette.peers.len()]
}

fn peer_style(
    peer: &crate::geo::snapshot::LocatedPeer,
    palette: MapPalette,
) -> Style {
    Style::default().fg(peer_color(peer, palette)).bg(palette.background)
}

fn set_cell(
    buf: &mut Buffer,
    x: i32,
    y: i32,
    area: Rect,
    symbol: &str,
    style: Style,
) {
    if x < area.x as i32
        || x >= (area.x + area.width) as i32
        || y < area.y as i32
        || y >= (area.y + area.height) as i32
    {
        return;
    }
    let cell = buf.get_mut(x as u16, y as u16);
    cell.set_symbol(symbol);
    cell.set_style(style);
}

fn half_block_symbol(mask: u8) -> Option<&'static str> {
    match mask {
        1 => Some(LAND_TOP),
        2 => Some(LAND_BOTTOM),
        3 => Some(LAND_FULL),
        _ => None,
    }
}

fn peer_marker(count: usize) -> &'static str {
    match count {
        1 => PEER_DOT,
        2..=4 => PEER_DIAMOND,
        _ => PEER_SQUARE,
    }
}

#[derive(Debug, Clone, Copy)]
struct RenderMode {
    half_blocks: bool,
    show_stats: bool,
    show_legend: bool,
}

fn render_mode(area: Rect) -> RenderMode {
    if area.width < HALF_BLOCK_MIN_WIDTH || area.height < HALF_BLOCK_MIN_HEIGHT {
        return RenderMode {
            half_blocks: false,
            show_stats: false,
            show_legend: false,
        };
    }

    if area.width < 45 || area.height < 10 {
        return RenderMode {
            half_blocks: false,
            show_stats: true,
            show_legend: false,
        };
    }

    RenderMode {
        half_blocks: true,
        show_stats: true,
        show_legend: area.height >= LEGEND_MIN_HEIGHT,
    }
}

/// Static equirectangular panel showing the Geo View Snapshot.
///
/// The snapshot is loaded through the `PeerGeoStore` handle on `update`;
/// drawing never queries the database or external services.
pub struct PeerMapWidget {
    update_interval: Ratio<u64>,
    collect_data: SharedData,
    store: Arc<dyn PeerGeoStore>,
    snapshot: GeoViewSnapshot,
}

fn map_rect(area: Rect) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }

    let available_width = area.width.min(MAX_MAP_WIDTH);
    let available_height = area.height.min(MAX_MAP_HEIGHT);
    let max_height = available_width / MAP_CELL_ASPECT_RATIO;
    let height = available_height.min(max_height.max(1));
    let width = available_width.min(height.saturating_mul(MAP_CELL_ASPECT_RATIO)).max(1);

    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn map_cell(
    lat: f64,
    lng: f64,
    area: Rect,
) -> Option<(i32, i32)> {
    if area.width == 0 || area.height == 0 {
        return None;
    }

    let (x, y) = project(lat, lng)?;
    let screen_x = area.x as i32 + (x * area.width.saturating_sub(1) as f64).round() as i32;
    let screen_y = area.y as i32 + (y * area.height.saturating_sub(1) as f64).round() as i32;
    Some((screen_x, screen_y))
}

fn target_index(
    coordinate: f64,
    count: usize,
) -> usize {
    if count <= 1 {
        return 0;
    }

    (coordinate * (count - 1) as f64).round().clamp(0.0, (count - 1) as f64) as usize
}

/// Return the target indices touched by a one-degree source cell.
///
/// The source cell's extent may straddle a target boundary, so both endpoint
/// indices are included instead of sampling only the cell center. `map_rect`
/// keeps target resolution at or below the one-degree source mask, which bounds
/// this range to the source cell and at most one adjacent target index.
fn target_index_range(
    minimum: f64,
    maximum: f64,
    count: usize,
) -> (usize, usize) {
    let first = target_index(minimum, count);
    let last = target_index(maximum, count);
    (first.min(last), first.max(last))
}
fn land_cell_target_ranges(
    row: usize,
    column: usize,
    area: Rect,
    half_blocks: bool,
) -> Option<((usize, usize), (usize, usize))> {
    if area.width == 0 || area.height == 0 {
        return None;
    }

    let (lat, lng) = land_cell_coordinates(row, column);
    let x_min = project(lat, lng - 0.5)?.0;
    let x_max = project(lat, lng + 0.5)?.0;
    let y_min = project(lat - 0.5, lng)?.1;
    let y_max = project(lat + 0.5, lng)?.1;
    let vertical_subcells = if half_blocks {
        usize::from(area.height).saturating_mul(2)
    } else {
        usize::from(area.height)
    };

    Some((
        target_index_range(x_min.min(x_max), x_min.max(x_max), usize::from(area.width)),
        target_index_range(y_min.min(y_max), y_min.max(y_max), vertical_subcells),
    ))
}

impl PeerMapWidget {
    /// Create a widget backed by `collect_data` for status reporting and
    /// `store` for Geo View Snapshot reads.
    pub fn new(
        collect_data: SharedData,
        store: Arc<dyn PeerGeoStore>,
    ) -> PeerMapWidget {
        PeerMapWidget {
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

    fn stats_line(&self) -> String {
        format!(
            "peers {} / located {} / countries {}",
            self.snapshot.total_peers, self.snapshot.located_peers, self.snapshot.unique_countries
        )
    }

    fn render_stats(
        &self,
        buf: &mut Buffer,
        area: Rect,
        colors: MapPalette,
    ) {
        buf.set_stringn(
            area.x,
            area.y,
            self.stats_line(),
            area.width as usize,
            Style::default().fg(colors.stats).bg(colors.background),
        );
    }

    fn render_legend(
        buf: &mut Buffer,
        area: Rect,
        colors: MapPalette,
    ) {
        buf.set_stringn(
            area.x,
            area.y,
            LEGEND_TEXT,
            area.width as usize,
            Style::default().fg(colors.stats).bg(colors.background),
        );
    }

    fn render_map(
        &self,
        buf: &mut Buffer,
        area: Rect,
        mode: RenderMode,
        colors: MapPalette,
    ) {
        let mut land_cells = BTreeMap::new();
        for row in 0..LAND_MASK_HEIGHT {
            for column in 0..LAND_MASK_WIDTH {
                if !land_mask_cell(row, column) {
                    continue;
                }
                let Some(((x_start, x_end), (y_start, y_end))) =
                    land_cell_target_ranges(row, column, area, mode.half_blocks)
                else {
                    continue;
                };
                for target_x in x_start..=x_end {
                    for target_y in y_start..=y_end {
                        let screen_x = area.x as i32 + target_x as i32;
                        let screen_y = if mode.half_blocks {
                            area.y as i32 + (target_y / 2) as i32
                        } else {
                            area.y as i32 + target_y as i32
                        };
                        if mode.half_blocks {
                            let mask = land_cells.entry((screen_x, screen_y)).or_insert(0);
                            *mask |= if target_y % 2 == 0 { 1 } else { 2 };
                        } else {
                            land_cells.entry((screen_x, screen_y)).or_insert(3);
                        }
                    }
                }
            }
        }

        let land_style = Style::default().fg(colors.land).bg(colors.background);
        for ((screen_x, screen_y), mask) in land_cells {
            if let Some(symbol) = half_block_symbol(mask) {
                set_cell(buf, screen_x, screen_y, area, symbol, land_style);
            }
        }

        let mut peer_cells =
            BTreeMap::<(i32, i32), (&crate::geo::snapshot::LocatedPeer, usize)>::new();
        for peer in &self.snapshot.peers {
            if let Some(cell) = map_cell(peer.lat, peer.lng, area) {
                if let Some((_, count)) = peer_cells.get_mut(&cell) {
                    *count = count.saturating_add(1);
                } else {
                    peer_cells.insert(cell, (peer, 1usize));
                }
            }
        }

        for ((screen_x, screen_y), (peer, count)) in peer_cells {
            set_cell(buf, screen_x, screen_y, area, peer_marker(count), peer_style(peer, colors));
        }
    }
}

impl UpdatableWidget for PeerMapWidget {
    fn update(&mut self) {
        self.refresh_snapshot();
    }

    fn get_update_interval(&self) -> Ratio<u64> {
        self.update_interval
    }
}

impl PeerMapWidget {
    fn render_with_color_mode(
        &self,
        area: Rect,
        buf: &mut Buffer,
        color_mode: ColorMode,
    ) {
        let colors = palette(color_mode);
        panel::new(MAP_TITLE)
            .style(Style::default().bg(colors.background))
            .border_style(Style::default().fg(colors.border).bg(colors.background))
            .title_style(Style::default().fg(colors.title).bg(colors.background))
            .render(area, buf);

        let inner = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mode = render_mode(inner);
        let mut map_height = inner.height;
        if mode.show_stats {
            let stats_row = inner.y + inner.height - 1;
            self.render_stats(
                buf,
                Rect {
                    x: inner.x,
                    y: stats_row,
                    width: inner.width,
                    height: 1,
                },
                colors,
            );
            map_height = map_height.saturating_sub(1);
        }
        if mode.show_legend {
            let legend_row = inner.y + map_height - 1;
            PeerMapWidget::render_legend(
                buf,
                Rect {
                    x: inner.x,
                    y: legend_row,
                    width: inner.width,
                    height: 1,
                },
                colors,
            );
            map_height = map_height.saturating_sub(1);
        }

        let map_area = map_rect(Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: map_height,
        });
        if map_area.width > 0 && map_area.height > 0 {
            self.render_map(buf, map_area, mode, colors);
        }
    }
}

impl Widget for &PeerMapWidget {
    fn render(
        self,
        area: Rect,
        buf: &mut Buffer,
    ) {
        self.render_with_color_mode(area, buf, color_mode());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        style::Modifier,
        widgets::Widget,
    };

    use super::*;
    use crate::{
        collect::Data,
        geo::{
            snapshot::LocatedPeer,
            testutil::FakePeerGeoStore,
        },
    };

    fn render_map_widget(
        snapshot: GeoViewSnapshot,
        area: Rect,
    ) -> Buffer {
        let data = Data::new();
        let store = Arc::new(FakePeerGeoStore::new(snapshot));
        let mut widget = PeerMapWidget::new(data, store);
        widget.update();
        let mut buf = Buffer::empty(area);
        (&widget).render(area, &mut buf);
        buf
    }

    fn render_map_widget_in_color_mode(
        snapshot: GeoViewSnapshot,
        area: Rect,
        mode: ColorMode,
    ) -> Buffer {
        let data = Data::new();
        let store = Arc::new(FakePeerGeoStore::new(snapshot));
        let mut widget = PeerMapWidget::new(data, store);
        widget.update();
        let mut buf = Buffer::empty(area);
        widget.render_with_color_mode(area, &mut buf, mode);
        buf
    }

    fn snapshot_with_peers(peers: Vec<LocatedPeer>) -> GeoViewSnapshot {
        let located = peers.len();
        let countries = peers
            .iter()
            .map(|peer| peer.country.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len();
        GeoViewSnapshot {
            total_peers: located,
            located_peers: located,
            unique_countries: countries,
            peers,
            ..GeoViewSnapshot::default()
        }
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

    fn count_peer_markers(
        buf: &Buffer,
        area: Rect,
    ) -> usize {
        let inner = Rect {
            x: area.x.saturating_add(1),
            y: area.y.saturating_add(1),
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        let mode = render_mode(inner);
        let mut map_height = inner.height;
        if mode.show_stats {
            map_height = map_height.saturating_sub(1);
        }
        if mode.show_legend {
            map_height = map_height.saturating_sub(1);
        }

        let mut markers = 0;
        for x in inner.x..inner.x + inner.width {
            for y in inner.y..inner.y + map_height {
                if matches!(buf.get(x, y).symbol(), PEER_DOT | "◆" | "■") {
                    markers += 1;
                }
            }
        }
        markers
    }

    #[test]
    fn test_peer_map_title_constants() {
        assert_eq!(MAP_TITLE, " Peer Map ");
        assert_eq!(LAND_TOP, "▀");
        assert_eq!(LAND_BOTTOM, "▄");
        assert_eq!(LAND_FULL, "█");
        assert_eq!(PEER_DOT, "●");
        assert_eq!(PEER_DIAMOND, "◆");
        assert_eq!(PEER_SQUARE, "■");
        assert_eq!(LEGEND_TEXT, "●1 / ◆2-4 / ■5+");
    }

    #[test]
    fn test_land_mask_has_expected_shape_boundaries_and_locations() {
        assert_eq!(LAND_MASK_WIDTH, 360);
        assert_eq!(LAND_MASK_HEIGHT, 180);
        assert_eq!(LAND_MASK.len(), LAND_MASK_BYTES);
        assert_eq!(LAND_MASK_BYTES, 8_100);
        assert_eq!(land_cell_coordinates(0, 0), (89.5, -179.5));
        assert_eq!(land_cell_coordinates(179, 359), (-89.5, 179.5));
        assert!(!land_mask_cell(LAND_MASK_HEIGHT, 0));
        assert!(!land_mask_cell(0, LAND_MASK_WIDTH));

        assert!(land_mask_cell(89, 200), "Africa should include 0.5°N, 20.5°E");
        assert!(land_mask_cell(104, 120), "South America should include 14.5°S, 59.5°W");
        assert!(!land_mask_cell(89, 150), "the mid-Atlantic should be water");
        assert!(!land_mask_cell(89, 330), "the central Pacific should be water");
    }

    #[test]
    fn test_half_block_symbols_encode_land_coverage() {
        assert_eq!(half_block_symbol(0), None);
        assert_eq!(half_block_symbol(1), Some(LAND_TOP));
        assert_eq!(half_block_symbol(2), Some(LAND_BOTTOM));
        assert_eq!(half_block_symbol(3), Some(LAND_FULL));
    }

    #[test]
    fn test_color_capability_detection_uses_safe_fallback_order() {
        assert_eq!(color_mode_from_values(false, "xterm-direct", ""), ColorMode::TrueColor);
        assert_eq!(color_mode_from_values(false, "screen-256color", ""), ColorMode::Ansi256);
        assert_eq!(color_mode_from_values(false, "xterm", ""), ColorMode::Ansi16);
        assert_eq!(color_mode_from_values(false, "", ""), ColorMode::Ansi16);
        assert_eq!(color_mode_from_values(false, "xterm", "truecolor"), ColorMode::TrueColor);
        assert_eq!(
            color_mode_from_values(true, "xterm-direct", "truecolor"),
            ColorMode::Monochrome
        );
        assert_eq!(color_mode_from_values(false, "dumb", "truecolor"), ColorMode::Monochrome);
    }

    #[test]
    fn test_equirectangular_projection_covers_the_full_world_from_zero_meridian() {
        assert_eq!(project(0.0, 0.0), Some((0.5, 0.5)));
        assert_eq!(project(90.0, 0.0), Some((0.5, 0.0)));
        assert_eq!(project(-90.0, 0.0), Some((0.5, 1.0)));
        assert_eq!(project(0.0, -180.0), Some((0.0, 0.5)));
        assert_eq!(project(0.0, 180.0), Some((1.0, 0.5)));
        assert_eq!(project(0.0, 360.0), Some((0.5, 0.5)));
    }

    #[test]
    fn test_projection_rejects_invalid_latitudes() {
        assert!(project(90.1, 0.0).is_none());
        assert!(project(-90.1, 0.0).is_none());
        assert!(project(f64::NAN, 0.0).is_none());
        assert!(project(0.0, f64::INFINITY).is_none());
    }

    #[test]
    fn test_map_rect_preserves_terminal_aspect_and_centers_content() {
        assert_eq!(map_rect(Rect::new(1, 2, 38, 13)), Rect::new(2, 4, 36, 9));
        assert_eq!(map_rect(Rect::new(0, 0, 10, 3)), Rect::new(1, 0, 8, 2));
        assert_eq!(map_rect(Rect::new(0, 0, 400, 100)), Rect::new(20, 5, 360, 90));
    }

    #[test]
    fn test_compact_land_extent_reaches_adjacent_target_cell() {
        assert!(land_mask_cell(6, 142));
        let widget_area = Rect::new(0, 0, 30, 8);
        let map_area = map_rect(Rect::new(1, 1, 28, 5));
        assert_eq!(map_area, Rect::new(5, 1, 20, 5));

        let (lat, lng) = land_cell_coordinates(6, 142);
        assert_eq!(map_cell(lat, lng, map_area), Some((13, 1)));

        let buf = render_map_widget(GeoViewSnapshot::default(), widget_area);
        assert_eq!(buf.get(12, 1).symbol(), LAND_FULL);
        assert_eq!(buf.get(13, 1).symbol(), LAND_FULL);
    }

    #[test]
    fn test_render_modes_preserve_half_blocks_and_hide_stats_when_small() {
        let full = render_mode(Rect::new(0, 0, 50, 12));
        assert!(full.half_blocks);
        assert!(full.show_stats);
        assert!(full.show_legend);

        let normal = render_mode(Rect::new(0, 0, 50, 10));
        assert!(normal.half_blocks);
        assert!(normal.show_stats);
        assert!(!normal.show_legend);

        let compact = render_mode(Rect::new(0, 0, 30, 8));
        assert!(!compact.half_blocks);
        assert!(compact.show_stats);
        assert!(!compact.show_legend);

        let fallback = render_mode(Rect::new(0, 0, 23, 7));
        assert!(!fallback.half_blocks);
        assert!(!fallback.show_stats);

        let tiny = render_mode(Rect::new(0, 0, 10, 3));
        assert!(!tiny.half_blocks);
        assert!(!tiny.show_stats);

        let minimal = render_mode(Rect::new(0, 0, 7, 2));
        assert!(!minimal.half_blocks);
        assert!(!minimal.show_stats);
    }

    #[test]
    fn test_color_palette_falls_back_in_distinct_capability_steps() {
        let true_color = palette(ColorMode::TrueColor);
        assert_eq!(true_color.peers[0], Color::Rgb(71, 220, 214));
        assert_eq!(palette(ColorMode::Ansi256).peers[0], Color::Indexed(80));
        assert_eq!(palette(ColorMode::Ansi16).peers[0], Color::Cyan);
        assert_eq!(palette(ColorMode::Monochrome).peers, [Color::Reset; 4]);
        assert_eq!(palette(ColorMode::Ansi256).background, Color::Indexed(236));
        assert_eq!(palette(ColorMode::Ansi16).background, Color::Black);
    }

    #[test]
    fn test_peer_marker_style_keeps_colors_and_bold_behavior_across_modes() {
        let peer = LocatedPeer {
            ip: "1.1.1.1".to_string(),
            country: "AA".to_string(),
            lat: 0.0,
            lng: 0.0,
        };
        let expected_colors = [
            (ColorMode::TrueColor, Color::Rgb(71, 220, 214)),
            (ColorMode::Ansi256, Color::Indexed(80)),
            (ColorMode::Ansi16, Color::Cyan),
            (ColorMode::Monochrome, Color::Reset),
        ];

        for (mode, expected_color) in expected_colors {
            let colors = palette(mode);
            let style = peer_style(&peer, colors);

            assert_eq!(style.fg, Some(expected_color));
            assert_eq!(style.bg, Some(colors.background));
            assert_eq!(style.add_modifier, Modifier::empty());
            assert_eq!(style.sub_modifier, Modifier::empty());
        }
    }

    #[test]
    fn test_peer_map_renders_all_supported_color_modes() {
        let snapshot = snapshot_with_peers(vec![LocatedPeer {
            ip: "1.1.1.1".to_string(),
            country: "AA".to_string(),
            lat: 0.0,
            lng: 0.0,
        }]);
        let expected_colors = [
            (ColorMode::TrueColor, Color::Rgb(71, 220, 214)),
            (ColorMode::Ansi256, Color::Indexed(80)),
            (ColorMode::Ansi16, Color::Cyan),
            (ColorMode::Monochrome, Color::Reset),
        ];

        for (mode, expected_peer_color) in expected_colors {
            let colors = palette(mode);
            let buf =
                render_map_widget_in_color_mode(snapshot.clone(), Rect::new(0, 0, 60, 20), mode);
            let marker = (0..60)
                .flat_map(|x| (1..17).map(move |y| (x, y)))
                .find(|&(x, y)| buf.get(x, y).symbol() == PEER_DOT)
                .expect("single peer marker should be visible");
            let marker_cell = buf.get(marker.0, marker.1);

            assert_eq!(marker_cell.fg, expected_peer_color);
            assert_eq!(marker_cell.bg, colors.background);
            assert_eq!(marker_cell.modifier, Modifier::empty());
            assert_eq!(buf.get(1, 17).fg, colors.stats);
            assert_eq!(buf.get(1, 17).bg, colors.background);
            assert_eq!(buf.get(0, 0).fg, colors.border);
            assert_eq!(buf.get(2, 0).fg, colors.title);
        }
    }

    #[test]
    fn test_normal_size_map_snapshot() {
        let area = Rect::new(0, 0, 60, 20);
        let buf = render_map_widget(GeoViewSnapshot::default(), area);

        assert_eq!(buffer_text(&buf, area), include_str!("map_snapshot.txt"));
    }

    #[test]
    fn test_compact_map_snapshot() {
        let area = Rect::new(0, 0, 20, 6);
        let buf = render_map_widget(GeoViewSnapshot::default(), area);

        assert_eq!(buffer_text(&buf, area), include_str!("map_compact_snapshot.txt"));
    }

    #[test]
    fn test_empty_snapshot_renders_map_panel_and_stats() {
        let area = Rect::new(0, 0, 40, 16);
        let buf = render_map_widget(GeoViewSnapshot::default(), area);

        assert_eq!(buf.get(0, 0).symbol(), "┌");
        assert_eq!(buf.get(2, 0).symbol(), "P");
        assert_eq!(buf.get(7, 0).symbol(), "M");
        assert_eq!(buf.get(39, 15).symbol(), "┘");

        let stats = buf.get(1, 14).symbol();
        assert_eq!(stats, "p", "stats line should start at inner bottom row");
        assert_eq!(buf.get(7, 14).symbol(), "0");
    }

    #[test]
    fn test_peer_map_renders_peer_at_zero_meridian_and_equator() {
        let snapshot = snapshot_with_peers(vec![LocatedPeer {
            ip: "1.1.1.1".to_string(),
            country: "CN".to_string(),
            lat: 0.0,
            lng: 0.0,
        }]);
        let buf = render_map_widget(snapshot, Rect::new(0, 0, 40, 16));

        assert_eq!(buf.get(20, 7).symbol(), PEER_DOT);
    }

    #[test]
    fn test_peer_marker_overlays_land_cell() {
        let (lat, lng) = land_cell_coordinates(89, 200);
        let snapshot = snapshot_with_peers(vec![LocatedPeer {
            ip: "1.1.1.1".to_string(),
            country: "US".to_string(),
            lat,
            lng,
        }]);
        let area = Rect::new(0, 0, 60, 20);
        let buf = render_map_widget(snapshot, area);
        let inner = Rect::new(1, 1, 58, 18);
        let map_area = map_rect(Rect::new(1, 1, inner.width, inner.height - 1));
        let (x, y) = map_cell(lat, lng, map_area).expect("land point should project");

        assert_eq!(buf.get(x as u16, y as u16).symbol(), PEER_DOT);
    }

    #[test]
    fn test_peer_map_shows_peers_across_the_full_longitude_range() {
        let snapshot = snapshot_with_peers(vec![
            LocatedPeer {
                ip: "1.1.1.1".to_string(),
                country: "US".to_string(),
                lat: 0.0,
                lng: -179.0,
            },
            LocatedPeer {
                ip: "2.2.2.2".to_string(),
                country: "CN".to_string(),
                lat: 0.0,
                lng: 0.0,
            },
            LocatedPeer {
                ip: "3.3.3.3".to_string(),
                country: "JP".to_string(),
                lat: 0.0,
                lng: 179.0,
            },
        ]);
        let area = Rect::new(0, 0, 80, 20);
        let buf = render_map_widget(snapshot, area);

        assert_eq!(count_peer_markers(&buf, area), 3);
    }

    #[test]
    fn test_peer_near_date_line_is_rendered_once() {
        let snapshot = snapshot_with_peers(vec![LocatedPeer {
            ip: "1.1.1.1".to_string(),
            country: "US".to_string(),
            lat: 0.0,
            lng: 180.0,
        }]);
        let area = Rect::new(0, 0, 40, 16);
        let buf = render_map_widget(snapshot, area);

        assert_eq!(count_peer_markers(&buf, area), 1);
        assert_eq!(buf.get(37, 7).symbol(), PEER_DOT);
    }

    #[test]
    fn test_overlapping_peers_render_as_one_marker() {
        let snapshot = snapshot_with_peers(vec![
            LocatedPeer {
                ip: "1.1.1.1".to_string(),
                country: "CN".to_string(),
                lat: 0.0,
                lng: 0.0,
            },
            LocatedPeer {
                ip: "2.2.2.2".to_string(),
                country: "CN".to_string(),
                lat: 0.0,
                lng: 0.0,
            },
        ]);
        let area = Rect::new(0, 0, 40, 16);
        let buf = render_map_widget(snapshot, area);

        assert_eq!(count_peer_markers(&buf, area), 1);
        assert_eq!(buf.get(20, 7).symbol(), "◆");
    }

    #[test]
    fn test_peer_position_aggregation_uses_count_buckets() {
        for (count, expected) in [(1, "●"), (2, "◆"), (4, "◆"), (5, "■"), (9, "■")] {
            let peers = (0..count)
                .map(|index| LocatedPeer {
                    ip: format!("192.0.2.{index}"),
                    country: "CN".to_string(),
                    lat: 0.0,
                    lng: 0.0,
                })
                .collect();
            let area = Rect::new(0, 0, 40, 16);
            let buf = render_map_widget(snapshot_with_peers(peers), area);

            assert_eq!(count_peer_markers(&buf, area), 1);
            assert_eq!(buf.get(20, 7).symbol(), expected, "peer count: {count}");
        }
    }

    #[test]
    fn test_peer_map_legend_is_visible_only_when_the_map_has_room() {
        let normal = render_map_widget(GeoViewSnapshot::default(), Rect::new(0, 0, 60, 20));
        assert!(buffer_text(&normal, Rect::new(0, 0, 60, 20)).contains("●1 / ◆2-4 / ■5+"));

        let compact = render_map_widget(GeoViewSnapshot::default(), Rect::new(0, 0, 20, 6));
        assert!(!buffer_text(&compact, Rect::new(0, 0, 20, 6)).contains("●1 / ◆2-4 / ■5+"));
    }

    #[test]
    fn test_peer_map_handles_minimum_drawable_dimensions() {
        for width in 0..=4 {
            for height in 0..=4 {
                let area = Rect::new(0, 0, width, height);
                let _ = render_map_widget(GeoViewSnapshot::default(), area);
            }
        }

        let drawable = render_map_widget(GeoViewSnapshot::default(), Rect::new(0, 0, 3, 3));
        assert_eq!(drawable.get(0, 0).symbol(), "┌");
        assert_eq!(drawable.get(1, 1).symbol(), LAND_FULL);
        assert_eq!(drawable.get(2, 2).symbol(), "┘");

        let border_only = render_map_widget(GeoViewSnapshot::default(), Rect::new(0, 0, 2, 2));
        assert_eq!(border_only.get(0, 0).symbol(), "┌");
        assert_eq!(border_only.get(1, 1).symbol(), "┘");
    }

    #[test]
    fn test_peer_map_renders_land_half_blocks() {
        let area = Rect::new(0, 0, 60, 20);
        let buf = render_map_widget(GeoViewSnapshot::default(), area);

        let mut land_cells = 0;
        let mut land_symbols = [0; 3];
        for x in 1..59 {
            for y in 1..19 {
                match buf.get(x, y).symbol() {
                    LAND_TOP => {
                        land_cells += 1;
                        land_symbols[0] += 1;
                    },
                    LAND_BOTTOM => {
                        land_cells += 1;
                        land_symbols[1] += 1;
                    },
                    LAND_FULL => {
                        land_cells += 1;
                        land_symbols[2] += 1;
                    },
                    _ => {},
                }
            }
        }
        assert!(land_cells > 50, "continent half-block matrix should be visible: {land_cells}");
        assert!(land_symbols.iter().all(|count| *count > 0));
    }

    #[test]
    fn test_compact_peer_marker_overlays_land_cell() {
        let (lat, lng) = land_cell_coordinates(89, 200);
        let snapshot = snapshot_with_peers(vec![LocatedPeer {
            ip: "1.1.1.1".to_string(),
            country: "US".to_string(),
            lat,
            lng,
        }]);
        let area = Rect::new(0, 0, 20, 6);
        let buf = render_map_widget(snapshot, area);
        let map_area = map_rect(Rect::new(1, 1, 18, 4));
        let (x, y) = map_cell(lat, lng, map_area).expect("land point should project");

        assert_eq!(buf.get(x as u16, y as u16).symbol(), PEER_DOT);
    }

    #[test]
    fn test_compact_map_aggregates_land_into_full_cells() {
        let buf = render_map_widget(GeoViewSnapshot::default(), Rect::new(0, 0, 20, 6));
        let land_cells = (1..19)
            .flat_map(|x| (1..5).map(move |y| (x, y)))
            .filter(|&(x, y)| buf.get(x, y).symbol() == LAND_FULL)
            .count();

        assert!(land_cells > 0);
        assert!((1..19)
            .flat_map(|x| (1..5).map(move |y| (x, y)))
            .all(|(x, y)| buf.get(x, y).symbol() != "░"));
    }

    #[test]
    fn test_tiny_area_keeps_a_static_downsampled_map() {
        let snapshot = snapshot_with_peers(vec![LocatedPeer {
            ip: "1.1.1.1".to_string(),
            country: "CN".to_string(),
            lat: 0.0,
            lng: 0.0,
        }]);
        let buf = render_map_widget(snapshot, Rect::new(0, 0, 10, 3));

        assert_eq!(buf.get(5, 1).symbol(), PEER_DOT);
        assert_ne!(buf.get(1, 1).symbol(), "p", "stats should be hidden when space is tight");
    }

    #[test]
    fn test_peer_map_handles_tiny_area() {
        let area = Rect::new(0, 0, 10, 3);
        let buf = render_map_widget(GeoViewSnapshot::default(), area);

        assert_eq!(buf.get(0, 0).symbol(), "┌");
        assert_eq!(buf.get(9, 2).symbol(), "┘");
    }

    #[test]
    fn test_peer_map_update_interval_is_zero() {
        let data = Data::new();
        let store = Arc::new(FakePeerGeoStore::new(GeoViewSnapshot::default()));
        let widget = PeerMapWidget::new(data, store);

        assert_eq!(widget.get_update_interval(), Ratio::from_integer(0));
    }

    #[test]
    fn test_peer_map_update_loads_snapshot() {
        let data = Data::new();
        let snapshot = snapshot_with_peers(vec![LocatedPeer {
            ip: "1.1.1.1".to_string(),
            country: "CN".to_string(),
            lat: 0.0,
            lng: 0.0,
        }]);
        let store = Arc::new(FakePeerGeoStore::new(snapshot.clone()));
        let mut widget = PeerMapWidget::new(data, store);

        assert!(widget.refresh_snapshot());

        assert_eq!(widget.snapshot(), &snapshot);
    }

    #[test]
    fn test_peer_map_update_reports_read_failure() {
        let data = Data::new();
        let store: Arc<dyn PeerGeoStore> = Arc::new(FailingStore);
        let mut widget = PeerMapWidget::new(data.clone(), store);

        assert!(!widget.refresh_snapshot());

        assert_eq!(widget.snapshot(), &GeoViewSnapshot::default());
        let status = data.lock().expect("mutex poisoned").status_message();
        assert!(status.is_some());
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
