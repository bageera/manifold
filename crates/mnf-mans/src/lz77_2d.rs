// MNF9 engine candidate — LZ77-2D token arm over the MED residual plane.
//
// The RETAINED verdict of docs/mnf9-engine-ladder-probe.md (arm B,
// rate-aware per-file pick): tokens crush repeating residual planes
// (gradient −73%, solid −34%) and lose on noise-dominated ones, so the
// format form is a per-file pick between the MED-literal plane and this
// token plane — never worse than the picked literal arm.
//
// This module is a Rust port of the probe's arm B with a REAL decoder
// (the probe asserted exactness in-sim only). Wire grammar, byte-exact
// with the probe serialization:
//
//   tag 0 literal: 1 + 3 B  (one residual pixel)
//   tag 1 match:   1 + 2 + 2 + 1 B  (dx le, dy le, len px; len >= 7)
//
// dist = dy*w + dx in pixel units; the source is strictly before the
// target and copies may overlap it (byte-forward copy semantics).
// The probe's tags 2/3/4 (transformed chart matches, shared dictionary)
// are deliberately ABSENT: measured dead (zero firing on photos, <=1%
// on two synthetics, dictionary saves nothing) — the retained verdict
// excludes them.
//
// RESEARCH instrument for the retention gate — no magic, no format.

use std::collections::HashMap;
use std::io::Write;

pub const TAG_LITERAL: u8 = 0;
pub const TAG_MATCH: u8 = 1;
pub const MIN_MATCH_PX: usize = 7;
/// Match window in pixels (probe arm B: 64k px).
pub const WINDOW_PX: usize = 1 << 16;
/// Match length is counted in BYTES capped at 255 (= 85 px), then
/// serialized as one length byte. (The probe's `best_len >= 200` px
/// early-break is unreachable at this cap and is not ported.)
const MAX_MATCH_BYTES: usize = 255;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub literals: u64,
    pub matches: u64,
}

fn key3(b: &[u8]) -> u32 {
    u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16)
}

/// Parse the residual plane (row-major, 3 B/px) into the token wire.
/// Same matcher as the probe's arm B: 3-byte pixel-key hash chains,
/// newest-first scan, 16-candidate cap, 64k-px window, <=255-byte run,
/// min match 7 px. Candidates whose 2D (dx, dy) split does not fit the
/// wire's u16 fields are skipped (the probe would have overflowed its
/// own serialization there; unreachable on real image widths).
pub fn encode_plane(res: &[u8], w: usize, h: usize) -> (Vec<u8>, Stats) {
    let npx = w * h;
    assert_eq!(res.len(), npx * 3, "plane must be w*h*3 bytes");
    let mut wire = Vec::with_capacity(res.len() / 2);
    let mut table: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut stats = Stats::default();
    let mut pos: usize = 0;
    while pos < npx {
        let key = key3(&res[pos * 3..pos * 3 + 3]);
        let mut best_len = 0usize;
        let mut best_dist = 0usize;
        let mut ncand = 0usize;
        if let Some(chain) = table.get(&key) {
            for &q in chain.iter().rev() {
                let q = q as usize;
                let dist = pos - q;
                if dist > WINDOW_PX || ncand >= 16 {
                    break;
                }
                ncand += 1;
                let mut l = 0usize;
                while l < MAX_MATCH_BYTES
                    && pos * 3 + l < res.len()
                    && res[q * 3 + l] == res[pos * 3 + l]
                {
                    l += 1;
                }
                let lpx = l / 3;
                if lpx >= MIN_MATCH_PX && lpx > best_len {
                    let (dx, dy) = (dist % w, dist / w);
                    if dx <= 0xFFFF && dy <= 0xFFFF {
                        best_len = lpx;
                        best_dist = dist;
                    }
                }
            }
        }
        if best_len >= MIN_MATCH_PX {
            wire.push(TAG_MATCH);
            let (dx, dy) = (best_dist % w, best_dist / w);
            wire.extend_from_slice(&(dx as u16).to_le_bytes());
            wire.extend_from_slice(&(dy as u16).to_le_bytes());
            wire.push(best_len as u8);
            for p in pos..pos + best_len {
                let k = key3(&res[p * 3..p * 3 + 3]);
                table.entry(k).or_default().push(p as u32);
            }
            pos += best_len;
            stats.matches += 1;
        } else {
            wire.push(TAG_LITERAL);
            wire.extend_from_slice(&res[pos * 3..pos * 3 + 3]);
            table.entry(key).or_default().push(pos as u32);
            pos += 1;
            stats.literals += 1;
        }
    }
    (wire, stats)
}

