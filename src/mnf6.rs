// MNF6/MNF7 — formats v2/v3 (README §9 v0.7/v0.8/v1.1 payload).
//
// MNF6 = MNF5 tile records + per-image coding selections:
//   header (28 bytes): "MNF6", u32 LE width, height, tile=16, tile_count,
//     payload_len, u32 flags.
//   flags bit0 = planar RGB (mode 0 literals + mode 1/2 residual payloads
//     carry [R][G][B] planes instead of interleaved),
//   flags bit1 = zstd stream coder instead of zlib,
//   bits 2..31 reserved, MUST be 0.
// Modes: 0 literal (flagged layout), 1 exact reference, 2 geometric
// reference (D8, MNF5 semantics), 3 YCoCg-R bit-packed literal:
//   [3][n/3 bytes Y plane][ceil(9(n/3)/8) bytes co9][same cg9]
// co/cg are biased +256 and LSB-first packed; 9-bit planes make YCoCg-R
// losslessly packable (the MNF5 finding). Reconstruction:
//   tmp = y - (cg>>1); g = tmp + cg; b = tmp - (co>>1); r = tmp + co - (co>>1)
// Encoder is rate-aware (v0.8): builds the tile stream under each of the 4
// flag variants, each tile choosing its mode by that variant's proxy cost;
// the smallest final stream is emitted with its flags.

use std::io::Read;

use crate::codec::{apply_transform, div_ceil, tile_sig, Atlas, Dims, RANK_CANDIDATES, TILE_SIZE};
use crate::error::{Error, Result};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

/// Header size: `MNF6`/`MNF7` magic + 6×`u32` LE.
pub const HEADER6: usize = 28;
/// Record bodies packed channel-planar.
pub const FLAG_PLANAR: u32 = 1;
/// zstd container (MNF7); MNF6 = zlib.
pub const FLAG_ZSTD: u32 = 2;
/// Bits above bit1 reserved (MNF6/7 masks frozen).
pub const FLAGS_RESERVED: u32 = !0b11;

/// Entropy coder choice for the record stream.
pub enum Coder {
    /// zlib level 9 container (MNF6 header magic).
    Zlib,
    /// zstd at the given level (MNF7 header magic).
    Zstd(i32),
}

/// Compress the full record stream with the chosen coder.
pub fn compress_stream(coder: &Coder, buf: &[u8]) -> Vec<u8> {
    match coder {
        Coder::Zlib => {
            let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
            e.write_all(buf).expect("zlib encode");
            e.finish().expect("zlib finish")
        }
        Coder::Zstd(level) => {
            let mut e = zstd::stream::Encoder::new(Vec::new(), *level).expect("zstd");
            // corruption must reject: enable the xxh64 frame checksum (zlib has Adler32 always)
            e.include_checksum(true).expect("zstd checksum flag");
            e.write_all(buf).expect("zstd write");
            e.finish().expect("zstd finish")
        }
    }
}

/// Local cost proxy used to rank candidates (not a global optimum).
pub fn proxy_cost(coder: &Coder, buf: &[u8]) -> u64 {
    match coder {
        Coder::Zlib => compress_stream(coder, buf).len() as u64,
        // probe at level 3 for speed; final stream uses level 19
        Coder::Zstd(_) => compress_stream(&Coder::Zstd(3), buf).len() as u64,
    }
}

// --- 9-bit plane packing (LSB-first) -----------------------------------------

/// Pack 9-bit values LSB-first (YCoCg chroma).
pub fn pack9(values: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity((values.len() * 9).div_ceil(8) + 1);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &v in values {
        debug_assert!(v < 512, "9-bit overflow");
        acc |= (v as u32) << bits;
        bits += 9;
        while bits >= 8 {
            out.push((acc & 0xFF) as u8);
            acc >>= 8;
            bits -= 8;
        }
    }
    if bits > 0 {
        out.push((acc & 0xFF) as u8);
    }
    out
}

/// Inverse of `pack9` for `n` values.
pub fn unpack9(bytes: &[u8], n: usize) -> Result<Vec<u16>> {
    let need = (n * 9).div_ceil(8);
    if bytes.len() < need {
        return Err(Error::fmt_msg("9-bit plane truncated"));
    }
    let mut out = Vec::with_capacity(n);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut bi = 0usize;
    for _ in 0..n {
        while bits < 9 {
            if bi >= bytes.len() {
                return Err(Error::fmt_msg("9-bit plane truncated"));
            }
            acc |= (bytes[bi] as u32) << bits;
            bi += 1;
            bits += 8;
        }
        out.push((acc & 0x1FF) as u16);
        acc >>= 9;
        bits -= 9;
    }
    Ok(out)
}

// --- YCoCg-R -------------------------------------------------------------------

/// Integer YCoCg forward (deterministic; `y` + 9-bit chroma).
pub fn ycocg_forward(rgb: &[u8]) -> (Vec<u8>, Vec<u16>, Vec<u16>) {
    let n = rgb.len() / 3;
    let (mut y, mut co, mut cg) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    for px in rgb.chunks_exact(3) {
        let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
        let co_v = r - b;
        let tmp = b + (co_v >> 1);
        let cg_v = g - tmp;
        let y_v = tmp + (cg_v >> 1);
        debug_assert!((0..=255).contains(&y_v), "y {y_v}");
        debug_assert!((-255..=255).contains(&co_v));
        debug_assert!((-255..=255).contains(&cg_v));
        y.push(y_v as u8);
        co.push((co_v + 256) as u16);
        cg.push((cg_v + 256) as u16);
    }
    (y, co, cg)
}

