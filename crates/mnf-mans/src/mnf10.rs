//! MNF10 exact generative records (ADR-007 prototype) — the research-crate
//! port of crates/mnf-mans/scripts/generative_probe.py's families with a REAL
//! decoder: every malformed wire is an Err, never a panic; reconstruction is
//! the normative integer equation set evaluated deterministically.
//!
//! Wire (payload inside a 32-B MNF10 header; see ADR-007):
//!   u8 model_id; then per-model canonical params (LE):
//!     1 constant        r,g,b
//!     2 div-1D          per channel: i32 N, i32 T; u8 D-table index
//!     3 rec-1           per channel: u8 a0, i16 d; u8 orientation (0 row,1 col)
//!     4 bilinear joint  12 x u8 corners
//!     5 bilinear nested 12 x u8 corners
//!     6 piecewise       u8 nseg, u16 boundaries * (nseg-1), then per
//!                       segment/channel: i32 N, i32 T, u8 D index
//!   0 and 7..=255 reserved → Err (never panic).
//!
//! ponytail: fixed-point params (i32) + a shared denominator table; the
//! decoder re-derives the pixel grid in u32/i64 — no floats anywhere.

/// Minimal research error — mirror of the shipped lib's Error::Format arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub detail: String,
}

impl Error {
    pub(crate) fn fmt_msg(msg: impl Into<String>) -> Self {
        Error { detail: msg.into() }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}

pub const MODEL_CONST: u8 = 1;
pub const MODEL_DIV1D: u8 = 2;
pub const MODEL_REC1: u8 = 3;
pub const MODEL_BILIN_JOINT: u8 = 4;
pub const MODEL_BILIN_NESTED: u8 = 5;
pub const MODEL_PIECEWISE: u8 = 6;

/// Convention table for shared denominators (ADR-007). Indexed u8 → divisor.
pub fn den_from_index(idx: u8, w: u32, _h: u32) -> Result<i64, Error> {
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
        _ => Err(Error::fmt_msg("mnf10: denominator index out of table")),
    }
}

fn dim_checks(w: u32, h: u32) -> Result<(usize, usize), Error> {
    if w == 0 || h == 0 {
        return Err(Error::fmt_msg("mnf10: empty dims"));
    }
    let (wu, hu) = (w as usize, h as usize);
    if wu.checked_mul(hu).and_then(|n| n.checked_mul(3)).is_none() {
        return Err(Error::fmt_msg("mnf10: dims overflow"));
    }
    Ok((wu, hu))
}

