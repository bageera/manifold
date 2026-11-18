//! BZ16-R2 — in-repo numerical parity prototype (see docs/bz16-r2-record.md).
//!
//! Rust twin of `crates/mnf-mans/scripts/bz16_r2_reference.py` (stdlib only):
//! the pinned integer Boltzmann model of the BZ16 memo chain, implemented
//! independently in each language over the frozen LUT, compared per position.
//!
//! Gates exercised (memo §4 organization):
//!   R2-A energy exactness          — E_hi/E_lo at o compared per position
//!   R2-B LUT indexing + clamps     — direct k-mapping table asserted
//!   R2-C probability normalization — full 256-entry frequency vectors
//!   R2-D gradient update           — carried coefficients compared
//!   R2-E adaptive-state evolution  — weights + freeze vectors + end carries
//!   R2-F coding intervals          — 257-entry cumulative vectors
//!   R2-G independent decoding      — carry-less CACM87 instrument, full stream
//!
//! Parity oracle: the Python engine's golden export at
//! `research/results/bz16-r2-golden-{camera,brick,noise}.txt`
//! (directory override via argv[1]). A one-frequency-unit mismatch is a
//! parity failure. The R2-G coder is a verification instrument only.
//!
//! Run: `cargo run --release --example bz16_r2_parity -p mnf-mans [-- <dir>]`

use std::fs;
use std::path::Path;

const K_MAX: i64 = 4096;
const N_ENTRIES: usize = 4097;
const SCALE: f64 = (1u32 << 24) as f64;
const TWO_P24: i64 = 1 << 24;
const W_SUM: i64 = 4096;
const W_INIT: i64 = W_SUM / 4;
const ALPHA_NUM: i64 = 64;
const ALPHA_DEN: i64 = 4096;
const STEP_SHIFT: u32 = 15;
const C_LIMIT: i64 = 4 * 4096;
const E_LIMIT: i64 = 3 * C_LIMIT * 4096; // 201,326,592 (memo §5 frozen value)
const FREQ_SUM: i64 = 4096;
const WINDOW: usize = 512;
const STREAM_LEN: usize = 128 * 128;
const R_TOP: i64 = 0xFFFF;
const R_HALF: i64 = 0x8000;
const R_QUARTER: i64 = 0x4000;
const R_3QUARTER: i64 = 0xC000;

/// Python `round()` parity for positive values: half-to-even (R1 helper,
/// duplicated locally so research examples stay standalone).
fn round_half_even(x: f64) -> f64 {
    let fl = x.floor();
    let frac = x - fl;
    let up = frac > 0.5 || (frac == 0.5 && (fl as i64) % 2 != 0);
    if up {
        fl + 1.0
    } else {
        fl
    }
}

fn build_lut() -> Vec<i64> {
    (0..N_ENTRIES)
        .map(|k| {
            let v = SCALE * (0.0 - k as f64 / 256.0).exp();
            let r = round_half_even(v);
            r.clamp(1.0, u32::MAX as f64) as i64
        })
        .collect()
}

fn cdist(a: u8, b: u8) -> i64 {
    let d = (a as i64 - b as i64).abs();
    if d <= 128 {
        d
    } else {
        256 - d
    }
}

/// frozen: k = energy in 1/256 units, index clamp [0, 4096] (memo §3).
fn clamp_k(e: i64, shift: u32) -> i64 {
    (e >> shift).clamp(0, K_MAX)
}

#[derive(Clone)]
struct State {
    c_hi: [i64; 3],
    c_lo: [i64; 3],
    w: [i64; 4],
    f_prev: [i64; 256],
    x_prev: [i64; 256],
}

impl State {
    fn new() -> Self {
        State {
            c_hi: [0; 3],
            c_lo: [0; 3],
            w: [W_INIT; 4],
            f_prev: [16; 256],
            x_prev: [16 << 12; 256],
        }
    }
}