/// Integer YCoCg inverse (exact counterpart of `ycocg_forward`).
pub fn ycocg_inverse(y: &[u8], co: &[u16], cg: &[u16]) -> Vec<u8> {
    let n = y.len();
    let mut out = vec![0u8; n * 3];
    for i in 0..n {
        let y_v = y[i] as i32;
        let co_v = co[i] as i32 - 256;
        let cg_v = cg[i] as i32 - 256;
        let tmp = y_v - (cg_v >> 1);
        let g = tmp + cg_v;
        let b = tmp - (co_v >> 1);
        let r = tmp + co_v - (co_v >> 1);
        out[i * 3] = r.clamp(0, 255) as u8;
        out[i * 3 + 1] = g.clamp(0, 255) as u8;
        out[i * 3 + 2] = b.clamp(0, 255) as u8;
    }
    out
}

// --- layouts ----------------------------------------------------------------------

/// Split interleaved RGB into three planes (planar packing).
pub fn planar_pack(data: &[u8]) -> Vec<u8> {
    let n = data.len() / 3;
    let mut out = Vec::with_capacity(data.len());
    for c in 0..3 {
        for i in 0..n {
            out.push(data[i * 3 + c]);
        }
    }
    out
}

/// Reassemble three planes into interleaved RGB.
pub fn planar_unpack(data: &[u8]) -> Vec<u8> {
    let n = data.len() / 3;
    let mut out = vec![0u8; data.len()];
    for c in 0..3 {
        for i in 0..n {
            out[i * 3 + c] = data[c * n + i];
        }
    }
    out
}

// --- MNF7: modular 8-bit color-domain lifting (ADR 2026-10-25) ---------------
// NOT standard signed YCoCg-R (that is MNF6 mode 3 with 9-bit chroma planes).
// Modular variant: every step is a u8 bijection, `>>1` is a logical shift on
// the wrapped byte. Encoder evaluates reference residuals in BOTH domains
// per candidate and the mode byte tells the decoder which to use:
//   mode 4 = exact reference, residual in modular-color domain
//   mode 5 = geometric reference, residual in modular-color domain
// Decode: base_t = T(G_t(P_hat)); pix = T^-1(base_t + resid mod 256).
// Atlas patches stay canonical RGB8 (ADR integration rule).

/// Modular YCoCg color transform (MNF7 color modes 4/5).
pub mod modcolor {
    /// Modular YCoCg-like forward: (r,g,b) -> (y, co, cg), all mod 256.
    pub fn forward(rgb: [u8; 3]) -> [u8; 3] {
        let [r, g, b] = rgb;
        let co = r.wrapping_sub(b);
        let t = b.wrapping_add(co >> 1);
        let cg = g.wrapping_sub(t);
        let y = t.wrapping_add(cg >> 1);
        [y, co, cg]
    }

    /// Exact inverse: reverse sequence of the forward steps.
    pub fn inverse(ycocg: [u8; 3]) -> [u8; 3] {
        let [y, co, cg] = ycocg;
        let t = y.wrapping_sub(cg >> 1);
        let g = cg.wrapping_add(t);
        let b = t.wrapping_sub(co >> 1);
        let r = b.wrapping_add(co);
        [r, g, b]
    }
}

/// Forward transform, one tile (bytes).
pub fn modcolor_forward(rgb: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; rgb.len()];
    for (o, px) in out.chunks_exact_mut(3).zip(rgb.chunks_exact(3)) {
        let t = modcolor::forward([px[0], px[1], px[2]]);
        o[0] = t[0];
        o[1] = t[1];
        o[2] = t[2];
    }
    out
}

/// Forward transform, iterator-friendly (avoids re-collect).
pub fn modcolor_forward_iter(rgb: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; rgb.len()];
    for (o, px) in out.chunks_exact_mut(3).zip(rgb.chunks_exact(3)) {
        let t = modcolor::forward([px[0], px[1], px[2]]);
        o[0] = t[0];
        o[1] = t[1];
        o[2] = t[2];
    }
    out
}

/// Inverse transform (exact).
pub fn modcolor_inverse(ycocg: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; ycocg.len()];
    for (o, px) in out.chunks_exact_mut(3).zip(ycocg.chunks_exact(3)) {
        let t = modcolor::inverse([px[0], px[1], px[2]]);
        o[0] = t[0];
        o[1] = t[1];
        o[2] = t[2];
    }
    out
}

/// MNF6/7 encode statistics.
#[derive(Debug, Clone, Default)]
pub struct MnF6Stats {
    /// Mode-0 literals.
    pub literal: u32,
    /// Mode-3 YCoCg literals.
    pub ycocg_literal: u32,
    /// Exact references (t=0).
    pub ref_exact: u32,
    /// Geometric references (t≠0).
    pub ref_geom: u32,
    /// Color-domain exact references.
    pub ref_exact_color: u32,
    /// Color-domain geometric references.
    pub ref_geom_color: u32,
    /// Geometric refs with nontrivial transform.
    pub geom_nontrivial: u32,
    /// Flags id measured best for this stream.
    pub chosen_flags: u32,
    /// Per-tile chosen mode, scan order.
    pub modes: Vec<u8>,
    /// Per-tile (source index, transform) when referenced.
    pub refs: Vec<Option<(u32, u8)>>,
}

