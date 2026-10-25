// MNF6 — format v2 (README §9 v0.7/v0.8/v1.0 payload).
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
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

pub const HEADER6: usize = 28;
pub const FLAG_PLANAR: u32 = 1;
pub const FLAG_ZSTD: u32 = 2;
pub const FLAGS_RESERVED: u32 = !0b11;

pub enum Coder {
    Zlib,
    Zstd(i32),
}

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

fn proxy_cost(coder: &Coder, buf: &[u8]) -> u64 {
    match coder {
        Coder::Zlib => compress_stream(coder, buf).len() as u64,
        // probe at level 3 for speed; final stream uses level 19
        Coder::Zstd(_) => compress_stream(&Coder::Zstd(3), buf).len() as u64,
    }
}

// --- 9-bit plane packing (LSB-first) -----------------------------------------

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

pub fn unpack9(bytes: &[u8], n: usize) -> Result<Vec<u16>, String> {
    let need = (n * 9).div_ceil(8);
    if bytes.len() < need {
        return Err("9-bit plane truncated".into());
    }
    let mut out = Vec::with_capacity(n);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut bi = 0usize;
    for _ in 0..n {
        while bits < 9 {
            if bi >= bytes.len() {
                return Err("9-bit plane truncated".into());
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

#[derive(Debug, Clone, Default)]
pub struct MnF6Stats {
    pub literal: u32,
    pub ycocg_literal: u32,
    pub ref_exact: u32,
    pub ref_geom: u32,
    pub chosen_flags: u32,
    pub modes: Vec<u8>,
    pub refs: Vec<Option<(u32, u8)>>,
}

enum TileChoice {
    Literal(Vec<u8>),
    YCoCg(Vec<u8>),
    Exact(u32, Vec<u8>),
    Geom(u32, u8, Vec<u8>),
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

/// Encode RGB8 pixels as MNF6, choosing coding flags by measured stream size.
pub fn encode_mnf6(pixels: &[u8], dims: Dims, use_geom: bool) -> (Vec<u8>, MnF6Stats) {
    let tiles = crate::codec::split_tiles(pixels, dims);
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
                    stats.modes.push(2);
                    stats.refs.push(Some((*p, *t)));
                    raw.push(2u8);
                    raw.extend_from_slice(&p.to_le_bytes());
                    raw.push(*t);
                }
            }
            match &choice {
                TileChoice::Literal(b) | TileChoice::YCoCg(b) => raw.extend_from_slice(b),
                TileChoice::Exact(_, b) | TileChoice::Geom(_, _, b) => raw.extend_from_slice(b),
            }
            if std::env::var("MNF6_DEBUG").is_ok() {
                eprintln!(
                    "tile {} mode={} rawlen={}",
                    stats.modes.len(),
                    stats.modes.last().unwrap_or(&0),
                    raw.len()
                );
            }
            atlas.insert(tile);
        }

        let payload = compress_stream(&coder, &raw);
        let mut file = Vec::with_capacity(HEADER6 + payload.len());
        file.extend_from_slice(b"MNF6");
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

pub fn decode_mnf6(buf: &[u8]) -> Result<(Dims, Vec<u8>), String> {
    if buf.len() < HEADER6 {
        return Err("short".into());
    }
    if buf[0..4] != *b"MNF6" {
        return Err("magic".into());
    }
    let u32le = |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let width = u32le(4);
    let height = u32le(8);
    if width == 0 || height == 0 {
        return Err("dims".into());
    }
    if width as u64 * height as u64 > (1 << 26) {
        return Err("pixels".into());
    }
    if u32le(12) != TILE_SIZE {
        return Err("tilesize".into());
    }
    let cols = div_ceil(width, TILE_SIZE);
    let rows = div_ceil(height, TILE_SIZE);
    let tile_count = u32le(16);
    if tile_count != cols * rows {
        return Err("tilecount".into());
    }
    let payload_len = u32le(20);
    if payload_len > (1 << 31) - 1 {
        return Err("payloadlen".into());
    }
    if buf.len() != HEADER6 + payload_len as usize {
        return Err("filelen".into());
    }
    let flags = u32le(24);
    if flags & FLAGS_RESERVED != 0 {
        return Err("reserved-flags".into());
    }
    let planar = flags & FLAG_PLANAR != 0;
    let zlib_in = flags & FLAG_ZSTD == 0;

    let zlib_data = &buf[HEADER6..];
    // max record: 1 mode + 5 refmeta + 768 residual (+2 = mode3 planes 256+288+288 - 768)
    let limit = tile_count as u64 * (1 + 5 + (TILE_SIZE * TILE_SIZE * 3) as u64 + 2);
    let raw: Vec<u8> = if zlib_in {
        let mut d = flate2::read::ZlibDecoder::new(zlib_data).take(limit + 1);
        let mut v = Vec::new();
        d.read_to_end(&mut v).map_err(|_| "zlib".to_string())?;
        v
    } else {
        let d = zstd::stream::read::Decoder::new(zlib_data).map_err(|_| "zstd-init".to_string())?;
        let mut d = d.take(limit + 1);
        let mut v = Vec::new();
        d.read_to_end(&mut v).map_err(|_| "zstd".to_string())?;
        v
    };
    if raw.len() as u64 > limit {
        return Err("bomb".into());
    }

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
            return Err(format!("eof at tile {i}"));
        }
        let mode = raw[ip];
        ip += 1;

        let tile = match mode {
            0 => {
                if ip + n > raw.len() {
                    return Err("body".into());
                }
                let body = raw[ip..ip + n].to_vec();
                ip += n;
                if planar {
                    planar_unpack(&body)
                } else {
                    body
                }
            }
            3 => {
                // Y plane is 1 byte per pixel (np = n/3); n counts RGB bytes
                let np = n / 3;
                let n9 = (np * 9).div_ceil(8);
                if ip + np + 2 * n9 > raw.len() {
                    return Err(format!("tile {i}: body"));
                }
                let y = raw[ip..ip + np].to_vec();
                let co9 = &raw[ip + np..ip + np + n9];
                let cg9 = &raw[ip + np + n9..ip + np + 2 * n9];
                ip += np + 2 * n9;
                let co = unpack9(co9, np)?;
                let cg = unpack9(cg9, np)?;
                ycocg_inverse(&y, &co, &cg)
            }
            1 | 2 => {
                let meta = if mode == 1 { 4 } else { 5 };
                if ip + meta > raw.len() {
                    return Err("refmeta".into());
                }
                let idx = u32::from_le_bytes(raw[ip..ip + 4].try_into().unwrap()) as usize;
                let tr = if mode == 2 { raw[ip + 4] } else { 0 };
                ip += meta;
                if idx >= i as usize {
                    return Err("reforder".into());
                }
                if tr >= 8 {
                    return Err("tranform-range".into());
                }
                let (rw, rh, ref_rgb) = &decoded[idx];
                if *rw != tw || *rh != th {
                    return Err("refshape".into());
                }
                if tw != th && tr >= 4 {
                    return Err("xform-shape".into());
                }
                if ip + n > raw.len() {
                    return Err("body".into());
                }
                let body = raw[ip..ip + n].to_vec();
                ip += n;
                let resid = if planar { planar_unpack(&body) } else { body };
                let base = if tr == 0 {
                    ref_rgb.clone()
                } else {
                    apply_transform(tr, tw, th, ref_rgb)
                };
                base.iter()
                    .zip(resid.iter())
                    .map(|(a, r)| a.wrapping_add(*r))
                    .collect()
            }
            _ => return Err(format!("tile {i}: mode byte {mode:#04x} pos {ip}")),
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
        return Err("extra".into());
    }
    Ok((Dims { width, height }, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_mnf6(px: &[u8], dims: Dims, use_geom: bool) {
        let (buf, st) = encode_mnf6(px, dims, use_geom);
        let (d, out) = decode_mnf6(&buf).expect("decode");
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
            roundtrip_mnf6(
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
        let (buf, st) = encode_mnf6(
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
        let (_, out) = decode_mnf6(&buf).expect("decode");
        assert_eq!(out, px);
    }

    #[test]
    fn mnf6_rejects() {
        assert!(decode_mnf6(&[]).is_err());
        assert!(decode_mnf6(&[0u8; 27]).is_err());
        assert!(decode_mnf6(b"MNF5\x01\x00\x00\x00\x01\x00\x00\x00\x10\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00").is_err(), "mnf5 magic");
        // reserved flag bits
        let mut f = vec![0u8; 28];
        f[0..4].copy_from_slice(b"MNF6");
        f[24] = 0b100; // bit2 reserved
        assert!(decode_mnf6(&f).is_err(), "reserved flag");
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
        let (m6, _) = encode_mnf6(&px, d, true);
        assert!(decode_mnf6(&m6).is_ok());
        assert!(
            m6.len() <= m5.len(),
            "MNF6 ({}) should not lose to MNF5 ({}) on gradient",
            m6.len(),
            m5.len()
        );
    }
}
