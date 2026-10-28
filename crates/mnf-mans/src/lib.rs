// MNF8 research: mixed run/copy + contextual-rANS residual coder (prototype).
//
// Design fixed by the measured stage-1/2 evidence (docs/entropy-backends-v13.md):
// pure per-symbol entropy coders lose to LZ on tile-structured residual
// streams; a competitive custom coder must be MIXED:
//   Layer S (structural): byte-level LZ — literal / zero-run / (dist,len)
//     copies chosen greedily (min match 4, 32 KiB window, 64-probe chains).
//   Layer E (entropy): the token stream's class sequence, extra payloads
//     (len/dist), and literal bytes are sized with an empirical-entropy
//     accounting (table transport included); the rANS machinery itself is
//     proven by an actual constriction range-coder roundtrip on the class
//     sequence (gate test).
// Decoder = exact inverse everywhere; deterministic; no fp in reconstruction.
//
// RESEARCH instrument for the retention gate — no magic, no format. Gate
// numbers: crates/mnf-mans tests (mixed_beats_zlib_gate_report) compare
// against zlib-9 on identical streams.

pub mod lz77_2d;
pub mod rans_mixed;
pub use lz77_2d::{decode_plane, encode_plane, pick_plane, zlib9_len, Stats};

use constriction::stream::{
    model::ContiguousCategoricalEntropyModel, queue::DefaultRangeDecoder,
    queue::DefaultRangeEncoder, Decode, Encode,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Lit(u8),
    Run(u16), // zero run (residual streams are zero-heavy)
    Copy { len: u16, dist: u16 },
}

pub const MIN_MATCH: usize = 4;
pub const MAX_MATCH: usize = 258;

/// Structural LZ parse: literals, zero runs, greedy matches.
pub fn parse(data: &[u8]) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut table: HashMap<usize, Vec<usize>> = HashMap::new(); // hash -> positions
    let mut i = 0usize;
    while i < data.len() {
        if data[i] == 0 {
            let start = i;
            while i < data.len() && data[i] == 0 {
                i += 1;
            }
            let mut left = i - start;
            while left > 0xFFFF {
                toks.push(Tok::Run(0xFFFF));
                left -= 0xFFFF;
            }
            if left > 0 {
                toks.push(Tok::Run(left as u16));
            }
            continue;
        }
        let mut best_len = 0usize;
        let mut best_dist = 0usize;
        if i + MIN_MATCH <= data.len() {
            let key = hash4(&data[i..i + 4]);
            if let Some(chain) = table.get(&key) {
                let floor = i.saturating_sub(0x8000);
                for &p in chain.iter().rev() {
                    if p < floor {
                        break;
                    }
                    let dist = i - p;
                    let mut l = 0usize;
                    while l < MAX_MATCH && i + l < data.len() && data[p + l] == data[i + l] {
                        l += 1;
                    }
                    if l > best_len {
                        best_len = l;
                        best_dist = dist;
                        if l == MAX_MATCH {
                            break;
                        }
                    }
                }
            }
            table.entry(key).or_default().push(i);
        }
        if best_len >= MIN_MATCH {
            // insert intermediate positions so later tokens can match into
            // the middle of this copy (cheap, keeps chains useful)
            for k in 1..best_len {
                if i + k + MIN_MATCH <= data.len() && data[i + k] != 0 {
                    let key = hash4(&data[i + k..i + k + 4]);
                    table.entry(key).or_default().push(i + k);
                }
            }
            toks.push(Tok::Copy {
                len: best_len as u16,
                dist: best_dist as u16,
            });
            i += best_len;
        } else {
            toks.push(Tok::Lit(data[i]));
            i += 1;
        }
    }
    toks
}

fn hash4(b: &[u8]) -> usize {
    let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    ((v.wrapping_mul(0x9E3779B1)) >> 17) as usize
}

/// Token classes for the entropy pass:
///   0 Lit, 1 Run, 2 Copy(len<=16, dist<=64), 3 Copy(len<=16, dist>64),
///   4 Copy(len>16, dist<=64), 5 Copy(len>16, dist>64)
pub fn class_of(t: &Tok) -> u8 {
    match t {
        Tok::Lit(_) => 0,
        Tok::Run(_) => 1,
        Tok::Copy { len, dist } => {
            let long = *len > 16u16;
            let far = *dist > 64u16;
            match (long, far) {
                (false, false) => 2,
                (false, true) => 3,
                (true, false) => 4,
                (true, true) => 5,
            }
        }
    }
}

/// Rebuild `data` from tokens (the decoder's structural inverse).
pub fn rebuild(toks: &[Tok], out_len: usize) -> Vec<u8> {
    let mut back: Vec<u8> = Vec::with_capacity(out_len);
    for t in toks {
        match t {
            Tok::Lit(b) => back.push(*b),
            Tok::Run(n) => back.extend(std::iter::repeat_n(0u8, *n as usize)),
            Tok::Copy { len, dist } => {
                let s = back.len() - *dist as usize;
                for k in 0..*len as usize {
                    let b = back[s + k];
                    back.push(b);
                }
            }
        }
    }
    back
}

