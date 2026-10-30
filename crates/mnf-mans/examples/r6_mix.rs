// MNF13-R6 bridge: the dual-timescale mix coder with parametrized
// adaptation/quantization knobs (measurement-only; the shipped mix_coder
// stays untouched — this is the instrument).
//
// stdin: line 1 = "n mode\n"; then n symbols; then n context keys
// (one byte each). stdout:
//   'c' (arg 1): raw wire bytes ([u32 stream_len][LE words])
//   'e' (arg 1): "<encoded_len> ok"
// mode (arg 2) = hyphen-separated tokens:
//   mix<F>_<S>  fast/slow halving caps (default 4096_65536)
//   lr<X>       EWA learning rate ×100 (lr15 = 0.15)
//   inc<N>      per-symbol count increment (default 32)
//   s<N>        mixed-vector fixed-point scale 2^N before the coder's 2^24
//               round (default 24)
//   ctx8|ctx16  informational tag (python precomputes the key stream)
// The exact-inverse contract (fresh decode-side models, per-symbol
// evolution from the DECODED symbol, decode(encode) == symbols) is
// asserted on EVERY run before any size is printed.
use constriction::stream::{
    model::ContiguousCategoricalEntropyModel, queue::DefaultRangeDecoder,
    queue::DefaultRangeEncoder, Decode, Encode,
};
use std::io::Read;

#[derive(Clone)]
struct Dual {
    fast: Vec<u64>,
    slow: Vec<u64>,
    log_w: f64,
    cap_fast: u64,
    cap_slow: u64,
    lr: f64,
    inc: u64,
    scale: u64,
}

impl Dual {
    fn new(cap_fast: u64, cap_slow: u64, lr: f64, inc: u64, scale: u64) -> Self {
        Self {
            fast: vec![1; 256],
            slow: vec![1; 256],
            log_w: 0.0,
            cap_fast,
            cap_slow,
            lr,
            inc,
            scale,
        }
    }

    /// Mixed fixed-point vector at this context's own scale; every
    /// symbol >= 1; total exactly `scale`.
    ///
    /// Rebalance: remainder to slot 0, zeros topped to 1 from the
    /// densest entry.
    fn probs(&self) -> Vec<u64> {
        let tf = self.fast.iter().sum::<u64>() as f64;
        let ts = self.slow.iter().sum::<u64>() as f64;
        let wf = 1.0 / (1.0 + (-self.log_w).exp());
        let mut p: Vec<u64> = self
            .fast
            .iter()
            .zip(&self.slow)
            .map(|(&cf, &cs)| {
                ((wf * (cf as f64 / tf) + (1.0 - wf) * (cs as f64 / ts)) * self.scale as f64) as u64
            })
            .collect();
        let sum: u64 = p.iter().sum();
        p[0] += self.scale - sum;
        let mut dense = 0usize;
        for (i, &v) in p.iter().enumerate() {
            if v > p[dense] {
                dense = i;
            }
        }
        let zeros = p.iter().filter(|&&v| v == 0).count();
        p[dense] = p[dense].max(1 + zeros as u64);
        for v in p.iter_mut() {
            if *v == 0 {
                *v = 1;
            }
        }
        p
    }

    fn model(&self) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
        // constriction's stream models are fixed at 2^24. Scale != 2^24 =
        // the quantization axis: round the mixed vector to its own scale
        // first, THEN lift to 2^24 (coarser probabilities, same coder).
        let p24: Vec<u64> = if self.scale == 1 << 24 {
            self.probs()
        } else {
            let q = self.probs();
            let sum: u64 = q.iter().sum();
            let mut s: Vec<u64> = q.iter().map(|&v| v * (1 << 24) / sum).collect();
            let total: u64 = s.iter().sum();
            s[0] += (1 << 24) - total;
            let mut dense = 0usize;
            for (i, &v) in s.iter().enumerate() {
                if v > s[dense] {
                    dense = i;
                }
            }
            let zeros = s.iter().filter(|&&v| v == 0).count();
            s[dense] = s[dense].max(1 + zeros as u64);
            for v in s.iter_mut() {
                if *v == 0 {
                    *v = 1;
                }
            }
            s
        };
        let p32: Vec<u32> = p24.iter().map(|&v| v as u32).collect();
        ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            p32,
            false,
        )
        .ok()
    }

    fn step(&mut self, s: u8) {
        let si = usize::from(s);
        let tf = self.fast.iter().sum::<u64>() as f64;
        let ts = self.slow.iter().sum::<u64>() as f64;
        let pf = (self.fast[si] as f64 / tf).max(1e-9);
        let ps = (self.slow[si] as f64 / ts).max(1e-9);
        self.log_w = (self.log_w + self.lr * (pf.ln() - ps.ln())).clamp(-20.0, 20.0);
        self.fast[si] += self.inc;
        self.slow[si] += self.inc;
        if self.fast.iter().sum::<u64>() > self.cap_fast {
            for c in &mut self.fast {
                *c = (*c + 1) >> 1;
            }
        }
        if self.slow.iter().sum::<u64>() > self.cap_slow {
            for c in &mut self.slow {
                *c = (*c + 1) >> 1;
            }
        }
    }
}

