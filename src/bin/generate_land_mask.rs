use std::{
    env,
    fs,
    io::Write,
    process,
};

const WIDTH: usize = 360;
const HEIGHT: usize = 180;
const MASK_BYTES: usize = WIDTH * HEIGHT / 8;
const SHAPE_HEADER_BYTES: usize = 100;

#[derive(Clone, Copy)]
struct Point {
    longitude: f64,
    latitude: f64,
}

struct Ring {
    points: Vec<Point>,
}

fn usage() -> ! {
    eprintln!("usage: generate_land_mask <input.shp> <output.bin>");
    process::exit(2);
}

fn read_i32_le(
    bytes: &[u8],
    offset: usize,
) -> Result<i32, String> {
    let end = offset.checked_add(4).ok_or_else(|| "integer offset overflow".to_string())?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| format!("truncated little-endian integer at byte {offset}"))?;
    Ok(i32::from_le_bytes(value.try_into().expect("four-byte slice")))
}

fn read_i32_be(
    bytes: &[u8],
    offset: usize,
) -> Result<i32, String> {
    let end = offset.checked_add(4).ok_or_else(|| "integer offset overflow".to_string())?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| format!("truncated big-endian integer at byte {offset}"))?;
    Ok(i32::from_be_bytes(value.try_into().expect("four-byte slice")))
}

fn read_f64_le(
    bytes: &[u8],
    offset: usize,
) -> Result<f64, String> {
    let end = offset.checked_add(8).ok_or_else(|| "float offset overflow".to_string())?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| format!("truncated little-endian float at byte {offset}"))?;
    Ok(f64::from_le_bytes(value.try_into().expect("eight-byte slice")))
}

fn parse_polygon_record(
    content: &[u8],
    rings: &mut Vec<Ring>,
) -> Result<(), String> {
    if content.len() < 4 {
        return Err("polygon record is shorter than its shape type".to_string());
    }

    let shape_type = read_i32_le(content, 0)?;
    if shape_type == 0 {
        return Ok(());
    }
    if shape_type != 5 {
        return Err(format!("expected Polygon record (type 5), got type {shape_type}"));
    }
    if content.len() < 44 {
        return Err("polygon record is shorter than its fixed header".to_string());
    }

    let parts = read_i32_le(content, 36)?;
    let points = read_i32_le(content, 40)?;
    if parts < 1 || points < 3 {
        return Err(format!("invalid polygon counts: {parts} parts, {points} points"));
    }
    let parts = parts as usize;
    let points = points as usize;
    let parts_end = 44usize
        .checked_add(parts.checked_mul(4).ok_or_else(|| "parts overflow".to_string())?)
        .ok_or_else(|| "parts offset overflow".to_string())?;
    let points_end = parts_end
        .checked_add(points.checked_mul(16).ok_or_else(|| "points overflow".to_string())?)
        .ok_or_else(|| "points offset overflow".to_string())?;
    if points_end > content.len() {
        return Err("polygon record is truncated".to_string());
    }

    let mut starts = Vec::with_capacity(parts);
    for part in 0..parts {
        let start = read_i32_le(content, 44 + part * 4)?;
        if start < 0 || start as usize >= points {
            return Err(format!("invalid polygon part start {start}"));
        }
        starts.push(start as usize);
    }

    for (part, &start) in starts.iter().enumerate() {
        let end = starts.get(part + 1).copied().unwrap_or(points);
        if end <= start || end - start < 3 {
            return Err("polygon ring has fewer than three points".to_string());
        }
        let mut ring = Vec::with_capacity(end - start);
        for point in start..end {
            let offset = parts_end + point * 16;
            let longitude = read_f64_le(content, offset)?;
            let latitude = read_f64_le(content, offset + 8)?;
            if !longitude.is_finite() || !latitude.is_finite() {
                return Err("polygon contains a non-finite coordinate".to_string());
            }
            ring.push(Point {
                longitude,
                latitude,
            });
        }
        rings.push(Ring { points: ring });
    }

    Ok(())
}

fn parse_shapefile(bytes: &[u8]) -> Result<Vec<Ring>, String> {
    if bytes.len() < SHAPE_HEADER_BYTES {
        return Err("shapefile is shorter than its 100-byte header".to_string());
    }
    if read_i32_be(bytes, 0)? != 9994 {
        return Err("input is not an ESRI shapefile".to_string());
    }
    if read_i32_le(bytes, 28)? != 1000 {
        return Err("unsupported shapefile version".to_string());
    }
    if read_i32_le(bytes, 32)? != 5 {
        return Err("input shapefile is not a Polygon layer".to_string());
    }

    let mut rings = Vec::new();
    let mut offset = SHAPE_HEADER_BYTES;
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            return Err(format!("truncated record header at byte {offset}"));
        }
        let content_words = read_i32_be(bytes, offset + 4)?;
        if content_words < 2 {
            return Err(format!("invalid record length at byte {offset}"));
        }
        let content_bytes = (content_words as usize)
            .checked_mul(2)
            .ok_or_else(|| "record length overflow".to_string())?;
        let content_start = offset + 8;
        let content_end = content_start
            .checked_add(content_bytes)
            .ok_or_else(|| "record offset overflow".to_string())?;
        if content_end > bytes.len() {
            return Err(format!("truncated record at byte {offset}"));
        }
        parse_polygon_record(&bytes[content_start..content_end], &mut rings)?;
        offset = content_end;
    }

    if rings.is_empty() {
        return Err("input shapefile contains no polygon rings".to_string());
    }
    Ok(rings)
}

