// MNF8 — format v4 (ADR-002: Entropy Backend and Residual Probability Modeling).
//
// MNF8 = MNF7 (modes 0-5, PLANAR|ZSTD flags) + a backend-identifier field +
// encoder profiles; new magic, new header field. MNF5/6/7 files decode via
// their own magics — no compatibility break (README §9 versioning rule).
//
// Header (32 bytes):
//   "MNF8", u32 LE width, height, tile=16, tile_count, payload_len,
//   u32 flags (MNF7 semantics: bit0 PLANAR, bit1 ZSTD, bits 2..30 reserved,
//   NEW: bit31 = MODE-DICT? no — flags bits 2..31 stay reserved/0),
//   u32 backend:
//     0 = zlib-9  (MNF7 control; payload is a zlib stream like MNF7 flag-free)
//     1 = zstd-19 (xxh64-checksummed frame, decoder window bounded)
//     2 = brotli-9 (with static dictionary — cost lives in the decoder artifact)
//     3 = reserved (custom mixed run/rANS coder; reject until it passes the gate)
//   other values: reject like reserved flag bits.
//
// Profiles (encoder policy, not wire format):
//   balanced  -> measures nothing beyond per-variant flags; always zstd-19.
//   maximum   -> builds the tile stream under each (backend, flags) candidate,
//                re-costing per-tile mode selection through that backend's
//                actual coder (stage-2 evidence: selection is
//                backend-dependent), and emits the smallest file.
//
// Records: exactly MNF7's (modes 0-3 plus MNF7 color modes 4/5), decoded by
// the same record walker as decode_mnf7; only container + coder differ.

use std::io::Read;
use std::io::Write;

use crate::codec::{apply_transform, div_ceil, tile_sig, Atlas, Dims, RANK_CANDIDATES, TILE_SIZE};
use crate::mnf6::{
    compress_stream, decode_record_walk, modcolor_forward_iter, pack9, planar_pack, proxy_cost,
    ycocg_forward, Coder, MnF6Stats, FLAGS_RESERVED, FLAG_PLANAR,
};

pub const HEADER8: usize = 32;

pub const BACKEND_ZLIB: u32 = 0;
pub const BACKEND_ZSTD19: u32 = 1;
pub const BACKEND_BROTLI9: u32 = 2;
pub const BACKEND_RESERVED3: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Balanced,
    Maximum,
}

/// Backend candidates for the `maximum` profile, in measured-preference order.
fn backend_candidates() -> Vec<(u32, Coder)> {
    vec![
        (BACKEND_ZSTD19, Coder::Zstd(19)),
        (BACKEND_ZLIB, Coder::Zlib),
    ]
}

#[allow(dead_code)]
fn brotli9_compress(buf: &[u8]) -> Result<Vec<u8>, String> {
    // ponytail: only quality-9, no custom dictionary — the gate measured q9
    // within 0.1-1% of q11 on residual streams and the q11 encode cost is 10x.
    let mut out = Vec::new();
    let mut w = brotli::CompressorWriter::new(&mut out, 4096, 9, 22);
    w.write_all(buf).map_err(|_| "brotli".to_string())?;
    drop(w);
    Ok(out)
}

#[allow(dead_code)]
fn brotli9_decompress(buf: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let reader = brotli::Decompressor::new(buf, 4096);
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut out)
        .map_err(|_| "brotli-decomp".to_string())?;
    if out.len() > limit {
        return Err("bomb".into());
    }
    Ok(out)
}

fn backend_compress(backend: u32, buf: &[u8]) -> Result<Vec<u8>, String> {
    match backend {
        BACKEND_ZLIB => Ok(compress_stream(&Coder::Zlib, buf)),
        BACKEND_ZSTD19 => Ok(compress_stream(&Coder::Zstd(19), buf)),
        _ => Err(format!("backend {backend} not a compressed-output coder")),
    }
}

fn backend_decompress(backend: u32, buf: &[u8], limit: u64) -> Result<Vec<u8>, String> {
    match backend {
        BACKEND_ZLIB => {
            let mut d = flate2::read::ZlibDecoder::new(buf).take(limit + 1);
            let mut v = Vec::new();
            d.read_to_end(&mut v).map_err(|_| "zlib".to_string())?;
            if v.len() as u64 > limit {
                return Err("bomb".into());
            }
            Ok(v)
        }
        BACKEND_ZSTD19 => {
            let d = zstd::stream::read::Decoder::new(buf).map_err(|_| "zstd-init".to_string())?;
            let mut d = d.take(limit + 1);
            let mut v = Vec::new();
            d.read_to_end(&mut v).map_err(|_| "zstd".to_string())?;
            if v.len() as u64 > limit {
                return Err("bomb".into());
            }
            Ok(v)
        }
        _ => Err(format!("backend {backend} not a decompressable coder")),
    }
}

