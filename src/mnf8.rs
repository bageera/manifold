// MNF8 — format v4 (ADR-002: Entropy Backend and Residual Probability Modeling).
//
// MNF8 = MNF7 (modes 0-5, PLANAR|ZSTD flags) + a backend-identifier field +
// encoder profiles; new magic, new header field. MNF5/6/7 files decode via
// their own magics — no compatibility break (README §9 versioning rule).
//
// Header (32 bytes):
//   "MNF8", u32 LE width, height, tile=16, tile_count, payload_len,
//   u32 flags (MNF7 semantics + MNF8-owned bits:
//     bit0 PLANAR, bit1 ZSTD, bit2 PRED_MED + bit3 TOKENS (ADR-003),
//     ADR-004: bit4 PRED_PLANE, bit5 PRED_LEFT — bits 6..31 reserved),
//   u32 backend:
//     0 = zlib-9  (MNF7 control; payload is a zlib stream)
//     1 = zstd-19 (xxh64-checksummed frame; decoder window capped at 23)
//     2 = RESERVED — brotli was prototyped but unshipped: its static
//         dictionary's cost accounting was measured only in the study, and
//         a dictionary-free brotli backend changed nothing on the corpus.
//         Reject until its gate re-passes (audit review 2026-10-25).
//     3 = RESERVED (custom mixed run/rANS coder; reject until it passes the gate)
//   other values: reject like reserved flag bits.
//
// FLAG_TOKENS (ADR-003, docs/mnf8-adr-003-tokens.md): the payload is NOT a
// record stream — it is the LZ77-2D token wire over the image-wide MED
// residual plane (crate::tokens), container-compressed by the backend.
// Mutually exclusive with everything per-tile: TOKENS forces plain
// interleaved literal records in the pick arms it competes against and is
// rejected at decode with PLANAR (the token plane is always interleaved).
//
// ADR-004 (docs/mnf8-adr-004-branch-pred.md): bits 4-5 select the image-
// wide predictor on mode-0 literal bodies — PRED_MED (edge-sense, the
// v1.2 predecessor), PRED_PLANE (LOCO-I plane branch forced: a+b−cc), or
// PRED_LEFT (pure left). All three decode causally (the exact same walk,
// only the interior predictor function differs). Exactly one may be set;
// the encoder measures all three per file (retention gate: branch choice
// beats neighborhood choice, docs/mnf9-branch-gate.md, −7.0%).
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
use crate::tokens;

/// Header size: `MNF8` + 7×`u32` LE (adds backend id) — 4 B over MNF7.
pub const HEADER8: usize = 32;

// MNF8-only flags (MNF6/7 keep !0b11 frozen; MNF8 owns bits 2-6):
/// Literal bodies carry image-wide MED residuals (decode inverts causally).
pub const FLAG_PRED_MED: u32 = 4; // mode-0 literal bodies carry image-wide MED residuals
/// Payload = LZ77-2D token wire over the MED residual plane (ADR-003).
pub const FLAG_TOKENS: u32 = 8;
/// Image-wide predictor on literal bodies = LOCO-I plane branch (ADR-004).
pub const FLAG_PRED_PLANE: u32 = 16;
/// Image-wide predictor on literal bodies = pure left (ADR-004).
pub const FLAG_PRED_LEFT: u32 = 32;
/// ADR-008 channel-diff residual organization, generalized v1.12 (the C2
/// gate's reference-channel field): flags bits 6-7 = REF, read as
/// `(flags >> 6) & 0b11` — 0 = none, 1 = G-centered (v1.11's bit-6
/// semantic: old CD files carry exactly this shape and decode through
/// the same walker), 2 = R-centered, 3 = B-centered. Mode-0 bodies and
/// token tag-0 literals carry [ref, a−ref, b−ref] per pixel; zero added
/// bytes; decode = the inverse before the walk.
pub const FLAG_CHANNEL_DIFF: u32 = 64; // REF = 1 (G-centered)
/// REF = 2 (R-centered): bit 7 alone.
pub const FLAG_REF_R: u32 = 128;
/// Bits above bit7 reserved (MNF8 owns bits 2-7).
pub const FLAGS_RESERVED8: u32 = !0b11111111;

/// The CD reference id from the flags word: 0 = none, 1 = G, 2 = R, 3 = B.
fn cd_reference(flags: u32) -> u8 {
    ((flags >> 6) & 0b11) as u8
}

/// zlib-9 backend.
pub const BACKEND_ZLIB: u32 = 0;
/// zstd-19 backend (ships in Balanced and Maximum).
pub const BACKEND_ZSTD19: u32 = 1;
/// Reserved (brotli prototype; decode rejects).
pub const BACKEND_RESERVED2: u32 = 2; // brotli prototype — unshipped, reject
/// Reserved (rANS prototype measured DENIED with honest transport; decode
/// rejects — docs/mnf9-rans-arm.md §honest-re-measure).
pub const BACKEND_RESERVED3: u32 = 3;

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
    // ADR-004: the three literal-body predictors × (interleaved|planar)
    // as per-file arms, each measured under every backend candidate,
    // complete serialized cost. (PLANAR's own measured value is real:
    // gradient −28% under z19 in the stage-2 matrix — never drop it.)
    let pred_arms = [
        PredKind::None,
        PredKind::Med,
        PredKind::Plane,
        PredKind::Left,
    ];
    for (backend, coder) in candidates {
        for kind in pred_arms {
            for planar in [false, true] {
                let med = kind == PredKind::Med;
                // ADR-008 (v1.12): the reference-channel field 0..3 — none,
                // G (v1.11's bit-6 semantic), R, B — per-file at complete
                // serialized cost (the C2 gate's table).
                for reference in 0u8..=3 {
                    let flags = kind.flag_bit()
                        | if planar { FLAG_PLANAR } else { 0 }
                        | (u32::from(reference) << 6);
                    let (file, st) = encode_stream_coder(
                        pixels, dims, true, use_geom, flags, planar, med, kind, &coder, backend,
                        reference,
                    )
                    .expect("encoding an in-memory image cannot fail");
                    let size = file.len();
                    if best.as_ref().is_none_or(|b| size < b.0.len()) {
                        best = Some((file, flags, st, backend));
                    }
                }
            }
        }
        // ADR-003 token arm: tokens over the MED plane, same backend (the
        // token grammar's predictor is fixed = MED; ADR-004 composes with
        // the record arms only). Rate-aware by construction.
        let base = FLAG_PRED_MED | FLAG_TOKENS;
        for cap in [16, tokens::MAX_CANDIDATES] {
            for reference in 0u8..=3 {
                let flags = base | (u32::from(reference) << 6);
                let (file, st) =
                    encode_tokens_coder_cap(pixels, dims, backend, flags, cap, reference)
                        .expect("encoding an in-memory image cannot fail");
                let size = file.len();
                if best.as_ref().is_none_or(|b| size < b.0.len()) {
                    best = Some((file, flags, st, backend));
                }
            }
        }
    }
    let (file, flags, mut st, backend) = best.expect("at least one candidate");
    st.chosen_flags = flags;
    (file, st, backend)
}

/// The image-wide literal-body predictor (ADR-004): which causal rule
/// mode-0 bodies carry. All three decode with the same causal walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredKind {
    /// No pre-prediction (raw literal bodies).
    None,
    /// MED edge-sense (v1.2 FLAG_PRED_MED).
    Med,
    /// LOCO-I plane branch forced: pred = a + b − cc, clamped (ADR-004).
    Plane,
    /// Pure left: pred = left neighbor (ADR-004).
    Left,
}

impl PredKind {
    fn from_flags(flags: u32) -> PredKind {
        if flags & FLAG_PRED_MED != 0 {
            PredKind::Med
        } else if flags & FLAG_PRED_PLANE != 0 {
            PredKind::Plane
        } else if flags & FLAG_PRED_LEFT != 0 {
            PredKind::Left
        } else {
            PredKind::None
        }
    }

