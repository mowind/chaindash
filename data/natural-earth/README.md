# Natural Earth 1:110m land input

`ne_110m_land.shp` is the geometry input used to generate the embedded Peer Map
land mask. It is the `ne_110m_land` layer from Natural Earth **version 5.1.2**
(the archive reports dataset version 4.1.0), downloaded from the fixed archive
URL:

<https://naturalearth.s3.amazonaws.com/5.1.2/110m_physical/ne_110m_land.zip>

The archive SHA-256 is
`1926c621afd6ac67c3f36639bb1236134a48d82226dc675d3e3df53d02d2a3de`; the
checked-in `.shp` component has SHA-256
`8689e6932b8e370e2ca4587cf3ba21e460b1235db37b6ed3c172c35b4a6088de`.
The checked-in `.prj` file records the source coordinate system. Natural Earth
data is public domain; see <https://www.naturalearthdata.com/about/terms-of-use/>.

The checked-in shapefile is intentionally the geometry-only `.shp` component:
the generator does not need the attribute table or spatial index. Recreate the
mask with:

```text
cargo run --quiet --bin generate_land_mask -- data/natural-earth/ne_110m_land.shp data/land_mask.bin
```

The output is exactly 8,100 bytes: row-major cells for 360 longitudes by 180
latitudes, with the first cell centered at `(89.5°N, 179.5°W)`. Bits are stored
most-significant-bit first within each byte. The application embeds this file
at compile time; it never reads the source geometry or mask from disk at
runtime.