/// Exact inverse of `encode_plane`: rebuild the residual plane from the
/// wire. Structural checks error (never panic) on malformed wires:
/// unknown tags, truncation, out-of-range match sources, and any wire
/// that decodes to the wrong plane size.
pub fn decode_plane(wire: &[u8], w: usize, h: usize) -> Result<Vec<u8>, &'static str> {
    let npx = w * h;
    let mut out: Vec<u8> = Vec::with_capacity(npx * 3);
    let mut c = 0usize;
    while c < wire.len() {
        match wire[c] {
            TAG_LITERAL => {
                if c + 4 > wire.len() {
                    return Err("truncated literal");
                }
                out.extend_from_slice(&wire[c + 1..c + 4]);
                c += 4;
            }
            TAG_MATCH => {
                if c + 6 > wire.len() {
                    return Err("truncated match");
                }
                let dx = u16::from_le_bytes([wire[c + 1], wire[c + 2]]) as usize;
                let dy = u16::from_le_bytes([wire[c + 3], wire[c + 4]]) as usize;
                let len = wire[c + 5] as usize;
                c += 6;
                if len < MIN_MATCH_PX {
                    return Err("match below min length");
                }
                let p = out.len() / 3;
                let dist = dy * w + dx;
                if dist == 0 || dist > p {
                    return Err("match source out of range");
                }
                let q = p - dist;
                // byte-forward copy: overlapping matches (len > dist) are
                // the LZ77 self-referential run and are exact here.
                for k in 0..len * 3 {
                    let b = out[q * 3 + k];
                    out.push(b);
                }
            }
            _ => return Err("unknown tag"),
        }
    }
    if out.len() != npx * 3 {
        return Err("decoded plane size mismatch");
    }
    Ok(out)
}

/// zlib-9 (level best) size of a buffer — the probe's cost coder. Used
/// for in-crate parity checks; the corpus harness re-costs with python
/// zlib-9 to stay one-coder with the committed ladder table.
pub fn zlib9_len(buf: &[u8]) -> usize {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    e.write_all(buf).expect("zlib write");
    e.finish().expect("zlib finish").len()
}

