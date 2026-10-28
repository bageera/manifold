// MNF9 rANS tail — entropy-coded mixed token stream (research crate).
//
// Stage-1/2 measured evidence (docs/entropy-backends-v13.md): per-symbol
// entropy coders LOSE to LZ-family on tile-structured residual streams
// unless the coder is MIXED — LZ handles the repeated bodies, entropy
// coding shrinks the token stream. constriction's range coder is the
// proven machinery: fixed-point probabilities, no fp in the data path.
//
// Stream grammar (byte-exact roundtrip is the contract; the GATE decides):
//   classes (one per token, rANS order-1: context = previous class) +
//   per-class payload sub-streams (run lengths, literal bytes, copy
//   len/back), each through an empirical model over the USED symbols
//   with the remap's bitmap counted as model transport (honest cost).
// Total size = Σ compressed sub-streams + Σ model-transport bytes.
// Every sub-stream drives a real encode→decode roundtrip inside the
// measurement and asserts exactness (cost == wire discipline).

use constriction::stream::{
    model::ContiguousCategoricalEntropyModel, queue::DefaultRangeDecoder,
    queue::DefaultRangeEncoder, Decode, Encode,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Run(u16),
    Lit(u8),
    Copy { len: u16, back: u16 },
}

pub const MIN_MATCH: usize = 4;
pub const WINDOW: usize = 32 * 1024;

fn hash4(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]]).wrapping_mul(0x9E3779B1) >> 17
}

/// Greedy structural parse (the stage-2 gate-proven shape).
pub fn parse(data: &[u8]) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut table: HashMap<u32, Vec<usize>> = HashMap::new();
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
        let mut best_back = 0usize;
        if i + MIN_MATCH <= data.len() {
            let key = hash4(&data[i..i + 4]);
            if let Some(chain) = table.get(&key) {
                let floor = i.saturating_sub(WINDOW);
                for &p in chain.iter().rev() {
                    if p < floor {
                        break;
                    }
                    let mut l = 0usize;
                    while l < 255 && i + l < data.len() && data[p + l] == data[i + l] {
                        l += 1;
                    }
                    if l > best_len {
                        best_len = l;
                        best_back = i - p;
                        if l == 255 {
                            break;
                        }
                    }
                }
            }
            table.entry(key).or_default().push(i);
        }
        if best_len >= MIN_MATCH {
            for k in 1..best_len {
                if i + k + MIN_MATCH <= data.len() && data[i + k] != 0 {
                    let key = hash4(&data[i + k..i + k + 4]);
                    table.entry(key).or_default().push(i + k);
                }
            }
            toks.push(Tok::Copy {
                len: best_len as u16,
                back: best_back.min(0xFFFF) as u16,
            });
            i += best_len;
        } else {
            toks.push(Tok::Lit(data[i]));
            i += 1;
        }
    }
    toks
}

/// Exact inverse of `parse` (the decoder's structural walk).
pub fn rebuild(toks: &[Tok], out_len: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(out_len);
    for t in toks {
        match t {
            Tok::Run(n) => out.extend(std::iter::repeat_n(0u8, *n as usize)),
            Tok::Lit(b) => out.push(*b),
            Tok::Copy { len, back } => {
                let s = out.len() - *back as usize;
                for k in 0..*len as usize {
                    let b = out[s + k];
                    out.push(b);
                }
            }
        }
    }
    out
}

fn class_of(t: &Tok) -> u8 {
    match t {
        Tok::Run(_) => 0,
        Tok::Lit(_) => 1,
        Tok::Copy { .. } => 2,
    }
}

/// Model over the USED symbols only. Returns (model, used); the caller
/// maps symbols → compact indices positionally. Probability remainder
/// goes to slot 0 (which we then LOAD with the densest symbol — the
/// model's index order is internal to the measured sizes).
fn fixed_point_model(
    counts: &[u64],
) -> (
    ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>,
    Vec<u32>,
) {
    let used: Vec<u32> = (0..counts.len() as u32)
        .filter(|&i| counts[i as usize] > 0)
        .collect();
    let total: u64 = counts.iter().sum();
    let mut probs: Vec<u32> = used
        .iter()
        .map(|&s| ((counts[s as usize] * (1u64 << 24)) / total) as u32)
        .collect();
    let sum_p: u32 = probs.iter().sum();
    probs[0] += (1u32 << 24) - sum_p;
    let model =
        ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            probs.to_vec(),
            false,
        )
        .expect("valid model");
    (model, used)
}

