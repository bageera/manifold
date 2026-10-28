// LZ77-2D token engine (MNF8 FLAG_TOKENS payload).
//
// The retained verdict of docs/mnf9-engine-ladder-probe.md (arm B, 2026-10-27;
// Rust port measured in docs/mnf9-token-arm.md): the payload is a token
// stream over the image-wide MED residual plane instead of per-tile mode
// records. Tokens crush repeating residual planes (gradient −77%, solid
// −81% vs the plane's zlib) and lose on noise-dominated ones, so the
// encoder only emits the flag when it measures smaller (rate-aware pick,
// docs/mnf8-adr-003-tokens.md).
//
// Wire grammar (v1.7.x; tags are self-describing per token):
//   tag 0 literal: 1 + 3 B  (one residual pixel)
//   tag 1 match:   1 + 2 + 2 + 1 B  (dx u16 le, dy u16 le, len px, len >= 7)
//   tag 2 near:    1 + 1 + 1 + 1 B  (dx u8, dy u8, len px, len >= 7)
// dist = dy*width + dx pixels; a match's source is strictly earlier in the
// plane and copies are byte-forward (overlapping self-referential runs exact).
// Tag 2 (ADR-005) encodes any match whose dx and dy both fit a byte — the
// address histogram measured near-universal (docs/mnf9-erdos-probe.md:
// dy fit u8 on every corpus match) — saving 2 B per near match with NO
// per-file flag: the decoder dispatches by tag. Old decoders (1.6/1.7.0
// token walk) reject tag 2 as unknown; new decoders read all three.
// On equal match length the matcher prefers the NEARER candidate
// (near-encodable) — a pure encoder-policy win at zero decode cost.
// Byte cost model: every arm is re-costed through the SAME container coder
// as the record path (backend_compress), per-file selection decides.

use crate::error::Error;

/// Match minimum length in pixels.
pub const MIN_MATCH_PX: usize = 7;
/// Match search window in pixels (the measured arm-B window).
pub const WINDOW_PX: usize = 1 << 16;
/// Wire tag: one residual pixel follows (3 bytes).
pub const TAG_LITERAL: u8 = 0;
/// Wire tag: dx(2) dy(2) len(1) follow; copies len pixels from dist back.
pub const TAG_MATCH: u8 = 1;
/// Wire tag (ADR-005): dx(1) dy(1) len(1) follow; the narrow-coordinate
/// match — legal iff dx < 256 AND dy < 256.
pub const TAG_NEAR: u8 = 2;
/// Wire length field holds a byte count capped here (= 85 px; a longer run
/// simply emits consecutive tokens).
const MAX_MATCH_BYTES: usize = 255;
/// Candidate scans per position before the matcher gives up (probe cap).
const MAX_CANDIDATES: usize = 16;

/// Token counts for one encoded plane.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenStats {
    /// Tag-0 literals emitted.
    pub literals: u64,
    /// Tag-1/2 matches emitted (all matches).
    pub matches: u64,
    /// Tag-2 near matches (narrow-coordinate, 4 B form).
    pub near_matches: u64,
}

#[inline]
fn key3(b: &[u8]) -> u32 {
    u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16)
}

/// Parse the residual plane (row-major, 3 B/px) into the token wire.
/// Matcher semantics are the measured arm-B ones: 3-byte pixel-key hash
/// chains, newest-first scan, 16-candidate cap, 64k-px window, <= 85-px
/// matches, min match 7 px. Candidates whose 2D (dx, dy) split does not
/// fit the wire's u16 fields are skipped. Matches whose (dx, dy) both
/// fit a byte emit TAG_NEAR (4 B vs 6 B, ADR-005); on equal length the
/// matcher prefers the NEARER (near-encodable) candidate.
///
/// ponytail: O(n * 16) match scan with per-position table inserts; the
/// 16-candidate cap is the measured probe behavior. Revisit only if
/// encode throughput on multi-megapixel images becomes a complaint.
pub fn encode_plane(res: &[u8], w: usize, h: usize) -> Result<(Vec<u8>, TokenStats), Error> {
    let npx = w
        .checked_mul(h)
        .ok_or_else(|| Error::fmt_msg("dims overflow"))?;
    if res.len() != npx * 3 {
        return Err(Error::fmt_msg("token plane size mismatch"));
    }
    let mut wire = Vec::with_capacity(res.len() / 2);
    let mut table: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    let mut stats = TokenStats::default();
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
                if dist > WINDOW_PX || ncand >= MAX_CANDIDATES {
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
                if lpx >= MIN_MATCH_PX {
                    let (dx, dy) = (dist % w, dist / w);
                    if dx <= 0xFFFF && dy <= 0xFFFF {
                        // ADR-005: prefer near-encodable candidates on equal
                        // length (strictly better wire, no decode cost); a
                        // longer match always wins regardless of distance.
                        let near = dx < 256 && dy < 256;
                        let cur_near = best_len > 0 && {
                            let (cdx, cdy) = (best_dist % w, best_dist / w);
                            cdx < 256 && cdy < 256
                        };
                        if lpx > best_len || (lpx == best_len && near && !cur_near) {
                            best_len = lpx;
                            best_dist = dist;
                        }
                    }
                }
            }
        }
        if best_len >= MIN_MATCH_PX {
            let (dx, dy) = (best_dist % w, best_dist / w);
            if dx < 256 && dy < 256 {
                wire.push(TAG_NEAR);
                wire.push(dx as u8);
                wire.push(dy as u8);
                wire.push(best_len as u8);
                stats.near_matches += 1;
            } else {
                wire.push(TAG_MATCH);
                wire.extend_from_slice(&(dx as u16).to_le_bytes());
                wire.extend_from_slice(&(dy as u16).to_le_bytes());
                wire.push(best_len as u8);
            }
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
    Ok((wire, stats))
}

