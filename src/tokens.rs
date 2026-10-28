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
// Wire grammar (bit-exact with the measured prototype):
//   tag 0 literal: 1 + 3 B  (one residual pixel)
//   tag 1 match:   1 + 2 + 2 + 1 B  (dx u16 le, dy u16 le, len px, len >= 7)
// dist = dy*width + dx pixels; a match's source is strictly earlier in the
// plane and copies are byte-forward (overlapping self-referential runs exact).
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
    /// Tag-1 matches emitted.
    pub matches: u64,
}

#[inline]
fn key3(b: &[u8]) -> u32 {
    u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16)
}

/// Parse the residual plane (row-major, 3 B/px) into the token wire.
/// Matcher semantics are the measured arm-B ones: 3-byte pixel-key hash
/// chains, newest-first scan, 16-candidate cap, 64k-px window, <= 85-px
/// matches, min match 7 px. Candidates whose 2D (dx, dy) split does not
/// fit the wire's u16 fields are skipped.
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
            _ => return Err(Error::fmt_msg("token wire: unknown tag")),
        }
    }
    if out.len() != npx * 3 {
        return Err(Error::fmt_msg("token wire: plane size mismatch"));
    }
    Ok(out)
}
