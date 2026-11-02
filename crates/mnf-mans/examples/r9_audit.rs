// MNF13-R9-A bridge: the shipped backend-5 mix coder with ctx-auto key
// rules and a per-symbol PREQUENTIAL cost audit (measurement-only; the
// shipped mix_coder stays untouched — this is the instrument).
//
// stdin: line 1 = "n mode\n"; then n symbols. NO key bytes: keys are
// computed INTERNALLY by the shipped rule (mnf6.rs/mix_coder.rs byte
// rules) from prev/prev2 coded bytes — the identical schedule the wire's
// decoder recomputes, so byte-equality is checkable in both directions.
//
// stdout:
//   'e' (arg 1): raw wire bytes ([u32 stream_len][LE words]) — the
//        byte-equality anchor against the shipped payload.
//   'd' (arg 1): the decoded stream bytes (decode-side proof; fresh
//        models evolved from DECODED symbols, never encoder state)
//   'a' (arg 1): the audit: exact-roundtrip asserted, then binary
//        [u32 n][n × f64 LE] where f64_i = -log2(p_i) in bits for
//        symbol i under the model state BEFORE observing it (the
//        prequential self-information; sum = L_model_prediction).
//
// mode (arg 2) = hyphen-separated tokens (same grammar as r6_mix):
//   mix<F>_<S>  fast/slow halving caps (default = backend 5: 32768_1048576)
//   lr<X>       EWA learning rate ×100
//   inc<N>      per-symbol count increment
//   s<N>        fixed-point scale exponent (default 24)
//   ctx8|ctx16  context RULE: ctx16 = shipped backend-5 (prev bucket |
//               band<<3 of prev2, band = bucket >= 3); ctx8 = backend 4.
// The exact-inverse contract is asserted on EVERY run before anything
// is emitted: fresh decode-side models, per-symbol evolution from the
// DECODED symbol, decode(encode(bytes)) == bytes.
use constriction::stream::{
    model::ContiguousCategoricalEntropyModel, queue::DefaultRangeDecoder,
    queue::DefaultRangeEncoder, Decode, Encode,
};
use std::io::Read;

const SCALE: u64 = 1 << 24;

#[derive(Clone)]
struct Dual {
    fast: Vec<u64>,
    slow: Vec<u64>,
    log_w: f64,
    cap_fast: u64,
    cap_slow: u64,
    lr: f64,
    inc: u64,
}

impl Dual {
    fn new(cap_fast: u64, cap_slow: u64, lr: f64, inc: u64) -> Self {
        Self {
            fast: vec![1; 256],
            slow: vec![1; 256],
            log_w: 0.0,
            cap_fast,
            cap_slow,
            lr,
            inc,
        }
    }

