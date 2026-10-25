// MNF5 codec: encoder + decoder + shared header.
// Format: 24-byte header ("MNF5", u32 LE width, height, tile=16, tile_count, zlib_len),
// zlib payload of per-tile [u8 mode][mode==1: u32 prior_idx][tw*th*3 bytes].

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};

pub const HEADER_SIZE: usize = 24;
pub const TILE_SIZE: u32 = 16;
pub const ATLAS_CAPACITY: usize = 128;
pub const RANK_CANDIDATES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dims {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Literal,
    Reference(u32),
}

impl Mode {
    pub fn to_byte(self) -> u8 {
        match self {
            Mode::Literal => 0,
            Mode::Reference(_) => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub data: Vec<u8>, // tw*th*3, row-major RGB
    pub width: u32,
    pub height: u32,
}

impl Tile {
    pub fn nbytes(&self) -> usize {
        self.width as usize * self.height as usize * 3
    }
}

#[inline]
pub fn div_ceil(a: u32, b: u32) -> u32 {
    a / b + u32::from(a % b != 0)
}

pub fn tile_grid(dims: Dims) -> (u32, u32) {
    (div_ceil(dims.width, TILE_SIZE), div_ceil(dims.height, TILE_SIZE))
}

pub fn tile_dims(dims: Dims, t: u32) -> (u32, u32) {
    let (cols, rows) = tile_grid(dims);
    let tx = t % cols;
    let ty = t / cols;
    debug_assert!(tx < cols && ty < rows, "tile index out of grid");
    let tw = TILE_SIZE.min(dims.width - tx * TILE_SIZE);
    let th = TILE_SIZE.min(dims.height - ty * TILE_SIZE);
    (tw, th)
}

// ponytail: all patches retained (index space must match decoder); README's
// "last 128" applies to the ranking window, not storage. If a 20k+-tile photo
// makes ranking hot, add per-shape buckets + edge signatures (v0.6 backlog).
#[derive(Default)]
pub struct Atlas {
    patches: Vec<(u32, u32, Vec<u8>)>, // (w, h, rgb) of encoded-so-far tiles, in scan order
}

impl Atlas {
    pub fn new() -> Self {
        Atlas::default()
    }

    pub fn insert(&mut self, tile: &Tile) {
        self.patches
            .push((tile.width, tile.height, tile.data.clone()));
    }

    /// Rank candidate patch indices by squared RGB-mean distance to `tile`.
    /// Only identically shaped patches within the last `ATLAS_CAPACITY`
    /// (README: "last 128") are eligible.
    /// Returns at most `top` indices, best first — encoder re-scores each
    /// candidate by actual proxy cost anyway.
    pub fn rank(&self, tile: &Tile, top: usize) -> Vec<u32> {
        let want = (tile.width, tile.height);
        let mean = rgb_mean(&tile.data);
        let window_start = self.patches.len().saturating_sub(ATLAS_CAPACITY);
        let mut cands: Vec<(u64, u16)> = self
            .patches
            .iter()
            .enumerate()
            .skip(window_start)
            .filter(|(_, (w, h, _))| (*w, *h) == want)
            .map(|(i, (_, _, rgb))| {
                let m = rgb_mean(rgb);
                let d = (m.0 - mean.0).pow(2) as u64
                    + (m.1 - mean.1).pow(2) as u64
                    + (m.2 - mean.2).pow(2) as u64;
                (d, u16::try_from(i).unwrap_or(u16::MAX))
            })
            .collect();
        cands.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        cands.truncate(top);
        cands.into_iter().map(|(_, i)| i as u32).collect()
    }
}

#[inline]
fn rgb_mean(rgb: &[u8]) -> (i64, i64, i64) {
    let n = (rgb.len() / 3) as i64;
    let mut s = (0i64, 0i64, 0i64);
    for px in rgb.chunks_exact(3) {
        s.0 += px[0] as i64;
        s.1 += px[1] as i64;
        s.2 += px[2] as i64;
    }
    (s.0 / n, s.1 / n, s.2 / n)
}

/// Cost proxy for one tile's contribution to the compressed stream.
/// ponytail: per-tile zlib call is O(128) small compressions per tile — fine at
/// benchmark scale; if profiling shows it hot, cache or use a rolling window.
pub fn tile_proxy_cost(bytes: &[u8]) -> u64 {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::fast());
    e.write_all(bytes).ok();
    e.finish().map(|v| v.len() as u64).unwrap_or(u64::MAX)
}

/// Encode a tile stream (row-major, tile_dims shapes) with optional atlas matching.
/// Returns (raw_tile_bytes, payload). Raw bytes: per tile [mode][idx?][data].
pub fn encode_tiles(tiles: &[Tile], use_atlas: bool) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(tiles.len() * (TILE_SIZE as usize * TILE_SIZE as usize * 3 + 5));
    let mut atlas = Atlas::new();

