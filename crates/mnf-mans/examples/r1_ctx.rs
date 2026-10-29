// MNF13-R1 bridge — context-aware adaptive range coding of the ACTUAL
// v1.12 residual wires (Jon's promotion gate), constriction 0.5 (the
// rans_mixed.rs precedent's proven machinery).
//
// stdin protocol (from the python driver):
//   line 1: "n nctx mode"  (mode: "o1" = order-1 adaptive blocks;
//          "xc" = order-1 + causal cross-channel; "sp" = +prev-row
//          spatial bucket; ctx keys ride as the 2nd u8 per symbol)
//   then n bytes symbol + n bytes ctx-key (interleaved)
// stdout line: "<encoded_len> <ok|ERR>"
// The binary ALSO asserts an exact decode internally before printing
// (cost == wire discipline).
//
// Adaptive-block models: counts update after EVERY symbol on both
// sides; a block's model = the counts snapshot before the block
// (decoder-side recomputable exactly = zero transport).

use constriction::stream::{
    model::ContiguousCategoricalEntropyModel, queue::DefaultRangeDecoder,
    queue::DefaultRangeEncoder, Decode, Encode,
};
use std::io::Read;

const TOT: u32 = 4096;

struct Adaptive {
    counts: Vec<u64>, // per symbol
    used: Vec<u32>,
}

impl Adaptive {
    fn new(nsym: usize) -> Self {
        Self {
            counts: vec![1; nsym],
            used: (0..nsym as u32).collect(),
        }
    }
    fn model(&self) -> Option<(ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>, usize)> {
        // snapshot probabilities from counts over used symbols
        let mut used: Vec<u32> = (0..self.counts.len() as u32)
            .filter(|&i| self.counts[i as usize] > 0)
            .collect();
        let total: u64 = self.counts.iter().sum();
        let mut probs: Vec<u32> = used
            .iter()
            .map(|&s| ((self.counts[s as usize] * u64::from(1u32 << 24)) / total) as u32)
            .collect();
        let sum_p: u32 = probs.iter().sum();
        probs[0] += (1u32 << 24) - sum_p;
        // a zero-prob slot kills decode; collapse probs[0] == 0 by giving
        // the remainder to the densest
        if probs[0] == 0 {
            let dense = used
                .iter()
                .copied()
                .max_by_key(|&s| self.counts[s as usize])
                .unwrap_or(used[0]);
            let di = used.iter().position(|&u| u == dense).unwrap();
            probs[0] = 1;
            probs[di] = probs[di].saturating_sub(1);
            if probs[di] == 0 {
                return None;
            }
        }
        let m = ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            probs.clone(),
            false,
        )
        .ok()?;
        Some((m, self.counts.len()))
    }
    fn update(&mut self, s: u32) {
        self.counts[s as usize] += 32;
        // halve on overflow (both sides evolve identically)
        let total: u64 = self.counts.iter().sum();
        if total > u64::from(TOT) {
            for c in &mut self.counts {
                *c = (*c + 1) >> 1;
            }
        }
    }
}

fn main() {
    let mut stdin = Vec::new();
    std::io::stdin().read_to_end(&mut stdin).expect("stdin");
    let hdr: Vec<u8> = stdin
        .iter()
        .copied()
        .take_while(|&b| b != b'\n')
        .collect();
    let hdr = String::from_utf8(hdr).expect("utf8");
    let mut it = hdr.split_whitespace();
    let n: usize = it.next().expect("n").parse().expect("n");
    let _nctx: usize = it.next().expect("nctx").parse().expect("nctx");
    let body = &stdin[hdr.len() + 1..];
    if body.len() < 2 * n {
        eprintln!("ERR short body {} < {}", body.len(), 2 * n);
        return;
    }
    let syms: Vec<u32> = body[..n].iter().map(|&b| u32::from(b)).collect();
    let ctx: Vec<usize> = body[n..2 * n].iter().map(|&b| usize::from(b) & 0x3F).collect();
    let nctx = ctx.iter().copied().max().map(|m| m + 1).unwrap_or(1);

    // ---- encode: per-context adaptive models, chunk = 1 symbol/model
    // (simplest exact shape; constriction accepts many encode calls in
    // sequence on one encoder)
    let mut models: Vec<Adaptive> = (0..nctx).map(|_| Adaptive::new(256)).collect();
    let mut enc = DefaultRangeEncoder::new();
    for i in 0..n {
        let (m, _) = match models[ctx[i]].model() {
            Some(x) => x,
            None => {
                eprintln!("ERR model ctx {} at {}", ctx[i], i);
                return;
            }
        };
        enc.encode_symbols(std::iter::once((syms[i] as usize, &m)))
            .expect("encode sym");
        models[ctx[i]].update(syms[i]);
    }
    let compressed = enc.into_compressed().expect("sealed");
    let nbytes = compressed.len() * std::mem::size_of::<u32>();

    // ---- decode (the exact mirror) — cost must equal wire
    let mut dmodels: Vec<Adaptive> = (0..nctx).map(|_| Adaptive::new(256)).collect();
    let mut dec = DefaultRangeDecoder::from_compressed(compressed.as_slice().to_vec())
        .expect("decoder open");
    let mut back: Vec<u32> = Vec::with_capacity(n);
    for i in 0..n {
        let (m, _) = match dmodels[ctx[i]].model() {
            Some(x) => x,
            None => {
                eprintln!("ERR decode model ctx {} at {}", ctx[i], i);
                return;
            }
        };
        let s = dec
            .decode_symbols(std::iter::once(&m))
            .next()
            .expect("symbol")
            .expect("decode sym");
        back.push(s as u32);
        dmodels[ctx[i]].update(s as u32);
    }
    assert_eq!(back, syms, "R1 bridge exact roundtrip");
    println!("{nbytes} ok");
}