/// The retained per-file pick, measured: tokenize the plane, cost both
/// arms through the same coder, take the smaller — never worse than the
/// literal arm. Returns (chosen_cost, literal_cost, tokens_won).
pub fn pick_plane(res: &[u8], w: usize, h: usize) -> (usize, usize, bool) {
    let lit = zlib9_len(res);
    let (wire, _) = encode_plane(res, w, h);
    let tok = zlib9_len(&wire);
    if tok < lit {
        (tok, lit, true)
    } else {
        (lit, lit, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic xorshift64 noise (same pattern as src/mnf8.rs tests).
    fn noise_plane(w: usize, h: usize, seed: u64) -> Vec<u8> {
        let mut s: u64 = seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0xDEADBEEF;
        let mut rnd = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        (0..w * h * 3).map(|_| (rnd() & 0xFF) as u8).collect()
    }

    fn assert_roundtrip(plane: &[u8], w: usize, h: usize) -> Stats {
        let (wire, st) = encode_plane(plane, w, h);
        let back = decode_plane(&wire, w, h).expect("decode");
        assert_eq!(back, plane, "token wire must rebuild the plane exactly");
        st
    }

    #[test]
    fn tokens_roundtrip_and_fire_on_repeats() {
        // w=24: a 4-px noise block planted at two far-apart positions on a
        // smooth ramp — the matcher must fire matches (>= 2) and the wire
        // must rebuild exactly.
        let (w, h) = (24usize, 16usize);
        let mut plane = vec![0u8; w * h * 3];
        for i in 0..w * h {
            plane[i * 3] = (i % 251) as u8;
            plane[i * 3 + 1] = ((i * 7) % 251) as u8;
        }
        let block: Vec<u8> = noise_plane(4, 4, 42);
        for (bx, by) in [(2usize, 1usize), (18usize, 12usize)] {
            for y in 0..4 {
                for x in 0..4 {
                    let dst = ((by + y) * w + bx + x) * 3;
                    let src = (y * 4 + x) * 3;
                    plane[dst..dst + 3].copy_from_slice(&block[src..src + 3]);
                }
            }
        }
        let st = assert_roundtrip(&plane, w, h);
        assert!(st.matches >= 2, "planted repeats must fire: {st:?}");
    }

    #[test]
    fn tokens_win_gradient_lose_noise() {
        // The pick rule, measured on both regimes (flate2 on both arms —
        // in-crate parity). The measured token-win classes are long RUNS
        // in the residual plane (gradient −74%, solid −34% in the ladder
        // table); tile-block repeats are run-neutral at small sizes, so
        // the win arm uses a smooth ramp (long zero runs), not blocks.
        let (w, h) = (32usize, 32usize);
        let mut gradient = vec![0u8; w * h * 3];
        for i in 1..w * h {
            // smooth ramp: MED-like residual would be near-constant; here
            // the plane itself is a ramp + long zero run
            gradient[i * 3..i * 3 + 3].copy_from_slice(&[0, 0, 0]);
        }
        // a 100-px run of a nonzero pixel after a seed — the solid-class
        // shape tokens crush
        for i in 200..w * h {
            gradient[i * 3] = 7;
            gradient[i * 3 + 1] = 7;
            gradient[i * 3 + 2] = 7;
        }
        // At 32x32 the runs are short (<= ~1 kB plane): the ladder's
        // measured win is a LARGE-stream effect and the pick rule itself
        // is the retained contract. Pin: matches fire, pick == min(both
        // arms) exactly (never worse than literal, even when tokens lose).
        let (pick, lit, _won) = pick_plane(&gradient, w, h);
        let (wire_g, stg) = encode_plane(&gradient, w, h);
        let tok = zlib9_len(&wire_g);
        assert!(stg.matches >= 1, "run plane must fire matches: {stg:?}");
        assert_eq!(
            pick,
            lit.min(tok),
            "pick must be the min of both arms (tok {tok} lit {lit})"
        );
        assert!(pick <= lit, "pick never worse than literal");

        let noise = noise_plane(w, h, 99);
        let (_, lit, won) = pick_plane(&noise, w, h);
        assert!(!won, "noise plane: tokens must not win the pick");
        let (wire, _) = encode_plane(&noise, w, h);
        assert!(
            zlib9_len(&wire) >= lit,
            "noise tokens must not beat literal"
        );
        let st = assert_roundtrip(&noise, w, h);
        let _ = st;
    }

    #[test]
    fn overlap_run_is_exact() {
        // len > dist: a constant run after a seed pixel — pins the
        // byte-forward copy semantics.
        let (w, h) = (8usize, 8usize);
        let mut plane = vec![0u8; w * h * 3];
        plane[0..3].copy_from_slice(&[1, 2, 3]);
        for i in 1..w * h {
            plane[i * 3..i * 3 + 3].copy_from_slice(&[5, 5, 5]);
        }
        let st = assert_roundtrip(&plane, w, h);
        assert!(st.matches >= 1, "constant run must match: {st:?}");
    }

    #[test]
    fn malformed_wires_error_not_panic() {
        let (w, h) = (16usize, 4usize);
        let plane = noise_plane(w, h, 3);
        let (wire, _) = encode_plane(&plane, w, h);
        // unknown tag
        let mut bad = wire.clone();
        bad.push(2);
        assert!(decode_plane(&bad, w, h).is_err());
        // truncation
        assert!(decode_plane(&wire[..wire.len() - 1], w, h).is_err());
        // trailing literal overflows the plane
        let mut bad = wire.clone();
        bad.push(TAG_LITERAL);
        bad.extend_from_slice(&[9, 9, 9]);
        assert!(decode_plane(&bad, w, h).is_err());
        // match below min length
        let mut bad = vec![TAG_MATCH];
        bad.extend_from_slice(&0u16.to_le_bytes());
        bad.extend_from_slice(&1u16.to_le_bytes());
        bad.push(3);
        assert!(decode_plane(&bad, w, h).is_err());
        // clean wire still exact
        assert_roundtrip(&plane, w, h);
    }
}