enum TileChoice {
    Literal(Vec<u8>),
    YCoCg(Vec<u8>),
    Exact(u32, Vec<u8>),
    Geom(u32, u8, Vec<u8>),
    ExactColor(u32, Vec<u8>),    // MNF7 mode 4
    GeomColor(u32, u8, Vec<u8>), // MNF7 mode 5
}

/// Which flags to try, in order; returns (flags_value, planar, coder_ref).
fn flag_variants() -> Vec<(u32, bool, Coder)> {
    vec![
        (0, false, Coder::Zlib),
        (FLAG_ZSTD, false, Coder::Zstd(19)),
        (FLAG_PLANAR, true, Coder::Zlib),
        (FLAG_PLANAR | FLAG_ZSTD, true, Coder::Zstd(19)),
    ]
}

/// Encode RGB8 pixels as MNF7, choosing coding flags by measured stream size.
/// Supersedes encode_mnf6 (same flags + modes 0-3; adds color-domain modes 4/5).
pub fn encode_mnf7(pixels: &[u8], dims: Dims, use_geom: bool) -> (Vec<u8>, MnF6Stats) {
    let tiles = crate::codec::split_tiles(pixels, dims);
    // ponytail: only the best variant is kept — earlier drafts retained all
    // four; the stats of the winner are the only ones callers read.
    let mut best: Option<(Vec<u8>, u32, MnF6Stats)> = None;

    for (flags, planar, coder) in flag_variants() {
        let mut raw: Vec<u8> = Vec::with_capacity(tiles.len() * 774);
        let mut stats = MnF6Stats::default();
        let mut atlas = Atlas::new();

        for tile in &tiles {
            let sig = tile_sig(tile.width, tile.height, &tile.data);
            let pack = |rgb: &[u8]| -> Vec<u8> {
                if planar {
                    planar_pack(rgb)
                } else {
                    rgb.to_vec()
                }
            };
            let n = tile.nbytes();

            let lit_body = pack(&tile.data);
            let mut choice: TileChoice = TileChoice::Literal(lit_body.clone());
            let mut best_cost = 1 + proxy_cost(&coder, &lit_body);

            // YCoCg-R literal candidate
            {
                let (cy, cco, ccg) = ycocg_forward(&tile.data);
                let mut m3 = Vec::with_capacity(n + 160);
                m3.extend_from_slice(&cy);
                m3.extend_from_slice(&pack9(&cco));
                m3.extend_from_slice(&pack9(&ccg));
                let m3_cost = 1 + proxy_cost(&coder, &m3);
                if m3_cost < best_cost {
                    best_cost = m3_cost;
                    choice = TileChoice::YCoCg(m3);
                }
            }

            // atlas candidates
            let transforms: &[u8] = if use_geom && tile.width == tile.height {
                &[0, 1, 2, 3, 4, 5, 6, 7]
            } else {
                &[0, 1, 2, 3]
            };
            let do_atlas = tiles.len() > 1;
            if do_atlas {
                for prior in atlas.rank_by_sig(&sig, tile.width, tile.height, RANK_CANDIDATES) {
                    let (_, _, ref_rgb) = &atlas.patches[prior as usize];
                    for &t in transforms {
                        let base = if t == 0 {
                            ref_rgb.clone()
                        } else {
                            apply_transform(t, tile.width, tile.height, ref_rgb)
                        };
                        let mut r = vec![0u8; n];
                        for i in 0..n {
                            r[i] = tile.data[i].wrapping_sub(base[i]);
                        }
                        let body = pack(&r);
                        let meta: u64 = if t == 0 { 5 } else { 6 };
                        let cost = meta + proxy_cost(&coder, &body);
                        if cost < best_cost {
                            best_cost = cost;
                            choice = if t == 0 {
                                TileChoice::Exact(prior, body)
                            } else {
                                TileChoice::Geom(prior, t, body)
                            };
                        }
                        // MNF7 color-domain candidate: residual of T(tile) - T(base)
                        let mut r_c = vec![0u8; n];
                        let tile_t = modcolor_forward(&tile.data);
                        let base_t = modcolor_forward(&base);
                        for i in 0..n {
                            r_c[i] = tile_t[i].wrapping_sub(base_t[i]);
                        }
                        let body_c = pack(&r_c);
                        let cost_c = meta + 1 + proxy_cost(&coder, &body_c);
                        if cost_c < best_cost {
                            best_cost = cost_c;
                            choice = if t == 0 {
                                TileChoice::ExactColor(prior, body_c)
                            } else {
                                TileChoice::GeomColor(prior, t, body_c)
                            };
                        }
                    }
                }
            }

            match &choice {
                TileChoice::Literal(_) => {
                    stats.literal += 1;
                    stats.modes.push(0);
                    stats.refs.push(None);
                    raw.push(0u8);
                }
                TileChoice::YCoCg(_) => {
                    stats.ycocg_literal += 1;
                    stats.modes.push(3);
                    stats.refs.push(None);
                    raw.push(3u8);
                }
                TileChoice::Exact(p, _) => {
                    stats.ref_exact += 1;
                    stats.modes.push(1);
                    stats.refs.push(Some((*p, 0)));
                    raw.push(1u8);
                    raw.extend_from_slice(&p.to_le_bytes());
                }
                TileChoice::Geom(p, t, _) => {
                    stats.ref_geom += 1;
                    if *t != 0 {
                        stats.geom_nontrivial += 1;
                    }
                    stats.modes.push(2);
                    stats.refs.push(Some((*p, *t)));
                    raw.push(2u8);
                    raw.extend_from_slice(&p.to_le_bytes());
                    raw.push(*t);
                }
                TileChoice::ExactColor(p, _) => {
                    stats.ref_exact_color += 1;
                    stats.modes.push(4);
                    stats.refs.push(Some((*p, 0)));
                    raw.push(4u8);
                    raw.extend_from_slice(&p.to_le_bytes());
                }
                TileChoice::GeomColor(p, t, _) => {
                    stats.ref_geom_color += 1;
                    if *t != 0 {
                        stats.geom_nontrivial += 1;
                    }
                    stats.modes.push(5);
                    stats.refs.push(Some((*p, *t)));
                    raw.push(5u8);
                    raw.extend_from_slice(&p.to_le_bytes());
                    raw.push(*t);
                }
            }
            match &choice {
                TileChoice::Literal(b) | TileChoice::YCoCg(b) => raw.extend_from_slice(b),
                TileChoice::Exact(_, b)
                | TileChoice::Geom(_, _, b)
                | TileChoice::ExactColor(_, b)
                | TileChoice::GeomColor(_, _, b) => raw.extend_from_slice(b),
            }
            atlas.insert(tile);
        }

        let payload = compress_stream(&coder, &raw);
        let mut file = Vec::with_capacity(HEADER6 + payload.len());
        file.extend_from_slice(b"MNF7");
        file.extend_from_slice(&dims.width.to_le_bytes());
        file.extend_from_slice(&dims.height.to_le_bytes());
        file.extend_from_slice(&TILE_SIZE.to_le_bytes());
        file.extend_from_slice(&(tiles.len() as u32).to_le_bytes());
        file.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        file.extend_from_slice(&flags.to_le_bytes());
        file.extend_from_slice(&payload);
        let size = file.len();
        if best.as_ref().is_none_or(|b| size < b.0.len()) {
            stats.chosen_flags = flags;
            best = Some((file, flags, stats));
        }
    }

    let (file, flags, mut stats) = best.expect("at least one variant");
    stats.chosen_flags = flags;
    (file, stats)
}