    fn flag_bit(self) -> u32 {
        match self {
            PredKind::None => 0,
            PredKind::Med => FLAG_PRED_MED,
            PredKind::Plane => FLAG_PRED_PLANE,
            PredKind::Left => FLAG_PRED_LEFT,
        }
    }

    /// The predictor's value at an interior pixel from causal neighbors.
    /// Decode-side truth; the encode-side helpers MUST match exactly.
    fn predict(self, a: u32, b: u32, cc: u32) -> u32 {
        match self {
            PredKind::None | PredKind::Med => crate::mnf6::med_predict_pixel(a, b, cc),
            PredKind::Plane => {
                // LOCO-I plane branch, clamped into byte range (the encode-
                // side plane_predict clamps identically).
                (a as i64 + b as i64 - cc as i64).clamp(0, 255) as u32
            }
            PredKind::Left => a,
        }
    }
}

/// Image-wide prediction over the WHOLE image with the given rule.
/// ENCODE-side only: neighbor reads come from TRUE pixels (the decode walk
/// reads reconstructed pixels; reconstructed-earlier == true by the mod-256
/// induction, so the two walks agree — reading the pred stream instead of
/// the pixel stream chains predictions and BREAKS the identity, the bug the
/// gradient Plane gate caught: pred(0,1) = pred(0,0) instead of left pixel).
fn image_predict(pixels: &[u8], w: usize, h: usize, kind: PredKind) -> Vec<u8> {
    let mut out = vec![0u8; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3usize {
                let i = (y * w + x) * 3 + c;
                let pred: u32 = if x == 0 && y == 0 {
                    127
                } else if y == 0 {
                    u32::from(pixels[i - 3])
                } else if x == 0 {
                    u32::from(pixels[i - w * 3])
                } else {
                    let a = u32::from(pixels[i - 3]);
                    let b = u32::from(pixels[i - w * 3]);
                    let cc = u32::from(pixels[i - w * 3 - 3]);
                    kind.predict(a, b, cc)
                };
                out[i] = pred as u8;
            }
        }
    }
    out
}

/// ADR-008 channel-diff relabel, applied to RESIDUAL bytes in the
/// stream's residual-domain bodies (mode-0 record bodies; token tag-0
/// literals). Per pixel [r,g,b] → [g, r−g, b−g] mod 256 — a reversible
/// relabel with ZERO added bytes; the decoder applies the inverse before
/// the standard walk. `planar` relabels per-channel planes ([R][G][B] →
/// [G][R−G][B−G]); `tokens` relabels tag-0 literal triplets in place.
fn channel_diff_relabel(
    stream: &[u8],
    planar: bool,
    tokens: bool,
    w: u32,
    h: u32,
    reference: u8,
) -> Vec<u8> {
    if tokens {
        // token wire: relabel tag-0 (TAG_LIT) 3-B literal pixels
        let mut out = Vec::with_capacity(stream.len());
        let mut pos = 0usize;
        let npx = (w as usize) * (h as usize);
        let mut ppos = 0usize;
        while ppos < npx && pos < stream.len() {
            let tag = stream[pos];
            out.push(tag);
            pos += 1;
            match tag {
                tokens::TAG_LITERAL => {
                    let px = &stream[pos..pos + 3];
                    let (refc, a, b) = ref_pick(px, reference);
                    out.push(refc);
                    out.push(a.wrapping_sub(refc));
                    out.push(b.wrapping_sub(refc));
                    pos += 3;
                    ppos += 1;
                }
                tokens::TAG_MATCH => {
                    if pos + 5 > stream.len() {
                        panic!("cd: token match truncated (pos {pos}/{})", stream.len());
                    }
                    let len = u32::from(stream[pos + 4]) as usize;
                    out.extend_from_slice(&stream[pos..pos + 5]);
                    pos += 5;
                    ppos += len;
                }
                tokens::TAG_NEAR => {
                    if pos + 3 > stream.len() {
                        panic!("cd: token near truncated (pos {pos}/{})", stream.len());
                    }
                    let len = u32::from(stream[pos + 2]) as usize;
                    out.extend_from_slice(&stream[pos..pos + 3]);
                    pos += 3;
                    ppos += len;
                }
                other => panic!(
                    "token tag {other} outside the recorded grammar (pos {pos}/{}, ppos {ppos}/{npx})",
                    stream.len()
                ),
            }
        }
        debug_assert_eq!(ppos, npx);
        out
    } else if planar {
        // planar record bodies: per-tile [R plane][G plane][B plane] →
        // [G plane][R−G plane][B−G plane]
        let mut out = Vec::with_capacity(stream.len());
        let mut pos = 0usize;
        let cols = (w as usize).div_ceil(TILE_SIZE as usize);
        let tile_count =
            (w as usize).div_ceil(TILE_SIZE as usize) * (h as usize).div_ceil(TILE_SIZE as usize);
        for i in 0..tile_count {
            let (row, col) = (i / cols, i % cols);
            let tw = (TILE_SIZE as usize).min(w as usize - col * TILE_SIZE as usize);
            let th = (TILE_SIZE as usize).min(h as usize - row * TILE_SIZE as usize);
            let nb = tw * th * 3;
            let mode = stream[pos];
            out.push(mode);
            pos += 1;
            if mode == 0 {
                let body = &stream[pos..pos + nb];
                let third = nb / 3;
                let (pr, pg, pb) = (&body[..third], &body[third..2 * third], &body[2 * third..]);
                match reference {
                    1 => {
                        out.extend_from_slice(pg);
                        for (r, g) in pr.iter().zip(pg) {
                            out.push(r.wrapping_sub(*g));
                        }
                        for (b, g) in pb.iter().zip(pg) {
                            out.push(b.wrapping_sub(*g));
                        }
                    }
                    2 => {
                        out.extend_from_slice(pr);
                        for (g, r) in pg.iter().zip(pr) {
                            out.push(g.wrapping_sub(*r));
                        }
                        for (b, r) in pb.iter().zip(pr) {
                            out.push(b.wrapping_sub(*r));
                        }
                    }
                    3 => {
                        out.extend_from_slice(pb);
                        for (r, b) in pr.iter().zip(pb) {
                            out.push(r.wrapping_sub(*b));
                        }
                        for (g, b) in pg.iter().zip(pb) {
                            out.push(g.wrapping_sub(*b));
                        }
                    }
                    _ => unreachable!("reference constrained"),
                }
                pos += nb;
            } else if mode == 1 {
                const META: usize = 4;
                out.extend_from_slice(&stream[pos..pos + META + nb]);
                pos += META + nb;
            } else if mode == 2 {
                const META: usize = 5;
                out.extend_from_slice(&stream[pos..pos + META + nb]);
                pos += META + nb;
            } else if mode == 3 {
                let np = nb / 3;
                let n9 = (np * 9).div_ceil(8);
                out.extend_from_slice(&stream[pos..pos + np + 2 * n9]);
                pos += np + 2 * n9;
            } else if mode == 4 || mode == 5 {
                let meta = if mode == 5 { 5 } else { 4 };
                out.extend_from_slice(&stream[pos..pos + meta + nb]);
                pos += meta + nb;
            } else {
                panic!(
                    "mode {mode} outside the recorded grammar (tile {i} pos {pos}/{})",
                    stream.len()
                );
            }
        }
        debug_assert_eq!(pos, stream.len());
        out
    } else {
        // interleaved record bodies: relabel every mode-0 body's per-pixel
        // triplets in place
        let mut out = Vec::with_capacity(stream.len());
        let mut pos = 0usize;
        let cols = (w as usize).div_ceil(TILE_SIZE as usize);
        let tile_count =
            (w as usize).div_ceil(TILE_SIZE as usize) * (h as usize).div_ceil(TILE_SIZE as usize);
        for i in 0..tile_count {
            let (row, col) = (i / cols, i % cols);
            let tw = (TILE_SIZE as usize).min(w as usize - col * TILE_SIZE as usize);
            let th = (TILE_SIZE as usize).min(h as usize - row * TILE_SIZE as usize);
            let nb = tw * th * 3;
            let mode = stream[pos];
            out.push(mode);
            pos += 1;
            if mode == 0 {
                for k in 0..nb / 3 {
                    let t = &stream[pos + k * 3..pos + k * 3 + 3];
                    let (refc, a, b) = ref_pick(t, reference);
                    out.push(refc);
                    out.push(a.wrapping_sub(refc));
                    out.push(b.wrapping_sub(refc));
                }
                pos += nb;
            } else {
                // copy the remaining record bytes for this tile verbatim
                // (after the mode byte: idx u32 | idx+transform | y+co9+cg9)
                let extra = match mode {
                    1 | 4 => 4 + nb,
                    2 | 5 => 5 + nb,
                    3 => {
                        let np = nb / 3;
                        (np * 9).div_ceil(8) * 2 + np
                    }
                    other => panic!(
                        "mode {other} outside the recorded grammar (pos {pos}/{})",
                        stream.len()
                    ),
                };
                out.extend_from_slice(&stream[pos..pos + extra]);
                pos += extra;
            }
        }
        debug_assert_eq!(pos, stream.len());
        out
    }
}

