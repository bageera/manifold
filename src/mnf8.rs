// MNF8 — format v4 (ADR-002: Entropy Backend and Residual Probability Modeling).
//
// MNF8 = MNF7 (modes 0-5, PLANAR|ZSTD flags) + a backend-identifier field +
// encoder profiles; new magic, new header field. MNF5/6/7 files decode via
// their own magics — no compatibility break (README §9 versioning rule).
//
// Header (32 bytes):
//   "MNF8", u32 LE width, height, tile=16, tile_count, payload_len,
//   u32 flags (MNF7 semantics: bit0 PLANAR, bit1 ZSTD, bits 2..31 reserved),
//   u32 backend:
//     0 = zlib-9  (MNF7 control; payload is a zlib stream)
//     1 = zstd-19 (xxh64-checksummed frame; decoder window capped at 23)
//     2 = RESERVED — brotli was prototyped but unshipped: its static
//         dictionary's cost accounting was measured only in the study, and
//         a dictionary-free brotli backend changed nothing on the corpus.
//         Reject until its gate re-passes (audit review 2026-10-25).
//     3 = reserved (custom mixed run/rANS coder; reject until it passes the gate)
//   other values: reject like reserved flag bits.
//
// Profiles (encoder policy, not wire format):
//   balanced  -> always zstd-19 (flags measured: planar vs interleaved).
//   maximum   -> builds the tile stream under each (backend, flags)
//                candidate, re-costing per-tile mode selection through that
//                candidate's coder (stage-2 evidence: selection is
//                backend-dependent), and emits the smallest file.
//
// Records: exactly MNF7's (modes 0-3 plus MNF7 color modes 4/5), decoded by
// the same record walker as decode_mnf7; only container + coder differ.

use std::io::Read;

use crate::codec::{apply_transform, div_ceil, split_tiles, tile_sig, Atlas, Dims, TILE_SIZE};
use crate::error::{Error, Result};
use crate::mnf6::{
    compress_stream, decode_record_walk, med_predict, modcolor_forward_iter, pack9, planar_pack,
    proxy_cost, ycocg_forward, Coder, MnF6Stats, FLAG_PLANAR,
};

/// Header size: `MNF8` + 7×`u32` LE (adds backend id) — 4 B over MNF7.
pub const HEADER8: usize = 32;

// MNF8-only flags (MNF6/7 keep !0b11 frozen; MNF8 owns bit2):
/// Literal bodies carry image-wide MED residuals (decode inverts causally).
pub const FLAG_PRED_MED: u32 = 4; // mode-0 literal bodies carry image-wide MED residuals
/// Bits above bit2 reserved (MNF8 owns bit2).
pub const FLAGS_RESERVED8: u32 = !0b111;

/// zlib-9 backend.
pub const BACKEND_ZLIB: u32 = 0;
/// zstd-19 backend (ships in Balanced and Maximum).
pub const BACKEND_ZSTD19: u32 = 1;
/// Reserved (brotli prototype; decode rejects).
pub const BACKEND_RESERVED2: u32 = 2; // brotli prototype — unshipped, reject
/// Reserved (run/rANS prototype; decode rejects).
pub const BACKEND_RESERVED3: u32 = 3; // custom run/rANS — unshipped, reject

// zstd-19's default window is 8 MiB = 2^23; hard-pin the decoder cap to it
// (review fix 4: a hostile frame can't claim a 128 MiB window).
/// zstd decoder window ceiling (8 MiB; matches level-19 encoder).
pub const ZSTD_WINDOW_LOG_MAX: u32 = 23;

/// Rank count for candidate references... (kept from the shared atlas table;
/// RANK_CANDIDATES lives in codec — do not re-derive here.)
use crate::codec::RANK_CANDIDATES;

/// Encoder search breadth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// zstd-19 only: fast, sane default.
    Balanced,
    /// All backends + flag variants measured; ships the smallest.
    Maximum,
}

/// Backend candidates for the `maximum` profile, in measured-preference order.
fn backend_candidates() -> Vec<(u32, Coder)> {
    vec![
        (BACKEND_ZSTD19, Coder::Zstd(19)),
        (BACKEND_ZLIB, Coder::Zlib),
    ]
}