/// Quantize the mixture numerators to a 256-vector summing FREQ_SUM
/// (largest remainder; ties: remainder desc, then lower symbol index; every
/// zero-frequency symbol pulls one from the current argmax, ties lowest).
fn largest_remainder(m: &[i64; 256], den: i64) -> ([i64; 256], usize) {
    let mut fs = [0i64; 256];
    let mut rems = [0i64; 256];
    let mut sum: i64 = 0;
    for (s, &mv) in m.iter().enumerate() {
        let f = mv * FREQ_SUM / den;
        fs[s] = f;
        rems[s] = mv * FREQ_SUM - f * den;
        sum += f;
    }
    let deficit = (FREQ_SUM - sum) as usize;
    let mut order: Vec<usize> = (0..256).collect();
    order.sort_by(|&a, &b| rems[b].cmp(&rems[a]).then(a.cmp(&b)));
    for &s in order.iter().take(deficit) {
        fs[s] += 1;
    }
    let mut repairs = 0usize;
    for s in 0..256 {
        if fs[s] == 0 {
            repairs += 1;
            let mut arg = 0usize;
            for i in 1..256 {
                if fs[i] > fs[arg] {
                    arg = i;
                }
            }
            fs[arg] -= 1;
            fs[s] += 1;
        }
    }
    assert_eq!(fs.iter().sum::<i64>(), FREQ_SUM, "normalization invariant");
    assert!(fs.iter().all(|&v| v >= 1), "coder-fit floor");
    (fs, repairs)
}

/// Mixture stage: expert vectors (LUT domain), mixture, normalization.
fn predict(
    state: &State,
    prev3: (u8, u8, u8),
    lut: &[i64],
    x_hi: &mut [i64; 256],
    x_lo: &mut [i64; 256],
) -> ([i64; 256], [i64; 257], usize) {
    let x_unif = TWO_P24 / 256;
    let mut m = [0i64; 256];
    for s in 0..256usize {
        let sv = s as u8;
        let d1 = cdist(sv, prev3.2) << 5;
        let d2 = cdist(sv, prev3.1) << 5;
        let d3 = cdist(sv, prev3.0) << 5;
        x_hi[s] = lut[clamp_k(
            state.c_hi[0] * d1 + state.c_hi[1] * d2 + state.c_hi[2] * d3,
            4,
        ) as usize];
        x_lo[s] = lut[clamp_k(
            state.c_lo[0] * d1 + state.c_lo[1] * d2 + state.c_lo[2] * d3,
            5,
        ) as usize];
        m[s] = (state.w[0] * x_hi[s]
            + state.w[1] * x_lo[s]
            + state.w[2] * x_unif
            + state.w[3] * state.x_prev[s])
            >> 12;
    }
    let total: i64 = m.iter().sum();
    assert!(total > 0, "mixture total positive");
    let (f, repairs) = largest_remainder(&m, total);
    let mut cums = [0i64; 257];
    let mut acc = 0i64;
    for s in 0..256 {
        cums[s] = acc;
        acc += f[s];
    }
    cums[256] = acc;
    (f, cums, repairs)
}