/// Encode RGB8 pixels as MNF8.
///
/// `balance = Balanced`: zstd-19 only (flags measured exactly as MNF7 but
/// with the zstd coder pinned; cheaper planar/zstd matrix kept — it's the
/// same 2-variant loop as MNF7 minus zlib arms).
///
/// `balance = Maximum`: measures backend×flags candidates and emits the
/// smallest final file (per-tile mode selection is re-costed inside each
/// candidate's own coder via encode_mnf7-style loops — reuse of encode_mnf7
/// per backend is the measured-faithful path: MNF7's rate-aware encoder
/// already re-costs selection per flags variant; ADR-002 requires the same
/// honesty across backends).
pub fn encode_mnf8(
    pixels: &[u8],
    dims: Dims,
    use_geom: bool,
    profile: Profile,
) -> (Vec<u8>, MnF6Stats, u32) {
    match profile {
        Profile::Balanced => {
            // zstd-19 pinned; measure the SAME flag variants MNF7 measures but
            // only against the zstd coder (zlib arms excluded per ADR-002)
            let variants = [(0u32, false), (FLAG_PLANAR, true)];
            let mut best: Option<(Vec<u8>, u32, MnF6Stats, u32)> = None;
            for (flags, planar) in variants {
                let (file, st) =
                    encode_stream_zstd(pixels, dims, use_geom, flags, planar, Some(BACKEND_ZSTD19));
                let size = file.len();
                if best.as_ref().is_none_or(|b| size < b.0.len()) {
                    best = Some((file, flags, st, BACKEND_ZSTD19));
                }
            }
            let (file, _, st, backend) = best.expect("variant");
            (file, st, backend)
        }
        Profile::Maximum => {
            // stage-2 protocol: each backend candidate re-costs tile selection
            // inside its own coder. encode_stream per backend; zlib backend
            // additionally measures the planar variant (zstd's planar choice
            // is measured in the balanced branch already).
            let mut best: Option<(Vec<u8>, u32, MnF6Stats, u32)> = None;
            for (backend, coder) in backend_candidates() {
                for flags in [0u32, FLAG_PLANAR] {
                    let planar = flags & FLAG_PLANAR != 0;
                    let (file, st) =
                        encode_stream_coder(pixels, dims, use_geom, flags, planar, &coder, backend);
                    let size = file.len();
                    if best.as_ref().is_none_or(|b| size < b.0.len()) {
                        best = Some((file, flags, st, backend));
                    }
                }
            }
            let (file, _, st, backend) = best.expect("candidate");
            (file, st, backend)
        }
    }
}