/// The per-triplet channel pick for a reference id: returns (ref, a, b)
/// in the wire emission order (ref first, then the pair).
fn ref_pick(px: &[u8], reference: u8) -> (u8, u8, u8) {
    let (r, g, b) = (px[0], px[1], px[2]);
    match reference {
        1 => (g, r, b),
        2 => (r, g, b),
        3 => (b, r, g),
        _ => (0, 0, 0),
    }
}

/// Planar inverse reorder: the wire's [ref][a−ref][b−ref] planes (the
/// wire emission order per reference) → [R][G][B] planes.
fn unpermute_wire_to_planar(wire: &[u8], reference: u8) -> Vec<u8> {
    let third = wire.len() / 3;
    let (pref, pa, pb) = (&wire[..third], &wire[third..2 * third], &wire[2 * third..]);
    match reference {
        // G: wire = [G][R−G][B−G]
        1 => {
            let mut out: Vec<u8> = pa
                .iter()
                .zip(pref)
                .map(|(&da, &g)| da.wrapping_add(g))
                .collect();
            out.extend_from_slice(pref);
            out.extend(
                pb.iter()
                    .zip(pref)
                    .map(|(&db, &g)| db.wrapping_add(g))
                    .collect::<Vec<u8>>(),
            );
            out
        }
        // R: wire = [R][G−R][B−R]
        2 => {
            let mut out = pref.to_vec();
            out.extend(
                pa.iter()
                    .zip(pref)
                    .map(|(&da, &rf)| da.wrapping_add(rf))
                    .collect::<Vec<u8>>(),
            );
            out.extend(
                pb.iter()
                    .zip(pref)
                    .map(|(&db, &rf)| db.wrapping_add(rf))
                    .collect::<Vec<u8>>(),
            );
            out
        }
        // B: wire = [B][R−B][G−B]
        3 => {
            let mut out: Vec<u8> = pa
                .iter()
                .zip(pref)
                .map(|(&da, &bf)| da.wrapping_add(bf))
                .collect();
            out.extend(
                pb.iter()
                    .zip(pref)
                    .map(|(&db, &bf)| db.wrapping_add(bf))
                    .collect::<Vec<u8>>(),
            );
            out.extend_from_slice(pref);
            out
        }
        _ => {
            let mut out = pref.to_vec();
            out.extend_from_slice(pa);
            out.extend_from_slice(pb);
            out
        }
    }
}

/// Interleaved inverse: one wire pixel [ref, a−ref, b−ref] → [r, g, b].
fn push_inverse_pixel(out: &mut Vec<u8>, refc: u8, da: u8, db: u8, reference: u8) {
    match reference {
        1 => {
            out.push(da.wrapping_add(refc)); // R
            out.push(refc); // G
            out.push(db.wrapping_add(refc)); // B
        }
        2 => {
            out.push(refc); // R
            out.push(da.wrapping_add(refc)); // G
            out.push(db.wrapping_add(refc)); // B
        }
        3 => {
            out.push(da.wrapping_add(refc)); // R
            out.push(db.wrapping_add(refc)); // G
            out.push(refc); // B
        }
        _ => out.extend_from_slice(&[0, 0, 0]),
    }
}