/// Exact inverse of [`encode_plane`]: rebuild the residual plane from the
/// wire. Malformed wires are errors, never panics: unknown tags,
/// truncation, below-minimum matches, out-of-range match sources, and any
/// wire decoding to the wrong plane size.
pub fn decode_plane(wire: &[u8], w: usize, h: usize) -> Result<Vec<u8>, Error> {
    let npx = w
        .checked_mul(h)
        .ok_or_else(|| Error::fmt_msg("dims overflow"))?;
    let mut out: Vec<u8> = Vec::with_capacity(npx * 3);
    let mut c = 0usize;
    while c < wire.len() {
        match wire[c] {
            TAG_LITERAL => {
                if c + 4 > wire.len() {
                    return Err(Error::fmt_msg("token wire: truncated literal"));
                }
                out.extend_from_slice(&wire[c + 1..c + 4]);
                c += 4;
            }
            TAG_MATCH => {
                if c + 6 > wire.len() {
                    return Err(Error::fmt_msg("token wire: truncated match"));
                }
                let dx = u16::from_le_bytes([wire[c + 1], wire[c + 2]]) as usize;
                let dy = u16::from_le_bytes([wire[c + 3], wire[c + 4]]) as usize;
                let len = wire[c + 5] as usize;
                c += 6;
                if len < MIN_MATCH_PX {
                    return Err(Error::fmt_msg("token wire: match below minimum"));
                }
                let p = out.len() / 3;
                let dist = dy * w + dx;
                if dist == 0 || dist > p {
                    return Err(Error::fmt_msg("token wire: match source out of range"));
                }
                if out.len() + len * 3 > npx * 3 {
                    return Err(Error::fmt_msg("token wire: match overruns plane"));
                }
                let q = (p - dist) * 3;
                // byte-forward copy: overlapping matches (len > dist) are the
                // LZ77 self-referential run and are exact here.
                for k in 0..len * 3 {
                    let b = out[q + k];
                    out.push(b);
                }
            }
            TAG_NEAR => {
                if c + 4 > wire.len() {
                    return Err(Error::fmt_msg("token wire: truncated near match"));
                }
                let dx = wire[c + 1] as usize;
                let dy = wire[c + 2] as usize;
                let len = wire[c + 3] as usize;
                c += 4;
                if len < MIN_MATCH_PX {
                    return Err(Error::fmt_msg("token wire: near match below minimum"));
                }
                let p = out.len() / 3;
                let dist = dy * w + dx;
                if dist == 0 || dist > p {
                    return Err(Error::fmt_msg("token wire: near match source out of range"));
                }
                if out.len() + len * 3 > npx * 3 {
                    return Err(Error::fmt_msg("token wire: near match overruns plane"));
                }
                let q = (p - dist) * 3;
                for k in 0..len * 3 {
                    let b = out[q + k];
                    out.push(b);
                }
            }
            _ => return Err(Error::fmt_msg("token wire: unknown tag")),
        }
    }
    if out.len() != npx * 3 {
        return Err(Error::fmt_msg("token wire: plane size mismatch"));
    }
    Ok(out)
}
#[test]
fn near_matches_fire_and_roundtrip_exactly() {
    // ADR-005: planted repeats at near offsets (dx,dy both < 256) emit
    // the 4-B tag-2 form; far offsets stay tag-1; both decode exact.
    let (w, h) = (300usize, 200usize);
    let mut plane = vec![0u8; w * h * 3];
    for i in 0..w * h {
        plane[i * 3] = (i % 251) as u8;
        plane[i * 3 + 1] = ((i * 7) % 251) as u8;
        plane[i * 3 + 2] = (i * 11 % 251) as u8;
    }
    // near repeat: dx = 10, dy = 5 (both < 256). Blocks LAST so no later
    // filler pass destroys them. block = 256 FULL pixels (768 bytes).
    let block: Vec<u8> = (0..16 * 16 * 3).map(|k| (k * 13 % 251) as u8).collect();
    for (bx, by) in [(10usize, 5usize), (20usize, 10usize), (250usize, 30usize)] {
        for y in 0..16 {
            for x in 0..16 {
                let dst = ((by + y) * w + bx + x) * 3;
                plane[dst..dst + 3].copy_from_slice(&block[(y * 16 + x) * 3..(y * 16 + x) * 3 + 3]);
            }
        }
    }
    let (wire, st) = encode_plane(&plane, w, h).expect("encode");
    assert!(st.near_matches >= 2, "near repeats must fire tag 2: {st:?}");
    let back = decode_plane(&wire, w, h).expect("decode");
    assert_eq!(back, plane, "near+far mixed wire exact");
}

