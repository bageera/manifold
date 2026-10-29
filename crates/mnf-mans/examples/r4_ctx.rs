// MNF13-R4/R5 bridge — adaptation-curve ablation + dual-timescale
// mixture coding of the ACTUAL v1.12 residual wires.
// (Extends r1_ctx.rs per R4's own recommendation: the R5 hypothesis —
// per-context fast/slow estimators combined by a deterministic,
// decoder-reproducible EWA weight rule.)
//
// stdin protocol: line 1 = "n mode" where mode =
//   "a<THETA>"    single adaptive model, halving cap THETA
//   "mix<F>_<S>"  dual estimators (caps F and S) mixed by an EWA
//                 weight updated per symbol from the DECODED symbol
//                 (zero transport; deterministic, exact both sides)
// then n bytes symbols + n bytes ctx keys.
// stdout: "<encoded_len> ok"  (exact roundtrip asserted internally).
//
// Machinery: constriction 0.5 stream range coder (the rans_mixed.rs
// precedent); fixed-point probabilities, no fp in the wire.

use constriction::stream::{
    model::ContiguousCategoricalEntropyModel, queue::DefaultRangeDecoder,
    queue::DefaultRangeEncoder, Decode, Encode,
};
use std::io::Read;

const SCALE: u64 = 1 << 24;

/// Fixed-point rebalance: remainder to entry 0, then pull any zero
/// slots up to 1 from the densest entry.
fn rebalance(mut p: Vec<u64>) -> Vec<u64> {
    let sum_p: u64 = p.iter().sum();
    p[0] += SCALE - sum_p;
    let mut dense = 0usize;
    for (i, &v) in p.iter().enumerate() {
        if v > p[dense] {
            dense = i;
        }
    }
    for (i, v) in p.iter_mut().enumerate() {
        if *v == 0 && i != dense {
            *v = 1;
        }
    }
    // second pass: subtract the stolen ones from the densest (may need
    // several if many zeros — count them first)
    let zeros = p.iter().filter(|&&v| v == 0).count();
    let need = zeros as u64;
    p[dense] = p[dense].max(1 + need);
    for v in p.iter_mut() {
        if *v == 0 {
            *v = 1;
        }
    }
    p
}

struct Adaptive {
    counts: Vec<u64>,
    theta: u64, // halving cap (R4's adaptation limit)
}

impl Adaptive {
    fn new(nsym: usize, theta: u64) -> Self {
        Self {
            counts: vec![1; nsym],
            theta,
        }
    }
    /// Fixed-point probability vector over the FULL alphabet, all >= 1
    /// (every symbol must stay decodable); remainder to the densest.
    fn probs(&self) -> Vec<u64> {
        let total: u64 = self.counts.iter().sum();
        let p: Vec<u64> = self.counts.iter().map(|&c| c * SCALE / total).collect();
        rebalance(p)
    }
    fn model(&self) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
        let p: Vec<u32> = self.probs().iter().map(|&v| v as u32).collect();
        ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            p, false,
        )
        .ok()
    }
    fn update(&mut self, s: u32) {
        self.counts[s as usize] += 32;
        let total: u64 = self.counts.iter().sum();
        if total > self.theta {
            for c in &mut self.counts {
                *c = (*c + 1) >> 1;
            }
        }
    }
}

/// R5: fast + slow estimators, mixed by an exponentiated-weight rule
/// updated from the DECODED symbol (identical evolution both sides =
/// exact inverse, zero transport).
struct Dual {
    fast: Adaptive,
    slow: Adaptive,
    log_w: f64, // log weight of fast, EWA in log-space (numerically safe)
}

impl Dual {
    fn new(fast: u64, slow: u64) -> Self {
        Self {
            fast: Adaptive::new(256, fast),
            slow: Adaptive::new(256, slow),
            log_w: 0.0,
        }
    }
    fn mixed_probs(&self) -> Vec<u64> {
        let total_f = self.fast.counts.iter().sum::<u64>() as f64;
        let total_s = self.slow.counts.iter().sum::<u64>() as f64;
        let wf = 1.0 / (1.0 + (-self.log_w).exp());
        let mixed: Vec<u64> = self
            .fast
            .counts
            .iter()
            .zip(&self.slow.counts)
            .map(|(&cf, &cs)| {
                ((wf * (cf as f64 / total_f) + (1.0 - wf) * (cs as f64 / total_s)) * SCALE as f64)
                    as u64
            })
            .collect();
        rebalance(mixed)
    }
    fn model(&self) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
        let p: Vec<u32> = self.mixed_probs().iter().map(|&v| v as u32).collect();
        ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            p, false,
        )
        .ok()
    }
    fn step(&mut self, s: u32) {
        // EWA update in log space: log w += lr * (log pf - log ps)
        let total_f = self.fast.counts.iter().sum::<u64>() as f64;
        let total_s = self.slow.counts.iter().sum::<u64>() as f64;
        let pf = (self.fast.counts[s as usize] as f64 / total_f).max(1e-9);
        let ps = (self.slow.counts[s as usize] as f64 / total_s).max(1e-9);
        const LR: f64 = 0.15;
        self.log_w = (self.log_w + LR * (pf.ln() - ps.ln())).clamp(-20.0, 20.0);
        self.fast.update(s);
        self.slow.update(s);
    }
}