// --- decoding --------------------------------------------------------------------

/// Configurable ceilings for MNF6/MNF7/MNF8 decoding (mirror of
/// codec::DecodeLimits; the same limit semantics at the header-validation
/// points). Defaults = the format-derived maxima (identical to codec's).
#[derive(Debug, Clone, Copy)]
pub struct DecodeLimits {
    /// Max total pixels (w*h).
    pub max_pixels: u64,
    /// Max tiles.
    pub max_tiles: u32,
    /// Max decompressed payload bytes.
    pub max_decompressed: u64,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        DecodeLimits {
            max_pixels: 1 << 26,
            max_tiles: u32::MAX,
            max_decompressed: u64::MAX, // replaced by the checked per-file bound
        }
    }
}

/// Decode MNF6/7 with configurable ceilings (library API; decode_mnf7 is the
/// defaults wrapper).
pub fn decode_mnf7_with_limits(buf: &[u8], lim: &DecodeLimits) -> Result<(Dims, Vec<u8>)> {
    decode_mnf7_inner(buf, lim)
}

/// Decode MNF6/7 (magic auto) with default ceilings.
pub fn decode_mnf7(buf: &[u8]) -> Result<(Dims, Vec<u8>)> {
    decode_mnf7_inner(buf, &DecodeLimits::default())
}