    /// Fixed-point probability vector; every symbol >= 1; total exactly
    /// SCALE. Same rebalance as src/mix_coder.rs (remainder to slot 0).
    fn probs(&self) -> Vec<u64> {
        let tf = self.fast.iter().sum::<u64>() as f64;
        let ts = self.slow.iter().sum::<u64>() as f64;
        let wf = 1.0 / (1.0 + (-self.log_w).exp());
        let mut p: Vec<u64> = self
            .fast
            .iter()
            .zip(&self.slow)
            .map(|(&cf, &cs)| {
                ((wf * (cf as f64 / tf) + (1.0 - wf) * (cs as f64 / ts)) * SCALE as f64) as u64
            })
            .collect();
        let sum: u64 = p.iter().sum();
        p[0] += SCALE - sum;
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
        let p32: Vec<u32> = self.probs().iter().map(|&v| v as u32).collect();
        ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            p32, false,
        )
        .ok()
    }

    /// Prequential self-information of symbol s under the CURRENT state
    /// (before the update): -log2(p_s), exact under the fixed-point
    /// vector this model codes with.
    fn self_info(&self, s: u8) -> f64 {
        let p = self.probs()[usize::from(s)] as f64;
        (SCALE as f64 / p).log2()
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

/// The shipped magnitude-bucket rule (mix_coder.rs mix_ctx_bucket).
fn bucket(b: u8) -> usize {
    match b {
        0 => 0,
        1..=2 => 1,
        3..=5 => 2,
        6..=11 => 3,
        12..=24 => 4,
        25..=48 => 5,
        49..=96 => 6,
        _ => 7,
    }
}

#[derive(Clone, Copy)]
struct Cfg {
    cap_f: u64,
    cap_s: u64,
    lr: f64,
    inc: u64,
    nctx: usize,
}

fn parse_mode(mode: &str) -> Cfg {
    let mut c = Cfg {
        cap_f: 32768,
        cap_s: 1048576,
        lr: 0.15,
        inc: 32,
        nctx: 16,
    };
    for tok in mode.split('-') {
        if let Some(rest) = tok.strip_prefix("mix") {
            let mut p = rest.split('_');
            c.cap_f = p.next().unwrap_or("32768").parse().unwrap_or(32768);
            c.cap_s = p.next().unwrap_or("1048576").parse().unwrap_or(1048576);
        } else if let Some(rest) = tok.strip_prefix("lr") {
            c.lr = rest.parse::<u64>().unwrap_or(15) as f64 / 100.0;
        } else if let Some(rest) = tok.strip_prefix("inc") {
            c.inc = rest.parse::<u64>().unwrap_or(32);
        } else if tok == "ctx8" {
            c.nctx = 8;
        } else if tok == "ctx16" {
            c.nctx = 16;
        }
        // s<N> accepted-and-ignored here: the audit reads self-info from
        // the 2^24 vector this instrument codes with.
    }
    c
}

fn key_of(cfg: Cfg, prev: u8, prev2: u8) -> usize {
    let pb = bucket(prev);
    if cfg.nctx == 8 {
        pb
    } else {
        pb | ((bucket(prev2) >= 3) as usize) << 3
    }
}

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "e".into());
    let mode = std::env::args().nth(2).unwrap_or_else(|| {
        eprintln!("usage: r9_audit <e|d|a> <mode>");
        std::process::exit(2);
    });
    let mut stdin = Vec::new();
    std::io::stdin().read_to_end(&mut stdin).expect("stdin");
    let hdr_len = stdin.iter().position(|&b| b == b'\n').expect("header");
    let hdr = String::from_utf8(stdin[..hdr_len].to_vec()).expect("utf8");
    let n: usize = hdr
        .split_whitespace()
        .next()
        .expect("n")
        .parse()
        .expect("n");
    let body = &stdin[hdr_len + 1..];
    // 'p' (payload decode): the declared "n" is the payload's own byte
    // length, not a symbol count — the payload carries its own stream_len
    // word. Everything else (encode/audit modes) takes n symbols.
    if cmd != "p" && body.len() < n {
        eprintln!("ERR short body");
        std::process::exit(2);
    }
    let syms: Vec<u8> = if cmd == "p" {
        Vec::new() // payload bytes are read in the 'p' branch from `body`
    } else {
        body[..n].to_vec()
    };
    let cfg = parse_mode(&mode);

    // ---- encode path (wire + optional audit), keys computed as shipped.
    // 'p' is decode-only: no symbols, no encode, no roundtrip section.
    let (syms, do_roundtrip) = if cmd == "p" {
        (Vec::new(), false)
    } else {
        (syms, true)
    };
    let mut models: Vec<Dual> = (0..cfg.nctx).map(|_| Dual::new(cfg.cap_f, cfg.cap_s, cfg.lr, cfg.inc)).collect();
    let mut enc = DefaultRangeEncoder::new();
    let mut costs: Vec<f64> = Vec::new();
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    for &b in &syms {
        let k = key_of(cfg, prev, prev2);
        if cmd == "a" {
            costs.push(models[k].self_info(b));
        }
        let m = models[k].model().expect("model buildable");
        enc.encode_symbols(std::iter::once((usize::from(b), &m)))
            .expect("encode sym");
        models[k].step(b);
        prev2 = prev;
        prev = b;
    }
    let compressed = enc.into_compressed().expect("seal");

    if do_roundtrip {
    // ---- exact-inverse proof: FRESH decode-side models, evolution from
    // the DECODED symbol (mix_coder.rs's contract). The key schedule
    // restarts at (prev, prev2) = (0, 0) exactly like the shipped
    // decoder — reusing the encode loop's tail state here was the
    // Frontend(InvalidData) source (a poisoned first context). Any
    // failure is a clean instrument error, never a panic.
    let mut dmodels: Vec<Dual> = (0..cfg.nctx)
        .map(|_| Dual::new(cfg.cap_f, cfg.cap_s, cfg.lr, cfg.inc))
        .collect();
    let mut dec = match DefaultRangeDecoder::from_compressed(compressed.as_slice().to_vec()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("ERR decoder open {e:?}");
            std::process::exit(2);
        }
    };
    let mut back: Vec<u8> = Vec::with_capacity(n);
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    for _ in 0..n {
        let k = key_of(cfg, prev, prev2);
        let m = match dmodels[k].model() {
            Some(m) => m,
            None => {
                eprintln!("ERR model build (decode side)");
                std::process::exit(2);
            }
        };
        match dec.decode_symbols(std::iter::once(&m)).next() {
            Some(Ok(s)) => back.push(s as u8),
            _ => {
                eprintln!("ERR decode symbol (roundtrip divergence)");
                std::process::exit(2);
            }
        }
        let s = *back.last().expect("just pushed");
        dmodels[k].step(s);
        prev2 = prev;
        prev = s;
    }
    if back != syms {
        eprintln!("ERR roundtrip mismatch");
        std::process::exit(2);
    }
    } // do_roundtrip

    use std::io::Write;
    match cmd.as_str() {
        "e" => {
            let mut out = Vec::with_capacity(compressed.len() * 4 + 4);
            out.extend_from_slice(&u32::try_from(n).expect("len").to_le_bytes());
            for w in &compressed {
                out.extend_from_slice(&w.to_le_bytes());
            }
            std::io::stdout().write_all(&out).expect("write");
        }
        // 'p' = TRUE payload decode: stdin body = the FULL mix payload
        // ([u32 stream_len][words]), decoded with fresh models under the
        // mode's own ctx rule — the shipped decoder's shape. (The earlier
        // form treated stdin as symbols and roundtripped them, which
        // returned the payload bytes and failed the anchor spuriously.)
        "p" => {
            let mut words = Vec::new();
            for chunk in body[4..].chunks_exact(4) { // body[0..4] = the length word
                words.push(u32::from_le_bytes(chunk.try_into().expect("word")));
            }
            if words.is_empty() {
                eprintln!("ERR empty payload");
                std::process::exit(2);
            }
            let mut dmodels: Vec<Dual> = (0..cfg.nctx)
                .map(|_| Dual::new(cfg.cap_f, cfg.cap_s, cfg.lr, cfg.inc))
                .collect();
            let mut dec = match DefaultRangeDecoder::from_compressed(words) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("ERR decoder open {e:?}");
                    std::process::exit(2);
                }
            };
            let mut prev = 0u8;
            let mut prev2 = 0u8;
            let stream_len = u32::from_le_bytes(body[..4].try_into().expect("len word")) as usize;
            let mut back: Vec<u8> = Vec::with_capacity(stream_len);
            for _ in 0..stream_len {
                let k = key_of(cfg, prev, prev2);
                let m = match dmodels[k].model() {
                    Some(m) => m,
                    None => {
                        eprintln!("ERR model build");
                        std::process::exit(2);
                    }
                };
                match dec.decode_symbols(std::iter::once(&m)).next() {
                    Some(Ok(s)) => {
                        let s = s as u8;
                        dmodels[k].step(s);
                        back.push(s);
                        prev2 = prev;
                        prev = s;
                    }
                    _ => {
                        eprintln!("ERR decode symbol at {} of {}", back.len(), stream_len);
                        std::process::exit(2);
                    }
                }
            }
            std::io::stdout().write_all(&back).expect("write");
        }
        "d" => {} // obsolete debug mode (roundtrip output lives inside the do_roundtrip block)
        "a" => {
            let mut out = Vec::with_capacity(4 + n * 8);
            out.extend_from_slice(&u32::try_from(n).expect("len").to_le_bytes());
            for c in &costs {
                out.extend_from_slice(&c.to_le_bytes());
            }
            std::io::stdout().write_all(&out).expect("write");
        }
        _ => {
            eprintln!("unknown cmd {cmd}");
            std::process::exit(2);
        }
    }
}