    for tile in tiles {
        let n = tile.nbytes();
        let mut best_mode = Mode::Literal;
        let mut best_bytes = tile.data.clone();
        let mut best_cost = tile_proxy_cost(&[0u8]) + tile_proxy_cost(&tile.data);

        if use_atlas {
            for prior in atlas.rank(tile, RANK_CANDIDATES) {
                let (_, _, ref_rgb) = &atlas.patches[prior as usize];
                let mut residual = vec![0u8; n];
                for i in 0..n {
                    residual[i] = tile.data[i].wrapping_sub(ref_rgb[i]);
                }
                // metadata: mode byte + u32 index
                let meta_cost = 1 + 4;
                let cost = meta_cost as u64 + tile_proxy_cost(&residual);
                if cost < best_cost {
                    best_cost = cost;
                    best_mode = Mode::Reference(prior);
                    best_bytes = residual;
                }
            }
        }

        out.push(best_mode.to_byte());
        if let Mode::Reference(prior) = best_mode {
            out.extend_from_slice(&prior.to_le_bytes());
        }
        out.extend_from_slice(&best_bytes);
        atlas.insert(tile);
    }
    out
}

pub fn compress_payload(raw: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(raw).expect("zlib encode");
    e.finish().expect("zlib finish")
}

pub fn build_header(dims: Dims, tile_count: u32, payload_len: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(HEADER_SIZE);
    h.extend_from_slice(b"MNF5");
    h.extend_from_slice(&dims.width.to_le_bytes());
    h.extend_from_slice(&dims.height.to_le_bytes());
    h.extend_from_slice(&TILE_SIZE.to_le_bytes());
    h.extend_from_slice(&tile_count.to_le_bytes());
    h.extend_from_slice(&payload_len.to_le_bytes());
    debug_assert_eq!(h.len(), HEADER_SIZE);
    h
}

#[derive(Debug)]
pub struct DecodeResult {
    pub dims: Dims,
    pub pixels: Vec<u8>,
}