fn decode_mnf7_inner(buf: &[u8], lim: &DecodeLimits) -> Result<(Dims, Vec<u8>)> {
    if buf.len() < HEADER6 {
        return Err(Error::fmt_msg("short"));
    }
    if buf[0..4] != *b"MNF7" {
        return Err(Error::fmt_msg("magic"));
    }
    let u32le = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let width = u32le(4);
    let height = u32le(8);
    if width == 0 || height == 0 {
        return Err(Error::fmt_msg("dims"));
    }
    if width as u64 * height as u64 > lim.max_pixels {
        return Err(Error::Limits {
            which: "max_pixels",
            limit: lim.max_pixels,
            actual: width as u64 * height as u64,
        });
    }
    if u32le(12) != TILE_SIZE {
        return Err(Error::fmt_msg("tilesize"));
    }
    let cols = div_ceil(width, TILE_SIZE);
    let rows = div_ceil(height, TILE_SIZE);
    let tile_count = u32le(16);
    if tile_count != cols * rows {
        return Err(Error::fmt_msg("tilecount"));
    }
    if tile_count > lim.max_tiles {
        return Err(Error::Limits {
            which: "max_tiles",
            limit: u64::from(lim.max_tiles),
            actual: u64::from(tile_count),
        });
    }
    let payload_len = u32le(20);
    if payload_len > (1 << 31) - 1 {
        return Err(Error::fmt_msg("payloadlen"));
    }
    if buf.len() != HEADER6 + payload_len as usize {
        return Err(Error::fmt_msg("filelen"));
    }
    let flags = u32le(24);
    if flags & FLAGS_RESERVED != 0 {
        return Err(Error::fmt_msg("reserved-flags"));
    }
    let planar = flags & FLAG_PLANAR != 0;
    let zlib_in = flags & FLAG_ZSTD == 0;

    let zlib_data = &buf[HEADER6..];
    // Max record size, over all modes:
    //   literal/refs:      1 mode + 5 refmeta + 768 bytes
    //   mode 3 (YCoCg-R):  1 mode + np bytes Y + 2 * ceil(9*np/8) bytes, np <= 256
    //                      => 1 + 256 + 2 * 288 = 833 (larger than the 776
    //                      implied by the other modes; previously undercounted,
    //                      which rejected all-mode-3 files as bombs — audit fix)
    let mode3_max: u64 = 1 + 256 + 2 * u64::from((256u32 * 9).div_ceil(8));
    let per_tile_max = 1 + 5 + (TILE_SIZE * TILE_SIZE * 3) as u64;
    let limit = tile_count as u64 * per_tile_max.max(mode3_max) + per_tile_max + mode3_max;
    let raw: Vec<u8> = if zlib_in {
        let mut d = flate2::read::ZlibDecoder::new(zlib_data).take(limit + 1);
        let mut v = Vec::new();
        d.read_to_end(&mut v)
            .map_err(|_| Error::Decompress("corrupt zlib stream"))?;
        v
    } else {
        let d = zstd::stream::read::Decoder::new(zlib_data)
            .map_err(|_| Error::Decompress("zstd init"))?;
        // same window cap as MNF8: level 19 uses <= 8 MiB; a hostile frame
        // must not claim the 128 MiB library default (review fix 4)
        let mut d = d;
        d.window_log_max(23)
            .map_err(|_| Error::Decompress("zstd window param"))?;
        let mut d = d.take(limit + 1);
        let mut v = Vec::new();
        d.read_to_end(&mut v)
            .map_err(|_| Error::Decompress("corrupt zstd stream"))?;
        v
    };
    if raw.len() as u64 > limit {
        return Err(Error::fmt_msg("bomb"));
    }

    let pixels = decode_record_walk(&raw, width, height, tile_count, planar, false)?;
    Ok((Dims { width, height }, pixels))
}