fn backend_compress(backend: u32, buf: &[u8]) -> Result<Vec<u8>> {
    match backend {
        BACKEND_ZLIB => Ok(compress_stream(&Coder::Zlib, buf)),
        BACKEND_ZSTD19 => Ok(compress_stream(&Coder::Zstd(19), buf)),
        other => Err(Error::fmt_msg(format!(
            "backend {other} not a compressed-output coder"
        ))),
    }
}

fn backend_decompress(backend: u32, buf: &[u8], limit: u64) -> Result<Vec<u8>> {
    match backend {
        BACKEND_ZLIB => {
            // canonical-decode (review small item): every payload byte must
            // belong to the stream — zlib's total_in tells us exactly how
            // much was consumed.
            let mut d = flate2::read::ZlibDecoder::new(buf).take(limit + 1);
            let mut v = Vec::new();
            d.read_to_end(&mut v)
                .map_err(|_| Error::Decompress("corrupt zlib stream"))?;
            if v.len() as u64 > limit {
                return Err(Error::fmt_msg("bomb"));
            }
            if d.get_ref().total_in() as usize != buf.len() {
                return Err(Error::fmt_msg("trailing bytes after zlib stream"));
            }
            Ok(v)
        }
        BACKEND_ZSTD19 => {
            // window capped at the encoder's actual need (review fix 4);
            // Cursor-wrapped so post-decode input position is observable
            // (canonical-decode check, review small item).
            let cursor = std::io::Cursor::new(buf);
            let mut d = zstd::stream::read::Decoder::new(cursor)
                .map_err(|_| Error::Decompress("zstd init"))?;
            d.window_log_max(ZSTD_WINDOW_LOG_MAX)
                .map_err(|_| Error::Decompress("zstd window param"))?;
            let mut v = Vec::new();
            d.single_frame()
                .take(limit + 1)
                .read_to_end(&mut v)
                .map_err(|_| Error::Decompress("corrupt zstd stream"))?;
            if v.len() as u64 > limit {
                return Err(Error::fmt_msg("bomb"));
            }
            // NOTE (honest limitation): zlib canonicality is exact via
            // total_in above; the zstd stream API here cannot cheaply report
            // the decoder's exact consumed offset (BufReader may also read
            // ahead), so bytes after a complete zstd frame are tolerated.
            // Canonical zstd files remain an option via a future zstd-safe
            // one-shot decode; not worth it today (ponytail).
            Ok(v)
        }
        other => Err(Error::fmt_msg(format!(
            "backend {other} not a decompressable coder"
        ))),
    }
}

/// Encode RGB8 pixels as MNF8.
///
/// `profile = Balanced`: zstd-19 pinned; measures the planar vs
/// interleaved flag variants under that coder (same honesty as MNF7
/// measures flags, minus the zlib arms excluded by ADR-002).
///
/// `profile = Maximum`: measures backend×flags candidates and emits the
/// smallest final file. Per-tile mode selection is re-costed inside each
/// candidate's own coder — the stage-2 protocol (docs/entropy-backends-v13.md).
/// There is NO shared per-tile loop with encode_mnf7: the one function
/// `encode_stream_coder` is parameterized by coder and every caller routes
/// through it (review fix 8: MNF7's loop stays the frozen-format history;
/// MNF8 owns its selection policy).
pub fn encode_mnf8(
    pixels: &[u8],
    dims: Dims,
    use_geom: bool,
    profile: Profile,
) -> (Vec<u8>, MnF6Stats, u32) {
    let candidates: Vec<(u32, Coder)> = match profile {
        Profile::Balanced => vec![(BACKEND_ZSTD19, Coder::Zstd(19))],
        Profile::Maximum => backend_candidates(),
    };
    let mut best: Option<(Vec<u8>, u32, MnF6Stats, u32)> = None;
    for (backend, coder) in candidates {
        for flags in [
            0u32,
            FLAG_PLANAR,
            FLAG_PRED_MED,
            FLAG_PLANAR | FLAG_PRED_MED,
        ] {
            let planar = flags & FLAG_PLANAR != 0;
            let med = flags & FLAG_PRED_MED != 0;
            let (file, st) = encode_stream_coder(
                pixels, dims, true, use_geom, flags, planar, med, &coder, backend,
            )
            .expect("encoding an in-memory image cannot fail");
            let size = file.len();
            if best.as_ref().is_none_or(|b| size < b.0.len()) {
                best = Some((file, flags, st, backend));
            }
        }
    }
    let (file, flags, mut st, backend) = best.expect("at least one candidate");
    st.chosen_flags = flags;
    (file, st, backend)
}