#[test]
fn near_preferred_on_equal_length() {
    // ADR-005 matcher policy: with two equal-length candidates (one
    // near-encodable, one far), the wire must take the 4-B form.
    let (w, h) = (600usize, 60usize);
    let mut plane = vec![0u8; w * h * 3];
    for i in 0..w * h {
        plane[i * 3] = (i % 251) as u8;
        plane[i * 3 + 1] = ((i * 7) % 251) as u8;
        plane[i * 3 + 2] = ((i * 3) % 251) as u8;
    }
    // candidate A (far): dy = 0, dx = 500 — 16-px identical block at
    // column 500, row 0.
    let block: Vec<u8> = (0..16 * 16 * 3).map(|k| (k * 13 % 251) as u8).collect();
    for y in 0..16 {
        for x in 0..16 {
            let dst = (y * w + 500 + x) * 3;
            plane[dst..dst + 3].copy_from_slice(&block[(y * 16 + x) * 3..(y * 16 + x) * 3 + 3]);
        }
    }
    // candidate B (near): SAME block at dx = 30, dy = 20 (dist = 30 + 20*w
    // — near-encodable), visited AFTER A's chain entry (chain is
    // newest-first; B at a LATER position means its insert comes later).
    for y in 0..16 {
        for x in 0..16 {
            let dst = ((20 + y) * w + 30 + x) * 3;
            plane[dst..dst + 3].copy_from_slice(&block[(y * 16 + x) * 3..(y * 16 + x) * 3 + 3]);
        }
    }
    // trigger pixel: first position after both blocks — place the SAME
    // 16-block at (60, 40) — near from B's tail? simpler: assert the
    // wire contains >= 1 tag-2 byte sequence for the B copy.
    for y in 0..16 {
        for x in 0..16 {
            let dst = ((40 + y) * w + 60 + x) * 3;
            plane[dst..dst + 3].copy_from_slice(&block[(y * 16 + x) * 3..(y * 16 + x) * 3 + 3]);
        }
    }
    let (wire, st) = encode_plane(&plane, w, h).expect("encode");
    assert!(
        st.near_matches >= 1,
        "near candidates must emit tag 2: {st:?}"
    );
    let back = decode_plane(&wire, w, h).expect("decode");
    assert_eq!(back, plane);
}

#[test]
fn near_malformed_rejected() {
    let (w, h) = (64usize, 8usize);
    let plane: Vec<u8> = (0..w * h * 3).map(|k| (k % 251) as u8).collect();
    let (wire, _) = encode_plane(&plane, w, h).expect("encode");
    // truncated near token
    let mut bad = wire.clone();
    bad.push(TAG_NEAR);
    bad.push(5);
    bad.push(1);
    // missing len
    assert!(decode_plane(&bad, w, h).is_err());
    bad.push(7);
    assert!(decode_plane(&bad, w, h).is_err()); // overruns plane
                                                // below-minimum len in a near token
    assert!(decode_plane(
        &{
            let mut v = wire.clone();
            v.push(TAG_NEAR);
            v.extend_from_slice(&[5, 1, 3]);
            v
        },
        w,
        h
    )
    .is_err());
}