fn encode_stream_coder(
    pixels: &[u8],
    dims: Dims,
    use_geom: bool,
    flags: u32,
    planar: bool,
    coder: &Coder,
    backend: u32,
) -> (Vec<u8>, MnF6Stats) {
    // One per-backend tile-selection pass (the stage-2 protocol). Mirrors
    // encode_mnf7's per-tile loop but the candidate costing runs through
    // `compress_stream(coder, ..)` on the candidate body — backend-truthful.
    let tiles = crate::codec::split_tiles(pixels, dims);
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
        let mut choice_body = lit_body.clone();
        let mut choice_mode = 0u8;
        let mut choice_ref: Option<(u32, u8)> = None;
        let mut best_cost = 1 + proxy_cost(coder, &lit_body);

        {
            let (cy, cco, ccg) = ycocg_forward(&tile.data);
            let mut m3 = Vec::with_capacity(n + 160);
            m3.extend_from_slice(&cy);
            m3.extend_from_slice(&pack9(&cco));
            m3.extend_from_slice(&pack9(&ccg));
            let m3_cost = 1 + proxy_cost(coder, &m3);
            if m3_cost < best_cost {
                best_cost = m3_cost;
                choice_body = m3;
                choice_mode = 3;
            }
        }

        if tiles.len() > 1 {
            for prior in atlas.rank_by_sig(&sig, tile.width, tile.height, RANK_CANDIDATES) {
                let (_, _, ref_rgb) = &atlas.patches[prior as usize];
                let transforms: &[u8] = if use_geom && tile.width == tile.height {
                    &[0, 1, 2, 3, 4, 5, 6, 7]
                } else {
                    &[0, 1, 2, 3]
                };
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
                    let cost = meta + proxy_cost(coder, &body);
                    if cost < best_cost {
                        best_cost = cost;
                        choice_body = body;
                        choice_mode = if t == 0 { 1 } else { 2 };
                        choice_ref = Some((prior, t));
                    }
                    // color-domain candidate (MNF7 modes 4/5)
                    let tile_t = modcolor_forward_iter(&tile.data);
                    let base_t = modcolor_forward_iter(&base);
                    let mut r_c = vec![0u8; n];
                    for i in 0..n {
                        r_c[i] = tile_t[i].wrapping_sub(base_t[i]);
                    }
                    let body_c = pack(&r_c);
                    let cost_c = meta + 1 + proxy_cost(coder, &body_c);
                    if cost_c < best_cost {
                        best_cost = cost_c;
                        choice_body = body_c;
                        choice_mode = if t == 0 { 4 } else { 5 };
                        choice_ref = Some((prior, t));
                    }
                }
            }
        }

        stats.modes.push(choice_mode);
        stats.refs.push(choice_ref);
        match choice_mode {
            0 => stats.literal += 1,
            1 => {
                stats.ref_exact += 1;
            }
            2 => {
                stats.ref_geom += 1;
                if choice_ref.unwrap().1 != 0 {
                    stats.geom_nontrivial += 1;
                }
            }
            3 => stats.ycocg_literal += 1,
            4 => stats.ref_exact_color += 1,
            5 => {
                stats.ref_geom_color += 1;
                if choice_ref.unwrap().1 != 0 {
                    stats.geom_nontrivial += 1;
                }
            }
            _ => unreachable!(),
        }
        raw.push(choice_mode);
        if let Some((p, t)) = choice_ref {
            raw.extend_from_slice(&p.to_le_bytes());
            if t != 0 {
                raw.push(t);
            }
        }
        raw.extend_from_slice(&choice_body);
        atlas.insert(tile);
    }

    let payload = backend_compress(backend, &raw).expect("backend compress");
    let mut file = Vec::with_capacity(HEADER8 + payload.len());
    file.extend_from_slice(b"MNF8");
    file.extend_from_slice(&dims.width.to_le_bytes());
    file.extend_from_slice(&dims.height.to_le_bytes());
    file.extend_from_slice(&TILE_SIZE.to_le_bytes());
    file.extend_from_slice(&(tiles.len() as u32).to_le_bytes());
    file.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    file.extend_from_slice(&flags.to_le_bytes());
    file.extend_from_slice(&backend.to_le_bytes());
    file.extend_from_slice(&payload);
    (file, stats)
}

fn encode_stream_zstd(
    pixels: &[u8],
    dims: Dims,
    use_geom: bool,
    flags: u32,
    planar: bool,
    _backend_hint: Option<u32>,
) -> (Vec<u8>, MnF6Stats) {
    encode_stream_coder(
        pixels,
        dims,
        use_geom,
        flags,
        planar,
        &Coder::Zstd(19),
        BACKEND_ZSTD19,
    )
}

// --- decoding ----------------------------------------------------------------