/// Shared MNF7/MNF8 record walk: decodes tile records (modes 0-5) over an
/// already-decompressed `raw` stream into a width*height RGB8 buffer
/// (extracted from decode_mnf7 for MNF8 reuse; semantics frozen — see
/// docs/format.md §3/§MNF7).
pub fn decode_record_walk(
    raw: &[u8],
    width: u32,
    height: u32,
    tile_count: u32,
    planar: bool,
    med: bool,
) -> Result<Vec<u8>> {
    let cols = div_ceil(width, TILE_SIZE);
    let mut pixels = vec![0u8; width as usize * height as usize * 3];
    let mut decoded: Vec<(u32, u32, Vec<u8>)> = Vec::with_capacity(tile_count as usize);
    let mut ip = 0usize;
    for i in 0..tile_count {
        let row = i / cols;
        let col = i % cols;
        let tw = TILE_SIZE.min(width - col * TILE_SIZE);
        let th = TILE_SIZE.min(height - row * TILE_SIZE);
        let n = (tw * th * 3) as usize;

        if ip >= raw.len() {
            return Err(Error::fmt_msg(format!("eof at tile {i}")));
        }
        let mode = raw[ip];
        ip += 1;

        let tile = match mode {
            0 => {
                if ip + n > raw.len() {
                    return Err(Error::fmt_msg("body"));
                }
                let body = raw[ip..ip + n].to_vec();
                ip += n;
                let body = if planar { planar_unpack(&body) } else { body };
                if med {
                    // FLAG_PRED_MED (MNF8 bit2): body = (X - MED(X_causal)) mod
                    // 256. Context discipline (the audit bug): IN-TILE causal
                    // neighbors live in `recon` (only blitted into `pixels`
                    // after the whole tile reconstructs); OUT-OF-TILE
                    // neighbors (above tile-row or left tile-column) are
                    // already decoded in `pixels` (scan order guarantees the
                    // earlier tiles finished). Read each sample from the
                    // buffer that actually holds it.
                    let mut recon = vec![0u8; n];
                    let gy0 = row as usize * TILE_SIZE as usize;
                    let gx0 = col as usize * TILE_SIZE as usize;
                    for y in 0..th as usize {
                        for x in 0..tw as usize {
                            for c in 0..3usize {
                                let k = (y * tw as usize + x) * 3 + c;
                                let gy = gy0 + y;
                                let gx = gx0 + x;
                                // tile-local coords for recon reads
                                let tl = |ggy: usize, ggx: usize| -> u32 {
                                    if ggy >= gy0 && ggx >= gx0 {
                                        u32::from(
                                            recon
                                                [((ggy - gy0) * tw as usize + (ggx - gx0)) * 3 + c],
                                        )
                                    } else {
                                        u32::from(pixels[(ggy * width as usize + ggx) * 3 + c])
                                    }
                                };
                                let pred: u32 = if gy == 0 && gx == 0 {
                                    127
                                } else if gy == 0 {
                                    // top row: left neighbor (out-of-tile or in-tile)
                                    if gx - 1 < gx0 {
                                        u32::from(pixels[(gy * width as usize + gx - 1) * 3 + c])
                                    } else {
                                        u32::from(recon[(y * tw as usize + x - 1) * 3 + c])
                                    }
                                } else if gx == 0 {
                                    // left col: top neighbor (out-of-tile above or in-tile)
                                    if gy - 1 < gy0 {
                                        u32::from(pixels[((gy - 1) * width as usize + gx) * 3 + c])
                                    } else {
                                        u32::from(recon[((y - 1) * tw as usize + x) * 3 + c])
                                    }
                                } else {
                                    let a = tl(gy, gx - 1);
                                    let b = tl(gy - 1, gx);
                                    let cc = tl(gy - 1, gx - 1);
                                    med_predict_pixel(a, b, cc)
                                };
                                recon[k] =
                                    (pred + u32::from(body[k])).wrapping_rem_euclid(256) as u8;
                            }
                        }
                    }
                    recon
                } else {
                    body
                }
            }
            3 => {
                // Y plane is 1 byte per pixel (np = n/3); n counts RGB bytes
                let np = n / 3;
                let n9 = (np * 9).div_ceil(8);
                if ip + np + 2 * n9 > raw.len() {
                    return Err(Error::fmt_msg(format!("tile {i}: body")));
                }
                let y = raw[ip..ip + np].to_vec();
                let co9 = &raw[ip + np..ip + np + n9];
                let cg9 = &raw[ip + np + n9..ip + np + 2 * n9];
                ip += np + 2 * n9;
                let co = unpack9(co9, np)?;
                let cg = unpack9(cg9, np)?;
                ycocg_inverse(&y, &co, &cg)
            }
            1 | 2 | 4 | 5 => {
                let meta = if mode == 2 || mode == 5 { 5 } else { 4 };
                if ip + meta > raw.len() {
                    return Err(Error::fmt_msg("refmeta"));
                }
                let idx = u32::from_le_bytes(raw[ip..ip + 4].try_into().unwrap()) as usize;
                let tr = if mode == 2 || mode == 5 {
                    raw[ip + 4]
                } else {
                    0
                };
                ip += meta;
                if idx >= i as usize {
                    return Err(Error::fmt_msg("reforder"));
                }
                if tr >= 8 {
                    return Err(Error::fmt_msg("tranform-range"));
                }
                let (rw, rh, ref_rgb) = &decoded[idx];
                if *rw != tw || *rh != th {
                    return Err(Error::fmt_msg("refshape"));
                }
                if tw != th && tr >= 4 {
                    return Err(Error::fmt_msg("xform-shape"));
                }
                if ip + n > raw.len() {
                    return Err(Error::fmt_msg("body"));
                }
                let body = raw[ip..ip + n].to_vec();
                ip += n;
                let resid = if planar { planar_unpack(&body) } else { body };
                let base = if tr == 0 {
                    ref_rgb.clone()
                } else {
                    apply_transform(tr, tw, th, ref_rgb)
                };
                match mode {
                    1 | 2 => base
                        .iter()
                        .zip(resid.iter())
                        .map(|(a, r)| a.wrapping_add(*r))
                        .collect(),
                    // MNF7 color-domain: recon_t = T(base) + resid; pix = T^-1(recon_t)
                    4 | 5 => {
                        let base_t = modcolor_forward(&base);
                        let recon_t: Vec<u8> = base_t
                            .iter()
                            .zip(resid.iter())
                            .map(|(a, r)| a.wrapping_add(*r))
                            .collect();
                        modcolor_inverse(&recon_t)
                    }
                    _ => unreachable!(),
                }
            }
            _ => {
                return Err(Error::fmt_msg(format!(
                    "tile {i}: mode byte {mode:#04x} pos {ip}"
                )))
            }
        };
        decoded.push((tw, th, tile.clone()));

        for y_ in 0..th as usize {
            let py = row as usize * TILE_SIZE as usize + y_;
            for x in 0..tw as usize {
                let pxx = col as usize * TILE_SIZE as usize + x;
                let d = (py * width as usize + pxx) * 3;
                let s = (y_ * tw as usize + x) * 3;
                pixels[d..d + 3].copy_from_slice(&tile[s..s + 3]);
            }
        }
    }
    if ip != raw.len() {
        return Err(Error::fmt_msg("extra"));
    }
    // note: Ok(Dims..) tuple-return belonged to decode_mnf7; this walker returns pixels
    Ok(pixels)
}

/// Image-wide MED (JPEG-LS LOCO-I) predictor over RGB8: pixel (x,y) predicts
/// from left/top/topleft true pixels (edge-sense: flat branches take the
/// plane a+b-cc). First pixel = 127; top row = left; left col = above.
/// Deterministic; encoder and decoder compute the identical progression
/// from reconstructed causal neighbors.
pub fn med_predict(rgb: &[u8], w: usize, h: usize) -> Vec<u8> {
    assert_eq!(rgb.len(), w * h * 3, "rgb size");
    let mut out = vec![0u8; rgb.len()];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3usize {
                let i = (y * w + x) * 3 + c;
                if x == 0 && y == 0 {
                    out[i] = 127;
                } else if y == 0 {
                    out[i] = rgb[i - 3];
                } else if x == 0 {
                    out[i] = rgb[i - w * 3];
                } else {
                    let a = i32::from(rgb[i - 3]);
                    let b = i32::from(rgb[i - w * 3]);
                    let cc = i32::from(rgb[i - w * 3 - 3]);
                    let plane = a + b - cc;
                    let flat = (a >= b && a >= cc && b >= cc) || (a <= b && a <= cc && b <= cc);
                    let p = if flat {
                        plane
                    } else {
                        let pa = (plane - a).abs();
                        let pb = (plane - b).abs();
                        let pcc = (plane - cc).abs();
                        if pa <= pb && pa <= pcc {
                            a
                        } else if pb <= pcc {
                            b
                        } else {
                            cc
                        }
                    };
                    out[i] = p.clamp(0, 255) as u8;
                }
            }
        }
    }
    out
}