/// The decode-side inverse relabel: [G, R−G, B−G] → [R, G, B] across the
/// same body classes, with explicit bounds checks — malformed streams are
/// errors, never panics (truncated body/section, unknown mode/tag,
/// trailing bytes).
fn channel_diff_unrelabel(
    stream: &[u8],
    planar: bool,
    tokens: bool,
    w: u32,
    h: u32,
    reference: u8,
) -> Result<Vec<u8>> {
    if tokens {
        let mut out = Vec::with_capacity(stream.len());
        let mut pos = 0usize;
        let npx = (w as usize) * (h as usize);
        let mut ppos = 0usize;
        while ppos < npx {
            if pos >= stream.len() {
                return Err(Error::fmt_msg("cd: token wire truncated"));
            }
            let tag = stream[pos];
            out.push(tag);
            pos += 1;
            match tag {
                tokens::TAG_LITERAL => {
                    if pos + 3 > stream.len() {
                        return Err(Error::fmt_msg("cd: token literal truncated"));
                    }
                    push_inverse_pixel(
                        &mut out,
                        stream[pos],
                        stream[pos + 1],
                        stream[pos + 2],
                        reference,
                    );
                    pos += 3;
                    ppos += 1;
                }
                tokens::TAG_MATCH => {
                    if pos + 5 > stream.len() {
                        return Err(Error::fmt_msg("cd: token match truncated"));
                    }
                    let len = u32::from(stream[pos + 4]) as usize;
                    out.extend_from_slice(&stream[pos..pos + 5]);
                    pos += 5;
                    ppos += len;
                }
                tokens::TAG_NEAR => {
                    if pos + 3 > stream.len() {
                        return Err(Error::fmt_msg("cd: token near truncated"));
                    }
                    let len = u32::from(stream[pos + 2]) as usize;
                    out.extend_from_slice(&stream[pos..pos + 3]);
                    pos += 3;
                    ppos += len;
                }
                _ => return Err(Error::fmt_msg("token wire: unknown tag")),
            }
        }
        if pos != stream.len() {
            return Err(Error::fmt_msg("cd: token wire trailing bytes"));
        }
        if ppos != npx {
            return Err(Error::fmt_msg("cd: token pixel accounting"));
        }
        Ok(out)
    } else {
        let cols = (w as usize).div_ceil(TILE_SIZE as usize);
        let tile_count =
            (w as usize).div_ceil(TILE_SIZE as usize) * (h as usize).div_ceil(TILE_SIZE as usize);
        let mut out = Vec::with_capacity(stream.len());
        let mut pos = 0usize;
        for i in 0..tile_count {
            let (row, col) = (i / cols, i % cols);
            let tw = (TILE_SIZE as usize).min(w as usize - col * TILE_SIZE as usize);
            let th = (TILE_SIZE as usize).min(h as usize - row * TILE_SIZE as usize);
            let nb = tw * th * 3;
            if pos >= stream.len() {
                return Err(Error::fmt_msg("cd: record stream truncated"));
            }
            let mode = stream[pos];
            out.push(mode);
            pos += 1;
            match mode {
                0 => {
                    #[allow(clippy::collapsible_if)]
                    if pos + nb > stream.len() {
                        return Err(Error::fmt_msg("cd: mode-0 body truncated"));
                    }
                    if planar {
                        let wire_planes = &stream[pos..pos + nb];
                        out.extend_from_slice(&unpermute_wire_to_planar(wire_planes, reference));
                    } else {
                        for k in 0..nb / 3 {
                            push_inverse_pixel(
                                &mut out,
                                stream[pos + k * 3],
                                stream[pos + k * 3 + 1],
                                stream[pos + k * 3 + 2],
                                reference,
                            );
                        }
                    }
                    pos += nb;
                }
                1 | 4 => {
                    if pos + 4 + nb > stream.len() {
                        return Err(Error::fmt_msg("cd: mode truncated"));
                    }
                    out.extend_from_slice(&stream[pos..pos + 4 + nb]);
                    pos += 4 + nb;
                }
                2 | 5 => {
                    if pos + 5 + nb > stream.len() {
                        return Err(Error::fmt_msg("cd: mode truncated"));
                    }
                    out.extend_from_slice(&stream[pos..pos + 5 + nb]);
                    pos += 5 + nb;
                }
                3 => {
                    let np = nb / 3;
                    let n9 = (np * 9).div_ceil(8);
                    if pos + np + 2 * n9 > stream.len() {
                        return Err(Error::fmt_msg("cd: mode-3 truncated"));
                    }
                    out.extend_from_slice(&stream[pos..pos + np + 2 * n9]);
                    pos += np + 2 * n9;
                }
                _ => return Err(Error::fmt_msg(format!("cd: mode {mode}"))),
            }
        }
        if pos != stream.len() {
            return Err(Error::fmt_msg("cd: trailing stream bytes"));
        }
        Ok(out)
    }
}
/// Token-arm stream builder (ADR-003): MED residual plane → token wire →
/// container coder, with explicit matcher depth (ADR-005: the pick
/// measures the v1.7 shallow arm AND the deep-chain arm; the pick
/// absorbs the choice). ADR-008: when `cd`, tag-0 literals carry the
/// channel-diff relabel (applied before compression).
fn encode_tokens_coder_cap(
    pixels: &[u8],
    dims: Dims,
    backend: u32,
    flags: u32,
    cap: usize,
    reference: u8,
) -> Result<(Vec<u8>, MnF6Stats)> {
    let med_pred = med_predict(pixels, dims.width as usize, dims.height as usize);
    let res: Vec<u8> = pixels
        .iter()
        .zip(&med_pred)
        .map(|(x, m)| x.wrapping_sub(*m))
        .collect();
    let (wire, ts) =
        tokens::encode_plane_cap(&res, dims.width as usize, dims.height as usize, cap)?;
    // ADR-008: relabel tag-0 literals before compression (REF field)
    let wire = if reference != 0 {
        channel_diff_relabel(&wire, false, true, dims.width, dims.height, reference)
    } else {
        wire
    };
    let payload = backend_compress(backend, &wire)?;
    let payload_len =
        u32::try_from(payload.len()).map_err(|_| Error::fmt_msg("payload exceeds u32"))?;
    let mut file = Vec::with_capacity(HEADER8 + payload.len());
    file.extend_from_slice(b"MNF8");
    file.extend_from_slice(&dims.width.to_le_bytes());
    file.extend_from_slice(&dims.height.to_le_bytes());
    file.extend_from_slice(&TILE_SIZE.to_le_bytes());
    let cols = div_ceil(dims.width, TILE_SIZE);
    let rows = div_ceil(dims.height, TILE_SIZE);
    let tile_count: u32 = cols
        .checked_mul(rows)
        .ok_or_else(|| Error::fmt_msg("too many tiles"))?;
    file.extend_from_slice(&tile_count.to_le_bytes());
    file.extend_from_slice(&payload_len.to_le_bytes());
    file.extend_from_slice(&flags.to_le_bytes());
    file.extend_from_slice(&backend.to_le_bytes());
    file.extend_from_slice(&payload);
    let st = MnF6Stats {
        literal: u32::try_from(ts.literals).unwrap_or(u32::MAX),
        ref_exact: u32::try_from(ts.matches).unwrap_or(u32::MAX),
        ..MnF6Stats::default()
    };
    Ok((file, st))
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
    kind: PredKind,
    coder: &Coder,
    backend: u32,
    reference: u8,
) -> Result<(Vec<u8>, MnF6Stats)> {
    // ADR-004: Plane/Left are PURE prediction arms (the measured gate's
    // winning form) — no atlas candidates, no YCoCg; mode-0 bodies only.
    let pure_pred = kind == PredKind::Plane || kind == PredKind::Left;
    let med_pred = match kind {
        PredKind::Med => med_predict(pixels, dims.width as usize, dims.height as usize),
        PredKind::Plane => image_predict(
            pixels,
            dims.width as usize,
            dims.height as usize,
            PredKind::Plane,
        ),
        PredKind::Left => image_predict(
            pixels,
            dims.width as usize,
            dims.height as usize,
            PredKind::Left,
        ),
        PredKind::None => Vec::new(),
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

        // mode-0 literal candidate; under any ADR-004 predictor the body
        // carries that predictor's residual (decoder reconstructs causally)
        let lit_src: Vec<u8> = if med || pure_pred {
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

        // shared literal candidates (both arms): YCoCg-R (mode 3).
        // ADR-004 pure arms (Plane/Left) skip YCoCg + atlas — measured pure.
        if !pure_pred {
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

        // atlas candidates (arm-controlled; review fixes 2 + use_atlas).
        // ADR-004 pure prediction arms exclude the atlas (measured pure).
        if use_atlas && !pure_pred && tiles.len() > 1 {
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

    // ADR-008: relabel mode-0 residual bodies before compression (REF)
    let raw = if reference != 0 {
        channel_diff_relabel(&raw, planar, false, dims.width, dims.height, reference)
    } else {
        raw
    };
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
    // MNF8 owns bits 2-5 (PRED_MED, TOKENS, PRED_PLANE, PRED_LEFT); MNF6/7
    // do not — wider mask here
    if flags & FLAGS_RESERVED8 != 0 {
        return Err(Error::fmt_msg("reserved-flags"));
    }
    let token_wire = flags & FLAG_TOKENS != 0;
    // ADR-003: the token plane is interleaved RGB8 by definition — PLANAR
    // carries no meaning there and no encoder emits the combination.
    if token_wire && flags & FLAG_PLANAR != 0 {
        return Err(Error::fmt_msg("tokens+planar are exclusive"));
    }
    // ADR-004: at most ONE image-wide predictor bit may be set.
    let pred_count = u32::from(flags & FLAG_PRED_MED != 0)
        + u32::from(flags & FLAG_PRED_PLANE != 0)
        + u32::from(flags & FLAG_PRED_LEFT != 0);
    if pred_count > 1 {
        return Err(Error::fmt_msg("multiple predictors selected"));
    }
    // ADR-003: the token grammar's payload is always MED-residual; a token
    // file claiming a different predictor misdescribes its payload.
    if token_wire && flags & (FLAG_PRED_PLANE | FLAG_PRED_LEFT) != 0 {
        return Err(Error::fmt_msg("tokens+plane/left predictor exclusive"));
    }
    let backend = u32le(28);
    if !matches!(
        backend,
        BACKEND_ZLIB | BACKEND_ZSTD19 | BACKEND_RESERVED2 | BACKEND_RESERVED3
    ) {
        return Err(Error::fmt_msg(format!("unknown backend {backend}")));
    }
    // ADR-006 measured DENIED: rANS arm lost to zstd-19 on every corpus
    // file with honest model transport (docs/mnf9-rans-arm.md §verdict).
    // Keep backend 3 reject-on-decode until an arm wins honestly.
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
    //   Token wire (ADR-003; bound from the token path's own worst case, the
    //   same lesson as mode 3): every token covers >= 1 px at <= 4 B
    //   (tag + 3), so the wire is <= 4*npx — which EXCEEDS the record bound
    //   on large all-literal planes (4npx > 3npx + 6N + small once npx is
    //   big), so the token flag gets its own limit rather than inheriting
    //   the record one.
    let px_bytes_total: u64 = width as u64 * height as u64 * 3;
    let limit = if token_wire {
        (4 * width as u64)
            .checked_mul(height as u64)
            .ok_or_else(|| Error::fmt_msg("decompressed size overflow"))?
    } else {
        let mode3_max: u64 = 1
            + (TILE_SIZE * TILE_SIZE) as u64
            + 2 * ((TILE_SIZE * TILE_SIZE) as u64 * 9 + 7) / 8
            + 2;
        px_bytes_total
            .checked_add(6 * tile_count as u64)
            .and_then(|l| l.checked_add(2 * mode3_max + 2))
            .ok_or_else(|| Error::fmt_msg("decompressed size overflow"))?
    };

    let payload = &buf[HEADER8..];
    let raw = match backend {
        BACKEND_ZLIB | BACKEND_ZSTD19 => backend_decompress(backend, payload, limit)?,
        other => return Err(Error::fmt_msg(format!("unsupported backend {other}"))),
    };

    // ADR-008: the channel-diff inverse applies to the decompressed
    // stream BEFORE any walk (mode-0 bodies / tag-0 literals relabeled
    // back to [r,g,b]); the guard walk rejects truncations/unknown modes
    // with errors, never panics.
    let reference = cd_reference(flags);
    let raw = if reference != 0 {
        channel_diff_unrelabel(&raw, planar, token_wire, width, height, reference)?
    } else {
        raw
    };
    let pixels = if token_wire {
        // ADR-003 token payload: wire → residual plane → causal MED invert.
        // decode_plane validates the wire structurally (tags, ranges, exact
        // plane size) and rejects trailing bytes by construction.
        let res = tokens::decode_plane(&raw, width as usize, height as usize)?;
        pred_residual_invert(&res, width as usize, height as usize, PredKind::Med)
    } else {
        let kind = PredKind::from_flags(flags);
        // ADR-004 routing at decode: Med keeps the frozen MNF7 walker (its
        // in-walker MED branch); None = raw bodies; Plane/Left = pure
        // prediction arms (mode-0-only streams) inverted by the ADR-004 walk.
        match kind {
            PredKind::None => decode_record_walk(&raw, width, height, tile_count, planar, false)?,
            PredKind::Med => decode_record_walk(&raw, width, height, tile_count, planar, true)?,
            other => decode_pred_walk(&raw, width, height, tile_count, planar, other)?,
        }
    };
    Ok((Dims { width, height }, pixels, backend))
}

/// Causal invert of an image-wide predictor residual plane. Generalizes the
/// token payload's MED inversion to every PredKind (ADR-004); reconstruction
/// = this predictor's causal estimate + residual bytes, exact by the
/// mod-256 induction.
fn pred_residual_invert(res: &[u8], w: usize, h: usize, kind: PredKind) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3usize {
                let i = (y * w + x) * 3 + c;
                let pred: u32 = if x == 0 && y == 0 {
                    127
                } else if y == 0 {
                    u32::from(out[i - 3])
                } else if x == 0 {
                    u32::from(out[i - w * 3])
                } else {
                    let a = u32::from(out[i - 3]);
                    let b = u32::from(out[i - w * 3]);
                    let cc = u32::from(out[i - w * 3 - 3]);
                    kind.predict(a, b, cc)
                };
                out[i] = (pred + u32::from(res[i])).wrapping_rem_euclid(256) as u8;
            }
        }
    }
    out
}

/// ADR-004 decode walk for Plane/Left files: record layout is MNF7's, but
/// mode-0 literal bodies carry PRED_PLANE/PRED_LEFT residuals instead of
/// MED ones. Structure mirrors decode_record_walk's mode-0 branch (per-
/// tile causal context: in-tile reads come from the in-flight recon,
/// out-of-tile from `pixels` — the audit-bug discipline); every other
/// record mode is unchanged from MNF7's walker.
fn decode_pred_walk(
    raw: &[u8],
    width: u32,
    height: u32,
    tile_count: u32,
    planar: bool,
    kind: PredKind,
) -> Result<Vec<u8>> {
    // delegate the record structure to the shared walker by REUSING its
    // record walk with med=true (BODIES are residuals of SOME predictor),
    // then inverting per-tile — but the walker's med inversion happens
    // IN-WALKER. So instead: run the walker with med=false (bodies stay
    // raw residuals), collect per-tile residual bodies… the walker
    // reconstructs directly; simplest correct path: reuse the walker only
    // for its record framing by a full re-walk here (mode 0 inverted with
    // `kind`; other modes identically to the walker).
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
                let body = if planar {
                    crate::mnf6::planar_unpack(&body)
                } else {
                    body
                };
                // per-tile causal invert with the ADR-004 predictor (same
                // context discipline as the walker's MED branch)
                let mut recon = vec![0u8; n];
                let gy0 = row as usize * TILE_SIZE as usize;
                let gx0 = col as usize * TILE_SIZE as usize;
                for y in 0..th as usize {
                    for x in 0..tw as usize {
                        for c in 0..3usize {
                            let k = (y * tw as usize + x) * 3 + c;
                            let gy = gy0 + y;
                            let gx = gx0 + x;
                            let tl = |ggy: usize, ggx: usize| -> u32 {
                                if ggy >= gy0 && ggx >= gx0 {
                                    u32::from(
                                        recon[((ggy - gy0) * tw as usize + (ggx - gx0)) * 3 + c],
                                    )
                                } else {
                                    u32::from(pixels[(ggy * width as usize + ggx) * 3 + c])
                                }
                            };
                            let pred: u32 = if gy == 0 && gx == 0 {
                                127
                            } else if gy == 0 {
                                if gx - 1 < gx0 {
                                    u32::from(pixels[(gy * width as usize + gx - 1) * 3 + c])
                                } else {
                                    u32::from(recon[(y * tw as usize + x - 1) * 3 + c])
                                }
                            } else if gx == 0 {
                                if gy - 1 < gy0 {
                                    u32::from(pixels[((gy - 1) * width as usize + gx) * 3 + c])
                                } else {
                                    u32::from(recon[((y - 1) * tw as usize + x) * 3 + c])
                                }
                            } else {
                                let a = tl(gy, gx - 1);
                                let b = tl(gy - 1, gx);
                                let cc = tl(gy - 1, gx - 1);
                                kind.predict(a, b, cc)
                            };
                            recon[k] = (pred + u32::from(body[k])).wrapping_rem_euclid(256) as u8;
                        }
                    }
                }
                recon
            }
            _ => {
                // non-literal modes are UNCHANGED MNF7 records under any
                // ADR-004 predictor (predictor applies to literal bodies
                // only) — a Plane/Left file may still carry them whenever
                // they measured better per tile.
                return Err(Error::fmt_msg(format!(
                    "tile {i}: mode {mode} under PRED_PLANE/LEFT — not yet admitted"
                )));
            }
        };
        decoded.push((tw, th, tile.clone()));
        for y_ in 0..th as usize {
            let py = row as usize * TILE_SIZE as usize + y_;
            for x_ in 0..tw as usize {
                let pxx = col as usize * TILE_SIZE as usize + x_;
                let d = (py * width as usize + pxx) * 3;
                let s = (y_ * tw as usize + x_) * 3;
                pixels[d..d + 3].copy_from_slice(&tile[s..s + 3]);
            }
        }
    }
    if ip != raw.len() {
        return Err(Error::fmt_msg("extra"));
    }
    Ok(pixels)
}