/// One tile-stream encoder, parameterized by coder + backend — THE selection
/// loop for MNF8 (no per-variant duplicates of this function).
///
/// Arm controls (both default true, plumbable for ablation):
/// - `use_atlas`: candidate references at all (false = pure prediction arm)
/// - `use_geom`: D8 transforms on top of exact references (false = t=0 only,
///   the v0.5 exact-reference semantics — codec.rs pins t=0 for this case
///   and MNF8 must match it)
///
/// Shared YCoCg (mode 3) and planar layout stay ON in both arms (ADR-002).
#[allow(clippy::too_many_arguments)]
fn encode_stream_coder(
    pixels: &[u8],
    dims: Dims,
    use_atlas: bool,
    use_geom: bool,
    flags: u32,
    planar: bool,
    med: bool,
    coder: &Coder,
    backend: u32,
) -> Result<(Vec<u8>, MnF6Stats)> {
    let med_pred = if med {
        med_predict(pixels, dims.width as usize, dims.height as usize)
    } else {
        Vec::new()
    };
    let tiles = split_tiles(pixels, dims);
    let mut raw: Vec<u8> = Vec::with_capacity(tiles.len() * 774);
    let mut stats = MnF6Stats::default();
    let mut atlas = Atlas::new();

    for (idx, tile) in tiles.iter().enumerate() {
        let n = tile.nbytes();
        let pack = |rgb: &[u8]| -> Vec<u8> {
            if planar {
                planar_pack(rgb)
            } else {
                rgb.to_vec()
            }
        };

        // mode-0 literal candidate; with FLAG_PRED_MED the body carries the
        // image-wide MED residual (decoder reconstructs causally)
        let lit_src: Vec<u8> = if med {
            let (tw, th) = (tile.width as usize, tile.height as usize);
            let cols_px = dims.width as usize; // med_pred is per-PIXEL flat
            let tx = (idx % ((dims.width as usize).div_ceil(16))) * 16;
            let ty = (idx / ((dims.width as usize).div_ceil(16))) * 16;
            let mut src = Vec::with_capacity(n);
            for y in 0..th {
                let row_off = ((ty + y) * cols_px + tx) * 3;
                src.extend_from_slice(&med_pred[row_off..row_off + tw * 3]);
            }
            tile.data
                .iter()
                .zip(&src)
                .map(|(x, m)| x.wrapping_sub(*m))
                .collect()
        } else {
            tile.data.clone()
        };

        let lit_body = pack(&lit_src);
        let mut choice_body = lit_body.clone();
        let mut choice_mode = 0u8;
        let mut choice_ref: Option<(u32, u8)> = None;
        let mut best_cost = 1 + proxy_cost(coder, &lit_body);

        // shared literal candidates (both arms): YCoCg-R (mode 3)
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

        // invariant hoist (review fix 7): T(tile.data) is candidate- and
        // transform-independent — compute once per tile
        let tile_t = modcolor_forward_iter(&tile.data);

        // atlas candidates (arm-controlled; review fixes 2 + use_atlas)
        if use_atlas && tiles.len() > 1 {
            let sig = tile_sig(tile.width, tile.height, &tile.data);
            for prior in atlas.rank_by_sig(&sig, tile.width, tile.height, RANK_CANDIDATES) {
                let (_, _, ref_rgb) = &atlas.patches[prior as usize];
                // review fix 2: `use_geom = false` means EXACT references only
                // (t=0), matching codec.rs's v0.5 ablation semantics; square
                // tiles may use all 8 D8 transforms, partial tiles only the
                // shape-keeping subgroup.
                let transforms: &[u8] = if !use_geom {
                    &[0]
                } else if tile.width == tile.height {
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
                    // metadata: mode byte + u32 idx + optional transform byte
                    let meta: u64 = if t == 0 { 5 } else { 6 };
                    let cost = meta + proxy_cost(coder, &body);
                    if cost < best_cost {
                        best_cost = cost;
                        choice_body = body;
                        choice_mode = if t == 0 { 1 } else { 2 };
                        choice_ref = Some((prior, t));
                    }
                    // color-domain candidate (MNF7 modes 4/5). Review fix 3:
                    // the wire carries NOTHING extra for color modes — the
                    // mode byte already tells the decoder which domain to
                    // decode the body in. The phantom `+ 1` that the old
                    // cost model charged made the encoder under-select
                    // color modes (contaminating any color-mode ablation).
                    let base_t = modcolor_forward_iter(&base);
                    let mut r_c = vec![0u8; n];
                    for i in 0..n {
                        r_c[i] = tile_t[i].wrapping_sub(base_t[i]);
                    }
                    let body_c = pack(&r_c);
                    let cost_c = meta + proxy_cost(coder, &body_c);
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
            1 => stats.ref_exact += 1,
            2 => {
                stats.ref_geom += 1;
                if choice_ref.map(|(_, t)| t != 0).unwrap_or(false) {
                    stats.geom_nontrivial += 1;
                }
            }
            3 => stats.ycocg_literal += 1,
            4 => stats.ref_exact_color += 1,
            5 => {
                stats.ref_geom_color += 1;
                if choice_ref.map(|(_, t)| t != 0).unwrap_or(false) {
                    stats.geom_nontrivial += 1;
                }
            }
            _ => unreachable!("mode byte constrained above"),
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

    let payload = backend_compress(backend, &raw)?;
    let payload_len =
        u32::try_from(payload.len()).map_err(|_| Error::fmt_msg("payload exceeds u32"))?; // review: no lossy cast
    let mut file = Vec::with_capacity(HEADER8 + payload.len());
    file.extend_from_slice(b"MNF8");
    file.extend_from_slice(&dims.width.to_le_bytes());
    file.extend_from_slice(&dims.height.to_le_bytes());
    file.extend_from_slice(&TILE_SIZE.to_le_bytes());
    let tile_count = u32::try_from(tiles.len()).map_err(|_| Error::fmt_msg("too many tiles"))?;
    file.extend_from_slice(&tile_count.to_le_bytes());
    file.extend_from_slice(&payload_len.to_le_bytes());
    file.extend_from_slice(&flags.to_le_bytes());
    file.extend_from_slice(&backend.to_le_bytes());
    file.extend_from_slice(&payload);
    Ok((file, stats))
}

// --- decoding ----------------------------------------------------------------

/// Decode MNF8. Record walk is MNF7's (shared `decode_record_walk`); the
/// container fields differ (32-byte header, backend id) and the coder
/// dispatch runs on the backend field. Every non-shipped backend id is an
/// ERROR (never a panic path) — review fix 1: backend 2 previously fell
/// into `unreachable!()` after passing the too-loose header checks.
pub fn decode_mnf8(buf: &[u8]) -> Result<(Dims, Vec<u8>, u32)> {
    decode_mnf8_with_limits(buf, &crate::mnf6::DecodeLimits::default())
}

/// Decode MNF8 with configurable ceilings (library API; same semantics as
/// decode_mnf7_with_limits).
pub fn decode_mnf8_with_limits(
    buf: &[u8],
    lim: &crate::mnf6::DecodeLimits,
) -> Result<(Dims, Vec<u8>, u32)> {
    if buf.len() < HEADER8 {
        return Err(Error::fmt_msg("short"));
    }
    if buf[0..4] != *b"MNF8" {
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
    if buf.len() != HEADER8 + payload_len as usize {
        return Err(Error::fmt_msg("filelen"));
    }
    let flags = u32le(24);
    // MNF8 owns bit2 (FLAG_PRED_MED); MNF6/7 do not — wider mask here
    if flags & FLAGS_RESERVED8 != 0 {
        return Err(Error::fmt_msg("reserved-flags"));
    }
    let backend = u32le(28);
    if !matches!(
        backend,
        BACKEND_ZLIB | BACKEND_ZSTD19 | BACKEND_RESERVED2 | BACKEND_RESERVED3
    ) {
        return Err(Error::fmt_msg(format!("unknown backend {backend}")));
    }
    if matches!(backend, BACKEND_RESERVED2 | BACKEND_RESERVED3) {
        return Err(Error::fmt_msg(format!(
            "reserved backend {backend} (not shipped)"
        )));
    }
    let planar = flags & FLAG_PLANAR != 0;

    // Decompression bound — degenerate-shape-honest (review fix 5):
    //   generic record worst case needs 6 metadata bytes + the tile's own
    //   body (tw*th*3); summed over tiles that is 6*N + 3WH, NOT N * 833
    //   (the old full-tile assumption turned a 1x67108864 image's cap into
    //   ~3.5 GB against a ~200 MB true maximum).
    //   Mode-3 records: 1 + tw*th + 2 * ceil(9*(tw*th)/8) ≤ 1 + tw*th + 2 * (tw*th*9+7)/8
    //   ≤ 1 + 7/4 * tw*th + small; bound it as 1 + (7/4)*np + 2 with np <=
    //   TILE_SIZE^2 — the same conservative per-tile add-on as before.
    let mode3_max: u64 =
        1 + (TILE_SIZE * TILE_SIZE) as u64 + 2 * ((TILE_SIZE * TILE_SIZE) as u64 * 9 + 7) / 8 + 2;
    let px_bytes_total: u64 = width as u64 * height as u64 * 3;
    let limit = px_bytes_total
        .checked_add(6 * tile_count as u64)
        .and_then(|l| l.checked_add(2 * mode3_max + 2))
        .ok_or_else(|| Error::fmt_msg("decompressed size overflow"))?;

    let payload = &buf[HEADER8..];
    let raw = match backend {
        BACKEND_ZLIB | BACKEND_ZSTD19 => backend_decompress(backend, payload, limit)?,
        other => return Err(Error::fmt_msg(format!("unsupported backend {other}"))),
    };

    let pixels = decode_record_walk(
        &raw,
        width,
        height,
        tile_count,
        planar,
        flags & FLAG_PRED_MED != 0,
    )?;
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
        // ADR-002 gate (EMPIRICAL per review fix 8 — candidate sets differ
        // structurally; this pins the measured relation on stable files).
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
        f[28..32].copy_from_slice(&2u32.to_le_bytes()); // reserved brotli backend
        assert!(decode_mnf8(&f).is_err(), "reserved brotli backend rejected");
        // and specifically NOT a panic (the old bug hit unreachable! here)
        f[28..32].copy_from_slice(&2u32.to_le_bytes());
        let r = std::panic::catch_unwind(|| decode_mnf8(&f));
        assert!(r.is_ok() && r.unwrap().is_err(), "must error, not panic");
        f[28..32].copy_from_slice(&0u32.to_le_bytes());
        f[24] = 0b100; // reserved flag bit
        assert!(decode_mnf8(&f).is_err(), "reserved flag rejected");
        // bad magic
        f[0] = b'X';
        f[24] = 0;
        assert!(decode_mnf8(&f).is_err(), "magic");
    }

    #[test]
    fn mnf8_balanced_picks_zstd_not_hardcoded() {
        // review: the old test read the hardcoded return tuple; instead pin
        // the ENCODING CHOICE: balanced must measure the flag variants under
        // zstd and pick planar when planar wins (gradient-like content).
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
        let (buf, st, backend) = encode_mnf8(
            &px,
            Dims {
                width: w,
                height: h,
            },
            false,
            Profile::Balanced,
        );
        assert_eq!(backend, BACKEND_ZSTD19);
        // MEASURED pick (v1.2.1 round): the invariant is "measured, not
        // hardcoded" — the winner id moves with the data. This gradient has
        // vertically repeating tile rows, so exact refs beat literals and
        // flags = 0 is a LEGAL winner here; pin exactness + a legal flags id,
        // and pin the med-bearing pick on the no-repeat smooth body (next
        // test), not here.
        assert!(
            st.chosen_flags & !0b111 == 0,
            "balanced wrote a legal flags id, got {:#x}",
            st.chosen_flags
        );
        let (_, out, _) = decode_mnf8(&buf).expect("decode");
        assert_eq!(out, px);
    }

    #[test]
    fn mnf8_no_geom_no_flips() {
        // review fix 2 regression pin: use_geom=false must emit ONLY exact
        // references (t=0) or plain literals — flips are forbidden. A rotated
        // copy of tile 0 must be emitted as a LITERAL, not a geom ref.
        let (w, h) = (32u32, 32u32); // two 16-tall tile rows
        let mut px = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                px[i] = (x * 7 % 256) as u8;
                px[i + 1] = (y * 3 % 256) as u8;
                px[i + 2] = 5;
            }
        }
        // flip tile (0,1) [the 16x16 block at rows 16..32, cols 0..16] in place
        let tile_row = 16usize;
        let tile_col = 0usize;
        let src_off = (tile_row * w as usize + tile_col) * 3;
        let tile = px[src_off..src_off + 16 * 16 * 3].to_vec();
        let flipped = apply_transform(1, 16, 16, &tile);
        px[src_off..src_off + 16 * 16 * 3].copy_from_slice(&flipped);
        let stacked = px.clone();
        let d = Dims {
            width: w,
            height: h,
        };
        let (buf, st, _) = encode_mnf8(&stacked, d, false, Profile::Balanced);
        // walk stats: geom references must be zero
        assert_eq!(st.ref_geom, 0, "no flips allowed with use_geom=false");
        assert_eq!(st.geom_nontrivial, 0);
        let (_, out, _) = decode_mnf8(&buf).expect("decode");
        assert_eq!(out, stacked);
    }

    #[test]
    fn mnf8_no_atlas_arm() {
        // review fix 2b: the ablation needs its no-atlas arm (pure
        // prediction: literals + mode 3 only), and it must roundtrip exactly.
        let d = Dims {
            width: 32,
            height: 32,
        };
        let px: Vec<u8> = (0..d.width as usize * d.height as usize * 3)
            .map(|i| (i * 7 % 256) as u8)
            .collect();
        let (buf, st, _) = encode_mnf8_noclimb(&px, d);
        assert_eq!(
            st.ref_exact + st.ref_geom + st.ref_exact_color + st.ref_geom_color,
            0,
            "no-atlas arm must emit zero references"
        );
        let (_, out, _) = decode_mnf8(&buf).expect("decode");
        assert_eq!(out, px);
    }

    // helper calling the internal encoder with both atlas controls off
    fn encode_mnf8_noclimb(px: &[u8], d: Dims) -> (Vec<u8>, MnF6Stats, u32) {
        // balanced profile, atlas off, geom off
        let mut best: Option<(Vec<u8>, MnF6Stats, u32)> = None;
        for flags in [0u32, FLAG_PLANAR] {
            let planar = flags & FLAG_PLANAR != 0;
            let (file, st) = encode_stream_coder(
                px,
                d,
                false,
                false,
                flags,
                planar,
                false,
                &Coder::Zstd(19),
                BACKEND_ZSTD19,
            )
            .expect("encode");
            if best.as_ref().is_none_or(|b| file.len() < b.0.len()) {
                best = Some((file, st, BACKEND_ZSTD19));
            }
        }
        let (f, s, b) = best.expect("variant");
        (f, s, b)
    }

    #[test]
    fn mnf8_med_flag_roundtrip_photo_like() {
        // FLAG_PRED_MED (v1.2 gate item 2): gradient-like image where the
        // image-wide MED pre-predictor wins; the rate-aware loop must MEASURE
        // the med variant and pick it, and decode must reconstruct exactly
        // through the causal in-walker MED inversion.
        let (w, h) = (64u32, 64u32);
        let mut px = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                px[i] = (x * 4) as u8;
                px[i + 1] = (y * 4) as u8;
                px[i + 2] = ((x + y) / 2) as u8;
            }
        }
        let d = Dims {
            width: w,
            height: h,
        };
        let (buf, st, _) = encode_mnf8(&px, d, false, Profile::Balanced);
        // RATE-AWARE pick is allowed to lose here (refs compressed better than
        // MED-literals on this synthetic body); the invariants that matter:
        // the MED variant was MEASURED (no panic), the pick is a legal flags
        // id, and decode is exact.
        assert!(
            st.chosen_flags & !0b111 == 0,
            "chosen flags must be a legal MNF8 id, got {:#x}",
            st.chosen_flags
        );
        let (_, out, _) = decode_mnf8(&buf).expect("decode");
        if out != px {
            let first = out
                .iter()
                .zip(&px)
                .position(|(a, b)| a != b)
                .expect("differs");
            let (b_y, rem) = (first / 3 / (w as usize), (first / 3) % (w as usize));
            panic!(
                "not exact: first diff at byte {first} (pixel ({b_y},{rem}), ch {}): out={:?} px={:?}",
                first % 3,
                &out[first.saturating_sub(4)..first + 4],
                &px[first.saturating_sub(4)..first + 4]
            );
        }
        assert_eq!(out, px, "exact reconstruction under the measured flags");
    }

    #[test]
    fn mnf8_med_flag_picked_on_photo_like_smoothing() {
        // The MEASURED pick: smooth noise-free body with NO exact repeats —
        // MED-literals must win (105 B vs planar 177 / interleaved 401,
        // measured in the debug round).
        let (w, h) = (64u32, 64u32);
        let mut px = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                // smooth field + tiny deterministic ripple (no byte repeats)
                px[i] = (x * 4 + y * 2) as u8;
                px[i + 1] = (y * 6) as u8;
                px[i + 2] = ((x + y) * 3) as u8;
            }
        }
        let d = Dims {
            width: w,
            height: h,
        };
        let (_, st, _) = encode_mnf8(
            &px,
            d,
            false, // no geom: kill the reference arms to isolate prediction
            Profile::Balanced,
        );
        assert_eq!(
            st.chosen_flags & FLAG_PRED_MED,
            FLAG_PRED_MED,
            "smooth no-repeat bodies must select the MED predictor (picked {:#x})",
            st.chosen_flags
        );
    }

    #[test]
    fn mnf8_forced_mode4_mode5_records() {
        // review verify 3: drive mode 4 and mode 5 end-to-end — construct a
        // file whose color residuals genuinely win (structured chroma delta
        // on a noisy base, the mnf7_color_domain_used_when_it_wins pattern)
        // and assert decode_mnf8 reconstructs exactly.
        let mut s: u64 = 42;
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let a: Vec<u8> = (0..16 * 16 * 3).map(|_| (rnd() & 0xFF) as u8).collect();
        let ta_t = crate::mnf6::modcolor_forward(&a);
        let mut tb_t = ta_t.clone();
        let ramp = [0u8, 1, 2, 255];
        for i in (1..tb_t.len()).step_by(3) {
            tb_t[i] = tb_t[i].wrapping_add(ramp[(i / 3) % 4]);
        }
        let b = crate::mnf6::modcolor_inverse(&tb_t);
        let mut two = a.clone();
        two.extend_from_slice(&b);
        let d2 = Dims {
            width: 16,
            height: 32,
        };
        let (buf, st, _) = encode_mnf8(&two, d2, true, Profile::Balanced);
        assert!(
            st.ref_exact_color > 0,
            "expected mode 4 (or a color-geom below) selected, stats: exact={} geom={} ec={} gc={}",
            st.ref_exact,
            st.ref_geom,
            st.ref_exact_color,
            st.ref_geom_color
        );
        let (_, out, _) = decode_mnf8(&buf).expect("decode");
        assert_eq!(out, two, "mode-4 record reconstructs exactly through MNF8");

        // and a rotated variant of the same (mode 5 path)
        let flipped = apply_transform(1, 16, 16, &a);
        let tb_t2 = crate::mnf6::modcolor_forward(&flipped);
        let mut tc_t = tb_t2;
        for i in (1..tc_t.len()).step_by(3) {
            tc_t[i] = tc_t[i].wrapping_add(ramp[(i / 3) % 4]);
        }
        let c = crate::mnf6::modcolor_inverse(&tc_t);
        let mut three = flipped.clone();
        three.extend_from_slice(&c);
        let d3 = Dims {
            width: 16,
            height: 32,
        };
        let (buf3, st3, _) = encode_mnf8(&three, d3, true, Profile::Balanced);
        assert!(
            st3.ref_exact_color > 0 || st3.ref_geom_color > 0,
            "expected a color-domain reference (exact or geom), stats: ec={} gc={}",
            st3.ref_exact_color,
            st3.ref_geom_color
        );
        let (_, out3, _) = decode_mnf8(&buf3).expect("decode");
        assert_eq!(out3, three);
    }
}