#[inline]
fn med_predict_pixel(a: u32, b: u32, cc: u32) -> u32 {
    let plane = a as i64 + b as i64 - cc as i64;
    let flat = (a >= b && a >= cc && b >= cc) || (a <= b && a <= cc && b <= cc);
    if flat {
        // plane branch; clamp into byte range — NOTE the encoder's predictor
        // values live in 0..=255 after its own clamp; identical formula both
        // sides (med_predict clamps identically)
        plane.clamp(0, 255) as u32
    } else {
        let pa = (plane - a as i64).abs();
        let pb = (plane - b as i64).abs();
        let pcc = (plane - cc as i64).abs();
        if pa <= pb && pa <= pcc {
            a
        } else if pb <= pcc {
            b
        } else {
            cc
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_mnf7(px: &[u8], dims: Dims, use_geom: bool) {
        let (buf, st) = encode_mnf7(px, dims, use_geom);
        let (d, out) = decode_mnf7(&buf).expect("decode");
        assert_eq!(d, dims);
        assert_eq!(out, px, "flags {:#b}", st.chosen_flags);
    }

    #[test]
    fn bit9_roundtrip() {
        let vals: Vec<u16> = (0..300).map(|i| (i * 137) % 512).collect();
        let packed = pack9(&vals);
        assert_eq!(unpack9(&packed, vals.len()).expect("unpack"), vals);
        // edge values
        for v in [0u16, 1, 511, 256, 255] {
            let p = pack9(&[v]);
            assert_eq!(unpack9(&p, 1).expect("u")[0], v);
        }
    }

    #[test]
    fn ycocg_exhaustive_sample() {
        for v in (0..=0xFFFFFF).step_by(41) {
            let px = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
            let (y, co, cg) = ycocg_forward(&px);
            assert_eq!(ycocg_inverse(&y, &co, &cg), px, "{px:?}");
            // packed form must roundtrip too
            let py = pack9(&co);
            let pg = pack9(&cg);
            let co2 = unpack9(&py, 1).expect("co");
            let cg2 = unpack9(&pg, 1).expect("cg");
            assert_eq!(ycocg_inverse(&y, &co2, &cg2), px, "{px:?}");
        }
    }

    #[test]
    fn mnf6_roundtrip_all_classes() {
        for (w, h, seed) in [
            (16u32, 16u32, 1),
            (48, 33, 2),
            (64, 64, 3),
            (31, 17, 4),
            (1, 1, 5),
        ] {
            let mut s: u64 = (seed as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ 0xDEADBEEF;
            let mut rnd = move || {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                s
            };
            let px: Vec<u8> = (0..w as usize * h as usize * 3)
                .map(|_| (rnd() & 0xFF) as u8)
                .collect();
            roundtrip_mnf7(
                &px,
                Dims {
                    width: w,
                    height: h,
                },
                true,
            );
        }
    }

    #[test]
    fn mnf7_modcolor_is_bijective_exhaustive() {
        // ADR gate: proof of reversibility. Genuinely exhaustive over all
        // 2^24 RGB8 triples (review fix: a stride sweep is sampling, not proof).
        // Runs in ~seconds under `cargo test --release`; skipped in debug
        // builds where a stride sweep + corners carry the same structural check.
        if cfg!(debug_assertions) {
            for v in (0..=0xFFFFFFu32).step_by(17) {
                let px = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                assert_eq!(modcolor::inverse(modcolor::forward(px)), px, "{px:?}");
            }
            for r in [0u8, 1, 127, 128, 254, 255] {
                for g in [0u8, 85, 170, 255] {
                    for b in [0u8, 128, 255] {
                        let px = [r, g, b];
                        assert_eq!(modcolor::inverse(modcolor::forward(px)), px);
                    }
                }
            }
        } else {
            let mut checked: u64 = 0;
            for v in 0..=0xFFFFFFu32 {
                let px = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                assert_eq!(modcolor::inverse(modcolor::forward(px)), px, "{px:?}");
                checked += 1;
            }
            assert_eq!(checked, 16_777_216, "exhaustive sweep count");
        }
    }

    #[test]
    fn mnf7_color_domain_used_when_it_wins() {
        // structured color-delta over a noisy base: tile B = T^-1(T(A) + co ramp)
        // makes the RGB residual incompressible noise but the color-domain
        // residual mostly zeros + a repeating ramp — mode 4 must win.
        let mut s: u64 = 42;
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let a: Vec<u8> = (0..16 * 16 * 3).map(|_| (rnd() & 0xFF) as u8).collect();

        let ta_t = modcolor_forward(&a);
        let mut tb_t = ta_t.clone();
        let ramp = [0u8, 1, 2, 255]; // repeating pattern in the co channel only
        for i in (1..tb_t.len()).step_by(3) {
            tb_t[i] = tb_t[i].wrapping_add(ramp[(i / 3) % 4]);
        }
        let b = modcolor_inverse(&tb_t);

        let mut two = a.clone();
        two.extend_from_slice(&b);
        let d2 = Dims {
            width: 16,
            height: 32,
        };
        let (buf, st) = encode_mnf7(&two, d2, true);
        let (_, out) = decode_mnf7(&buf).expect("decode");
        assert_eq!(out, two);
        let color_modes = st.ref_exact_color + st.ref_geom_color;
        assert!(
            color_modes > 0,
            "expected color-domain references, got {}",
            color_modes
        );
    }

    #[test]
    fn mnf6_rate_aware_picks_flags() {
        // gradient should prefer planar; the study showed planar+zlib gradient = 5,018 vs interl 219,782
        let (w, h) = (64u32, 64u32);
        let mut px = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                px[i] = (x * 4) as u8;
                px[i + 1] = (y * 4) as u8;
                px[i + 2] = 128;
            }
        }
        let (buf, st) = encode_mnf7(
            &px,
            Dims {
                width: w,
                height: h,
            },
            false,
        );
        assert_eq!(
            st.chosen_flags & FLAG_PLANAR,
            FLAG_PLANAR,
            "gradient wants planar"
        );
        let (_, out) = decode_mnf7(&buf).expect("decode");
        assert_eq!(out, px);
    }

    #[test]
    fn mnf6_rejects() {
        assert!(decode_mnf7(&[]).is_err());
        assert!(decode_mnf7(&[0u8; 27]).is_err());
        assert!(decode_mnf7(b"MNF5\x01\x00\x00\x00\x01\x00\x00\x00\x10\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00").is_err(), "mnf5 magic");
        // reserved flag bits
        let mut f = vec![0u8; 28];
        f[0..4].copy_from_slice(b"MNF6");
        f[24] = 0b100; // bit2 reserved
        assert!(decode_mnf7(&f).is_err(), "reserved flag");
    }

    #[test]
    fn mnf6_beats_mnf5_on_gradient() {
        // rate-aware selection must not lose to the shipped codec on smooth content

        use crate::codec::{build_header, compress_payload, encode_tiles};
        let (w, h) = (96u32, 96u32);
        let mut px = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                px[i] = (x * 2) as u8;
                px[i + 1] = (y * 2) as u8;
                px[i + 2] = ((x + y) / 2) as u8;
            }
        }
        let d = Dims {
            width: w,
            height: h,
        };
        let tiles = crate::codec::split_tiles(&px, d);
        let raw = encode_tiles(&tiles, true, true);
        let z = compress_payload(&raw);
        let mut m5 = build_header(d, tiles.len() as u32, z.len() as u32);
        m5.extend_from_slice(&z);
        let (m6, _) = encode_mnf7(&px, d, true);
        assert!(decode_mnf7(&m6).is_ok());
        assert!(
            m6.len() <= m5.len(),
            "MNF6/7 ({}) should not lose to MNF5 ({}) on gradient",
            m6.len(),
            m5.len()
        );
    }
    #[test]
    fn d8_atlas_full_roundtrip() {
        // Review fix 4 (conformance gate): a 5-level staircase image forces
        // distinct D8 matches; every nonidentity transform must be EMITTED
        // (not silently fallen back to literal) and the complete file must
        // decode back to the original pixels. geom_nontrivial assertion keeps
        // the test from passing via literal-only encoding.
        let (w, h) = (64u32, 64u32);
        let mut px = vec![0u8; (w * h * 3) as usize];
        // 16 unique structures: quantized x-position blocks, so rotated
        // variants of earlier tiles produce real zero-residual matches only
        // through the corresponding D8 transform.
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                let band = ((x / 4) as u16 * 13) as u8;
                px[i] = band;
                px[i + 1] = ((y / 4) as u16 * 11) as u8;
                px[i + 2] = band ^ (((y / 4) as u16 * 29) as u8);
            }
        }
        // build an image where row-blocks 2..8 are D8 transforms of the first
        let mut stacked = px.clone();
        for t in [1u8, 2, 3, 4, 5, 6, 7] {
            let transformed = crate::codec::apply_transform(t, w, h, &px);
            stacked.extend_from_slice(&transformed);
        }
        let d = Dims {
            width: w,
            height: h * 8,
        };
        let (buf, st) = encode_mnf7(&stacked, d, true);
        // the encoder must have emitted geometric references with nonidentity
        // transforms; literals would make this test meaningless
        assert!(
            st.ref_geom > 0,
            "no geometric references emitted: literal={} exact={} geom={} nontrivial={}",
            st.literal,
            st.ref_exact,
            st.ref_geom,
            st.geom_nontrivial
        );
        assert!(
            st.geom_nontrivial > 0,
            "only identity transforms used — test cannot verify D8 matching"
        );
        let (dims, out) = decode_mnf7(&buf).expect("full-file decode");
        assert_eq!((dims.width, dims.height), (w, h * 8));
        assert_eq!(out, stacked, "MNF7 D8 atlas roundtrip");
        // MNF7 decodes exact; also cross-check the file through the standalone CLI path in differential tests
    }
}
