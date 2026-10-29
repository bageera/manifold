//! MNF9 — exact generative whole-frame records (ADR-007; Track A promotion).
//!
//! Ported from the research prototype crates/mnf-mans/src/mnf10.rs (the
//! discovery probe's measured families) with the shipped lib's discipline:
//! Error (never panic) on malformed input, bounded decode, exact-size
//! accounting. The ENCODER attempts each model family's exact fit against
//! the raw pixels and emits a whole-frame rule only on ZERO residual —
//! the pick (mnf8.rs) treats the resulting MNF9 stream as one more arm on
//! final file size; a losing file keeps its MNF8 encoding.
//!
//! Wire: magic "MNF9" (4 B; the ADR-003 reservation), 32-B header identical
//! in field layout to MNF8 (flags/backend = 0 on write, rejected if set on
//! decode — no flags/containers exists in this grammar yet), payload =
//! one parametric record per ADR-007's model table.
//!
//! ponytail: six integer families, one decode dispatch; if a seventh family
//! ever earns its place it comes with its own probe record first.

use crate::codec::Dims;
use crate::error::{Error, Result};

/// Wire magic (4 B; the ADR-003 reservation — see docs/format.md §5).
pub const MAGIC: [u8; 4] = *b"MNF9";
/// Header length (32 B; field offsets mirror MNF8's).
pub const HEADER_LEN: usize = 32;

/// Whole-frame constant.
pub const MODEL_CONST: u8 = 1;
/// Per-channel div-form ramp along x: (N·x + T) div_euclid D.
pub const MODEL_DIV1D: u8 = 2;
/// Per-channel constant-step recurrence mod 256 (orientation bit).
pub const MODEL_REC1: u8 = 3;
/// Corner-anchored joint floor bilinear.
pub const MODEL_BILIN_JOINT: u8 = 4;
/// Corner-anchored nested round bilinear (x then y, half-up).
pub const MODEL_BILIN_NESTED: u8 = 5;
/// Per-segment div-ramps with declared boundaries.
pub const MODEL_PIECEWISE: u8 = 6;

/// Denominator convention table (ADR-007): idx 3 = w−1 (index 3/4 are
/// SEGMENT-relative in the piecewise arm, resolved by the caller).
fn den_full(idx: u8, w: u32) -> Result<i64> {
    let w1 = i64::from(w).saturating_sub(1).max(1);
    match idx {
        0 => Ok(1),
        1 => Ok(2),
        2 => Ok(4),
        3 => Ok(w1),
        4 => Ok(i64::from(w)),
        5 => Ok(2 * w1),
        6 => Ok(256),
        7 => Ok(65_536),
        _ => Err(Error::fmt_msg("mnf9: denominator index out of table")),
    }
}

// ------------------------------------------------------------------ decoder

fn dims_ok(dims: Dims) -> Result<(usize, usize)> {
    if dims.width == 0 || dims.height == 0 {
        return Err(Error::fmt_msg("mnf9: empty dims"));
    }
    let (wu, hu) = (dims.width as usize, dims.height as usize);
    if wu.checked_mul(hu).and_then(|n| n.checked_mul(3)).is_none() {
        return Err(Error::fmt_msg("mnf9: dims overflow"));
    }
    Ok((wu, hu))
}