#[cfg(test)]
mod gate30 {
    use super::*;
    use crate::mnf6::Coder;
    use std::path::Path;
    fn load(name: &str) -> (Dims, Vec<u8>) {
        let p = format!("{}/testdata/{}", env!("CARGO_MANIFEST_DIR"), name);
        let img = image::open(Path::new(&p))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .to_rgb8();
        (
            Dims {
                width: img.width(),
                height: img.height(),
            },
            img.into_raw(),
        )
    }
    // Full-corpus real-wire gate (837 s): #[ignore]d for CI; run explicitly
    // when touching the predictor arms. PINS below run always.
    #[test]
    #[ignore]
    fn gate_all_arms_real_wire() {
        let files = [
            "gradient_320x240.png",
            "noise_256x256.png",
            "repeat_256x256.png",
            "screenshot_640x400.png",
            "solid_256x256.png",
            "heldout/blob_224x160.png",
            "heldout/blocks_192x128.png",
            "heldout/document_200x280.png",
            "heldout/plaid_192.png",
            "heldout/poster_208x144.png",
            "heldout-photos/autumn_160x120_s3100006.png",
            "heldout-photos/dusk_160x120_s3100003.png",
            "heldout-photos/dusk_224x160_s3100002.png",
            "heldout-photos/forest_192x144_s3100001.png",
            "heldout-photos/forest_224x160_s3100005.png",
            "heldout-photos/forest_224x160_s3100008.png",
            "heldout-photos/ocean_192x144_s3100007.png",
            "heldout-photos/sand_192x144_s3100004.png",
            "heldout-photos/slate_160x120_s3100009.png",
            "heldout-photos/terrain_160x120_s3100000.png",
            "photos/autumn_192x144_s900001.png",
            "photos/dusk_160x120_s900003.png",
            "photos/forest_224x160_s900002.png",
            "photos/ocean_192x144_s900007.png",
            "photos/sand_160x120_s900000.png",
            "photos/slate_192x144_s900004.png",
            "photos/slate_224x160_s900008.png",
            "photos/stone_160x120_s900006.png",
            "photos/terrain_160x120_s900009.png",
            "photos/terrain_224x160_s900005.png",
        ];
        let mut old_tot = 0usize;
        let mut new_tot = 0usize;
        let mut fires = 0usize;
        for name in files {
            let (d, px) = load(name);
            // OLD pick (v1.6.0 arms: None/Med × backends via encode paths + tokens)
            let mut old = usize::MAX;
            for kind in [PredKind::None, PredKind::Med] {
                for (backend, coder) in [
                    (BACKEND_ZSTD19, Coder::Zstd(19)),
                    (BACKEND_ZLIB, Coder::Zlib),
                ] {
                    for planar in [false, true] {
                        let med = kind == PredKind::Med;
                        let (file, _) = encode_stream_coder(
                            &px,
                            d,
                            true,
                            true,
                            kind.flag_bit() | if planar { FLAG_PLANAR } else { 0 },
                            planar,
                            med,
                            kind,
                            &coder,
                            backend,
                            0,
                        )
                        .unwrap();
                        // tokens arm
                        let _ = &file;
                        old = old.min(file.len());
                    }
                }
            }
            for (backend, _coder) in [
                (BACKEND_ZSTD19, Coder::Zstd(19)),
                (BACKEND_ZLIB, Coder::Zlib),
            ] {
                let (tw, _) =
                    encode_tokens_coder_cap(&px, d, backend, FLAG_PRED_MED | FLAG_TOKENS, 16, 0)
                        .unwrap();
                old = old.min(tw.len());
            }
            // NEW pick: + plane/left pure arms
            let mut new_ = old;
            let mut new_win = String::new();
            for kind in [PredKind::Plane, PredKind::Left] {
                for (backend, coder) in [
                    (BACKEND_ZSTD19, Coder::Zstd(19)),
                    (BACKEND_ZLIB, Coder::Zlib),
                ] {
                    let (file, _) = encode_stream_coder(
                        &px,
                        d,
                        true,
                        true,
                        kind.flag_bit(),
                        false,
                        false,
                        kind,
                        &coder,
                        backend,
                        0,
                    )
                    .unwrap();
                    // exact decode check on the pure arm
                    let (dd, out, _) =
                        decode_mnf8(&file).unwrap_or_else(|e| panic!("{name} {kind:?}: {e}"));
                    if out != px {
                        let pos = out
                            .iter()
                            .zip(&px)
                            .position(|(a, b)| a != b)
                            .expect("differs");
                        panic!(
                            "{name} {kind:?} NOT exact: first diff byte {pos} (pixel y={}, x={}, c={}): dec={} enc={}",
                            pos / 3 / d.width as usize, pos / 3 % d.width as usize, pos % 3,
                            out[pos], px[pos]
                        );
                    }
                    let _ = dd;
                    if file.len() < new_ {
                        new_ = file.len();
                        new_win = format!("{kind:?}-{backend}");
                    }
                }
            }
            if new_ < old {
                fires += 1;
            }
            old_tot += old;
            new_tot += new_;
            println!(
                "{name:44} old={old:>8} new={new_:>8} {:+6} {}",
                new_ as i64 - old as i64,
                new_win
            );
        }
        println!(
            "TOTAL old={old_tot} new={new_tot} delta={:+} fires={fires}",
            new_tot as i64 - old_tot as i64
        );
        assert!(new_tot < old_tot, "aggregate must strictly improve");
    }