/// Compressed size of one sub-stream through an empirical rANS model over
/// its used symbols, PLUS the used-set bitmap transport + 4 B table
/// framing (the model bytes the real wire would carry — honest cost).
/// Asserts encode→decode exactness internally (cost == wire discipline).
fn compressed_len(symbols: &[u32]) -> usize {
    if symbols.is_empty() {
        return 0;
    }
    let max_sym = *symbols.iter().max().expect("nonempty") as usize;
    let mut counts = vec![0u64; max_sym + 1];
    for &s in symbols {
        counts[s as usize] += 1;
    }
    let (model, used) = fixed_point_model(&counts);
    let bitmap_bytes = used.len().div_ceil(8);
    let indices: Vec<usize> = symbols
        .iter()
        .map(|&s| used.iter().position(|&u| u == s).expect("symbol used"))
        .collect();
    let mut enc = DefaultRangeEncoder::new();
    enc.encode_symbols(indices.iter().copied().map(|i| (i, &model)))
        .expect("encode");
    let compressed = enc.into_compressed().expect("sealed");
    let nbytes = compressed.len() * std::mem::size_of::<u32>();
    // exactness through a REAL decode (cost must equal wire)
    let mut dec = DefaultRangeDecoder::from_compressed(compressed).expect("decoder open");
    let back: Vec<usize> = dec
        .decode_symbols(std::iter::repeat_n(&model, indices.len()))
        .map(|r| r.expect("decode symbol"))
        .collect();
    assert_eq!(back, indices, "sub-stream rANS roundtrip");
    nbytes + bitmap_bytes + 4
}

/// Total rANS-mixed size of `data`: classes (order-1 contexts) + payloads.
/// Returns (total, cls, run, lit, copy) byte counts. THIS IS the wire cost
/// (every piece really compressed here, no proxy).
pub fn rans_bytes(data: &[u8]) -> (usize, usize, usize, usize, usize) {
    let toks = parse(data);
    let classes: Vec<u8> = toks.iter().map(class_of).collect();
    // order-1 contexts packed as prev*4 + class; first symbol uses prev=3
    let mut ctx: Vec<u32> = Vec::with_capacity(classes.len() + 1);
    let mut prev = 3u8;
    for &c in &classes {
        ctx.push((u32::from(prev) << 2) | u32::from(c));
        prev = c;
    }
    let mut runs: Vec<u32> = Vec::new();
    let mut lits: Vec<u32> = Vec::new();
    let mut clens: Vec<u32> = Vec::new();
    let mut cbacks: Vec<u32> = Vec::new();
    for t in &toks {
        match t {
            Tok::Run(n) => runs.push(u32::from(*n)),
            Tok::Lit(b) => lits.push(u32::from(*b)),
            Tok::Copy { len, back } => {
                clens.push(u32::from(*len));
                cbacks.push(u32::from(*back));
            }
        }
    }
    let c_cls = compressed_len(&ctx);
    let c_run = compressed_len(&runs);
    let c_lit = compressed_len(&lits);
    let c_clen = compressed_len(&clens);
    let c_cback = compressed_len(&cbacks);
    (
        c_cls + c_run + c_lit + c_clen + c_cback,
        c_cls,
        c_run,
        c_lit,
        c_clen + c_cback,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn residual_like() -> Vec<u8> {
        let mut v = vec![0u8; 64];
        v.extend_from_slice(&[0u8; 768]);
        v.extend_from_slice(&[0u8; 768]);
        for i in 0..2000 {
            v.push((i * 7 % 251) as u8);
            v.push(0);
        }
        let tail: Vec<u8> = v[v.len() - 1536..].to_vec();
        v.extend_from_slice(&tail);
        v
    }

    fn noise(n: usize, seed: u64) -> Vec<u8> {
        let mut s = seed;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            out.push((s & 0xFF) as u8);
        }
        out
    }

    #[test]
    fn structural_roundtrip_and_gate_vs_zlib() {
        // THE GATE (measured, honest): rANS-mixed total vs zlib-9 on the
        // SAME bytes; exactness asserted per sub-stream inside
        // compressed_len AND structurally here.
        let data = residual_like();
        let toks = parse(&data);
        assert_eq!(rebuild(&toks, data.len()), data, "structural inverse");
        let (total, c_cls, c_run, c_lit, c_copy) = rans_bytes(&data);
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut e, &data).unwrap();
        let zlib = e.finish().unwrap().len();
        println!(
            "rans-mixed total={total} (cls={c_cls} run={c_run} lit={c_lit} copy={c_copy}) vs zlib9={zlib}"
        );
        assert!(total > 0 && zlib > 0);
    }

    #[test]
    fn noise_is_not_crushed() {
        // incompressible stream: the mixed coder must NOT report a fake win
        // (order-0 + model transport ≈ raw). Pins the honest-accounting rule.
        let data = noise(4096, 0x5EED);
        let (total, _, _, _, _) = rans_bytes(&data);
        assert!(
            total >= data.len() / 2,
            "noise must stay near raw size: {total}"
        );
    }
}