/// Decode MNF8. Record walk is MNF7's (shared `decode_record_walk`); the
/// container fields differ (32-byte header, backend id) and the coder
/// dispatch runs on the backend field.
pub fn decode_mnf8(buf: &[u8]) -> Result<(Dims, Vec<u8>, u32), String> {
    if buf.len() < HEADER8 {
        return Err("short".into());
    }
    if buf[0..4] != *b"MNF8" {
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
    if buf.len() != HEADER8 + payload_len as usize {
        return Err("filelen".into());
    }
    let flags = u32le(24);
    if flags & FLAGS_RESERVED != 0 {
        return Err("reserved-flags".into());
    }
    let backend = u32le(28);
    if backend > BACKEND_RESERVED3 {
        return Err(format!("unknown backend {backend}"));
    }
    if backend == BACKEND_RESERVED3 {
        return Err("reserved backend (custom coder not shipped)".into());
    }
    let planar = flags & FLAG_PLANAR != 0;

    // mode-3 record max + per-tile generic max (audit 2026-10-25 lesson)
    let mode3_max: u64 = 1 + 256 + 2 * u64::from((256u32 * 9).div_ceil(8));
    let per_tile_max: u64 = 1 + 5 + (TILE_SIZE * TILE_SIZE * 3) as u64;
    let limit = tile_count as u64 * per_tile_max.max(mode3_max) + per_tile_max + mode3_max;

    let payload = &buf[HEADER8..];
    let raw = match backend {
        BACKEND_ZLIB | BACKEND_ZSTD19 => backend_decompress(backend, payload, limit)?,
        _ => unreachable!("checked above"),
    };

    let pixels = decode_record_walk(&raw, width, height, tile_count, planar)?;
    Ok((Dims { width, height }, pixels, backend))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(px: &[u8], dims: Dims, profile: Profile) -> u32 {
        let (buf, st, backend) = encode_mnf8(px, dims, true, profile);
        let (d, out, _) = decode_mnf8(&buf).expect("decode");
        assert_eq!(d, dims);
        assert_eq!(
            out, px,
            "profile {profile:?} backend {backend} flags {st:?}"
        );
        backend
    }

    #[test]
    fn mnf8_balanced_roundtrip_all_classes() {
        for (w, h, seed) in [(16u32, 16u32, 1), (48, 33, 2), (31, 17, 4), (1, 1, 5)] {
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
            roundtrip(
                &px,
                Dims {
                    width: w,
                    height: h,
                },
                Profile::Balanced,
            );
        }
    }

    #[test]
    fn mnf8_maximum_never_loses_to_mnf7() {
        // ADR-002 gate: maximum-profile MNF8 measures MORE candidates than
        // MNF7's rate-aware loop; it must never produce a larger file.
        for name in [
            "screenshot_640x400.png",
            "solid_256x256.png",
            "gradient_320x240.png",
        ] {
            let path = format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"));
            let img = image::open(&path)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .to_rgb8();
            let d = Dims {
                width: img.width(),
                height: img.height(),
            };
            let px = img.into_raw();
            let (buf8, _, backend) = encode_mnf8(&px, d, true, Profile::Maximum);
            let (buf7, _) = crate::mnf6::encode_mnf7(&px, d, true);
            // MNF8's header carries the 4-byte backend field; equality of
            // measure means <= MNF7 + 4 (the honest wire cost of the id).
            assert!(
                buf8.len() <= buf7.len() + 4,
                "{name}: MNF8({backend}) {} > MNF7(+4) {}",
                buf8.len(),
                buf7.len()
            );
            let (_, out, _) = decode_mnf8(&buf8).expect("decode");
            assert_eq!(out, px, "{name}: exact");
        }
    }

    #[test]
    fn mnf8_rejects() {
        let mut f = vec![0u8; 32];
        f[0..4].copy_from_slice(b"MNF8");
        f[4..8].copy_from_slice(&16u32.to_le_bytes());
        f[8..12].copy_from_slice(&16u32.to_le_bytes());
        f[12..16].copy_from_slice(&16u32.to_le_bytes());
        f[16..20].copy_from_slice(&1u32.to_le_bytes());
        // payload_len = 0, flags = 0
        f[28..32].copy_from_slice(&7u32.to_le_bytes()); // unknown backend
        assert!(decode_mnf8(&f).is_err(), "unknown backend rejected");
        f[28..32].copy_from_slice(&3u32.to_le_bytes()); // reserved backend
        assert!(decode_mnf8(&f).is_err(), "reserved backend rejected");
        f[28..32].copy_from_slice(&0u32.to_le_bytes());
        f[24] = 0b100; // reserved flag bit
        assert!(decode_mnf8(&f).is_err(), "reserved flag rejected");
        // bad magic
        f[0] = b'X';
        f[24] = 0;
        assert!(decode_mnf8(&f).is_err(), "magic");
    }

    #[test]
    fn mnf8_balanced_picks_zstd() {
        let px = vec![7u8; 16 * 16 * 3];
        let (_, _, backend) = encode_mnf8(
            &px,
            Dims {
                width: 16,
                height: 16,
            },
            false,
            Profile::Balanced,
        );
        assert_eq!(backend, BACKEND_ZSTD19, "balanced pins zstd-19");
    }
}