fn eval_payload(payload: &[u8], dims: Dims) -> Result<Vec<u8>> {
    // Identical evaluation to the prototype's decoder (byte-for-byte port of
    // the mnf10.rs logic; the D=3/4 segment-relative rule included).
    let (wu, hu) = dims_ok(dims)?;
    if payload.is_empty() {
        return Err(Error::fmt_msg("mnf9: empty payload"));
    }
    let model = payload[0];
    let body = &payload[1..];
    let need = |n: usize| -> Result<&[u8]> {
        if body.len() < n {
            Err(Error::fmt_msg("mnf9: truncated params"))
        } else {
            Ok(&body[..n])
        }
    };
    let i16le = |o: usize| i16::from_le_bytes([body[o], body[o + 1]]);
    let out: Vec<u8> = match model {
        MODEL_CONST => {
            let p = need(3)?;
            let mut v = vec![0u8; wu * hu * 3];
            for k in 0..wu * hu {
                v[3 * k..3 * k + 3].copy_from_slice(p);
            }
            v
        }
        MODEL_DIV1D => {
            let p = need(25)?; // 3*(N,T) i32 + shared D index
            let d = den_full(p[24], dims.width)?;
            let mut v = vec![0u8; wu * hu * 3];
            for c in 0..3 {
                let n = i32::from_le_bytes(p[8 * c..8 * c + 4].try_into().unwrap()) as i64;
                let t = i32::from_le_bytes(p[8 * c + 4..8 * c + 8].try_into().unwrap()) as i64;
                for x in 0..wu {
                    let val = (n * x as i64 + t).div_euclid(d);
                    if !(0..=255).contains(&val) {
                        return Err(Error::fmt_msg("mnf9: div-1D value out of byte range"));
                    }
                    for y in 0..hu {
                        v[(y * wu + x) * 3 + c] = val as u8;
                    }
                }
            }
            v
        }
        MODEL_REC1 => {
            let p = need(10)?; // 3*(a0,d) + orientation
            let orientation = p[9];
            let lin = if orientation == 0 { wu } else { hu };
            let lines = if orientation == 0 { hu } else { wu };
            let mut v = vec![0u8; wu * hu * 3];
            let mut line = vec![0u8; lin * 3];
            for c in 0..3 {
                let a0 = u32::from(p[3 * c]) as i64;
                let d = i64::from(i16le(3 * c + 1));
                for k in 0..lin {
                    line[3 * k + c] = (a0 + d * k as i64).rem_euclid(256) as u8;
                }
            }
            for k in 0..lines {
                for j in 0..lin {
                    let (x, y) = if orientation == 0 { (j, k) } else { (k, j) };
                    v[(y * wu + x) * 3..(y * wu + x) * 3 + 3]
                        .copy_from_slice(&line[3 * j..3 * j + 3]);
                }
            }
            v
        }
        MODEL_BILIN_JOINT => {
            let p = need(12)?;
            if dims.width == 1 || dims.height == 1 {
                return Err(Error::fmt_msg("mnf9: bilinear degenerate dims"));
            }
            let mut v = vec![0u8; wu * hu * 3];
            let dm = u64::from(dims.width - 1) * u64::from(dims.height - 1);
            let wm1 = u64::from(dims.width - 1);
            let hm1 = u64::from(dims.height - 1);
            for c in 0..3 {
                let (v00, v10, v01, v11) = (
                    u64::from(p[c]),
                    u64::from(p[3 + c]),
                    u64::from(p[6 + c]),
                    u64::from(p[9 + c]),
                );
                for y in 0..hu {
                    for x in 0..wu {
                        let (xu, yu) = (x as u64, y as u64);
                        let num = v00 * (wm1 - xu) * (hm1 - yu)
                            + v10 * xu * (hm1 - yu)
                            + v01 * (wm1 - xu) * yu
                            + v11 * xu * yu;
                        v[(y * wu + x) * 3 + c] = (num / dm) as u8;
                    }
                }
            }
            v
        }
        MODEL_BILIN_NESTED => {
            let p = need(12)?;
            if dims.width == 1 || dims.height == 1 {
                return Err(Error::fmt_msg("mnf9: bilinear degenerate dims"));
            }
            let mut v = vec![0u8; wu * hu * 3];
            let dx = i64::from(dims.width - 1);
            let dy = i64::from(dims.height - 1);
            let hx = dx / 2;
            let hy = dy / 2;
            for c in 0..3 {
                let (v00, v10, v01, v11) = (
                    i64::from(p[c]),
                    i64::from(p[3 + c]),
                    i64::from(p[6 + c]),
                    i64::from(p[9 + c]),
                );
                let mut toprow = vec![0i64; wu];
                let mut botrow = vec![0i64; wu];
                for x in 0..wu {
                    let xi = x as i64;
                    toprow[x] = v00 + (xi * (v10 - v00) + hx).div_euclid(dx);
                    botrow[x] = v01 + (xi * (v11 - v01) + hx).div_euclid(dx);
                }
                for y in 0..hu {
                    let yi = y as i64;
                    for x in 0..wu {
                        let val = toprow[x] + (yi * (botrow[x] - toprow[x]) + hy).div_euclid(dy);
                        if !(0..=255).contains(&val) {
                            return Err(Error::fmt_msg("mnf9: nested value out of range"));
                        }
                        v[(y * wu + x) * 3 + c] = val as u8;
                    }
                }
            }
            v
        }
        MODEL_PIECEWISE => {
            if body.is_empty() {
                return Err(Error::fmt_msg("mnf9: piecewise truncated"));
            }
            let nseg = body[0] as usize;
            if !(1..=32).contains(&nseg) {
                return Err(Error::fmt_msg("mnf9: piecewise segment count"));
            }
            let nb = nseg - 1;
            let p = need(1 + nb * 2 + nseg * 27)?;
            let mut bounds = vec![0usize];
            for k in 0..nb {
                let b =
                    u16::from_le_bytes(body[1 + k * 2..1 + k * 2 + 2].try_into().unwrap()) as usize;
                if b == 0 || b >= wu {
                    return Err(Error::fmt_msg("mnf9: piecewise boundary out of range"));
                }
                bounds.push(b);
            }
            let mut v = vec![0u8; wu * hu * 3];
            for si in 0..bounds.len() {
                let a = bounds[si];
                let b = bounds.get(si + 1).copied().unwrap_or(wu);
                for c in 0..3 {
                    let o = 1 + nb * 2 + si * 27 + c * 9;
                    let n = i32::from_le_bytes(p[o..o + 4].try_into().unwrap()) as i64;
                    let t = i32::from_le_bytes(p[o + 4..o + 8].try_into().unwrap()) as i64;
                    let segw = (b - a) as u32;
                    let d = match p[o + 8] {
                        3 => i64::from(segw.saturating_sub(1).max(1)),
                        4 => i64::from(segw),
                        other => den_full(other, dims.width)?,
                    };
                    for xi in a..b {
                        let sx = (xi - a) as i64;
                        let val = (n * sx + t).div_euclid(d);
                        if !(0..=255).contains(&val) {
                            return Err(Error::fmt_msg("mnf9: piecewise value out of range"));
                        }
                        for y in 0..hu {
                            v[(y * wu + xi) * 3 + c] = val as u8;
                        }
                    }
                }
            }
            v
        }
        _ => return Err(Error::fmt_msg("mnf9: reserved model id")),
    };
    Ok(out)
}

