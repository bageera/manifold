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
    // R9-C: optional parent link (0xFF = none). Parent = k & 7 (strip the
    // band bit): the 16-key rule split each 8-key R5 bucket by prev2's
    // band, so the R5 bucket IS the parent in the grammar's own tree.
    parent: u8,
    /// local updates SINCE the context went cold (used by Mix scheduling);
    /// u64::MAX after Init seeding stops counting.
    nlocal: u64,
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
            parent: 0xFF,
            nlocal: 0,
        }
    }

    fn cold(&self) -> bool {
        self.nlocal < super_small_threshold()
    }

    /// Fixed-point probability vector; every symbol >= 1; total exactly
    /// SCALE. Same rebalance as src/mix_coder.rs (remainder to slot 0).
    /// R9-C warm variants mix with the parent's vector here (the parent's
    /// own probs() — the hierarchy is recursive in principle; band0
    /// parents have no grandparent).
    fn probs_with(&self, tree: Option<&[Dual]>, warm: Warm) -> Vec<u64> {
        let base = self.probs();
        let parent = match (tree, self.parent) {
            (Some(t), p) if p != 0xFF => Some(&t[p as usize]),
            _ => None,
        };
        match (warm, parent) {
            (Warm::Init, _) => base, // seeding happens at step time, not here
            (Warm::Mix(n0), Some(p)) if self.cold() => {
                let n = self.nlocal as f64;
                let lam = n0 as f64 / (n0 as f64 + n);
                mix_vecs(&base, &p.probs(), lam)
            }
            (Warm::Fix(lam), Some(p)) => mix_vecs(&base, &p.probs(), lam),
            _ => base,
        }
    }

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
        rebalance(&mut p);
        p
    }

    /// Prequential self-information of symbol s under the CURRENT state
    /// (before the update): -log2(p_s) with warm mixing applied.
    fn self_info_with(&self, tree: Option<&[Dual]>, warm: Warm, s: u8) -> f64 {
        let p = self.probs_with(tree, warm)[usize::from(s)] as f64;
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
        // nlocal stops counting once Init-seeded (u64::MAX saturates)
        self.nlocal = self.nlocal.saturating_add(1);
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

fn super_small_threshold() -> u64 {
    // cold window for the warm-mix schedule (symbols); R9-B measured the
    // cold regime in the first ~10% of streams, but per-context symbol
    // counts are the honest local variable; 256 keeps the schedule tight.
    256
}

fn mix_vecs(a: &[u64], b: &[u64], lam: f64) -> Vec<u64> {
    // P = (1-λ)·a + λ·b in fixed point, then rebalance (both are valid
    // 2^24 vectors, so the mix is a convex combination; remainder to slot 0)
    let mut p: Vec<u64> = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| ((1.0 - lam) * x as f64 + lam * y as f64) as u64)
        .collect();
    rebalance(&mut p);
    p
}

fn rebalance(p: &mut [u64]) {
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

#[derive(Clone, Copy, PartialEq)]
enum Warm {
    None,
    /// C1: seed a child context's tables from its parent's on first use;
    /// ordinary updates continue afterwards.
    Init,
    /// C2: mix child with parent by warm-up schedule λ = n0/(n0+n).
    Mix(u64),
    /// C3: constant-λ mixing (the "broader sharing" comparator).
    Fix(f64),
}

#[derive(Clone, Copy)]
struct Cfg {
    cap_f: u64,
    cap_s: u64,
    lr: f64,
    inc: u64,
    nctx: usize,
    warm: Warm,
    refine: Refine,
    share: bool,
}

/// R9-D refinement scope (the registered arms). Base backend-5 key =
/// bucket(prev) | band(prev2)<<3 — keys 7 and 15 are the heavy band
/// (prev >= 97, R9-B: 3.25/2.75 b/sym). Refinement adds prev3's band
/// bit inside the heavy band, per the work order's attribution plan:
///   Merged  — register the MID-FLIGHT design verbatim: keys 7 AND 15
///             share one child pair {16,17} keyed on prev3's band; the
///             base-key band bit is discarded (this is what "refine"
///             first measured, +1088 B before its parents were fixed;
///             its comment at the time claimed per-key children).
///   K7Only  — D3b: key 7 only -> child 16+band3 (key15 stays base key 15)
///   K15Only — D3a: key 15 only -> child 18+band3 (key7 stays base key 7)
///   Both    — D1: distinct children per key (7 -> 16+b3, 15 -> 18+b3)
#[derive(Clone, Copy, PartialEq)]
enum Refine {
    None,
    Merged,
    K7Only,
    K15Only,
    Both,
}

fn parse_mode(mode: &str) -> Cfg {
    let mut c = Cfg {
        cap_f: 32768,
        cap_s: 1048576,
        lr: 0.15,
        inc: 32,
        nctx: 16,
        warm: Warm::None,
        refine: Refine::None,
        share: false,
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
        } else if let Some(rest) = tok.strip_prefix("refine") {
            // R9-D refinement scope — see the Refine enum comment for the
            // registered arm semantics. Children: 16,17 = key 7's (18,19
            // = key 15's), split on prev3's band; Merged shares one pair.
            match rest {
                "M" | "merged" => {
                    c.refine = Refine::Merged;
                    c.nctx = 18;
                }
                "7" => {
                    c.refine = Refine::K7Only;
                    c.nctx = 20;
                }
                "15" => {
                    c.refine = Refine::K15Only;
                    c.nctx = 20;
                }
                "A" | "both" => {
                    c.refine = Refine::Both;
                    c.nctx = 20;
                }
                _ => {}
            }
        } else if tok == "share" {
            // ancestor liveness: the chain's parent model also gets the
            // symbol's update (child -> base heavy key -> [R5 bucket]).
            // Decode-side replicates it; zero transport.
            c.share = true;
        } else if let Some(rest) = tok.strip_prefix("warm") {
            // warm<X>: C1 init / C2 schedule / C3 constant
            // warminit          — C1: seed from parent on first use
            // warm2^N           — C2: λ = 2^N / (2^N + n) mixing
            // warmfix<LR>       — C3: constant λ = LR (wfix15 = 0.15)
            if rest == "init" {
                c.warm = Warm::Init;
            } else if let Some(num) = rest.strip_prefix("2p") {
                c.warm = Warm::Mix(num.parse::<u64>().unwrap_or(10));
            } else if let Some(num) = rest.strip_prefix("fix") {
                c.warm = Warm::Fix(num.parse::<u64>().unwrap_or(15) as f64 / 100.0);
            }
        }
        // s<N> accepted-and-ignored here: the audit reads self-info from
        // the 2^24 vector this instrument codes with.
    }
    c
}