fn point_on_segment(
    point: Point,
    start: Point,
    end: Point,
) -> bool {
    let cross = (point.latitude - start.latitude) * (end.longitude - start.longitude)
        - (point.longitude - start.longitude) * (end.latitude - start.latitude);
    if cross.abs() > 1e-9 {
        return false;
    }
    let min_longitude = start.longitude.min(end.longitude) - 1e-9;
    let max_longitude = start.longitude.max(end.longitude) + 1e-9;
    let min_latitude = start.latitude.min(end.latitude) - 1e-9;
    let max_latitude = start.latitude.max(end.latitude) + 1e-9;
    (min_longitude..=max_longitude).contains(&point.longitude)
        && (min_latitude..=max_latitude).contains(&point.latitude)
}

fn point_in_ring(
    point: Point,
    ring: &Ring,
) -> bool {
    let mut inside = false;
    for index in 0..ring.points.len() {
        let start = ring.points[index];
        let end = ring.points[(index + 1) % ring.points.len()];
        if point_on_segment(point, start, end) {
            return true;
        }
        let crosses_latitude = (start.latitude > point.latitude) != (end.latitude > point.latitude);
        if crosses_latitude {
            let intersection = start.longitude
                + (end.longitude - start.longitude) * (point.latitude - start.latitude)
                    / (end.latitude - start.latitude);
            if point.longitude < intersection {
                inside = !inside;
            }
        }
    }
    inside
}

fn is_land(
    point: Point,
    rings: &[Ring],
) -> bool {
    rings.iter().filter(|ring| point_in_ring(point, ring)).count() % 2 == 1
}

fn set_bit(
    mask: &mut [u8],
    row: usize,
    column: usize,
) {
    let bit = row * WIDTH + column;
    mask[bit / 8] |= 1 << (7 - bit % 8);
}

fn generate_mask(rings: &[Ring]) -> Vec<u8> {
    let mut mask = vec![0; MASK_BYTES];
    for row in 0..HEIGHT {
        let latitude = 90.0 - row as f64 - 0.5;
        for column in 0..WIDTH {
            let point = Point {
                longitude: -180.0 + column as f64 + 0.5,
                latitude,
            };
            if is_land(point, rings) {
                set_bit(&mut mask, row, column);
            }
        }
    }
    mask
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let input = args.next().ok_or_else(|| "missing input path".to_string())?;
    let output = args.next().ok_or_else(|| "missing output path".to_string())?;
    if args.next().is_some() {
        return Err("too many arguments".to_string());
    }

    let source = fs::read(&input).map_err(|err| format!("read {input}: {err}"))?;
    let rings = parse_shapefile(&source)?;
    let mask = generate_mask(&rings);
    let mut file = fs::File::create(&output).map_err(|err| format!("create {output}: {err}"))?;
    file.write_all(&mask).map_err(|err| format!("write {output}: {err}"))?;
    file.flush().map_err(|err| format!("flush {output}: {err}"))?;

    eprintln!("wrote {} bytes from {} polygon rings to {output}", mask.len(), rings.len());
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        usage();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_packing_is_row_major_and_most_significant_bit_first() {
        let mut mask = vec![0; MASK_BYTES];
        set_bit(&mut mask, 0, 0);
        set_bit(&mut mask, 0, 7);
        set_bit(&mut mask, 0, 8);

        assert_eq!(mask[0], 0b1000_0001);
        assert_eq!(mask[1], 0b1000_0000);
        assert!(mask[2..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn rasterizer_handles_closed_and_open_square_rings() {
        let ring = Ring {
            points: vec![
                Point {
                    longitude: -1.0,
                    latitude: -1.0,
                },
                Point {
                    longitude: 1.0,
                    latitude: -1.0,
                },
                Point {
                    longitude: 1.0,
                    latitude: 1.0,
                },
                Point {
                    longitude: -1.0,
                    latitude: 1.0,
                },
            ],
        };
        assert!(point_in_ring(
            Point {
                longitude: 0.0,
                latitude: 0.0,
            },
            &ring,
        ));
        assert!(point_in_ring(
            Point {
                longitude: 1.0,
                latitude: 0.0,
            },
            &ring,
        ));
        assert!(!point_in_ring(
            Point {
                longitude: 2.0,
                latitude: 0.0,
            },
            &ring,
        ));

        let mask = generate_mask(&[ring]);
        for row in [89, 90] {
            for column in [179, 180] {
                let bit = row * WIDTH + column;
                assert_ne!(mask[bit / 8] & (1 << (7 - bit % 8)), 0);
            }
        }
    }

    #[test]
    fn shapefile_parser_rejects_invalid_headers() {
        assert!(parse_shapefile(&[]).is_err());
        assert!(parse_shapefile(&[0; SHAPE_HEADER_BYTES]).is_err());
    }
}