/// Ideal-entropy size estimate (bytes, table transport included) of the
/// mixed representation of `data`:
///   classes (static within-stream model)
/// + extra payloads u16 (len/dist per non-lit token; context = class)
/// + literals (order-1: prev-lit high-2bits × byte)
pub fn mixed_bytes(data: &[u8]) -> u64 {
    let toks = parse(data);
    let classes: Vec<u8> = toks.iter().map(class_of).collect();
    let mut extras: Vec<Vec<u16>> = vec![
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    let mut lit0: Vec<Vec<u8>> = (0..4).map(|_| Vec::new()).collect();
    let mut prev_lit = 0u8;
    for t in &toks {
        match t {
            Tok::Lit(b) => {
                lit0[(prev_lit >> 6) as usize].push(*b);
                prev_lit = *b;
            }
            Tok::Run(n) => extras[1].push(*n),
            Tok::Copy { len, dist } => {
                let c = class_of(t) as usize;
                extras[c].push(*len);
                extras[c].push(*dist);
            }
        }
    }
    let mut bits = empirical_bits_u8(&classes, 2);
    for e in &extras {
        if !e.is_empty() {
            bits += empirical_bits_u16(e, 2);
        }
    }
    for l in &lit0 {
        if !l.is_empty() {
            bits += empirical_bits_u8(l, 2);
        }
    }
    bits.div_ceil(8)
}

fn empirical_bits_u8(vals: &[u8], bytes_per_used_symbol: u64) -> u64 {
    let mut counts = [0u64; 256];
    for &v in vals {
        counts[v as usize] += 1;
    }
    let total = vals.len() as f64;
    let bits: f64 = counts
        .iter()
        .filter(|&&n| n > 0)
        .map(|&n| bytes_per_used_symbol as f64 + n as f64 * (total / n as f64).log2())
        .sum();
    bits as u64
}

fn empirical_bits_u16(vals: &[u16], bytes_per_used_symbol: u64) -> u64 {
    let mut counts = [0u64; 0x1_0000];
    for &v in vals {
        counts[v as usize] += 1;
    }
    let total = vals.len() as f64;
    let bits: f64 = counts
        .iter()
        .filter(|&&n| n > 0)
        .map(|&n| bytes_per_used_symbol as f64 + n as f64 * (total / n as f64).log2())
        .sum();
    bits as u64
}

/// Proof-of-machinery: the class sequence through a real constriction
/// range coder (queue): encode then decode, assert agreement, report bytes.
/// constriction symbols are `usize`; models are passed alongside symbols.
pub fn rans_roundtrip(classes: &[u8]) -> (Vec<u8>, usize) {
    let mut freq = [0u32; 6];
    for &c in classes {
        freq[c as usize] += 1;
    }
    for f in freq.iter_mut() {
        *f = (*f).max(1);
    }
    // fixed-point probabilities at PRECISION=24 directly (no fp round trip):
    let total: u32 = freq.iter().sum();
    let probs: Vec<u32> = freq
        .iter()
        .map(|&f| ((f as u64 * (1u64 << 24)) / total as u64) as u32)
        .collect();
    // fix rounding so the probabilities fill to 1<<24 exactly (append the
    // remainder to the FIRST nonzero class instead of assuming the last is nonzero)
    let used: u32 = probs.iter().sum();
    let first_nz = probs.iter().position(|&p| p > 0).expect("nonempty support");
    let mut adjusted = probs.clone();
    adjusted[first_nz] += (1u32 << 24) - used;
    let model = ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
        adjusted.iter().copied().filter(|&p| p > 0).collect::<Vec<_>>(),
        false,
    )
    .expect("nonzero-prob model");
    let mut enc = DefaultRangeEncoder::new();
    enc.encode_symbols(classes.iter().copied().map(|c| (c as usize, &model)))
        .expect("encode");
    let compressed = enc.into_compressed().expect("sealed");
    let bytes_out = compressed.len() * std::mem::size_of::<u32>();
    let mut dec = DefaultRangeDecoder::from_compressed(compressed).expect("decoder");
    let back: Vec<u8> = dec
        .decode_symbols(std::iter::repeat_n(&model, classes.len()))
        .map(|r| r.expect("decode symbol") as u8)
        .collect();
    (back, bytes_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn residual_stream() -> Vec<u8> {
        // synthetic MED-like residual stream: mostly zeros, structured repeats
        let mut v = vec![0u8; 64];
        v.extend_from_slice(&[0u8; 768]);
        v.extend_from_slice(&[0u8; 768]); // repeated zero tile
        for i in 0..2000 {
            // structured non-zero region: periodic bytes every 2nd slot
            v.push((i * 7 % 251) as u8);
            v.push(0);
        }
        let tail: Vec<u8> = v[v.len() - 1536..].to_vec();
        v.extend_from_slice(&tail); // exact repeat of last block
        v
    }

    #[test]
    fn structural_parse_rebuilds_exactly() {
        let data = residual_stream();
        let toks = parse(&data);
        assert_eq!(rebuild(&toks, data.len()), data, "LZ parse inverse");
    }

    #[test]
    fn rans_class_stream_roundtrip() {
        let toks = parse(&residual_stream());
        let classes: Vec<u8> = toks.iter().map(class_of).collect();
        let (back, nbytes) = rans_roundtrip(&classes);
        assert_eq!(back, classes, "constriction range coder roundtrip");
        println!(
            "classes: {} tokens -> {} bytes (rANS real coder)",
            classes.len(),
            nbytes
        );
    }

    #[test]
    fn mixed_beats_zlib_gate_report() {
        // RETENTION GATE (measured then decided — honest-reporting rule):
        let data = residual_stream();
        let mixed = mixed_bytes(&data);
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut e, &data).unwrap();
        let zlib = e.finish().unwrap().len();
        println!("gate(research stream): mixed={mixed} zlib9={zlib}");
        assert!(mixed > 0 && zlib > 0);
    }
}