enum CtxModels {
    Single(Vec<Adaptive>),
    Mix(Vec<Dual>),
}

fn main() {
    let mut stdin = Vec::new();
    std::io::stdin().read_to_end(&mut stdin).expect("stdin");
    let hdr_len = stdin.iter().position(|&b| b == b'\n').expect("header");
    let hdr = String::from_utf8(stdin[..hdr_len].to_vec()).expect("utf8");
    let mut it = hdr.split_whitespace();
    let n: usize = it.next().expect("n").parse().expect("n");
    let mode = it.next().unwrap_or_else(|| "a4096").to_string();
    let body = &stdin[hdr_len + 1..];
    if body.len() < 2 * n {
        eprintln!("ERR short body");
        return;
    }
    let syms: Vec<u32> = body[..n].iter().map(|&b| u32::from(b)).collect();
    let ck: Vec<usize> = body[n..2 * n]
        .iter()
        .map(|&b| usize::from(b) & 0x7F)
        .collect();
    let nctx = ck.iter().copied().max().map(|m| m + 1).unwrap_or(1);

    let mut models = if let Some(rest) = mode.strip_prefix("mix") {
        let mut p = rest.split('_');
        let f: u64 = p.next().unwrap_or("512").parse().unwrap_or(512);
        let s: u64 = p.next().unwrap_or("16384").parse().unwrap_or(16384);
        CtxModels::Mix((0..nctx).map(|_| Dual::new(f, s)).collect())
    } else {
        let t: u64 = mode
            .strip_prefix('a')
            .unwrap_or("4096")
            .parse()
            .unwrap_or(4096);
        CtxModels::Single((0..nctx).map(|_| Adaptive::new(256, t)).collect())
    };

    // ---- encode
    macro_rules! model_of {
        ($m:ident, $c:expr) => {
            match $m {
                CtxModels::Single(ref v) => v[$c].model(),
                CtxModels::Mix(ref v) => v[$c].model(),
            }
        };
    }
    let mut enc = DefaultRangeEncoder::new();
    for i in 0..n {
        let m = match model_of!(models, ck[i]) {
            Some(m) => m,
            None => {
                eprintln!("ERR model ctx {} at {}", ck[i], i);
                return;
            }
        };
        enc.encode_symbols(std::iter::once((syms[i] as usize, &m)))
            .expect("encode sym");
        match &mut models {
            CtxModels::Single(v) => v[ck[i]].update(syms[i]),
            CtxModels::Mix(v) => v[ck[i]].step(syms[i]),
        }
    }
    let compressed = enc.into_compressed().expect("sealed");
    let nbytes = compressed.len() * std::mem::size_of::<u32>();

    // ---- decode (exact mirror) — SEPARATE decoder-side models built
    // fresh (the encoder's `models` carry the encode-side evolution)
    let mut models = if let Some(rest) = mode.strip_prefix("mix") {
        let mut p = rest.split('_');
        let f: u64 = p.next().unwrap_or("512").parse().unwrap_or(512);
        let sl: u64 = p.next().unwrap_or("16384").parse().unwrap_or(16384);
        CtxModels::Mix((0..nctx).map(|_| Dual::new(f, sl)).collect())
    } else {
        let t: u64 = mode
            .strip_prefix('a')
            .unwrap_or("4096")
            .parse()
            .unwrap_or(4096);
        CtxModels::Single((0..nctx).map(|_| Adaptive::new(256, t)).collect())
    };
    let mut dec = match DefaultRangeDecoder::from_compressed(compressed.as_slice().to_vec()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("ERR decoder open {e:?}");
            return;
        }
    };
    let mut back: Vec<u32> = Vec::with_capacity(n);
    for i in 0..n {
        let m = match model_of!(models, ck[i]) {
            Some(m) => m,
            None => {
                eprintln!("ERR decode ctx {} at {}", ck[i], i);
                return;
            }
        };
        let s = match dec
            .decode_symbols(std::iter::once(&m))
            .next()
            .expect("symbol")
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "ERR decode sym at {i} (enc={:?} ctx={}): {e:?}",
                    syms[i], ck[i]
                );
                return;
            }
        } as u32;
        back.push(s);
        match &mut models {
            CtxModels::Single(v) => v[ck[i]].update(s),
            CtxModels::Mix(v) => v[ck[i]].step(s),
        }
    }
    assert_eq!(back, syms, "R4/R5 bridge exact roundtrip");
    println!("{nbytes} ok");
}