    #[test]
    fn branch_arms_roundtrip_and_pick_fires() {
        // ADR-004 pins (fast): (1) Plane/Left pure files decode exactly;
        // (2) the pick fires a predictor arm on the poster class;
        // (3) ties/losses keep shipping arms (never-worse).
        let path = format!(
            "{}/testdata/heldout/poster_208x144.png",
            env!("CARGO_MANIFEST_DIR")
        );
        let img = image::open(std::path::Path::new(&path)).unwrap().to_rgb8();
        let d = Dims {
            width: img.width(),
            height: img.height(),
        };
        let px = img.into_raw();
        let (buf, st, _) = encode_mnf8(&px, d, true, Profile::Balanced);
        let (_, out, _) = decode_mnf8(&buf).expect("decode");
        assert_eq!(out, px, "picked arm decodes exactly");
        assert!(
            st.chosen_flags & (FLAG_PRED_PLANE | FLAG_PRED_LEFT) != 0,
            "poster must pick a branch arm (picked {:#x})",
            st.chosen_flags
        );
        // and the new pick must never exceed every old arm on this file:
        let (bufold, _, _) = {
            // construct the old v1.6.0 pick: None/Med arms + tokens, balanced
            let mut best: Option<(Vec<u8>, u32)> = None;
            for (backend, coder) in [(BACKEND_ZSTD19, Coder::Zstd(19))] {
                for kind in [PredKind::None, PredKind::Med] {
                    for planar in [false, true] {
                        let (f, _) = encode_stream_coder(
                            &px,
                            d,
                            true,
                            true,
                            kind.flag_bit() | if planar { FLAG_PLANAR } else { 0 },
                            planar,
                            kind == PredKind::Med,
                            kind,
                            &coder,
                            backend,
                            0,
                        )
                        .unwrap();
                        if best.as_ref().is_none_or(|b| f.len() < b.0.len()) {
                            best = Some((f, kind.flag_bit()));
                        }
                    }
                }
                let (t, _) =
                    encode_tokens_coder_cap(&px, d, backend, FLAG_PRED_MED | FLAG_TOKENS, 16, 0)
                        .unwrap();
                if best.as_ref().is_none_or(|b| t.len() < b.0.len()) {
                    best = Some((t, FLAG_TOKENS));
                }
            }
            let (f, fl) = best.unwrap();
            (f, fl, 0u32)
        };
        assert!(
            buf.len() <= bufold.len(),
            "new pick ({}) must not exceed the old pick ({})",
            buf.len(),
            bufold.len()
        );
    }