/// Decode an MNF5 file (header + zlib payload) back to RGB8 pixels.
pub fn decode(buf: &[u8]) -> Result<DecodeResult, String> {
    if buf.len() < HEADER_SIZE {
        return Err(format!("truncated header: {} < {HEADER_SIZE} bytes", buf.len()));
    }
    let (w, h) = read_dims(&buf[4..12])?;
    let ts = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    let tile_count = u32::from_le_bytes(buf[16..20].try_into().unwrap());
    if ts != TILE_SIZE {
        return Err(format!("unsupported tile size {ts}"));
    }
    let payload_len = u32::from_le_bytes(buf[20..24].try_into().unwrap()) as usize;
    let (cols, rows) = tile_grid(Dims { width: w, height: h });
    let expected = cols * rows;
    if tile_count != expected {
        return Err(format!("tile_count {tile_count} != expected {expected} for {w}x{h}"));
    }
    let _rows = rows;
    if (buf.len() - HEADER_SIZE) < payload_len {
        return Err("payload shorter than header-declared len".into());
    }
    if payload_len > MAX_PAYLOAD {
        return Err("payload too large".into());
    }
    let payload = &buf[HEADER_SIZE..HEADER_SIZE + payload_len];

    let mut dz = ZlibDecoder::new(payload);
    let mut raw = Vec::new();
    dz.read_to_end(&mut raw)
        .map_err(|e| format!("zlib: {e}"))?;
    // Reject bytes beyond the declared payload: a well-formed file is header +
    // exactly payload_len bytes. (Trailing bytes get ignored otherwise, which
    // hides corruption in downstream tooling.)
    if buf.len() != HEADER_SIZE + payload_len {
        return Err(format!(
            "file is {} bytes but header declares {} + {payload_len} payload",
            buf.len(), HEADER_SIZE
        ));
    }

    let total_px = w as usize * h as usize * 3;
    let mut out = vec![0u8; total_px];
    let mut patches: Vec<(u32, u32, Vec<u8>)> = Vec::with_capacity(tile_count as usize);
    let mut cursor = 0usize;

    for t in 0..tile_count {
        let (tw, th) = tile_dims(Dims { width: w, height: h }, t);
        let n = (tw * th * 3) as usize;
        if cursor + 1 > raw.len() {
            return Err(format!("tile {t}: truncated mode byte"));
        }
        let mode = raw[cursor];
        cursor += 1;
        let data: Vec<u8> = match mode {
            0 => {
                if cursor + n > raw.len() {
                    return Err(format!("tile {t}: truncated literal payload"));
                }
                let d = raw[cursor..cursor + n].to_vec();
                cursor += n;
                d
            }
            1 => {
                if cursor + 4 > raw.len() {
                    return Err(format!("tile {t}: truncated reference index"));
                }
                let idx = u32::from_le_bytes(raw[cursor..cursor + 4].try_into().unwrap());
                cursor += 4;
                let (rw, rh, ref_rgb) = patches
                    .get(idx as usize)
                    .ok_or_else(|| format!("tile {t}: reference to tile {idx} >= {len} decoded",
                        len = patches.len()))?;
                if *rw != tw || *rh != th {
                    return Err(format!("tile {t}: patch {idx} shaped {rw}x{rh}, need {tw}x{th}"));
                }
                if cursor + n > raw.len() {
                    return Err(format!("tile {t}: truncated residual"));
                }
                let residual = &raw[cursor..cursor + n];
                let mut d = vec![0u8; n];
                for i in 0..n {
                    d[i] = ref_rgb[i].wrapping_add(residual[i]);
                }
                cursor += n;
                d
            }
            m => return Err(format!("tile {t}: invalid mode byte {m}")),
        };
        blit_tile(&mut out, Dims { width: w, height: h }, t, &data, tw, th);
        patches.push((tw, th, data));
    }
    if cursor != raw.len() {
        return Err(format!("trailing bytes after last tile: {cursor} != {}", raw.len()));
    }
    Ok(DecodeResult {
        dims: Dims { width: w, height: h },
        pixels: out,
    })
}

const MAX_PAYLOAD: usize = 1 << 31; // 2 GiB hard stop for untrusted input

fn read_dims(b: &[u8]) -> Result<(u32, u32), String> {
    let w = u32::from_le_bytes(b[0..4].try_into().unwrap());
    let h = u32::from_le_bytes(b[4..8].try_into().unwrap());
    if w == 0 || h == 0 {
        return Err("zero dimensions".into());
    }
    // guard oversized allocations: total pixels capped at ~2^26 (2^24 px, 8 MiB RGB)
    if (w as u64) * (h as u64) > (1 << 26) {
        return Err("image too large".into());
    }
    Ok((w, h))
}

/// Split an RGB8 pixel buffer into row-major Tiles (public: CLI uses the same
/// tiling as encoder/decoder/tests — one definition of truth).
pub fn split_tiles(pixels: &[u8], d: Dims) -> Vec<Tile> {
    let (cols, rows) = tile_grid(d);
    (0..cols * rows)
        .map(|t| {
            let (tw, th) = tile_dims(d, t);
            let mut data = vec![0u8; (tw * th * 3) as usize];
            let tx = (t % cols) * TILE_SIZE;
            let ty = (t / cols) * TILE_SIZE;
            for row in 0..th as usize {
                for col in 0..tw as usize {
                    let py = ty as usize + row;
                    let px = tx as usize + col;
                    let dst = (row * tw as usize + col) * 3;
                    let src = (py * d.width as usize + px) * 3;
                    data[dst..dst + 3].copy_from_slice(&pixels[src..src + 3]);
                }
            }
            Tile { data, width: tw, height: th }
        })
        .collect()
}