fn key_of(cfg: Cfg, prev: u8, prev2: u8, prev3: u8) -> usize {
    let pb = bucket(prev);
    if cfg.nctx == 8 {
        pb
    } else {
        let band = (bucket(prev2) >= 3) as usize;
        let base = pb | band << 3;
        if cfg.refine == Refine::None || pb != 7 {
            return base;
        }
        // heavy band (keys 7/15): refined child lookup by prev3's band.
        // NOTE: the refined key rule REPLACES the base key, so for a
        // refined scope the base parent table stops receiving symbols
        // entirely — with `share` off its state stays at init.
        let band3 = (bucket(prev3) >= 3) as usize;
        match cfg.refine {
            Refine::Merged => 16 + band3,
            Refine::K7Only if band == 0 => 16 + band3,
            Refine::K15Only if band == 1 => 18 + band3,
            Refine::Both if band == 0 => 16 + band3,
            Refine::Both if band == 1 => 18 + band3,
            _ => base, // refinement scope did not select this key
        }
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
    let cfg = parse_mode(&mode);

    // R9-C: shared tree-building helper state — parent of band-split key
    // k = k & 7 (the R5 bucket; band-0 keys have no parent). Parents are
    // set UNCONDITIONALLY (probs_with ignores them for Warm::None); the
    // 0xFF default of base band-0 keys stays. Refined children (16..19)
    // look to the base heavy keys they refine; the merged slot's own R5
    // bucket is 7.
    let build_tree = |models: &mut Vec<Dual>| {
        models
            .iter_mut()
            .enumerate()
            .skip(8)
            .take(cfg.nctx - 8)
            .for_each(|(k, m)| m.parent = (k & 7) as u8);
        if cfg.nctx > 16 {
            models[16].parent = 7;
            models[17].parent = 7;
        }
        if cfg.nctx > 18 {
            models[18].parent = 15;
            models[19].parent = 15;
        }
    };
    // R9-D `share`: also step the ancestor chain's model with the same
    // symbol (child -> its parent; one chain level). The ancestor then
    // keeps LIVE statistics for the child to mix against while cold —
    // without it a refined arm's base parent stops receiving symbols.
    let share_update = |models: &mut [Dual], cfg: Cfg, k: usize, s: u8| {
        if cfg.share {
            let p = models[k].parent;
            if p != 0xFF {
                models[p as usize].step(s);
            }
        }
    };

    if cmd == "p" {
        // payload decode: full tree + seeding parity with the encoder
        let mut words = Vec::new();
        let (chunks, _) = body[4..].as_chunks::<4>();
        for chunk in chunks {
            words.push(u32::from_le_bytes(*chunk));
        }
        if words.is_empty() {
            eprintln!("ERR empty payload");
            std::process::exit(2);
        }
        let stream_len = u32::from_le_bytes(body[..4].try_into().expect("len word")) as usize;
        let mut dmodels: Vec<Dual> = (0..cfg.nctx)
            .map(|_| Dual::new(cfg.cap_f, cfg.cap_s, cfg.lr, cfg.inc))
            .collect();
        build_tree(&mut dmodels);
        let mut dec = match DefaultRangeDecoder::from_compressed(words) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("ERR decoder open {e:?}");
                std::process::exit(2);
            }
        };
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        let mut back: Vec<u8> = Vec::with_capacity(stream_len);
        for _ in 0..stream_len {
            let k = key_of(cfg, prev, prev2, prev3);
            if cfg.warm == Warm::Init && dmodels[k].nlocal == 0 && dmodels[k].parent != 0xFF {
                let p = dmodels[k].parent as usize;
                dmodels[k].fast = dmodels[p].fast.clone();
                dmodels[k].slow = dmodels[p].slow.clone();
                dmodels[k].log_w = dmodels[p].log_w;
                dmodels[k].nlocal = u64::MAX;
            }
            let m = ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
                dmodels[k].probs_with(Some(&dmodels), cfg.warm)
                    .iter()
                    .map(|&v| v as u32)
                    .collect::<Vec<u32>>(),
                false,
            )
            .expect("model buildable");
            match dec.decode_symbols(std::iter::once(&m)).next() {
                Some(Ok(s)) => {
                    let s = s as u8;
                    dmodels[k].step(s);
                    share_update(&mut dmodels, cfg, k, s);
                    back.push(s);
                    prev3 = prev2;
                    prev2 = prev;
                    prev = s;
                }
                _ => {
                    eprintln!("ERR decode symbol at {} of {}", back.len(), stream_len);
                    std::process::exit(2);
                }
            }
        }
        use std::io::Write as _;
        std::io::stdout().write_all(&back).expect("write");
        return;
    }

    let syms: Vec<u8> = body[..n].to_vec();
    let mut models: Vec<Dual> = (0..cfg.nctx)
        .map(|_| Dual::new(cfg.cap_f, cfg.cap_s, cfg.lr, cfg.inc))
        .collect();
    build_tree(&mut models);
    let mut enc = DefaultRangeEncoder::new();
    let mut costs: Vec<f64> = Vec::new();
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    let mut prev3 = 0u8;
    for &b in &syms {
        let k = key_of(cfg, prev, prev2, prev3);
        // C1 Init seeding BEFORE the model build (the decode side seeds
        // before its build; an encode-after-seed order made the first
        // symbol of a fresh context use different models and the
        // roundtrip diverged — proven by the synthetic cross-check).
        if cfg.warm == Warm::Init && models[k].nlocal == 0 && models[k].parent != 0xFF {
            let p = models[k].parent as usize;
            models[k].fast = models[p].fast.clone();
            models[k].slow = models[p].slow.clone();
            models[k].log_w = models[p].log_w;
            models[k].nlocal = u64::MAX; // stop cold-counting; updates continue
        }
        if cmd == "a" {
            costs.push(models[k].self_info_with(Some(&models), cfg.warm, b));
        }
        let m = ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            models[k]
                .probs_with(Some(&models), cfg.warm)
                .iter()
                .map(|&v| v as u32)
                .collect::<Vec<u32>>(),
            false,
        )
        .expect("model buildable");
        enc.encode_symbols(std::iter::once((usize::from(b), &m)))
            .expect("encode sym");
        models[k].step(b);
        share_update(&mut models, cfg, k, b);
        prev3 = prev2;
        prev2 = prev;
        prev = b;
    }
    let compressed = enc.into_compressed().expect("seal");

    // ---- exact-inverse proof: FRESH decode-side models, evolution from
    // the DECODED symbol (mix_coder.rs's contract). The key schedule
    // restarts at (prev, prev2) = (0, 0) exactly like the shipped decoder
    // — reusing the encode loop's tail state here was the
    // Frontend(InvalidData) source (a poisoned first context). Warm
    // variants: the decode side applies the SAME seeding/mixing rules —
    // the decoder never reruns discovery, but the warm state is pure
    // model evolution (zero transport), so encode == decode holds ONLY
    // if both sides seed/mix identically. Any failure = clean ERR.
    let mut dmodels: Vec<Dual> = (0..cfg.nctx)
        .map(|_| Dual::new(cfg.cap_f, cfg.cap_s, cfg.lr, cfg.inc))
        .collect();
    build_tree(&mut dmodels);
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
    let mut prev3 = 0u8;
    for _ in 0..n {
        let k = key_of(cfg, prev, prev2, prev3);
        if cfg.warm == Warm::Init && dmodels[k].nlocal == 0 && dmodels[k].parent != 0xFF {
            let p = dmodels[k].parent as usize;
            dmodels[k].fast = dmodels[p].fast.clone();
            dmodels[k].slow = dmodels[p].slow.clone();
            dmodels[k].log_w = dmodels[p].log_w;
            dmodels[k].nlocal = u64::MAX;
        }
        let pv: Vec<u32> = dmodels[k]
            .probs_with(Some(&dmodels), cfg.warm)
            .iter()
            .map(|&v| v as u32)
            .collect();
        let m = match ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            pv, false,
        ) {
            Ok(m) => m,
            Err(_) => {
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
        share_update(&mut dmodels, cfg, k, s);
        prev3 = prev2;
        prev2 = prev;
        prev = s;
    }
    if back != syms {
        eprintln!("ERR roundtrip mismatch");
        std::process::exit(2);
    }

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