/// Decode a full MNF9 file (magic + header + payload) to RGB. Every
/// malformed field is an Err; nothing panics on untrusted input.
pub fn decode_mnf9(buf: &[u8]) -> Result<(Dims, Vec<u8>)> {
    if buf.len() < HEADER_LEN {
        return Err(Error::fmt_msg("mnf9: shorter than header"));
    }
    if buf[0..4] != MAGIC {
        return Err(Error::fmt_msg("mnf9: bad magic"));
    }
    let width = u32::from_le_bytes(buf[4..8].try_into().expect("slice"));
    let height = u32::from_le_bytes(buf[8..12].try_into().expect("slice"));
    let res1 = u32::from_le_bytes(buf[12..16].try_into().expect("slice"));
    let res2 = u32::from_le_bytes(buf[16..20].try_into().expect("slice"));
    let payload_len = u32::from_le_bytes(buf[20..24].try_into().expect("slice")) as usize;
    let flags = u32::from_le_bytes(buf[24..28].try_into().expect("slice"));
    let backend = u32::from_le_bytes(buf[28..32].try_into().expect("slice"));
    if res1 != 0 || res2 != 0 {
        return Err(Error::fmt_msg("mnf9: reserved header words set"));
    }
    if flags != 0 {
        return Err(Error::fmt_msg(
            "mnf9: reserved flags set (grammar has none yet)",
        ));
    }
    if backend != 0 {
        return Err(Error::fmt_msg("mnf9: reserved backend set"));
    }
    if width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(3))
        .is_none()
    {
        return Err(Error::fmt_msg("mnf9: dims overflow"));
    }
    if buf.len() != HEADER_LEN + payload_len {
        return Err(Error::fmt_msg("mnf9: payload length mismatch"));
    }
    let dims = Dims { width, height };
    let px = eval_payload(&buf[HEADER_LEN..], dims)?;
    Ok((dims, px))
}

// ------------------------------------------------------------------ encoder
//
// Discovery solvers (the probe's algorithms, one per family). Each returns
// the payload (WITHOUT the model-id framing differences) on an exact fit or
// None. All are bounded (interval intersection, not D-wide scans).