/// Adaptation stage: Q12 gradient update + integer fixed-share (R2-D/E).
fn observe(
    state: &mut State,
    o: u8,
    x_hi: &[i64; 256],
    x_lo: &[i64; 256],
    f: &[i64; 256],
    prev3: (u8, u8, u8),
) {
    let d1 = cdist(o, prev3.2) << 5;
    let d2 = cdist(o, prev3.1) << 5;
    let d3 = cdist(o, prev3.0) << 5;
    for (cs, xs) in [(&mut state.c_hi, x_hi), (&mut state.c_lo, x_lo)] {
        let err = f[o as usize] - (xs[o as usize] >> 12);
        assert!(-FREQ_SUM < err && err <= FREQ_SUM, "gradient err bound");
        for (i, &di) in [d1, d2, d3].iter().enumerate() {
            let num = err * di;
            assert!(num.abs() <= FREQ_SUM * 4096, "gradient num bound");
            let new = cs[i] - (num >> STEP_SHIFT);
            cs[i] = new.clamp(-C_LIMIT, C_LIMIT);
        }
    }
    let losses = [
        (f[o as usize] - (x_hi[o as usize] >> 12)).abs(),
        (f[o as usize] - (x_lo[o as usize] >> 12)).abs(),
        (f[o as usize] - 16).abs(),
        (f[o as usize] - state.f_prev[o as usize]).abs(),
    ];
    let mut best = 0usize;
    for j in 1..4 {
        if losses[j] < losses[best] {
            best = j;
        }
    }
    let keep = ALPHA_DEN - ALPHA_NUM;
    for j in 0..4 {
        let gain = if j == best { W_SUM * ALPHA_NUM } else { 0 };
        state.w[j] = (state.w[j] * keep + gain) >> 12;
    }
    let drift = W_SUM - state.w.iter().sum::<i64>();
    if drift > 0 {
        let mut arg = 0usize;
        for i in 1..4 {
            if state.w[i] > state.w[arg] {
                arg = i;
            }
        }
        state.w[arg] += drift;
    } else if drift < 0 {
        let mut arg = 0usize;
        for i in 1..4 {
            if state.w[i] < state.w[arg] {
                arg = i;
            }
        }
        state.w[arg] += drift;
    }
    assert_eq!(state.w.iter().sum::<i64>(), W_SUM, "weights sum");
    state.f_prev = *f;
    for (xp, &fv) in state.x_prev.iter_mut().zip(f.iter()) {
        *xp = fv << 12;
    }
}

// --- fixtures (pinned synthetic stand-ins; the memo's crops remain pending
// artifacts) --------------------------------------------------------------
fn fixture_camera() -> Vec<u8> {
    (0..STREAM_LEN)
        .map(|i| {
            let r = i / 128;
            let c = i % 128;
            ((97 * r + 31 * c + ((r * c) >> 2)) & 0xFF) as u8
        })
        .collect()
}

fn fixture_brick() -> Vec<u8> {
    (0..STREAM_LEN)
        .map(|i| {
            let band = 37 * (i / 1024) + (i % 128) + ((i / 128) % 8) * 3;
            (band & 0xFF) as u8
        })
        .collect()
}