/// Decode an MNF10 payload to RGB. `w`,`h`: claimed dims. Bounded: params
/// drive deterministic evaluation; no loops over untrusted lengths.
pub fn decode_payload(payload: &[u8], w: u32, h: u32) -> Result<Vec<u8>, Error> {
    let (wu, hu) = dim_checks(w, h)?;
    if payload.is_empty() {
        return Err(Error::fmt_msg("mnf10: empty payload"));
    }
    let model = payload[0];
    let body = &payload[1..];
    let need = |n: usize| -> Result<&[u8], Error> {
        if body.len() < n {
            Err(Error::fmt_msg("mnf10: truncated params"))
        } else {
            Ok(&body[..n])
        }
    };
    let i16le = |o: usize| i16::from_le_bytes([body[o], body[o + 1]]);
    let u16le = |o: usize| u16::from_le_bytes([body[o], body[o + 1]]);
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
            // per channel: i32 N, i32 T (8 B each); u8 shared D index last
            // (layout: 1 id + 3*8 params + 1 didx = 26)
            let p = need(25)?; // params only (body excludes the id byte)
            let didx = p[24];
            let d = den_from_index(didx, w, h)?;
            let mut v = vec![0u8; wu * hu * 3];
            for c in 0..3 {
                let n = i32::from_le_bytes(p[8 * c..8 * c + 4].try_into().unwrap()) as i64;
                let t = i32::from_le_bytes(p[8 * c + 4..8 * c + 8].try_into().unwrap()) as i64;
                for x in 0..wu {
                    let val = (n * x as i64 + t).div_euclid(d);
                    if !(0..=255).contains(&val) {
                        return Err(Error::fmt_msg("mnf10: div-1D value out of byte range"));
                    }
                    for y in 0..hu {
                        v[(y * wu + x) * 3 + c] = val as u8;
                    }
                }
            }
            v
        }
        MODEL_REC1 => {
            // per channel: u8 a0, i16 d (3 B x 3 = 9), u8 orientation at 9
            let p = need(10)?;
            let orientation = p[9];
            let mut v = vec![0u8; wu * hu * 3];
            let lin = if orientation == 0 { wu } else { hu };
            let mut line = vec![0u8; lin * 3];
            for c in 0..3 {
                let a0 = u32::from(p[3 * c]) as i64;
                let d = i64::from(i16le(3 * c + 1));
                for k in 0..lin {
                    let val = ((a0 + d * k as i64).rem_euclid(256)) as u8;
                    line[3 * k + c] = val;
                }
            }
            for k in 0..(if orientation == 0 { hu } else { wu }) {
                for j in 0..(if orientation == 0 { wu } else { hu }) {
                    let (x, y) = if orientation == 0 { (j, k) } else { (k, j) };
                    v[(y * wu + x) * 3..(y * wu + x) * 3 + 3]
                        .copy_from_slice(&line[3 * j..3 * j + 3]);
                }
            }
            v
        }
        MODEL_BILIN_JOINT => {
            let p = need(12)?; // corners c00,c10,c01,c11 * RGB
            if w == 1 || h == 1 {
                return Err(Error::fmt_msg("mnf10: bilinear degenerate dims"));
            }
            let mut v = vec![0u8; wu * hu * 3];
            let dm = u64::from(w - 1) * u64::from(h - 1);
            let wm1 = u64::from(w - 1);
            let hm1 = u64::from(h - 1);
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
            if w == 1 || h == 1 {
                return Err(Error::fmt_msg("mnf10: bilinear degenerate dims"));
            }
            let mut v = vec![0u8; wu * hu * 3];
            let dx = i64::from(w - 1);
            let dy = i64::from(h - 1);
            let hx = dx / 2;
            let hy = dy / 2;
            for c in 0..3 {
                let (v00, v10, v01, v11) = (
                    u64::from(p[c]) as i64,
                    u64::from(p[3 + c]) as i64,
                    u64::from(p[6 + c]) as i64,
                    u64::from(p[9 + c]) as i64,
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
                            return Err(Error::fmt_msg("mnf10: nested value out of range"));
                        }
                        v[(y * wu + x) * 3 + c] = val as u8;
                    }
                }
            }
            v
        }
        MODEL_PIECEWISE => {
            // u8 nseg; (nseg-1) u16 boundaries; per seg/chan: i32 N, i32 T, u8 Didx
            if body.is_empty() {
                return Err(Error::fmt_msg("mnf10: piecewise truncated"));
            }
            let nseg = body[0] as usize;
            if !(1..=32).contains(&nseg) {
                return Err(Error::fmt_msg("mnf10: piecewise segment count"));
            }
            let nb = nseg - 1;
            let p = need(1 + nb * 2 + nseg * 27)?;
            let mut bounds = vec![0usize];
            for k in 0..nb {
                let b = u16le(1 + k * 2) as usize;
                if b == 0 || b >= wu {
                    return Err(Error::fmt_msg("mnf10: piecewise boundary out of range"));
                }
                bounds.push(b);
            }
            if bounds.len() < 2 || *bounds.last().expect("nonempty") != wu {
                // decoder requires final boundary == w implicitly
                bounds.push(wu);
            }
            let mut v = vec![0u8; wu * hu * 3];
            for si in 0..bounds.len() - 1 {
                let a = bounds[si];
                let b = bounds[si + 1];
                for c in 0..3 {
                    let o = 1 + nb * 2 + si * 27 + c * 9;
                    let n = i32::from_le_bytes(p[o..o + 4].try_into().unwrap()) as i64;
                    let t = i32::from_le_bytes(p[o + 4..o + 8].try_into().unwrap()) as i64;
                    // per-segment natural denominators: the convention table
                    // evaluated at the SEGMENT width (seglen-1 / seglen), not
                    // the full width — indices 3/4 are segment-relative.
                    let segw = (b - a) as u32;
                    let d = match p[o + 8] {
                        3 => i64::from(segw.saturating_sub(1).max(1)),
                        4 => i64::from(segw),
                        other => den_from_index(other, w, h)?,
                    };
                    for xi in a..b {
                        let sx = (xi - a) as i64;
                        let val = (n * sx + t).div_euclid(d);
                        if !(0..=255).contains(&val) {
                            return Err(Error::fmt_msg("mnf10: piecewise value out of range"));
                        }
                        for y in 0..hu {
                            v[(y * wu + xi) * 3 + c] = val as u8;
                        }
                    }
                }
            }
            v
        }
        _ => return Err(Error::fmt_msg("mnf10: reserved model id")),
    };
    Ok(out)
}