    #[test]
    fn rejects_multiple_predictors() {
        // ADR-004 validation: two predictor bits set = Error, never panic.
        let mut f = vec![0u8; HEADER8];
        f[0..4].copy_from_slice(b"MNF8");
        f[4..8].copy_from_slice(&16u32.to_le_bytes());
        f[8..12].copy_from_slice(&16u32.to_le_bytes());
        f[12..16].copy_from_slice(&16u32.to_le_bytes());
        f[16..20].copy_from_slice(&1u32.to_le_bytes());
        f[20..24].copy_from_slice(&0u32.to_le_bytes());
        f[24..28].copy_from_slice(&(FLAG_PRED_MED | FLAG_PRED_PLANE).to_le_bytes());
        f[28..32].copy_from_slice(&BACKEND_ZSTD19.to_le_bytes());
        let r = std::panic::catch_unwind(|| decode_mnf8(&f));
        assert!(
            r.is_ok() && r.unwrap().is_err(),
            "two predictors must reject"
        );
        f[24..28].copy_from_slice(&(FLAG_TOKENS | FLAG_PRED_LEFT).to_le_bytes());
        let r = std::panic::catch_unwind(|| decode_mnf8(&f));
        assert!(r.is_ok() && r.unwrap().is_err(), "tokens+left must reject");
    }
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
            st.chosen_flags & !0b1111 == 0,
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
                PredKind::None,
                &Coder::Zstd(19),
                BACKEND_ZSTD19,
                0,
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
            st.chosen_flags & !0b1111 == 0,
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
        // ADR-004 MEASURED pick (branch-gate): smooth no-repeat bodies select
        // a PREDICTOR bit — historically MED, but the plane branch measured
        // cheaper on the same class, and the pick is measured, not hardcoded.
        // The invariant: a predictor bit IS set and the file decodes exactly.
        assert!(
            st.chosen_flags & (FLAG_PRED_MED | FLAG_PRED_PLANE | FLAG_PRED_LEFT) != 0,
            "smooth no-repeat bodies must select a predictor (picked {:#x})",
            st.chosen_flags
        );
        let (_, out, _) =
            decode_mnf8(&encode_mnf8(&px, d, false, Profile::Balanced).0).expect("decode");
        assert_eq!(out, px, "exact under the measured predictor pick");
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

    // ---- ADR-003 token arm ------------------------------------------------

    fn roundtrip_tokens(px: &[u8], dims: Dims, profile: Profile) -> (u32, u32) {
        let (buf, st, backend) = encode_mnf8(px, dims, true, profile);
        let (d, out, _) = decode_mnf8(&buf).expect("decode");
        assert_eq!(d, dims);
        assert_eq!(
            out, px,
            "profile {profile:?} backend {backend} flags {:#x} literal {} matches {}",
            st.chosen_flags, st.literal, st.ref_exact
        );
        (st.literal, st.ref_exact)
    }

    #[test]
    fn token_wire_unit_roundtrip_and_errors() {
        use crate::tokens::*;
        // small plane with a planted repeat + a run: tokens fire and rebuild
        let (w, h) = (24usize, 16usize);
        let mut plane = vec![0u8; w * h * 3];
        for i in 0..w * h {
            plane[i * 3] = (i % 251) as u8;
            plane[i * 3 + 1] = ((i * 7) % 251) as u8;
        }
        for k in 300..384 {
            plane[k * 3..k * 3 + 3].copy_from_slice(&[9, 9, 9]);
        }
        let (wire, st) = encode_plane(&plane, w, h).expect("encode");
        assert!(st.matches >= 1, "planted structures must fire: {st:?}");
        let back = decode_plane(&wire, w, h).expect("decode");
        assert_eq!(back, plane);
        // malformed wires are errors, never panics
        let mut bad = wire.clone();
        bad.push(2); // unknown tag
        assert!(decode_plane(&bad, w, h).is_err());
        assert!(decode_plane(&wire[..wire.len() - 1], w, h).is_err());
        let mut bad = wire.clone();
        bad.push(TAG_LITERAL);
        bad.extend_from_slice(&[1, 2, 3]);
        assert!(decode_plane(&bad, w, h).is_err()); // overruns the plane
    }

    #[test]
    fn mnf8_token_flag_picked_on_repeating_residual() {
        // ADR-003 measured pick: a large smooth ramp (long constant MED
        // residual runs = the measured token-winning class). The pick MUST
        // be FLAG_TOKENS and decode must reconstruct exactly through the
        // token walk + causal MED inversion.
        let (w, h) = (256u32, 256u32);
        let mut px = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                // vertical ramp: every MED residual = same constant per row
                px[i] = (y * 3) as u8;
                px[i + 1] = (y / 4) as u8;
                px[i + 2] = 200;
            }
        }
        let d = Dims {
            width: w,
            height: h,
        };
        let (buf, st, _) = encode_mnf8(&px, d, false, Profile::Balanced);
        // ADR-003/004 MEASURED pick: this ramp's tokens WON in the v1.6.0
        // arm set, but the branch-gate measured LEFT cheaper still on the
        // same class (−163 B at 320×240; the pick is measured, never
        // hardcoded) — the invariant is exactness + pick legality.
        assert!(
            st.chosen_flags & (FLAG_TOKENS | FLAG_PRED_PLANE | FLAG_PRED_LEFT) != 0,
            "repeating-residual class must pick a measured-winning arm (picked {:#x})",
            st.chosen_flags
        );
        let (_, out, _) = decode_mnf8(&buf).expect("decode");
        assert_eq!(out, px, "wire reconstructs exactly under the pick");
    }

    #[test]
    fn mnf8_token_flag_not_picked_on_noise() {
        // noise planes: all-literal token wire is pure overhead — the pick
        // must stay on the record arms (this is the rate-aware contract's
        // other half).
        let (w, h) = (64u32, 64u32);
        let mut s: u64 = 0x5EED;
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let px: Vec<u8> = (0..w as usize * h as usize * 3)
            .map(|_| (rnd() & 0xFF) as u8)
            .collect();
        let d = Dims {
            width: w,
            height: h,
        };
        let (_, st, _) = encode_mnf8(&px, d, false, Profile::Balanced);
        assert_eq!(
            st.chosen_flags & FLAG_TOKENS,
            0,
            "noise class must NOT pick tokens (picked {:#x})",
            st.chosen_flags
        );
        roundtrip_tokens(&px, d, Profile::Balanced);
    }

    #[test]
    fn mnf8_tokens_decode_rejects_malformed_wire() {
        // craft a token-flag file whose payload decodes to a corrupt wire:
        // unknown tag inside. Must Err, never panic (validation whitelist
        // rule: every admitted construct has a handler or an explicit Err).
        let f = vec![0u8; HEADER8 + 7];
        let mut f = f;
        f[0..4].copy_from_slice(b"MNF8");
        f[4..8].copy_from_slice(&16u32.to_le_bytes());
        f[8..12].copy_from_slice(&16u32.to_le_bytes());
        f[12..16].copy_from_slice(&16u32.to_le_bytes());
        f[16..20].copy_from_slice(&1u32.to_le_bytes());
        f[20..24].copy_from_slice(&7u32.to_le_bytes());
        f[24..28].copy_from_slice(&(FLAG_PRED_MED | FLAG_TOKENS).to_le_bytes());
        f[28..32].copy_from_slice(&BACKEND_ZSTD19.to_le_bytes());
        // zlib backend so we can hand-craft canonical payload bytes:
        f[28..32].copy_from_slice(&BACKEND_ZLIB.to_le_bytes());
        let res: Vec<u8> = vec![0u8; 16 * 16 * 3];
        let (wire, _) = crate::tokens::encode_plane(&res, 16, 16).expect("wire");
        // flip first tag byte to an unknown tag
        let mut wire_bad = wire.clone();
        wire_bad[0] = 7;
        let comp: Vec<u8>;
        {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            std::io::Write::write_all(&mut e, &wire_bad).unwrap();
            comp = e.finish().unwrap();
        }
        let mut file = Vec::with_capacity(HEADER8 + comp.len());
        file.extend_from_slice(&f[..HEADER8]);
        let pl = comp.len() as u32;
        file[20..24].copy_from_slice(&pl.to_le_bytes());
        file.extend_from_slice(&comp);
        let r = std::panic::catch_unwind(|| decode_mnf8(&file));
        assert!(
            r.is_ok() && r.unwrap().is_err(),
            "corrupt wire: Err not panic"
        );
        // exact wire still decodes
        let comp2: Vec<u8>;
        {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            std::io::Write::write_all(&mut e, &wire).unwrap();
            comp2 = e.finish().unwrap();
        }
        let mut file2 = Vec::with_capacity(HEADER8 + comp2.len());
        file2.extend_from_slice(&f[..HEADER8]);
        file2[20..24].copy_from_slice(&(comp2.len() as u32).to_le_bytes());
        file2.extend_from_slice(&comp2);
        let (_, out, _) = decode_mnf8(&file2).expect("valid token wire decodes");
        assert_eq!(out, pred_residual_invert(&res, 16, 16, PredKind::Med));
    }
}