fn parse_mode(mode: &str) -> (u64, u64, f64, u64, u64) {
    let mut cap_f = 4096u64;
    let mut cap_s = 65536u64;
    let mut lr = 0.15f64;
    let mut inc = 32u64;
    let mut scale = 1u64 << 24;
    for tok in mode.split('-') {
        if let Some(rest) = tok.strip_prefix("mix") {
            let mut p = rest.split('_');
            cap_f = p.next().unwrap_or("4096").parse().unwrap_or(4096);
            cap_s = p.next().unwrap_or("65536").parse().unwrap_or(65536);
        } else if let Some(rest) = tok.strip_prefix("lr") {
            let v: u64 = rest.parse().unwrap_or(15);
            lr = v as f64 / 100.0;
        } else if let Some(rest) = tok.strip_prefix("inc") {
            inc = rest.parse().unwrap_or(32);
        } else if let Some(rest) = tok.strip_prefix("s") {
            scale = 1u64 << rest.parse::<u32>().unwrap_or(24);
        }
        // ctx8/ctx16/anchor tokens: informational (keys precomputed)
    }
    (cap_f, cap_s, lr, inc, scale)
}

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "e".into());
    let mode = std::env::args().nth(2).unwrap_or_else(|| {
        eprintln!("usage: r6_mix <c|e> <mode>");
        std::process::exit(2);
    });
    let mut stdin = Vec::new();
    std::io::stdin().read_to_end(&mut stdin).expect("stdin");
    let hdr_len = stdin.iter().position(|&b| b == b'\n').expect("header");
    let hdr = String::from_utf8(stdin[..hdr_len].to_vec()).expect("utf8");
    let mut it = hdr.split_whitespace();
    let n: usize = it.next().expect("n").parse().expect("n");
    let body = &stdin[hdr_len + 1..];
    if body.len() < 2 * n {
        eprintln!("ERR short body");
        std::process::exit(2);
    }
    let syms: Vec<u32> = body[..n].iter().map(|&b| u32::from(b)).collect();
    let keys: Vec<usize> = body[n..2 * n].iter().map(|&b| usize::from(b)).collect();
    let nctx = keys.iter().copied().max().map(|m| m + 1).unwrap_or(1);

    let (cap_f, cap_s, lr, inc, scale) = parse_mode(&mode);
    let mut enc_models: Vec<Dual> = (0..nctx)
        .map(|_| Dual::new(cap_f, cap_s, lr, inc, scale))
        .collect();

    let mut enc = DefaultRangeEncoder::new();
    for i in 0..n {
        let m = enc_models[keys[i]].model().expect("model buildable");
        enc.encode_symbols(std::iter::once((syms[i] as usize, &m)))
            .expect("encode sym");
        enc_models[keys[i]].step(syms[i] as u8);
    }
    let compressed = enc.into_compressed().expect("seal");

    // exact-inverse assert: fresh decode-side models, per-symbol evolution
    // from the DECODED symbol (never reusing the encoder's models)
    let mut dec_models: Vec<Dual> = (0..nctx)
        .map(|_| Dual::new(cap_f, cap_s, lr, inc, scale))
        .collect();
    let mut dec =
        DefaultRangeDecoder::from_compressed(compressed.as_slice().to_vec()).expect("decoder open");
    let mut back: Vec<u32> = Vec::with_capacity(n);
    for i in 0..n {
        let m = dec_models[keys[i]].model().expect("model buildable");
        let s = dec
            .decode_symbols(std::iter::once(&m))
            .next()
            .expect("symbol")
            .expect("decode sym");
        back.push(s as u32);
        dec_models[keys[i]].step(s as u8);
    }
    assert_eq!(back, syms, "R6 bridge exact roundtrip");

    if cmd == "c" {
        let mut out = Vec::with_capacity(compressed.len() * 4 + 4);
        out.extend_from_slice(&u32::try_from(n).expect("len fits").to_le_bytes());
        for w in &compressed {
            out.extend_from_slice(&w.to_le_bytes());
        }
        use std::io::Write;
        std::io::stdout().write_all(&out).expect("write");
    } else {
        println!("{} ok", compressed.len() * 4);
    }
}