/// Encoder-side: emit the minimal exact-fit payload for a rule already proven
/// exact against the source (the discovery solver's job to prove it).
pub struct PiecewiseSeg {
    pub x_start: usize,
    pub per_channel: [(i32, i32, u8); 3], // (N, T, D-index)
}

pub fn payload_div1d(per_channel: [(i32, i32); 3], d_idx: u8) -> Vec<u8> {
    let mut p = vec![MODEL_DIV1D];
    for (n, t) in per_channel {
        p.extend_from_slice(&n.to_le_bytes());
        p.extend_from_slice(&t.to_le_bytes());
    }
    p.push(d_idx);
    p
}

pub fn payload_rec1(per_channel: [(u8, i16); 3], orientation: u8) -> Vec<u8> {
    let mut p = vec![MODEL_REC1];
    for (a0, d) in per_channel {
        p.push(a0);
        p.extend_from_slice(&d.to_le_bytes());
    }
    p.push(orientation);
    p
}

pub fn payload_bilinear(corners: [[u8; 3]; 4], nested: bool) -> Vec<u8> {
    let mut p = vec![if nested {
        MODEL_BILIN_NESTED
    } else {
        MODEL_BILIN_JOINT
    }];
    for trip in corners {
        p.extend_from_slice(&trip);
    }
    p
}

pub fn payload_const(rgb: [u8; 3]) -> Vec<u8> {
    vec![MODEL_CONST, rgb[0], rgb[1], rgb[2]]
}