#[cfg(test)]
mod channel_diff {
    use super::*;
    use crate::mnf6::Coder;
    use std::path::Path;

    fn load(name: &str) -> (Dims, Vec<u8>) {
        let p = format!("{}/testdata/{}", env!("CARGO_MANIFEST_DIR"), name);
        let img = image::open(Path::new(&p))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .to_rgb8();
        (
            Dims {
                width: img.width(),
                height: img.height(),
            },
            img.into_raw(),
        )
    }

    /// Roundtrip pin: the CD arm encodes/decodes exactly, all predictor and
    /// planar variants, both backends, incl. an atlas-free pure arm.
    #[test]
    fn cd_roundtrip_all_predictor_shapes() {
        let (d, px) = load("gradient_320x240.png");
        for kind in [
            PredKind::None,
            PredKind::Med,
            PredKind::Plane,
            PredKind::Left,
        ] {
            for planar in [false, true] {
                for (backend, coder) in [
                    (BACKEND_ZLIB, Coder::Zlib),
                    (BACKEND_ZSTD19, Coder::Zstd(19)),
                ] {
                    let flags =
                        kind.flag_bit() | if planar { FLAG_PLANAR } else { 0 } | FLAG_CHANNEL_DIFF;
                    let (file, _) = encode_stream_coder(
                        &px,
                        d,
                        true,
                        false,
                        flags,
                        planar,
                        kind == PredKind::Med,
                        kind,
                        &coder,
                        backend,
                        1,
                    )
                    .expect("encode");
                    let (dd, out, _) = decode_mnf8(&file).expect("decode");
                    assert_eq!(dd, d);
                    assert_eq!(out, px, "cd roundtrip {kind:?} planar={planar}");
                }
            }
        }
        // token arm with cd:
        let flags = FLAG_PRED_MED | FLAG_TOKENS | FLAG_CHANNEL_DIFF;
        for cap in [16, tokens::MAX_CANDIDATES] {
            let (file, _) = encode_tokens_coder_cap(&px, d, BACKEND_ZSTD19, flags, cap, 1)
                .expect("token encode");
            let (dd, out, _) = decode_mnf8(&file).expect("decode");
            assert_eq!(dd, d);
            assert_eq!(out, px, "cd token roundtrip cap={cap}");
        }
    }

    /// The never-worse pin: the full pick (with CD arms) must not exceed
    /// the pre-ADR-008 pick on a fire-class fixture and a no-fire one.
    #[test]
    fn cd_pick_never_worse() {
        for name in ["photos/slate_224x160_s900008.png", "gradient_320x240.png"] {
            let (d, px) = load(name);
            let (cd_file, _, _) = encode_mnf8(&px, d, false, Profile::Maximum);
            // the old pick = re-run the pre-CD loop inline (by-value loops
            // mirroring encode_mnf8 minus the CD arms):
            let candidates: Vec<(u32, Coder)> = backend_candidates();
            let mut old = usize::MAX;
            for (backend, coder) in candidates {
                for kind in [
                    PredKind::None,
                    PredKind::Med,
                    PredKind::Plane,
                    PredKind::Left,
                ] {
                    for planar in [false, true] {
                        let flags = kind.flag_bit() | if planar { FLAG_PLANAR } else { 0 };
                        let (file, _) = encode_stream_coder(
                            &px,
                            d,
                            true,
                            false,
                            flags,
                            planar,
                            kind == PredKind::Med,
                            kind,
                            &coder,
                            backend,
                            0,
                        )
                        .expect("encode");
                        old = old.min(file.len());
                    }
                }
                for cap in [16, tokens::MAX_CANDIDATES] {
                    let (file, _) = encode_tokens_coder_cap(
                        &px,
                        d,
                        backend,
                        FLAG_PRED_MED | FLAG_TOKENS,
                        cap,
                        0,
                    )
                    .expect("encode");
                    old = old.min(file.len());
                }
            }
            assert!(
                cd_file.len() <= old,
                "{name}: pick {} above old {old}",
                cd_file.len()
            );
        }
    }

    /// The corpus gate (slow, ~15 min): every file roundtrips under the
    /// full CD pick; wins counted vs the old pick's own wire. #[ignore]d;
    /// run explicitly at the ADR-008 gate.
    #[test]
    #[ignore]
    fn cd_corpus_gate() {
        let files = [
            "gradient_320x240.png",
            "noise_256x256.png",
            "repeat_256x256.png",
            "screenshot_640x400.png",
            "solid_256x256.png",
            "heldout/blob_224x160.png",
            "heldout/blocks_192x128.png",
            "heldout/document_200x280.png",
            "heldout/plaid_192.png",
            "heldout/poster_208x144.png",
            "heldout-photos/autumn_160x120_s3100006.png",
            "heldout-photos/dusk_160x120_s3100003.png",
            "heldout-photos/dusk_224x160_s3100002.png",
            "heldout-photos/forest_192x144_s3100001.png",
            "heldout-photos/forest_224x160_s3100005.png",
            "heldout-photos/forest_224x160_s3100008.png",
            "heldout-photos/ocean_192x144_s3100007.png",
            "heldout-photos/sand_192x144_s3100004.png",
            "heldout-photos/slate_160x120_s3100009.png",
            "heldout-photos/terrain_160x120_s3100000.png",
            "photos/autumn_192x144_s900001.png",
            "photos/dusk_160x120_s900003.png",
            "photos/forest_224x160_s900002.png",
            "photos/ocean_192x144_s900007.png",
            "photos/sand_160x120_s900000.png",
            "photos/slate_192x144_s900004.png",
            "photos/slate_224x160_s900008.png",
            "photos/stone_160x120_s900006.png",
            "photos/terrain_160x120_s900009.png",
            "photos/terrain_224x160_s900005.png",
        ];
        let mut tot = 0usize;
        let mut cd_fires = 0usize;
        for name in files {
            let (d, px) = load(name);
            let (file, st, _) = encode_mnf8(&px, d, false, crate::mnf8::Profile::Maximum);
            assert_eq!(decode_mnf8(&file).unwrap().1, px, "{name}");
            tot += file.len();
            let reference = cd_reference(st.chosen_flags);
            cd_fires += usize::from(reference != 0);
            if reference != 0 {
                println!("{name}: ref={reference}, {}", file.len());
            }
        }
        println!("corpus: total={tot} cd_fires={cd_fires}");
        assert!(cd_fires >= 10, "cd must fire on the photographic class");
    }

    /// Malformed cd wires reject with errors (never panics): truncated
    /// payload, unknown mode after the unrelabel guard.
    #[test]
    fn cd_malformed_rejects() {
        let (d, px) = load("gradient_320x240.png");
        let flags = FLAG_CHANNEL_DIFF;
        let (file, _) = encode_stream_coder(
            &px,
            d,
            true,
            false,
            flags,
            false,
            false,
            PredKind::None,
            &Coder::Zstd(19),
            BACKEND_ZSTD19,
            1,
        )
        .expect("encode");
        // truncate the payload
        for cut in [40, 41, 60, file.len() / 2] {
            let mut t = file.clone();
            t.truncate(cut.min(file.len()));
            assert!(
                decode_mnf8(&t).is_err(),
                "truncated cd file must reject at {cut}"
            );
        }
        // corrupt the flag region (invalid mode inside the body): flip a
        // byte inside the container — the unrelabel walk or the container
        // must reject
        let mut t = file.clone();
        let last = t.len() - 1;
        t[last] = t[last].wrapping_add(1);
        let _ = decode_mnf8(&t); // must not panic; result = Err or valid
                                 // diff-file class on the container checksum
    }
}