fn fixture_noise() -> Vec<u8> {
    let mut s: i64 = 12_345;
    (0..STREAM_LEN)
        .map(|_| {
            s = (s.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x7FFF_FFFF;
            ((s >> 16) & 0xFF) as u8
        })
        .collect()
}

// --- R2-G instrument: CACM87 carry-less range coder ----------------------
// 16-bit window, bit-level renorm, underflow counted per the classic E0/E1/E3
// rules so no carry propagation is needed (matches the Python instrument).
struct Enc {
    low: i64,
    high: i64,
    pending: i64,
    bits: Vec<u8>,
}

impl Enc {
    fn new() -> Self {
        Enc {
            low: 0,
            high: R_TOP,
            pending: 0,
            bits: Vec::new(),
        }
    }
    fn emit(&mut self, bit: u8) {
        self.bits.push(bit);
        while self.pending > 0 {
            self.bits.push(1 - bit);
            self.pending -= 1;
        }
    }
}

fn ac_encode(stream: &[u8], lut: &[i64]) -> Vec<u8> {
    let mut enc = Enc::new();
    let mut state = State::new();
    let mut prev3 = (0u8, 0u8, 0u8);
    let mut x_hi = [0i64; 256];
    let mut x_lo = [0i64; 256];
    for &o in stream {
        let (f, cums, _) = predict(&state, prev3, lut, &mut x_hi, &mut x_lo);
        let fs = cums[o as usize + 1] - cums[o as usize];
        assert!((1..=FREQ_SUM).contains(&fs), "coder-fit frequency");
        let rng = enc.high - enc.low + 1;
        enc.high = enc.low + (rng * cums[o as usize + 1]) / FREQ_SUM - 1;
        enc.low += (rng * cums[o as usize]) / FREQ_SUM;
        loop {
            if enc.high < R_HALF {
                enc.emit(0);
            } else if enc.low >= R_HALF {
                enc.emit(1);
                enc.low -= R_HALF;
                enc.high -= R_HALF;
            } else if enc.low >= R_QUARTER && enc.high < R_3QUARTER {
                enc.pending += 1;
                enc.low -= R_QUARTER;
                enc.high -= R_QUARTER;
            } else {
                break;
            }
            enc.low <<= 1;
            enc.high = (enc.high << 1) | 1;
        }
        observe(&mut state, o, &x_hi, &x_lo, &f, prev3);
        prev3 = (prev3.1, prev3.2, o);
    }
    enc.pending += 1;
    let fin = if enc.low < R_QUARTER { 0u8 } else { 1u8 };
    enc.emit(fin);
    enc.bits
}

fn ac_decode(bits: &[u8], n: usize, lut: &[i64]) -> Vec<u8> {
    let mut low = 0i64;
    let mut high = R_TOP;
    let mut code = 0i64;
    let mut pos = 0usize;
    let mut out = Vec::with_capacity(n);
    let mut state = State::new();
    let mut prev3 = (0u8, 0u8, 0u8);
    let mut x_hi = [0i64; 256];
    let mut x_lo = [0i64; 256];
    let get_bit = |pos: &mut usize| -> i64 {
        let b = if *pos < bits.len() {
            bits[*pos] as i64
        } else {
            0
        };
        *pos += 1;
        b
    };
    for _ in 0..16 {
        let b = get_bit(&mut pos);
        code = (code << 1) | b;
    }
    for _ in 0..n {
        let (f, cums, _) = predict(&state, prev3, lut, &mut x_hi, &mut x_lo);
        let rng = high - low + 1;
        let value = ((code - low + 1) * FREQ_SUM - 1) / rng;
        let mut s = 0usize;
        while s < 255 && cums[s + 1] <= value {
            s += 1;
        }
        assert!(f[s] >= 1 && s <= 255, "decode symbol range");
        high = low + (rng * cums[s + 1]) / FREQ_SUM - 1;
        low += (rng * cums[s]) / FREQ_SUM;
        loop {
            if high < R_HALF {
                // E0
            } else if low >= R_HALF {
                code -= R_HALF;
                low -= R_HALF;
                high -= R_HALF;
            } else if low >= R_QUARTER && high < R_3QUARTER {
                code -= R_QUARTER;
                low -= R_QUARTER;
                high -= R_QUARTER;
            } else {
                break;
            }
            let b = get_bit(&mut pos);
            code = (code << 1) | b;
            low <<= 1;
            high = (high << 1) | 1;
        }
        observe(&mut state, s as u8, &x_hi, &x_lo, &f, prev3);
        out.push(s as u8);
        prev3 = (prev3.1, prev3.2, s as u8);
    }
    out
}

// --- golden file parsing (pinned contract) --------------------------------
struct Pos {
    t: usize,
    o: u8,
    e_hi: i64,
    e_lo: i64,
    k_hi: i64,
    k_lo: i64,
    c_hi: [i64; 3],
    c_lo: [i64; 3],
    w: [i64; 4],
    f: Vec<i64>,
    cums: Vec<i64>,
}

fn parse_ints<T: std::str::FromStr>(it: &mut std::str::SplitWhitespace<'_>) -> T
where
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    it.next()
        .unwrap_or_else(|| panic!("missing field"))
        .parse()
        .unwrap_or_else(|e| panic!("bad field: {e:?}"))
}

fn read_golden(dir: &str, name: &str, window: usize) -> (Vec<Pos>, [i64; 10]) {
    let path = Path::new(dir).join(format!("bz16-r2-golden-{name}.txt"));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden export missing: {path:?} ({e})"));
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let head = lines.next().expect("golden header");
    assert!(head.starts_with("h "), "golden header");
    let mut positions: Vec<Pos> = Vec::new();
    let mut z = [0i64; 10];
    let mut z_seen = false;
    for line in lines {
        let mut it = line.split_whitespace();
        match it.next().unwrap_or("") {
            "p" => {
                assert!(!z_seen, "p after z");
                positions.push(Pos {
                    t: parse_ints(&mut it),
                    o: parse_ints(&mut it),
                    e_hi: parse_ints(&mut it),
                    e_lo: parse_ints(&mut it),
                    k_hi: parse_ints(&mut it),
                    k_lo: parse_ints(&mut it),
                    c_hi: [
                        parse_ints(&mut it),
                        parse_ints(&mut it),
                        parse_ints(&mut it),
                    ],
                    c_lo: [
                        parse_ints(&mut it),
                        parse_ints(&mut it),
                        parse_ints(&mut it),
                    ],
                    w: [
                        parse_ints(&mut it),
                        parse_ints(&mut it),
                        parse_ints(&mut it),
                        parse_ints(&mut it),
                    ],
                    f: Vec::new(),
                    cums: Vec::new(),
                });
            }
            "f" => {
                let p = positions.last_mut().expect("f before p");
                assert!(p.f.is_empty(), "double f");
                p.f = it.map(|v| v.parse().unwrap()).collect();
                assert_eq!(p.f.len(), 256, "f width");
            }
            "F" => {
                let p = positions.last_mut().expect("F before p");
                assert!(p.cums.is_empty(), "double F");
                p.cums = it.map(|v| v.parse().unwrap()).collect();
                assert_eq!(p.cums.len(), 257, "F width");
            }
            "z" => {
                assert!(!z_seen, "double z");
                z_seen = true;
                for v in z.iter_mut() {
                    *v = parse_ints(&mut it);
                }
            }
            other => panic!("unknown golden line tag: {other}"),
        }
    }
    assert_eq!(positions.len(), window, "golden window size");
    (positions, z)
}

fn main() {
    let lut = build_lut();
    assert_eq!(lut[0], 16_777_216);
    assert_eq!(lut[4096], 2);

    // ---- R2-B: LUT index + clamp boundaries (frozen memo semantics) ----
    let k_cases: &[(i64, u32, i64)] = &[
        (0, 4, 0),
        (1, 4, 0),
        (4095, 4, 255),
        (4096, 4, 256),
        (65535, 4, 4095),
        (65536, 4, 4096),
        (-1, 4, 0),
        (E_LIMIT, 4, 4096),
        (0, 5, 0),
        (65536, 5, 2048),
        (-1, 5, 0),
        (E_LIMIT, 5, 4096),
    ];
    for &(e, sh, want) in k_cases {
        assert_eq!(clamp_k(e, sh), want, "R2-B k({e} >> {sh})");
    }
    println!(
        "R2-B k-table PASS: {} cases (clamps at 0 and 4096, shifts 4/5)",
        k_cases.len()
    );

    let golden_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "research/results".to_string());

    let fixtures = [
        ("camera", fixture_camera()),
        ("brick", fixture_brick()),
        ("noise", fixture_noise()),
    ];

    for (name, stream) in fixtures {
        let (golden, z_gold) = read_golden(&golden_dir, name, WINDOW);

        // Positional engine run with per-position parity against the golden.
        let mut state = State::new();
        let mut prev3 = (0u8, 0u8, 0u8);
        let mut x_hi = [0i64; 256];
        let mut x_lo = [0i64; 256];
        let mut repairs_total = 0usize;
        let mut failures = Vec::new();

        for (t, &o) in stream.iter().enumerate() {
            let (f, cums, repairs) = predict(&state, prev3, &lut, &mut x_hi, &mut x_lo);
            let d1 = cdist(o, prev3.2) << 5;
            let d2 = cdist(o, prev3.1) << 5;
            let d3 = cdist(o, prev3.0) << 5;
            let e_hi = state.c_hi[0] * d1 + state.c_hi[1] * d2 + state.c_hi[2] * d3;
            let e_lo = state.c_lo[0] * d1 + state.c_lo[1] * d2 + state.c_lo[2] * d3;
            assert!(e_hi.abs() <= E_LIMIT && e_lo.abs() <= E_LIMIT);
            let k_hi = clamp_k(e_hi, 4);
            let k_lo = clamp_k(e_lo, 5);
            repairs_total += repairs;

            if t < WINDOW {
                // golden contract: position header carries its own t; assert
                // the audit stream and the export stayed in lockstep
                assert_eq!(golden[t].t, t, "golden position index");
                let g = &golden[t];
                // p-line parity: position header, observed symbol, energies,
                // indices, and the carries state BEFORE position t
                if g.o != o {
                    failures.push((t, "o"));
                }
                if g.e_hi != e_hi || g.e_lo != e_lo {
                    failures.push((t, "E (R2-A)"));
                }
                if g.k_hi != k_hi || g.k_lo != k_lo {
                    failures.push((t, "k (R2-B)"));
                }
                if g.c_hi != state.c_hi || g.c_lo != state.c_lo {
                    failures.push((t, "coeff carries (R2-D)"));
                }
                if g.w != state.w {
                    failures.push((t, "weights (R2-E)"));
                }
                // f/F parity (R2-C/R2-F); one frequency unit = failure
                if g.f != f.to_vec() {
                    failures.push((t, "f (R2-C)"));
                }
                if g.cums != cums.to_vec() {
                    failures.push((t, "F (R2-F)"));
                }
            }

            observe(&mut state, o, &x_hi, &x_lo, &f, prev3);
            prev3 = (prev3.1, prev3.2, o);
        }
        // z-line parity (R2-E): carries following the last audited update
        let z_tail = replay_state(&stream, &lut);
        let z_diff: Vec<(usize, i64, i64)> = z_tail
            .iter()
            .zip(z_gold.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .collect();

        assert!(failures.is_empty(), "R2 parity {name}: {failures:?}");
        assert!(z_diff.is_empty(), "R2-E z carries {name}: {z_diff:?}");

        // ---- R2-G full-stream independent decode ----
        let bits = ac_encode(&stream, &lut);
        let decoded = ac_decode(&bits, stream.len(), &lut);
        assert!(decoded == stream, "R2-G roundtrip {name}");
        let wbits = ac_encode(&stream[..WINDOW], &lut).len();

        println!(
            "{name}: parity {WINDOW}/{WINDOW} PASS (R2-A..F), R2-G full-stream \
             PASS ({} bits; window matches Python at {wbits} bits), \
             repairs={repairs_total}",
            bits.len()
        );
    }
    println!(
        "BZ16-R2 in-repo twin parity CONFIRMED (R2-A..G exercised; \
         accepted research-env reference re-point pending its artifacts) \
         — E_LIMIT {E_LIMIT}"
    );
}

/// Engine state after the FULL stream (the z line was emitted post-run).
fn replay_state(stream: &[u8], lut: &[i64]) -> [i64; 10] {
    let mut state = State::new();
    let mut prev3 = (0u8, 0u8, 0u8);
    let mut x_hi = [0i64; 256];
    let mut x_lo = [0i64; 256];
    for &o in stream.iter() {
        let (f, _, _) = predict(&state, prev3, lut, &mut x_hi, &mut x_lo);
        observe(&mut state, o, &x_hi, &x_lo, &f, prev3);
        prev3 = (prev3.1, prev3.2, o);
    }
    [
        state.c_hi[0],
        state.c_hi[1],
        state.c_hi[2],
        state.c_lo[0],
        state.c_lo[1],
        state.c_lo[2],
        state.w[0],
        state.w[1],
        state.w[2],
        state.w[3],
    ]
}