fn blit_tile(out: &mut [u8], dims: Dims, t: u32, data: &[u8], tw: u32, th: u32) {
    let (cols, _) = tile_grid(dims);
    let tx = (t % cols) * TILE_SIZE;
    let ty = (t / cols) * TILE_SIZE;
    for row in 0..th as usize {
        let py = ty as usize + row;
        for col in 0..tw as usize {
            let px = tx as usize + col;
            let dst = (py * dims.width as usize + px) * 3;
            let src = (row * tw as usize + col) * 3;
            out[dst..dst + 3].copy_from_slice(&data[src..src + 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims(w: u32, h: u32) -> Dims {
        Dims { width: w, height: h }
    }

    fn split_tiles(pixels: &[u8], d: Dims) -> Vec<Tile> {
        let (cols, rows) = tile_grid(d);
        (0..cols * rows)
            .map(|t| {
                let (tw, th) = tile_dims(d, t);
                let mut data = vec![0u8; (tw * th * 3) as usize];
                let tx = (t % cols) * TILE_SIZE;
                let ty = (t / cols) * TILE_SIZE;
                for row in 0..th as usize {
                    for col in 0..tw as usize {
                        let py = ty as usize + row;
                        let px = tx as usize + col;
                        let dst = (row * tw as usize + col) * 3;
                        let src = (py * d.width as usize + px) * 3;
                        data[dst..dst + 3].copy_from_slice(&pixels[src..src + 3]);
                    }
                }
                Tile { data, width: tw, height: th }
            })
            .collect()
    }

    fn roundtrip(d: Dims, pixels: &[u8], use_atlas: bool) -> Vec<u8> {
        let tiles = split_tiles(pixels, d);
        let raw = encode_tiles(&tiles, use_atlas);
        let payload = compress_payload(&raw);
        let header = build_header(d, tiles.len() as u32, payload.len() as u32);
        let mut buf = header;
        buf.extend_from_slice(&payload);
        let res = decode(&buf).expect("decode ok");
        assert_eq!(res.dims, d);
        res.pixels
    }

    fn solid(w: u32, h: u32, c: [u8; 3]) -> Vec<u8> {
        (0..w * h).flat_map(|_| c).collect()
    }

    fn gradient(w: u32, h: u32) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| {
                let i = i as u32;
                [(i % 256) as u8, ((i / w) % 256) as u8, ((i / 7) % 256) as u8]
            })
            .collect()
    }

    #[test]
    fn grid_math() {
        assert_eq!(tile_grid(dims(16, 16)), (1, 1));
        assert_eq!(tile_grid(dims(17, 16)), (2, 1));
        assert_eq!(tile_grid(dims(31, 33)), (2, 3));
        assert_eq!(tile_dims(dims(31, 33), 1), (15, 16));
        assert_eq!(tile_dims(dims(31, 33), 2), (16, 16));
        assert_eq!(tile_dims(dims(31, 33), 5), (15, 1));
        assert_eq!(div_ceil(0, 16), 0); // 0-sized grid ok for tests only
    }

    #[test]
    fn roundtrip_solid_16x16_atlas() {
        let px = solid(16, 16, [10, 20, 30]);
        assert_eq!(roundtrip(dims(16, 16), &px, true), px);
    }

    #[test]
    fn roundtrip_solid_16x16_no_atlas() {
        let px = solid(16, 16, [200, 100, 50]);
        assert_eq!(roundtrip(dims(16, 16), &px, false), px);
    }

    #[test]
    fn roundtrip_odd_dims() {
        for (w, h) in [(31u32, 33u32), (17u32, 1u32), (1u32, 17u32), (1u32, 1u32), (48u32, 47u32)] {
            let px = gradient(w, h);
            let dec = roundtrip(dims(w, h), &px, true);
            assert_eq!(dec.len(), (w * h * 3) as usize, "len {w}x{h}");
            assert_eq!(dec, px, "pixels {w}x{h}");
        }
    }

    #[test]
    fn roundtrip_random_pixels() {
        // deterministic pseudo-random
        let mut s: u32 = 12345;
        let mut rnd = || {
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            (s >> 16) as u8
        };
        let w = 64;
        let h = 48;
        let px: Vec<u8> = (0..w * h * 3).map(|_| rnd()).collect();
        assert_eq!(roundtrip(dims(w, h), &px, true), px);
    }

    #[test]
    fn roundtrip_uniform_atlas_matches_noatlas() {
        let px = solid(64, 64, [7, 7, 7]);
        assert_eq!(roundtrip(dims(64, 64), &px, true), px);
        assert_eq!(roundtrip(dims(64, 64), &px, false), px);
    }

    #[test]
    fn decode_rejects_bad_inputs() {
        assert!(decode(&[]).is_err(), "empty");
        assert!(decode(&[0u8; 23]).is_err(), "short header");
        let mut bad_magic = build_header(dims(16, 16), 1, 0);
        bad_magic[0] = b'X';
        assert!(decode(&bad_magic).is_err(), "bad magic");
        let bad_tile_size = {
            let mut h = build_header(dims(16, 16), 1, 0);
            h[12] = 15; // tile_size != 16
            h
        };
        assert!(decode(&bad_tile_size).is_err(), "tile size");
        let bad_count = build_header(dims(16, 16), 5, 0);
        assert!(decode(&bad_count).is_err(), "tile count");
        assert!(decode(&build_header(dims(0, 16), 1, 0)).is_err(), "zero dim");
        assert!(read_dims(&build_header(dims(0, 16), 1, 0)[4..12]).is_err(), "read_dims rejects zero width");
        // truncated payload: valid header, valid zlib, missing trailing tile data
        let d = dims(32, 16);
        let tiles = split_tiles(&solid(32, 16, [1, 2, 3]), d);
        let mut raw = encode_tiles(&tiles, true);
        raw.truncate(raw.len() - 5); // chop half of second tile's data
        let truncated = {
            let payload = compress_payload(&raw);
            let mut b = build_header(d, 2, payload.len() as u32);
            b.extend_from_slice(&payload);
            b
        };
        assert!(decode(&truncated).is_err(), "truncated tile data");
        // dangling reference: first tile points at tile 3 which doesn't exist
        let mut raw_ref = vec![1u8];
        raw_ref.extend_from_slice(&3u32.to_le_bytes());
        raw_ref.extend_from_slice(&[0u8; 16 * 16 * 3]);
        let dang = {
            let payload = compress_payload(&raw_ref);
            let mut b = build_header(dims(16, 16), 1, payload.len() as u32);
            b.extend_from_slice(&payload);
            b
        };
        assert!(decode(&dang).is_err(), "dangling reference");
    }

    #[test]
    fn decoder_rejects_oversized_dims() {
        let h = build_header(Dims { width: 1 << 24, height: 1 << 24 }, 0, 0);
        assert!(decode(&h).is_err(), "oversized dims");
    }

    #[test]
    fn atlas_reference_shape_mismatch_rejected() {
        // tile 0 is 16x16, tile 1 is 16x8; mode 1 reference from tile 1 to tile 0
        let d = Dims { width: 16, height: 24 };
        let mut raw = Vec::new();
        raw.push(0u8); // tile 0 literal
        raw.extend_from_slice(&[0u8; 16 * 16 * 3]);
        raw.push(1u8); // tile 1 reference
        raw.extend_from_slice(&0u32.to_le_bytes());
        raw.extend_from_slice(&[0u8; 16 * 8 * 3]);
        let payload = compress_payload(&raw);
        let mut b = build_header(d, 2, payload.len() as u32);
        b.extend_from_slice(&payload);
        let err = decode(&b).unwrap_err();
        assert!(err.contains("shaped"), "want shape mismatch, got: {err}");
    }

    #[test]
    fn encoder_picks_reference_when_residual_compresses() {
        // two identical solid tiles: tile 1 should reference tile 0
        let d = Dims { width: 32, height: 16 };
        let tiles = split_tiles(&solid(32, 16, [9, 9, 9]), d);
        let raw_tiles = encode_tiles(&tiles, true);
        let t1 = 1 + 16 * 16 * 3; // second tile's mode byte comes after tile 0's literal data
        assert_eq!(raw_tiles[0], 0, "tile 0 literal");
        assert_eq!(raw_tiles[t1], 1, "tile 1 should be a reference");
        let idx = u32::from_le_bytes(raw_tiles[t1 + 1..t1 + 5].try_into().unwrap());
        assert_eq!(idx, 0);
        // then 768 bytes of all-zero residual
        assert!(raw_tiles[t1 + 5..].iter().all(|&b| b == 0));
    }

    #[test]
    fn no_atlas_uses_literals() {
        let d = Dims { width: 32, height: 16 };
        let tiles = split_tiles(&solid(32, 16, [9, 9, 9]), d);
        let raw = encode_tiles(&tiles, false);
        assert_eq!(raw[0], 0);
        assert_eq!(raw[1 + 16 * 16 * 3], 0, "tile 1 literal when atlas disabled");
    }

    #[test]
    fn proxy_cost_monotone() {
        assert!(tile_proxy_cost(&[0u8; 16]) <= tile_proxy_cost(&[5u8; 32]));
    }
}