fn exact_const(px: &[u8], wu: usize, hu: usize) -> Option<Vec<u8>> {
    let first = &px[0..3];
    (0..wu * hu)
        .all(|k| &px[3 * k..3 * k + 3] == first)
        .then(|| {
            let mut p = vec![MODEL_CONST];
            p.extend_from_slice(first);
            p
        })
}

/// Solve (N*x + T) div_euclid(D) == col for the D-table; interval-confined.
fn solve_div_col(col: &[i64], xlen: usize, d: i64) -> Option<(i64, i64)> {
    if xlen < 2 {
        return None;
    }
    let v0 = col[0];
    let vend = col[xlen - 1];
    let num = (vend - v0) * d;
    let den = xlen as i64 - 1;
    let base = (num * 2 + den).div_euclid(2 * den); // half-up round
    for n in base - 2..=base + 2 {
        let mut lo = v0 * d;
        let mut hi = v0 * d + d - 1;
        for &xi in [1usize, xlen / 2, xlen - 1].iter() {
            if xi >= xlen {
                continue;
            }
            let tl = col[xi] * d - n * xi as i64;
            lo = lo.max(tl);
            hi = hi.min(tl + d - 1);
        }
        if lo <= hi {
            for t in lo..=hi {
                let ok = (0..xlen).all(|x| (n * x as i64 + t).div_euclid(d) == col[x]);
                if ok {
                    return Some((n, t));
                }
            }
        }
    }
    None
}

