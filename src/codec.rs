// MNF5 codec: encoder + decoder + shared header.
// Format: 24-byte header ("MNF5", u32 LE width, height, tile=16, tile_count, zlib_len),
// zlib payload of per-tile [u8 mode][mode 1: u32 idx | mode 2: u32 idx + u8 transform][tw*th*3 bytes].

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};

pub const HEADER_SIZE: usize = 24;
pub const TILE_SIZE: u32 = 16;
pub const ATLAS_CAPACITY: usize = 128;
pub const RANK_CANDIDATES: usize = 8;
pub const NUM_TRANSFORMS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dims {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Literal,
    Reference(u32),
    GeomReference(u32, u8), // prior tile index, D8 transform id 0..8
}

impl Mode {
    pub fn to_byte(self) -> u8 {
        match self {
            Mode::Literal => 0,
            Mode::Reference(_) => 1,
            Mode::GeomReference(_, _) => 2,
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

// MSRV (review fix 5): integer `is_multiple_of` is 1.87+, this crate
// pins 1.73+ (README's div_ceil requirement) — modulo form is deliberate.
#[inline]
#[allow(clippy::manual_is_multiple_of)]
pub fn div_ceil(a: u32, b: u32) -> u32 {
    a / b + u32::from(a % b != 0)
}

pub fn tile_grid(dims: Dims) -> (u32, u32) {
    (
        div_ceil(dims.width, TILE_SIZE),
        div_ceil(dims.height, TILE_SIZE),
    )
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
    pub(crate) patches: Vec<(u32, u32, Vec<u8>)>, // (w, h, rgb) of encoded-so-far tiles, in scan order
    sigs: Vec<Sig>,                               // parallel: structural signature per patch
}

impl Atlas {
    pub fn new() -> Self {
        Atlas::default()
    }

    pub fn insert(&mut self, tile: &Tile) {
        let sig = tile_sig(tile.width, tile.height, &tile.data);
        self.insert_with_sig(tile, sig);
    }

    /// Insert with a precomputed signature (review fix 5: callers that
    /// already computed sig for ranking should not pay for it twice).
    pub fn insert_with_sig(&mut self, tile: &Tile, sig: Sig) {
        self.sigs.push(sig);
        self.patches
            .push((tile.width, tile.height, tile.data.clone()));
    }

    /// Rank candidate patch indices by structural signature distance (v0.6
    /// P1: gradient/variance/luma, not mean-RGB-only). Only identically shaped
    /// patches within the last `ATLAS_CAPACITY` (README: "last 128") are
    /// eligible. Returns at most `top` indices, best first — encoder
    /// re-scores each candidate by actual proxy cost anyway.
    pub fn rank_by_sig(&self, sig: &Sig, w: u32, h: u32, top: usize) -> Vec<u32> {
        let want = (w, h);
        let window_start = self.patches.len().saturating_sub(ATLAS_CAPACITY);
        let mut cands: Vec<(u64, u32)> = self
            .sigs
            .iter()
            .enumerate()
            .skip(window_start)
            .filter(|(i, _)| {
                let (pw, ph, _) = &self.patches[*i];
                (*pw, *ph) == want
            })
            .map(|(i, s)| (sig_dist(s, sig), u32::try_from(i).unwrap_or(u32::MAX)))
            .collect();
        cands.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        cands.truncate(top);
        cands.into_iter().map(|(_, i)| i).collect()
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

// ponytail: mean+gradient+variance signature, D8-permutation-aware via the
// min(dx/dy swap) term. Full edge histograms would rank better if recall
// ever becomes the bottleneck (v0.7 backlog).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sig {
    mean: (i64, i64, i64),
    gdx: i64,
    gdy: i64,
    luma: i64,
    var: i64,
}

fn luma(px: &[u8]) -> i64 {
    (299 * px[0] as i64 + 587 * px[1] as i64 + 114 * px[2] as i64) / 1000
}

pub fn tile_sig(w: u32, h: u32, data: &[u8]) -> Sig {
    let n = (w * h) as i64;
    let l = data.chunks_exact(3).map(luma).collect::<Vec<i64>>();
    let (wu, hu) = (w as usize, h as usize);
    let mut gdx = 0i64;
    let mut gdy = 0i64;
    for y in 0..hu {
        for x in 0..wu {
            if x + 1 < wu {
                gdx += (l[y * wu + x] - l[y * wu + x + 1]).abs();
            }
            if y + 1 < hu {
                gdy += (l[y * wu + x] - l[(y + 1) * wu + x]).abs();
            }
        }
    }
    let mean_l = l.iter().sum::<i64>() / n;
    let var = l.iter().map(|v| (v - mean_l) * (v - mean_l)).sum::<i64>() / n;
    Sig {
        mean: rgb_mean(data),
        gdx: gdx / n,
        gdy: gdy / n,
        luma: mean_l,
        var,
    }
}

fn sig_dist(a: &Sig, b: &Sig) -> u64 {
    let rgb =
        (a.mean.0 - b.mean.0).pow(2) + (a.mean.1 - b.mean.1).pow(2) + (a.mean.2 - b.mean.2).pow(2);
    let grad = (((a.gdx - b.gdx).abs() + (a.gdy - b.gdy).abs()) as u64)
        .min(((a.gdy - b.gdx).abs() + (a.gdx - b.gdy).abs()) as u64);
    let lum = (a.luma - b.luma).pow(2) as u64 + (a.var - b.var).pow(2) as u64;
    rgb as u64 + 4 * grad + lum
}

// --- D8 reversible transforms (v0.6) -----------------------------------------
// Pull maps: output pixel (x, y) samples src(sx, sy). Encode and decode both
// call apply_transform, so any consistent map roundtrips losslessly.

pub fn transform_dims(t: u8, w: u32, h: u32) -> (u32, u32) {
    match t {
        4..=7 => (h, w),
        _ => (w, h),
    }
}

/// Which transforms keep the tile shape (a reference must reproduce tw x th):
/// all 8 on square patches, only the identity/hflip/vflip/rot180 subgroup on
/// non-square partial tiles.
pub fn transform_is_valid_for(t: u8, w: u32, h: u32) -> bool {
    if t >= NUM_TRANSFORMS {
        return false;
    }
    w == h || t <= 3
}

fn transform_source(t: u8, w: u32, h: u32, x: u32, y: u32) -> (u32, u32) {
    // callers guarantee t <= 4 and x/y within transform_dims output grid
    match t {
        0 => (x, y),
        1 => (w - 1 - x, y),
        2 => (x, h - 1 - y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (y, h - 1 - x),
        _ => unreachable!("composite transforms are composed in apply_transform"),
    }
}

pub fn apply_transform(t: u8, w: u32, h: u32, data: &[u8]) -> Vec<u8> {
    assert!(t < NUM_TRANSFORMS, "invalid transform {t}");
    if t >= 5 {
        let (w2, h2) = transform_dims(4, w, h);
        let base = apply_transform(4, w, h, data);
        return match t {
            5 => apply_transform(1, w2, h2, &base),
            6 => apply_transform(3, w2, h2, &base),
            _ => apply_transform(1, w2, h2, &apply_transform(3, w2, h2, &base)),
        };
    }
    let (ow, oh) = transform_dims(t, w, h);
    let mut out = vec![0u8; (ow as usize) * (oh as usize) * 3];
    for y in 0..oh {
        for x in 0..ow {
            let (sx, sy) = transform_source(t, w, h, x, y);
            let so = ((sy * w + sx) as usize) * 3;
            let d = ((y * ow + x) as usize) * 3;
            out[d..d + 3].copy_from_slice(&data[so..so + 3]);
        }
    }
    out
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
/// Raw bytes per tile: [mode][idx?][transform?][data]; mode 2 (geometric atlas,
/// v0.6) writes both index (u32 LE) and transform id (u8) after the mode byte.
/// `use_geom` extends matching to the D8 group; with `use_atlas` only, exact
/// prior-patch references (v0.5 behavior) are used.
pub fn encode_tiles(tiles: &[Tile], use_atlas: bool, use_geom: bool) -> Vec<u8> {
    encode_stats(tiles, use_atlas, use_geom).0
}

/// Encode + utilization counters (v0.6 exit metric: reference utilization).
pub fn encode_stats(tiles: &[Tile], use_atlas: bool, use_geom: bool) -> (Vec<u8>, GeomStats) {
    let mut out: Vec<u8> =
        Vec::with_capacity(tiles.len() * (TILE_SIZE as usize * TILE_SIZE as usize * 3 + 6));
    let mut atlas = Atlas::new();
    let mut stats = GeomStats::default();

    for tile in tiles {
        let n = tile.nbytes();
        let sig = tile_sig(tile.width, tile.height, &tile.data);
        let mut best_mode = Mode::Literal;
        let mut best_bytes = tile.data.clone();
        // literal pays only its own mode byte; reference metadata cost is
        // counted per-candidate below (review: don't double-charge literals)
        let lit_cost = 1 + tile_proxy_cost(&tile.data);
        let mut best_cost = lit_cost;

        if use_atlas {
            // v0.6 P1: rank by full structural signature instead of mean-RGB-only.
            let cands = atlas.rank_by_sig(&sig, tile.width, tile.height, RANK_CANDIDATES);
            // square tiles: all 8 D8 transforms; partial tiles: shape-keeping subgroup.
            // use_geom=false (atlas ablation) pins t=0 — the v0.5 exact-reference mode.
            let transforms: &[u8] = if !use_geom {
                &[0]
            } else if tile.width == tile.height {
                &[0, 1, 2, 3, 4, 5, 6, 7]
            } else {
                &[0, 1, 2, 3]
            };
            for prior in cands {
                let shape = (tile.width, tile.height);
                for &t in transforms {
                    let (_, _, ref_rgb) = &atlas.patches[prior as usize];
                    let transformed = if t == 0 {
                        ref_rgb.clone()
                    } else {
                        apply_transform(t, shape.0, shape.1, ref_rgb)
                    };
                    let mut residual = vec![0u8; n];
                    for i in 0..n {
                        residual[i] = tile.data[i].wrapping_sub(transformed[i]);
                    }
                    // metadata: mode byte + u32 index + (u8 transform when t != 0)
                    let meta_cost = 5 + u64::from(t != 0);
                    let cost = meta_cost + tile_proxy_cost(&residual);
                    if cost < best_cost {
                        best_cost = cost;
                        best_mode = Mode::GeomReference(prior, t);
                        best_bytes = residual;
                    }
                }
            }
            // exact mode-1 is a special case of mode 2 with t=0, so the encoder
            // emits only mode 2 (or mode 0) when matching wins.
        }

        // exact reference (t=0) is cheaper as mode 1: one byte shorter
        let emit = match best_mode {
            Mode::GeomReference(p, 0) => Mode::Reference(p),
            other => other,
        };
        match emit {
            Mode::Literal => stats.literal += 1,
            Mode::Reference(_) => stats.ref_exact += 1,
            Mode::GeomReference(_, t) => {
                stats.ref_geom += 1;
                if t != 0 {
                    stats.geom_nontrivial += 1;
                }
            }
        }
        stats.modes.push(emit.to_byte());
        stats.savings.push(lit_cost as i64 - best_cost as i64);
        stats.refs.push(match emit {
            Mode::Reference(p) => Some((p, 0)),
            Mode::GeomReference(p, t) => Some((p, t)),
            Mode::Literal => None,
        });
        out.push(emit.to_byte());
        match emit {
            Mode::Reference(prior) => out.extend_from_slice(&prior.to_le_bytes()),
            Mode::GeomReference(prior, t) => {
                out.extend_from_slice(&prior.to_le_bytes());
                out.push(t);
            }
            Mode::Literal => {}
        }
        out.extend_from_slice(&best_bytes);
        atlas.insert_with_sig(tile, sig);
    }
    (out, stats)
}

pub fn compress_payload(raw: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(raw).expect("zlib encode");
    e.finish().expect("zlib finish")
}

// v0.6 exit criterion: how much the atlas is actually used.
#[derive(Debug, Clone, Default)]
pub struct GeomStats {
    pub literal: u32,
    pub ref_exact: u32,
    pub ref_geom: u32,
    pub geom_nontrivial: u32,         // mode 2 with transform != 0
    pub modes: Vec<u8>,               // per-tile emit mode (0/1/2), scan order
    pub savings: Vec<i64>,            // per-tile literal-proxy-cost minus chosen-cost
    pub refs: Vec<Option<(u32, u8)>>, // per-tile (source index, transform) for references
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

/// Configurable decoder resource ceilings (review fix 3). Defaults are the
/// format-derived maxima; callers decoding untrusted input can tighten any
/// of them independently.
#[derive(Debug, Clone, Copy)]
pub struct DecodeLimits {
    pub max_pixels: u64,
    pub max_tiles: u32,
    pub max_decompressed: u64,
    pub max_total_memory: u64, // pixels buffer + atlas patches, bytes
}

impl Default for DecodeLimits {
    fn default() -> Self {
        DecodeLimits {
            max_pixels: 1 << 26,
            max_tiles: u32::MAX,
            max_decompressed: u64::MAX, // replaced by checked per-file bound
            max_total_memory: u64::MAX,
        }
    }
}

/// Decode an MNF5 file (header + zlib payload) back to RGB8 pixels.
pub fn decode(buf: &[u8]) -> Result<DecodeResult, String> {
    decode_with_limits(buf, &DecodeLimits::default())
}

pub fn decode_with_limits(buf: &[u8], lim: &DecodeLimits) -> Result<DecodeResult, String> {
    if buf.len() < HEADER_SIZE {
        return Err(format!(
            "truncated header: {} < {HEADER_SIZE} bytes",
            buf.len()
        ));
    }
    if &buf[0..4] != b"MNF5" {
        return Err("bad magic (want MNF5)".into());
    }
    let (w, h) = read_dims(&buf[4..12])?;
    if w as u64 * h as u64 > lim.max_pixels {
        return Err("image exceeds max_pixels limit".into());
    }
    let ts = u32::from_le_bytes(buf[12..16].try_into().unwrap());
    let tile_count = u32::from_le_bytes(buf[16..20].try_into().unwrap());
    if ts != TILE_SIZE {
        return Err(format!("unsupported tile size {ts}"));
    }
    if tile_count > lim.max_tiles {
        return Err("tile count exceeds max_tiles limit".into());
    }
    let payload_len = u32::from_le_bytes(buf[20..24].try_into().unwrap()) as usize;
    let (cols, rows) = tile_grid(Dims {
        width: w,
        height: h,
    });
    let expected = cols * rows;
    if tile_count != expected {
        return Err(format!(
            "tile_count {tile_count} != expected {expected} for {w}x{h}"
        ));
    }
    let _rows = rows;
    if payload_len > MAX_PAYLOAD {
        return Err("payload too large".into());
    }
    if (buf.len() - HEADER_SIZE) < payload_len {
        return Err("payload shorter than header-declared len".into());
    }
    // Reject bytes beyond the declared payload before decompressing: a
    // well-formed file is header + exactly payload_len bytes.
    if buf.len() != HEADER_SIZE + payload_len {
        return Err(format!(
            "file is {} bytes but header declares {} + {payload_len} payload",
            buf.len(),
            HEADER_SIZE
        ));
    }
    let payload = &buf[HEADER_SIZE..HEADER_SIZE + payload_len];

    // Compression bound (checked math, review fix 1): raw stream <=
    // 3WH pixel bytes total + per-tile metadata worst case
    // (1 mode + 4 idx + 1 transform = 6). Reject during decompression
    // via take(limit+1), not after read_to_end.
    let px_bytes_total = (w as u64) * (h as u64) * 3;
    let decomp_limit = px_bytes_total
        .checked_add(6 * tile_count as u64)
        .ok_or("decompressed size overflow")?;
    let mut dz = ZlibDecoder::new(payload).take(decomp_limit + 1);
    let mut raw = Vec::new();
    dz.read_to_end(&mut raw).map_err(|e| format!("zlib: {e}"))?;
    if raw.len() as u64 > decomp_limit {
        return Err("decompressed payload exceeds size limit".into());
    }

    // Premature-allocation gate (review fix 2): the stream must hold at
    // least 4 bytes per tile (mode byte + minimum ref metadata) before
    // any image-sized allocation happens.
    if raw.len() < 4 * tile_count as usize {
        return Err("decompressed stream shorter than per-tile minimum".into());
    }

    let total_px = w as usize * h as usize * 3;
    // decoder memory = pixels buffer + atlas patches (a second full copy
    // once all tiles are decoded). Enforce the ceiling before allocating.
    let mem_needed = (total_px as u64) * 2;
    if mem_needed > lim.max_total_memory {
        return Err(format!(
            "decoder memory {mem_needed} exceeds max_total_memory limit"
        ));
    }
    let mut out = vec![0u8; total_px];
    // no capacity reservation: patches grows as actually decoded
    let mut patches: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let mut cursor = 0usize;

    for t in 0..tile_count {
        let (tw, th) = tile_dims(
            Dims {
                width: w,
                height: h,
            },
            t,
        );
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
                let (rw, rh, ref_rgb) = patches.get(idx as usize).ok_or_else(|| {
                    format!(
                        "tile {t}: reference to tile {idx} >= {len} decoded",
                        len = patches.len()
                    )
                })?;
                if *rw != tw || *rh != th {
                    return Err(format!(
                        "tile {t}: patch {idx} shaped {rw}x{rh}, need {tw}x{th}"
                    ));
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
            2 => {
                if cursor + 5 > raw.len() {
                    return Err(format!("tile {t}: truncated geometric reference"));
                }
                let idx = u32::from_le_bytes(raw[cursor..cursor + 4].try_into().unwrap());
                let tr = raw[cursor + 4];
                cursor += 5;
                if !transform_is_valid_for(tr, tw, th) {
                    return Err(format!(
                        "tile {t}: invalid transform {tr} for {tw}x{th} tile"
                    ));
                }
                let (rw, rh, ref_rgb) = patches.get(idx as usize).ok_or_else(|| {
                    format!(
                        "tile {t}: reference to tile {idx} >= {len} decoded",
                        len = patches.len()
                    )
                })?;
                if *rw != tw || *rh != th {
                    return Err(format!(
                        "tile {t}: patch {idx} shaped {rw}x{rh}, need {tw}x{th}"
                    ));
                }
                if cursor + n > raw.len() {
                    return Err(format!("tile {t}: truncated residual"));
                }
                let residual = &raw[cursor..cursor + n];
                let transformed = if tr == 0 {
                    ref_rgb.clone()
                } else {
                    apply_transform(tr, tw, th, ref_rgb)
                };
                let mut d = vec![0u8; n];
                for i in 0..n {
                    d[i] = transformed[i].wrapping_add(residual[i]);
                }
                cursor += n;
                d
            }
            m => return Err(format!("tile {t}: invalid mode byte {m}")),
        };
        blit_tile(
            &mut out,
            Dims {
                width: w,
                height: h,
            },
            t,
            &data,
            tw,
            th,
        );
        patches.push((tw, th, data));
    }
    if cursor != raw.len() {
        return Err(format!(
            "trailing bytes after last tile: {cursor} != {}",
            raw.len()
        ));
    }
    Ok(DecodeResult {
        dims: Dims {
            width: w,
            height: h,
        },
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
    // guard oversized allocations: total pixels capped at 2^26 (~192 MiB RGB;
    // the atlas keeps a second copy of every patch, so budget ~2x while decoding)
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
            Tile {
                data,
                width: tw,
                height: th,
            }
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
        Dims {
            width: w,
            height: h,
        }
    }

    fn roundtrip(d: Dims, pixels: &[u8], use_atlas: bool) -> Vec<u8> {
        let tiles = split_tiles(pixels, d);
        let raw = encode_tiles(&tiles, use_atlas, true);
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
                [
                    (i % 256) as u8,
                    ((i / w) % 256) as u8,
                    ((i / 7) % 256) as u8,
                ]
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
        for (w, h) in [
            (31u32, 33u32),
            (17u32, 1u32),
            (1u32, 17u32),
            (1u32, 1u32),
            (48u32, 47u32),
        ] {
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
        assert!(
            decode(&build_header(dims(0, 16), 1, 0)).is_err(),
            "zero dim"
        );
        assert!(
            read_dims(&build_header(dims(0, 16), 1, 0)[4..12]).is_err(),
            "read_dims rejects zero width"
        );
        // truncated payload: valid header, valid zlib, missing trailing tile data
        let d = dims(32, 16);
        let tiles = split_tiles(&solid(32, 16, [1, 2, 3]), d);
        let mut raw = encode_tiles(&tiles, true, false);
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
        let h = build_header(
            Dims {
                width: 1 << 24,
                height: 1 << 24,
            },
            0,
            0,
        );
        assert!(decode(&h).is_err(), "oversized dims");
    }

    #[test]
    fn atlas_reference_shape_mismatch_rejected() {
        // tile 0 is 16x16, tile 1 is 16x8; mode 1 reference from tile 1 to tile 0
        let d = Dims {
            width: 16,
            height: 24,
        };
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
        // two identical NOISY tiles: literal costs ~768 bytes, the exact-match
        // residual is all zeros — the atlas must win. (Constant tiles don't
        // exercise this: deflate crushes a constant literal as cheaply as a
        // zero residual, so the honest cost model prefers the literal.)
        let mut s: u32 = 999;
        let mut rnd = || {
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            (s >> 16) as u8
        };
        let noisy: Vec<u8> = (0..32 * 16 * 3).map(|_| rnd()).collect();
        let tiles = vec![
            Tile {
                data: noisy.clone(),
                width: 32,
                height: 16,
            },
            Tile {
                data: noisy.clone(),
                width: 32,
                height: 16,
            },
        ];
        let raw_tiles = encode_tiles(&tiles, true, false);
        let t1 = 1 + 32 * 16 * 3; // second tile's metadata starts after tile 0's data
        assert_eq!(raw_tiles[0], 0, "tile 0 literal");
        assert_eq!(
            raw_tiles[t1], 1,
            "tile 1 should be an exact reference (mode 1)"
        );
        let idx = u32::from_le_bytes(raw_tiles[t1 + 1..t1 + 5].try_into().unwrap());
        assert_eq!(idx, 0, "reference points at tile 0");
        // then 768 bytes of all-zero residual
        assert!(raw_tiles[t1 + 5..].iter().all(|&b| b == 0));
    }

    #[test]
    fn d8_transforms_roundtrip_lossless() {
        // Put T_{t0}(original) into the atlas as tile 0 and original as tile 1.
        // The encoder must find some exact inverse t with T_t(base) == original
        // (t labels depend on composition order; only exactness matters), emit
        // mode 2 with a zero residual, and metadata must roundtrip.
        for (w, h) in [
            (16u32, 16u32),
            (30u32, 30u32),
            (16u32, 24u32),
            (31u32, 33u32),
        ] {
            let mut s: u32 = 777;
            let mut rnd = || {
                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                (s >> 16) as u8
            };
            let px: Vec<u8> = (0..w * h * 3).map(|_| rnd()).collect();
            for t0 in 0..NUM_TRANSFORMS {
                if !transform_is_valid_for(t0, w, h) {
                    continue;
                }
                let base = apply_transform(t0, w, h, &px);
                let base_tile = Tile {
                    data: base.clone(),
                    width: w,
                    height: h,
                };
                let target_tile = Tile {
                    data: px.clone(),
                    width: w,
                    height: h,
                };
                let raw = encode_tiles(&[base_tile, target_tile], true, true);
                let t1 = 1 + (w * h * 3) as usize;
                let mode = raw[t1];
                let emitted_t = match mode {
                    0 => panic!("tile 1 must reference: exact inverse exists (t0={t0}, {w}x{h})"),
                    1 => 0u8,
                    2 => raw[t1 + 5],
                    m => panic!("invalid mode {m}"),
                };
                assert!(
                    transform_is_valid_for(emitted_t, w, h),
                    "emitted transform {emitted_t} invalid for {w}x{h} (t0={t0})"
                );
                // encoder must have found an exact inverse: zero residual
                assert_eq!(
                    apply_transform(emitted_t, w, h, &base),
                    px,
                    "T_{emitted_t}(T_{t0}(px)) != px for {w}x{h}"
                );
            }
        }
    }

    #[test]
    fn geom_reference_transform4_nonsquare_rejected() {
        // mode 2 with transform 4 on a 16x8 partial tile changes shape -> must error
        let d = Dims {
            width: 16,
            height: 24,
        };
        let mut raw = Vec::new();
        raw.push(0u8); // tile 0 literal 16x16
        raw.extend_from_slice(&[0u8; 16 * 16 * 3]);
        raw.push(2u8); // tile 1 (16x8): geometric reference
        raw.extend_from_slice(&0u32.to_le_bytes());
        raw.push(4u8); // invalid: swaps 16x8 -> 8x16
        raw.extend_from_slice(&[0u8; 16 * 8 * 3]);
        let payload = compress_payload(&raw);
        let mut b = build_header(d, 2, payload.len() as u32);
        b.extend_from_slice(&payload);
        let err = decode(&b).unwrap_err();
        assert!(
            err.contains("invalid transform"),
            "want transform rejection, got: {err}"
        );
    }

    #[test]
    fn nonsquare_partial_tiles_use_shape_keeping_subset() {
        // every valid partial-tile transform must roundtrip through the codec
        let d = Dims {
            width: 31,
            height: 33,
        }; // has 15x1 and 15x16 partial tiles
        let mut s: u32 = 4242;
        let mut rnd = || {
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            (s >> 16) as u8
        };
        let px: Vec<u8> = (0..d.width as usize * d.height as usize * 3)
            .map(|_| rnd())
            .collect();
        assert_eq!(roundtrip(d, &px, true), px);
    }

    #[test]
    fn no_atlas_uses_literals() {
        let d = Dims {
            width: 32,
            height: 16,
        };
        let tiles = split_tiles(&solid(32, 16, [9, 9, 9]), d);
        let raw = encode_tiles(&tiles, false, false);
        assert_eq!(raw[0], 0);
        assert_eq!(
            raw[1 + 16 * 16 * 3],
            0,
            "tile 1 literal when atlas disabled"
        );
    }

    #[test]
    fn proxy_cost_monotone() {
        assert!(tile_proxy_cost(&[0u8; 16]) <= tile_proxy_cost(&[5u8; 32]));
    }
}