pub fn payload_piecewise(segs: &[PiecewiseSeg]) -> Result<Vec<u8>, Error> {
    if segs.is_empty() || segs.len() > 32 {
        return Err(Error::fmt_msg("mnf10: piecewise segment count"));
    }
    let mut p = vec![MODEL_PIECEWISE, segs.len() as u8];
    let mut sorted_segs: Vec<&PiecewiseSeg> = segs.iter().collect();
    sorted_segs.sort_by_key(|s| s.x_start);
    for k in 0..segs.len() - 1 {
        p.extend_from_slice(&(sorted_segs[k + 1].x_start as u16).to_le_bytes());
    }
    for s in sorted_segs {
        for (n, t, d) in s.per_channel {
            p.extend_from_slice(&n.to_le_bytes());
            p.extend_from_slice(&t.to_le_bytes());
            p.push(d);
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_eval_exact(payload: &[u8], w: u32, h: u32, expect: &[u8]) {
        let got = decode_payload(payload, w, h).expect("decode");
        assert_eq!(got, expect, "mnf10 decode must be exact");
    }

    #[test]
    fn div1d_floor_lerp_exact() {
        // Jon's home turf: 20→230 over 160 (D = w-1 = 159): N=210, T=3180
        // (verified independently in the probe; decode must reproduce).
        let payload = payload_div1d([(210, 3180), (-210, 36_570), (140, 9540)], 3);
        // decode at 160x120 and compare against the equation
        let img = decode_payload(&payload, 160, 120).expect("decode");
        for x in 0..160usize {
            for (c, (a, b)) in [(0usize, (20i64, 230i64)), (1, (230, 20)), (2, (60, 200))] {
                let expect = (a * 159 + x as i64 * (b - a)).div_euclid(159) as u8;
                assert_eq!(img[x * 3 + c], expect, "x={x} ch{c}");
            }
        }
        assert_eval_exact(&payload, 160, 120, &img);
    }

    #[test]
    fn rec1_mod256_exact() {
        let payload = payload_rec1([(30u8, 7i16), (200, 7), (90, 7)], 0);
        let img = decode_payload(&payload, 160, 120).expect("decode");
        for x in 0..160usize {
            for (c, a) in [(0usize, 30i64), (1, 200), (2, 90)] {
                let expect = ((a + 7 * x as i64).rem_euclid(256)) as u8;
                assert_eq!(img[x * 3 + c], expect, "x={x} ch{c}");
            }
        }
    }

    #[test]
    fn bilinear_joint_exact_roundtrip() {
        let payload = payload_bilinear(
            [[15, 200, 70], [225, 30, 180], [90, 90, 20], [245, 240, 240]],
            false,
        );
        let img = decode_payload(&payload, 160, 120).expect("decode");
        // decoder-formula recompute (independent):
        let (w, h) = (160usize, 120usize);
        let (v00, v10, v01, v11) = (
            [15u64, 200, 70],
            [225, 30, 180],
            [90, 90, 20],
            [245, 240, 240],
        );
        for c in 0..3 {
            let dm = ((w - 1) * (h - 1)) as u64;
            for y in 0..h {
                for x in 0..w {
                    let num = v00[c] * ((w - 1 - x) * (h - 1 - y)) as u64
                        + v10[c] * (x * (h - 1 - y)) as u64
                        + v01[c] * ((w - 1 - x) * y) as u64
                        + v11[c] * (x * y) as u64;
                    let expect = (num / dm) as u8;
                    assert_eq!(img[(y * w + x) * 3 + c], expect, "x={x} y={y} ch{c}");
                }
            }
        }
    }

    #[test]
    fn piecewise_exact_with_declared_bounds() {
        // two segments: [0,80) ramps 20→130/10→120/60→150; [80,160)
        // continues 130→230/120→235/150→200 — per-segment D = seglen-1 = 79
        // via the segment-relative convention indices (3 = seglen-1).
        let (n1, t1, c1) = (110i32, 20 * 79, 0u8); // ch0 seg1: N=110, T=1580
        let _ = (n1, t1, c1);
        let segs = vec![
            PiecewiseSeg {
                x_start: 0,
                per_channel: [(110, 1580, 3), (110, 790, 3), (90, 4740, 3)],
            },
            PiecewiseSeg {
                x_start: 80,
                per_channel: [(100, 10_270, 3), (115, 9_480, 3), (70, 11_850, 3)],
            },
        ];
        let payload = payload_piecewise(&segs).expect("payload");
        let img = decode_payload(&payload, 160, 120).expect("decode");
        let w = 160usize;
        for x in 0..w {
            let (a, b, sx) = if x < 80 {
                (20i64, 130i64, x as i64)
            } else {
                (130i64, 230i64, (x - 80) as i64)
            };
            let expect = (a + sx * (b - a) / 79) as u8;
            assert_eq!(img[x * 3], expect, "ch0 x={x}");
        }
    }

    #[test]
    fn malformed_payloads_error_not_panic() {
        let (w, h) = (160u32, 120u32);
        // reserved ids
        for id in [0u8, 7, 42, 255] {
            assert!(decode_payload(&[id], w, h).is_err());
            assert!(decode_payload(&[id, 0, 0, 0], w, h).is_err());
        }
        // truncations for every model
        let full = payload_div1d([(210, 3180), (-210, 36_570), (140, 9540)], 3);
        for cut in [0usize, 1, 8, 16] {
            assert!(decode_payload(&full[..1 + cut], w, h).is_err(), "cut={cut}");
        }
        // D index out of table
        let bad_d = payload_div1d([(210, 3180), (-210, 36_570), (140, 9540)], 9);
        assert!(decode_payload(&bad_d, w, h).is_err());
        // piecewise boundary >= w
        let segs = vec![
            PiecewiseSeg {
                x_start: 0,
                per_channel: [(110, 1580, 3), (110, 790, 3), (90, 4740, 3)],
            },
            PiecewiseSeg {
                x_start: 999,
                per_channel: [(100, 10_270, 3), (115, 9_480, 3), (70, 11_850, 3)],
            },
        ];
        let bad = payload_piecewise(&segs).unwrap();
        assert!(decode_payload(&bad, 160, 120).is_err());
        // bilinear at degenerate dims
        let bil = payload_bilinear([[0, 0, 0], [1, 1, 1], [2, 2, 2], [3, 3, 3]], false);
        assert!(decode_payload(&bil, 1, 120).is_err());
        assert!(decode_payload(&bil, 160, 1).is_err());
        // empty payload
        assert!(decode_payload(&[], w, h).is_err());
    }
}