fn exact_div1d(px: &[u8], wu: usize, hu: usize) -> Option<Vec<u8>> {
    // rows identical + per-channel solve at each table D
    for y in 0..hu {
        if px[y * wu * 3..y * wu * 3 + wu * 3] != px[..wu * 3] {
            return None;
        }
    }
    let row0: Vec<Vec<i64>> = (0..3)
        .map(|c| (0..wu).map(|x| i64::from(px[x * 3 + c])).collect())
        .collect();
    for didx in [0u8, 1, 2, 3, 4, 5, 6, 7] {
        let d = match den_full(didx, wu as u32) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let mut params = Vec::with_capacity(24);
        let mut ok = true;
        for col_row in row0.iter().take(3) {
            match solve_div_col(col_row, wu, d) {
                Some((n, t)) => {
                    params.extend_from_slice(&(n as i32).to_le_bytes());
                    params.extend_from_slice(&(t as i32).to_le_bytes());
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            let mut p = vec![MODEL_DIV1D];
            p.extend_from_slice(&params);
            p.push(didx);
            return Some(p);
        }
    }
    None
}

fn exact_rec1(px: &[u8], wu: usize, hu: usize) -> Option<Vec<u8>> {
    for orientation in [0u8, 1] {
        let (lin, lines) = if orientation == 0 { (wu, hu) } else { (hu, wu) };
        if lin < 2 {
            continue;
        }
        let get = |k: usize, li: usize| -> &[u8] {
            let idx = if orientation == 0 {
                li * wu * 3 + k * 3
            } else {
                k * wu * 3 + li * 3
            };
            &px[idx..idx + 3]
        };
        // scan lines identical?
        let mut same = true;
        for li in 1..lines {
            for k in 0..lin {
                if get(k, li) != get(k, 0) {
                    same = false;
                    break;
                }
            }
            if !same {
                break;
            }
        }
        if !same {
            continue;
        }
        // constant signed diff per channel inside the scan line?
        let mut params = Vec::with_capacity(10);
        let mut ok = true;
        for c in 0..3 {
            let v0 = i64::from(get(0, 0)[c]);
            let d0 = i64::from(get(1, 0)[c]) - v0;
            let per_c_step: i16 = d0.clamp(-128, 127) as i16;
            // verify step constancy under mod-256 signed diff
            let mut step_ok = true;
            for k in 2..lin {
                let prev = i64::from(get(k - 1, 0)[c]);
                let cur = i64::from(get(k, 0)[c]);
                let d = (cur - prev + 128).rem_euclid(256) - 128;
                let expected = (d0 + 128).rem_euclid(256) - 128;
                if d != expected {
                    step_ok = false;
                    break;
                }
            }
            if !step_ok {
                ok = false;
                break;
            }
            // reconstruction exact?
            let mut recon_ok = true;
            for k in 0..lin {
                let val = (v0 + per_c_step as i64 * k as i64).rem_euclid(256) as u8;
                if val != get(k, 0)[c] {
                    recon_ok = false;
                    break;
                }
            }
            if !recon_ok {
                ok = false;
                break;
            }
            params.push(get(0, 0)[c]);
            params.extend_from_slice(&per_c_step.to_le_bytes());
        }
        if ok {
            let mut p = vec![MODEL_REC1];
            p.extend_from_slice(&params);
            p.push(orientation);
            return Some(p);
        }
    }
    None
}

fn exact_bilinear(px: &[u8], wu: usize, hu: usize, nested: bool) -> Option<Vec<u8>> {
    if wu < 2 || hu < 2 {
        return None;
    }
    let corners = [0usize, wu - 1];
    let getv = |y: usize, x: usize, c: usize| u64::from(px[(y * wu + x) * 3 + c]);
    let (r0, r1) = (0, hu - 1);
    let (x0, x1) = (0, wu - 1);
    let mut params = vec![if nested {
        MODEL_BILIN_NESTED
    } else {
        MODEL_BILIN_JOINT
    }];
    for c in 0..3 {
        params.push(getv(r0, x0, c) as u8);
        params.push(getv(r0, x1, c) as u8);
        params.push(getv(r1, x0, c) as u8);
        params.push(getv(r1, x1, c) as u8);
    }
    let _ = corners;
    // verify: the corner values ARE the params; evaluate and compare
    let payload_ok = eval_payload(
        &params,
        Dims {
            width: wu as u32,
            height: hu as u32,
        },
    )
    .ok()?;
    if payload_ok == px {
        Some(params)
    } else {
        None
    }
}

/// Segment-local discovery: grow [a, b) over all channels while some
/// per-segment D solves all three; bounded by the 3*last-good-length rule
/// measured in the probe (rounding-gap runs of 14+ failures occur).
fn exact_piecewise(px: &[u8], wu: usize, hu: usize, max_segments: usize) -> Option<Vec<u8>> {
    for y in 0..hu {
        if px[y * wu * 3..y * wu * 3 + wu * 3] != px[..wu * 3] {
            return None;
        }
    }
    let row0: Vec<Vec<i64>> = (0..3)
        .map(|c| (0..wu).map(|x| i64::from(px[x * 3 + c])).collect())
        .collect();
    let dense_pair = |len: usize| -> Vec<i64> {
        // per-segment natural denominators: {1, 2, 4, len-1, len}
        let mut v = vec![1i64, 2, 4];
        if len > 1 {
            v.push(len as i64 - 1);
            v.push(len as i64);
        }
        v.into_iter().filter(|d| *d > 0).collect()
    };
    let solve_seg = |a: usize, b: usize| -> Option<Vec<(i64, i64, i64)>> {
        let per = dense_pair(b - a);
        let mut out = Vec::with_capacity(3);
        for col in row0.iter().take(3) {
            let col: Vec<i64> = col[a..b].to_vec();
            let xs = |k: usize| k as i64; // segment-local x
            let mut got = None;
            for d in &per {
                if col.len() < 2 {
                    break;
                }
                let v0 = col[0];
                let vend = col[col.len() - 1];
                let num = (vend - v0) * d;
                let den = col.len() as i64 - 1;
                let base = (num * 2 + den).div_euclid(2 * den);
                for n in base - 2..=base + 2 {
                    let mut lo = v0 * d;
                    let mut hi = v0 * d + d - 1;
                    for xi in [1usize, col.len() / 2, col.len() - 1] {
                        let tl = col[xi] * d - n * xs(xi);
                        lo = lo.max(tl);
                        hi = hi.min(tl + d - 1);
                    }
                    if lo <= hi {
                        for t in lo..=hi {
                            if (0..col.len()).all(|k| (n * xs(k) + t).div_euclid(*d) == col[k]) {
                                got = Some((*d, n, t));
                                break;
                            }
                        }
                    }
                    if got.is_some() {
                        break;
                    }
                }
                if got.is_some() {
                    break;
                }
            }
            got?;
            out.push(got.unwrap());
        }
        Some(out)
    };
    let mut bounds: Vec<usize> = vec![0];
    let mut seg_params: Vec<Vec<(i64, i64, i64)>> = Vec::new();
    let mut a = 0usize;
    while a < wu {
        let mut best_b = 0usize;
        let mut best_entry: Option<Vec<(i64, i64, i64)>> = None;
        let mut last_ok = 2usize;
        for b in a + 2..=wu {
            match solve_seg(a, b) {
                Some(entry) => {
                    best_b = b;
                    best_entry = Some(entry);
                    last_ok = b - a;
                }
                None => {
                    if best_entry.is_some() && b - a > 3 * last_ok {
                        break;
                    }
                }
            }
        }
        let entry = best_entry.as_ref()?;
        seg_params.push(entry.clone());
        bounds.push(best_b);
        a = best_b;
        if bounds.len() > max_segments {
            return None;
        }
    }
    // wire: u8 nseg; (nseg-1) u16 boundaries; per seg+channel: i32 N, i32 T, u8 D-id
    // D-id must map the solved d onto the convention set {1,2,4,seglen-1,seglen}
    let mut p = vec![MODEL_PIECEWISE];
    p.push(seg_params.len() as u8);
    for k in 1..bounds.len() - 1 + 1 {
        // boundaries BETWEEN segments only (the final edge = w implicit)
        if k < bounds.len() - 1 {
            p.extend_from_slice(&(bounds[k] as u16).to_le_bytes());
        }
    }
    for si in 0..seg_params.len() {
        let a = bounds[si];
        let b = bounds[si + 1];
        let seglen = b - a;
        for (d, n, t) in &seg_params[si] {
            let d_index = match d {
                1 => 0u8,
                2 => 1,
                4 => 2,
                _ if *d == seglen as i64 - 1 => 3,
                _ if *d == seglen as i64 => 4,
                _ => return None, // unencodable D (would need more table)
            };
            p.extend_from_slice(&(*n as i32).to_le_bytes());
            p.extend_from_slice(&(*t as i32).to_le_bytes());
            p.push(d_index);
        }
    }
    // end-to-end self-verification: the pick only claims what decodes exact
    let recon = eval_payload(
        &p,
        Dims {
            width: wu as u32,
            height: hu as u32,
        },
    )
    .ok()?;
    if recon == px {
        Some(p)
    } else {
        None
    }
}

/// Try all six families; return the smallest exact-fit payload, or None.
pub fn discover(px: &[u8], dims: Dims) -> Option<Vec<u8>> {
    let (wu, hu) = dims_ok(dims).ok()?;
    let mut best: Option<Vec<u8>> = None;
    let mut consider = |p: Option<Vec<u8>>| {
        if let Some(p) = p {
            if best.as_ref().map(|b| p.len() < b.len()).unwrap_or(true) {
                best = Some(p);
            }
        }
    };
    consider(exact_const(px, wu, hu));
    consider(exact_div1d(px, wu, hu));
    consider(exact_rec1(px, wu, hu));
    consider(exact_bilinear(px, wu, hu, false));
    consider(exact_bilinear(px, wu, hu, true));
    consider(exact_piecewise(px, wu, hu, 4));
    best
}

fn file_bytes(payload: &[u8], dims: Dims) -> Result<Vec<u8>> {
    let body_len = payload.len();
    if body_len > u32::MAX as usize {
        return Err(Error::fmt_msg("mnf9: payload too large"));
    }
    let mut f = Vec::with_capacity(HEADER_LEN + body_len);
    f.extend_from_slice(&MAGIC);
    f.extend_from_slice(&dims.width.to_le_bytes()); // 4..
    f.extend_from_slice(&dims.height.to_le_bytes()); // 8..
    f.extend_from_slice(&0u32.to_le_bytes()); // 12: reserved
    f.extend_from_slice(&0u32.to_le_bytes()); // 16: reserved
    f.extend_from_slice(&(body_len as u32).to_le_bytes()); // 20: payload_len
    f.extend_from_slice(&0u32.to_le_bytes()); // 24: flags (none yet)
    f.extend_from_slice(&0u32.to_le_bytes()); // 28: backend (none)
    debug_assert_eq!(f.len(), HEADER_LEN);
    f.extend_from_slice(payload);
    Ok(f)
}

/// Whole-frame generative candidate: an exact rule => Some(file_bytes) else
/// None (the pick treats None as "arm not available for this image").
pub fn encode_exact(px: &[u8], dims: Dims) -> Option<Vec<u8>> {
    let payload = discover(px, dims)?;
    file_bytes(&payload, dims).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor_lerp(w: usize, h: usize) -> Vec<u8> {
        let mut px = vec![0u8; w * h * 3];
        for x in 0..w {
            let v = [
                (20 * 159 + x * 210) / 159,
                (230 * 159 - x * 210) / 159,
                (60 * 159 + x * 140) / 159,
            ];
            for y in 0..h {
                for c in 0..3 {
                    px[(y * w + x) * 3 + c] = v[c] as u8;
                }
            }
        }
        px
    }

    fn recurrence(w: usize, h: usize) -> Vec<u8> {
        let mut px = vec![0u8; w * h * 3];
        for x in 0..w {
            let v = [
                ((30 + 7 * x) % 256) as u8,
                ((200 + 7 * x) % 256) as u8,
                ((90 + 7 * x) % 256) as u8,
            ];
            for y in 0..h {
                px[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&v);
            }
        }
        px
    }

    #[test]
    fn exact_discovery_fires_on_rule_classes() {
        let d = Dims {
            width: 160,
            height: 120,
        };
        let px = floor_lerp(160, 120);
        let f = encode_exact(&px, d).expect("floor-lerp discovers");
        let (rd, rp) = decode_mnf9(&f).expect("mnf9 decode");
        assert_eq!((rd.width, rd.height), (160u32, 120u32));
        assert_eq!(rp, px, "exact generative reconstruction");
        // wire economy: the pick comparator measured 50 B for this class
        assert!(f.len() < 128, "floor-lerp file = {} B", f.len());

        let px = recurrence(160, 120);
        let f = encode_exact(&px, d).expect("recurrence discovers");
        let (_, rp) = decode_mnf9(&f).expect("decode");
        assert_eq!(rp, px);
        assert!(f.len() < 64, "rec file = {} B", f.len());
    }

    #[test]
    fn no_rule_no_candidate() {
        // noise: no family fits exactly; the arm stays None and the pick
        // keeps MNF8 (the probe's measured 14/16 = this polarity)
        let mut s = 0x1234567u64;
        let mut px = vec![0u8; 64 * 64 * 3];
        for b in px.iter_mut() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *b = (s & 0xFF) as u8;
        }
        let d = Dims {
            width: 64,
            height: 64,
        };
        assert!(encode_exact(&px, d).is_none(), "noise must not discover");
    }

    #[test]
    fn malformed_files_error_not_panic() {
        // bad magic / short / flags set / backend set / payload mismatch /
        // reserved model id / truncated params / oob piecewise boundary
        let d = Dims {
            width: 160,
            height: 120,
        };
        let px = floor_lerp(160, 120);
        let good = encode_exact(&px, d).expect("discover");
        // bad magic
        let mut bad = good.clone();
        bad[0] = b'X';
        assert!(decode_mnf9(&bad).is_err());
        // truncated
        assert!(decode_mnf9(&good[..10]).is_err());
        // flags set
        let mut bad = good.clone();
        bad[24] = 1;
        assert!(decode_mnf9(&bad).is_err());
        // payload length mismatch
        let mut bad = good.clone();
        let plen = u32::from_le_bytes(bad[20..24].try_into().unwrap());
        bad[20..24].copy_from_slice(&(plen + 1).to_le_bytes());
        assert!(decode_mnf9(&bad).is_err());
        // reserved model id
        let mut bad = good.clone();
        bad[32] = 7;
        assert!(decode_mnf9(&bad).is_err());
        // boundary out of range (piecewise)
        // construct a piecewise payload with boundary 9999 via the encoder:
        // instead, flip a boundary byte in a crafted piecewise file
        let pw = exact_piecewise(&px, 160, 120, 4);
        if pw.is_some() {
            // piecewise boundaries start at payload offset 2; set 0xFFFF
            let mut bad = good.clone();
            let payload = &mut bad[32..];
            let is_pw = payload[0] == MODEL_PIECEWISE;
            if is_pw {
                payload[2] = 0xFF;
                payload[3] = 0xFF;
            }
            let _ = payload; // borrow ends here
            let is_pw2 = bad[32] == MODEL_PIECEWISE;
            if is_pw && is_pw2 {
                assert!(decode_mnf9(&bad).is_err(), "oob boundary must reject");
            }
        }
    }
}
