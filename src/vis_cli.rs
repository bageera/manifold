// Atlas visualization (v0.6): mode map + savings heat map as PNGs, one pixel
// per tile. Red = literal, green = exact reference, blue = geometric reference.
// Savings heat: black (no gain) -> white (max gain). Reads straight from
// encode_stats output; no bitstream parsing.

use crate::codec::{tile_grid, Dims, GeomStats};
use std::io::BufWriter;
use std::path::Path;

pub fn write_maps(out_prefix: &Path, dims: Dims, st: &GeomStats) -> Result<(), String> {
    let (cols, rows) = tile_grid(dims);
    let n = (cols * rows) as usize;
    if st.modes.len() < n || st.savings.len() < n {
        return Err(format!(
            "stats cover {} tiles, image needs {n}",
            st.modes.len()
        ));
    }

    // mode map
    let mut mode_px = vec![0u8; n * 3];
    for i in 0..n {
        let (r, g, b) = match st.modes[i] {
            0 => (220u8, 60u8, 50u8), // literal: red
            1 => (60u8, 180u8, 75u8), // exact ref: green
            2 => (55u8, 90u8, 220u8), // geom ref: blue
            m => return Err(format!("invalid mode {m} in stats")),
        };
        mode_px[i * 3..i * 3 + 3].copy_from_slice(&[r, g, b]);
    }
    save(
        &out_prefix.with_extension("modes.png"),
        cols,
        rows,
        &mode_px,
    )?;

    // savings heat map (normalize by max positive saving)
    let max_save = st.savings.iter().copied().max().unwrap_or(0).max(1);
    let mut heat = vec![0u8; n * 3];
    for i in 0..n {
        let s = st.savings[i].max(0) as f64 / max_save as f64;
        let v = (s * 255.0).round() as u8;
        heat[i * 3..i * 3 + 3].copy_from_slice(&[v, v, v]);
    }
    // origins map: each tile colored by the atlas patch it copies from
    // (deterministic index-derived palette) — shared provenance shows up as
    // same-colored runs; literals stay dark.
    let mut orig = vec![0u8; n * 3];
    for i in 0..n {
        let px: [u8; 3] = match st.refs[i] {
            Some((idx, _)) => [
                ((idx * 67) % 200 + 40) as u8,
                ((idx * 131) % 200 + 40) as u8,
                ((idx * 199) % 200 + 40) as u8,
            ],
            None => [30, 30, 34],
        };
        orig[i * 3..i * 3 + 3].copy_from_slice(&px);
    }
    save(&out_prefix.with_extension("origins.png"), cols, rows, &orig)?;
    Ok(())
}

fn save(path: &Path, w: u32, h: u32, px: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("{e}"))?;
    let mut enc = png::Encoder::new(BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| format!("png: {e}"))?;
    wr.write_image_data(px).map_err(|e| format!("png: {e}"))
}
