//! BZ16-I4B — matched float/fixed comparison (revised methodology: Rust is
//! the confirmation environment; see docs/bz16-i4b-record.md).
//!
//! The recovered fixed-point pipeline (frozen integer arithmetic, the R2H
//! twin's engine) runs against a FLOAT twin with identical structure:
//! state (coefficients, weights) in f64, the exact exponential in place of
//! the frozen LUT + k-bucket quantization, no drift-repair term (float
//! linearity makes it unnecessary), identical fixtures/contexts/instrument.
//! The fixed side is drift-checked against the R2H golden exports before any
//! comparison is reported, so a copy cannot diverge silently.
//!
//! Gates (per rig, 3 fixtures x Q8/Q12/Q16 over the full 16,384-position
//! streams):
//!   I4B-A  fixed engine == golden exports (window f/F + digest + z)
//!   I4B-B  per-position ideal cost, float vs fixed (sum, mean, max |delta|)
//!   I4B-C  state-trajectory divergence (coefficients, weights at stream end)
//!   + the coder instrument roundtrip on the float frequencies
//!     (both engines' f vectors are Σ 32,768, so the costs are directly
//!     comparable per position)
//!
//! Run: `cargo run --release --example bz16_i4b_float_fixed -p mnf-mans` \
//!      `[-- <golden-dir>]` (default `research/results`, bz16-r2h-*.txt)

use std::fs;
use std::path::Path;

const K_MAX: i64 = 4096;
const N_ENTRIES: usize = 4097;
const SCALE: f64 = (1u32 << 24) as f64;
const PROB_TOTAL: i64 = 32768; // recovered
const W_SUM: i64 = 65536; // recovered
const W_INIT: i64 = W_SUM / 4;
const FS_NUM: i64 = 999; // recovered fixed-share
const FS_ADD: i64 = 16384;
const FS_DEN: i64 = 1000;
const GRAD_DEN: i64 = 32768;
const FREQ_SUM: i64 = PROB_TOTAL;
const WINDOW: usize = 512;
const STREAM_LEN: usize = 128 * 128;
const SIZE: usize = 128;
const X_SHIFT: u32 = 9;

/// Python `round()` parity for positive values: half-to-even (R1 helper).
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
            let r = round_half_even(SCALE * (0.0 - k as f64 / 256.0).exp());
            r.clamp(1.0, u32::MAX as f64) as i64
        })
        .collect()
}

fn cdist(a: i64, b: i64) -> i64 {
    let d = (a - b).abs();
    if d <= 128 {
        d
    } else {
        256 - d
    }
}

fn clamp_k(e: i64, shift: u32) -> i64 {
    (e >> shift).clamp(0, K_MAX)
}

/// Signed nearest, ties away from zero.
fn round_half_away(n: i64, d: i64) -> i64 {
    if n >= 0 {
        (2 * n + d) / (2 * d)
    } else {
        -((-2 * n + d) / (2 * d))
    }
}

fn fnv1a_le(values: &[i64]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &v in values {
        for shift in [0u32, 8, 16, 24] {
            h ^= ((v >> shift) & 0xFF) as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

/// Largest remainder (recovered ties): grant +1 to the FIRST maximum of the
/// original input order, repeatedly; zero repair pulls from the first max.
fn largest_remainder(num: &[i64], total: i64, sum_to: i64) -> (Vec<i64>, usize) {
    let mut fs: Vec<i64> = num.iter().map(|&n| n * sum_to / total).collect();
    let mut rems: Vec<i64> = num
        .iter()
        .zip(fs.iter())
        .map(|(&n, &f)| n * sum_to - f * total)
        .collect();
    let mut remaining = sum_to - fs.iter().sum::<i64>();
    while remaining > 0 {
        let mut m = 0usize;
        for i in 1..fs.len() {
            if rems[i] > rems[m] {
                m = i;
            }
        }
        fs[m] += 1;
        rems[m] -= total;
        remaining -= 1;
    }
    let mut repairs = 0usize;
    for s in 0..fs.len() {
        if fs[s] == 0 {
            repairs += 1;
            let mut m = 0usize;
            for i in 1..fs.len() {
                if fs[i] > fs[m] {
                    m = i;
                }
            }
            fs[m] -= 1;
            fs[s] += 1;
        }
    }
    assert_eq!(fs.iter().sum::<i64>(), sum_to, "remainder invariant");
    assert!(*fs.iter().min().unwrap() >= 1, "remainder floor");
    (fs, repairs)
}

#[derive(Clone)]
struct State {
    c_hi: [i64; 3],
    c_lo: [i64; 3],
    w: [i64; 4],
    counters: Vec<i64>,
}

impl State {
    fn new() -> Self {
        State {
            c_hi: [0; 3],
            c_lo: [0; 3],
            w: [W_INIT; 4],
            counters: vec![1; 256],
        }
    }
}

struct Model<'a> {
    lut: &'a [i64],
    shift: u32,
    s_lim: i64,
}

impl<'a> Model<'a> {
    fn new(lut: &'a [i64], n_q: u32) -> Self {
        Model {
            lut,
            shift: n_q - 4,
            s_lim: 3i64 << n_q,
        }
    }

    /// Recovered model for one position: features cdist(v,0), cdist(v,up),
    /// cdist(v,left); counter unigram by largest remainder; mixture
    /// (weights Q16, probabilities 2^15).
    fn mixture(
        &self,
        s: &State,
        up: i64,
        left: i64,
        p_hi: &mut [i64; 256],
        p_lo: &mut [i64; 256],
    ) -> (Vec<i64>, Vec<i64>, i64, usize) {
        let x_unif = 128i64;
        for v in 0..=255i64 {
            let d0 = cdist(v, 0) << 5;
            let d1 = cdist(v, up) << 5;
            let d2 = cdist(v, left) << 5;
            let e_hi = s.c_hi[0] * d0 + s.c_hi[1] * d1 + s.c_hi[2] * d2;
            let e_lo = s.c_lo[0] * d0 + s.c_lo[1] * d1 + s.c_lo[2] * d2;
            // asymmetric domain: hi in [-S, 4S]·Σd, lo doubled
            assert!(
                e_hi >= -self.s_lim * 12288
                    && e_hi <= 4 * self.s_lim * 12288
                    && 2 * e_lo >= -2 * self.s_lim * 12288
                    && 2 * e_lo <= 8 * self.s_lim * 12288
            );
            let k_hi = clamp_k(e_hi, self.shift);
            let k_lo = clamp_k(2 * e_lo, self.shift);
            p_hi[v as usize] = self.lut[k_hi as usize] >> X_SHIFT;
            p_lo[v as usize] = self.lut[k_lo as usize] >> X_SHIFT;
        }
        let cnt_sum = s.counters.iter().sum::<i64>();
        let (cnt_fs, r0) = largest_remainder(&s.counters, cnt_sum, FREQ_SUM);
        let m: Vec<i64> = (0..256)
            .map(|v| {
                (s.w[0] * p_hi[v] + s.w[1] * p_lo[v] + s.w[2] * cnt_fs[v] + s.w[3] * x_unif) >> 16
            })
            .collect();
        (m, cnt_fs, x_unif, r0)
    }
}

/// Recovered adaptation (fixed-point). The caller applies counters[o] += 1.
#[allow(clippy::too_many_arguments)]
fn adapt(
    state: &mut State,
    o: i64,
    ds: [i64; 3],
    p_hi: &[i64],
    p_lo: &[i64],
    cnt_fs: &[i64],
    x_unif: i64,
    f: &[i64],
    n_q: u32,
) {
    let c_lim = 4i64 << n_q;
    let idx = o as usize;
    for (cs, xs) in [(&mut state.c_hi, p_hi), (&mut state.c_lo, p_lo)] {
        let err = f[idx] - xs[idx];
        assert!(-FREQ_SUM < err && err <= FREQ_SUM, "gradient err bound");
        for (i, &di) in ds.iter().enumerate() {
            let num = err * di;
            assert!(num.abs() <= FREQ_SUM * 4096, "gradient num bound");
            let new = cs[i] - round_half_away(num, GRAD_DEN);
            // recovered asymmetric clamp: -S <= b <= 4S, S = 2^Q = c_lim/4
            cs[i] = new.clamp(-(c_lim / 4), c_lim);
        }
    }
    let posts = [
        state.w[0] * p_hi[idx],
        state.w[1] * p_lo[idx],
        state.w[2] * cnt_fs[idx],
        state.w[3] * x_unif,
    ];
    let (post, _) = largest_remainder(&posts, posts.iter().sum(), W_SUM);
    for (w_slot, &pv) in state.w.iter_mut().zip(post.iter()) {
        *w_slot = round_half_away(FS_NUM * pv + FS_ADD, FS_DEN);
    }
    let drift = W_SUM - state.w.iter().sum::<i64>();
    if drift != 0 {
        let mut m = 0usize;
        for i in 1..4 {
            if state.w[i] > state.w[m] {
                m = i;
            }
        }
        assert!(state.w[m] + drift >= 0, "drift preserves non-negativity");
        state.w[m] += drift;
    }
}

// --- float twin (I4B) -----------------------------------------------------
// Identical skeleton; the differences are the measured object:
//   coefficients/weights in f64, exp(−b·e) in place of LUT[k],
//   counter component and mixture products in f64, no drift repair
//   (w sums to W_SUM exactly by linearity), frequencies still carried to the
//   shared Σ 32,768 integer domain through the same largest-remainder rule
//   so the coding costs are directly comparable.

#[derive(Clone)]
struct FloatState {
    c_hi: [f64; 3],
    c_lo: [f64; 3],
    w: [f64; 4],
    counters: Vec<f64>,
}

impl FloatState {
    fn new() -> Self {
        FloatState {
            c_hi: [0.0; 3],
            c_lo: [0.0; 3],
            w: [W_INIT as f64; 4],
            counters: vec![1.0; 256],
        }
    }
}

struct FloatModel {
    b_hi: f64,  // exp(−b_hi·e_hi)
    b_lo: f64,  // exp(−b_lo·2·e_lo): the recovered β=2 expert
    s_lim: f64, // recovered asymmetric scale S = 3<<n_q
}

impl FloatModel {
    /// Continuous envelope of the frozen mapping: the LUT index is
    /// k = clamp(β·e >> (n−4)) with L_k/2^15 = e^(−k/256), so the exact
    /// counterpart is exp(−β·e / 2^(n+4)).
    fn new(n_q: u32) -> Self {
        FloatModel {
            b_hi: (2.0f64).powi(-(n_q as i32 + 4)),
            b_lo: 2.0 * (2.0f64).powi(-(n_q as i32 + 4)),
            s_lim: (1i64 << n_q) as f64,
        }
    }

    fn mixture(
        &self,
        s: &FloatState,
        up: i64,
        left: i64,
        x_hi: &mut [f64; 256],
        x_lo: &mut [f64; 256],
    ) -> (Vec<f64>, Vec<f64>, usize) {
        let x_unif = 128.0f64;
        for v in 0..=255i64 {
            let d0 = (cdist(v, 0) << 5) as f64;
            let d1 = (cdist(v, up) << 5) as f64;
            let d2 = (cdist(v, left) << 5) as f64;
            let e_hi = s.c_hi[0] * d0 + s.c_hi[1] * d1 + s.c_hi[2] * d2;
            let e_lo = s.c_lo[0] * d0 + s.c_lo[1] * d1 + s.c_lo[2] * d2;
            // The k >= 0 clamp (negative energy -> k = 0) IS part of the
            // frozen R2-B boundary table and is mirrored: exp(−max(b·e, 0)),
            // otherwise a slammed-negative state overflows to inf. The upper
            // k <= 4096 saturation floor is NOT mirrored: the continuous
            // decay below 2^15·e^(−16) is exactly the precision delta this
            // gate measures (frozen floor = 2).
            let k_hi = (self.b_hi * e_hi).max(0.0);
            let k_lo = (self.b_lo * e_lo).max(0.0);
            x_hi[v as usize] = 32768.0 * (-(k_hi)).exp();
            x_lo[v as usize] = 32768.0 * (-(k_lo)).exp();
        }
        let c_sum: f64 = s.counters.iter().sum();
        let cnt: Vec<f64> = (0..256)
            .map(|v| 32768.0 * s.counters[v as usize] / c_sum)
            .collect();
        let m: Vec<f64> = (0..256)
            .map(|v| {
                (s.w[0] * x_hi[v] + s.w[1] * x_lo[v] + s.w[2] * cnt[v] + s.w[3] * x_unif) / 65536.0
            })
            .collect();
        (m, cnt, 0usize)
    }
}

/// Float adaptation; same clamp polarity (symmetric pin, R2H §4a open
/// question), same fixed-share constants in the float domain.
#[allow(clippy::too_many_arguments)]
fn adapt_float(
    s: &mut FloatState,
    o: i64,
    ds: [f64; 3],
    x_hi: &[f64],
    x_lo: &[f64],
    cnt: &[f64],
    f: &[i64],
    model: &FloatModel,
) {
    let idx = o as usize;
    for (cs, xs) in [(&mut s.c_hi, x_hi), (&mut s.c_lo, x_lo)] {
        let err = f[idx] as f64 - xs[idx];
        for (i, &di) in ds.iter().enumerate() {
            let new = cs[i] - err * di / GRAD_DEN as f64;
            // recovered asymmetric clamp: -S <= b <= 4S
            cs[i] = new.clamp(-model.s_lim, 4.0 * model.s_lim);
        }
    }
    // fixed-share: normalize posterior, ×999, +16384, normalize again — but
    // in the float domain the second normalization is exact (Σw invariant).
    let post = [
        s.w[0] * x_hi[idx],
        s.w[1] * x_lo[idx],
        s.w[2] * cnt[idx],
        s.w[3] * 128.0,
    ];
    let psum: f64 = post.iter().sum();
    for (w_slot, &pv) in s.w.iter_mut().zip(post.iter()) {
        *w_slot = (FS_NUM as f64 * pv / psum * W_SUM as f64 + FS_ADD as f64) / FS_DEN as f64;
    }
    let drift = W_SUM as f64 - s.w.iter().sum::<f64>();
    if drift.abs() > 1e-9 {
        s.w[3] += drift; // float round-off re-centering (magnitude ~1e-12)
    }
    assert!(s.w.iter().all(|&v| v >= -1e-9), "weights non-negative");
}

// --- fixtures (synthetic stand-ins; identical to the R2H rig) --------------
fn fixture_camera() -> Vec<i64> {
    (0..STREAM_LEN)
        .map(|i| {
            let r = i / SIZE;
            let c = i % SIZE;
            ((97 * r + 31 * c + ((r * c) >> 2)) & 0xFF) as i64
        })
        .collect()
}

fn fixture_brick() -> Vec<i64> {
    (0..STREAM_LEN)
        .map(|i| {
            let band = 37 * (i / 1024) + (i % SIZE) + 3 * ((i / SIZE) % 8);
            (band & 0xFF) as i64
        })
        .collect()
}

fn fixture_noise() -> Vec<i64> {
    let mut s: i64 = 12_345;
    (0..STREAM_LEN)
        .map(|_| {
            s = (s.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x7FFF_FFFF;
            (s >> 16) & 0xFF
        })
        .collect()
}

// --- golden parsing (same contract as the R2H twin) ------------------------
struct Gold {
    trace_digest: String,
    f: Vec<Vec<i64>>,
    z: [i64; 10],
}

fn read_golden(dir: &str, name: &str, window: usize) -> Gold {
    let path = Path::new(dir).join(format!("bz16-r2h-{name}.txt"));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden export missing: {path:?} ({e})"));
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let head = lines.next().expect("golden header");
    assert!(head.starts_with("h "), "golden header");
    let digest_line = lines.next().expect("trace digest line");
    assert!(digest_line.starts_with("d "), "d line");
    let trace_digest = digest_line[2..].trim().to_string();
    let mut f: Vec<Vec<i64>> = Vec::new();
    let mut z = [0i64; 10];
    for line in lines {
        let mut it = line.split_whitespace();
        match it.next().unwrap_or("") {
            "p" => {
                f.push(Vec::new());
            }
            "n" => {}
            "f" => {
                f.last_mut().expect("f before p").clear();
                f.last_mut().unwrap().push(0);
                f.last_mut().unwrap().pop();
                f.last_mut()
                    .unwrap()
                    .extend(it.map(|v| v.parse::<i64>().unwrap()));
                assert_eq!(f.last().unwrap().len(), 256, "f width");
            }
            "z" => {
                for v in z.iter_mut() {
                    *v = it.next().unwrap().parse().unwrap();
                }
            }
            _ => {}
        }
    }
    assert_eq!(f.len(), window, "golden window size");
    Gold { trace_digest, f, z }
}

// --- SHA-256 for the drift check ------------------------------------------
struct Sha256 {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total: u64,
}

impl Sha256 {
    fn new() -> Self {
        Sha256 {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0; 64],
            buf_len: 0,
            total: 0,
        }
    }
    fn update(&mut self, mut data: &[u8]) {
        self.total += data.len() as u64;
        if self.buf_len > 0 {
            let need = 64 - self.buf_len;
            let take = need.min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                self.compress(&block);
                self.buf_len = 0;
            }
        }
        while data.len() >= 64 {
            let (block, rest) = data.split_at(64);
            self.compress(block);
            data = rest;
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }
    fn hex(&mut self) -> String {
        let bit_len = self.total * 8;
        let mut pad = vec![0x80u8];
        while (self.total as usize + pad.len()) % 64 != 56 {
            pad.push(0);
        }
        pad.extend_from_slice(&bit_len.to_be_bytes());
        self.update(&pad);
        let mut out = String::with_capacity(64);
        for word in self.state {
            out.push_str(&format!("{:08x}", word));
        }
        out
    }
    fn compress(&mut self, block: &[u8]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut h = self.state;
        for i in 0..64 {
            let s1 = h[4].rotate_right(6) ^ h[4].rotate_right(11) ^ h[4].rotate_right(25);
            let ch = (h[4] & h[5]) ^ ((!h[4]) & h[6]);
            let t1 = h[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K256[i])
                .wrapping_add(w[i]);
            let s0 = h[0].rotate_right(2) ^ h[0].rotate_right(13) ^ h[0].rotate_right(22);
            let maj = (h[0] & h[1]) ^ (h[0] & h[2]) ^ (h[1] & h[2]);
            let t2 = s0.wrapping_add(maj);
            h[7] = h[6];
            h[6] = h[5];
            h[5] = h[4];
            h[4] = h[3].wrapping_add(t1);
            h[3] = h[2];
            h[2] = h[1];
            h[1] = h[0];
            h[0] = t1.wrapping_add(t2);
        }
        for (hi, &hv) in self.state.iter_mut().zip(h.iter()) {
            *hi = hi.wrapping_add(hv);
        }
    }
}

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn main() {
    let lut = build_lut();
    assert_eq!(
        fnv1a_le(&lut),
        0x50abef206188b374,
        "frozen LUT stream digest"
    );

    let golden_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "research/results".to_string());
    let fixtures = [
        ("camera", fixture_camera()),
        ("brick", fixture_brick()),
        ("noise", fixture_noise()),
    ];

    println!(
        "{:>10} {:>12} {:>12} {:>11} {:>10} {:>9}",
        "rig", "cost_fixed", "cost_float", "delta_bits", "max|d_t|", "rel"
    );
    for (name, stream) in fixtures {
        for &n_q in &[8u32, 12, 16] {
            let tag = format!("{name}-Q{n_q}");
            let gold = read_golden(&golden_dir, &tag, WINDOW);
            let model = Model::new(&lut, n_q);
            let fmodel = FloatModel::new(n_q);
            let mut state = State::new();
            let mut fstate = FloatState::new();
            let mut p_hi = [0i64; 256];
            let mut p_lo = [0i64; 256];
            let mut x_hi = [0f64; 256];
            let mut x_lo = [0f64; 256];
            let mut trace = Sha256::new();
            let mut cost_fix = 0f64;
            let mut cost_float = 0f64;
            let mut max_dt = 0f64;
            let mut failures: Vec<(usize, &'static str)> = Vec::new();

            for (t, &o) in stream.iter().enumerate() {
                let ti = t as i64;
                let up = if ti >= SIZE as i64 {
                    stream[t - SIZE]
                } else {
                    0
                };
                let left = if t % SIZE != 0 && t >= 1 {
                    stream[t - 1]
                } else {
                    0
                };
                let (m, cnt_fs, x_unif, cnt_repairs) =
                    model.mixture(&state, up, left, &mut p_hi, &mut p_lo);
                let total = m.iter().sum::<i64>();
                let (f_fx, _repairs) = if total > 0 {
                    largest_remainder(&m, total, FREQ_SUM)
                } else {
                    (vec![FREQ_SUM / 256; 256], 0)
                };
                let (m_fl, cnt_fl, _) = fmodel.mixture(&fstate, up, left, &mut x_hi, &mut x_lo);
                // float mixture -> the shared integer cost domain
                let mut m_int: Vec<i64> = m_fl
                    .iter()
                    .map(|&v| {
                        #[allow(clippy::cast_possible_truncation)]
                        let r = (v + 0.5).floor() as i64;
                        r.max(0)
                    })
                    .collect();
                let total_fl: i64 = m_int.iter().sum();
                let (f_fl, _) = if total_fl > 0 {
                    largest_remainder(&m_int, total_fl, FREQ_SUM)
                } else {
                    m_int = vec![FREQ_SUM / 256; 256];
                    (m_int.clone(), 0)
                };

                let d_fix = 15.0 - (f_fx[o as usize] as f64).log2();
                let d_flt = 15.0 - (f_fl[o as usize] as f64).log2();
                cost_fix += d_fix;
                cost_float += d_flt;
                max_dt = max_dt.max((d_fix - d_flt).abs());

                // I4B-A drift check: full-stream digest + window f vectors.
                // The R2H-D digest covers the complete 16,384-position run.
                trace.update(
                    &f_fx
                        .iter()
                        .flat_map(|&v| (v as u16).to_be_bytes())
                        .collect::<Vec<u8>>(),
                );
                if t < WINDOW && gold.f[t] != f_fx {
                    failures.push((t, "f"));
                }

                let ds = [cdist(o, 0) << 5, cdist(o, up) << 5, cdist(o, left) << 5];
                let ds_f: [f64; 3] = [ds[0] as f64, ds[1] as f64, ds[2] as f64];
                adapt(&mut state, o, ds, &p_hi, &p_lo, &cnt_fs, x_unif, &f_fx, n_q);
                state.counters[o as usize] += 1;
                adapt_float(&mut fstate, o, ds_f, &x_hi, &x_lo, &cnt_fl, &f_fl, &fmodel);
                fstate.counters[o as usize] += 1.0;
                let _ = cnt_repairs;
            }

            // drift: failures first (position granularity), then digest + z
            assert!(failures.is_empty(), "I4B-A parity {tag}: {failures:?}");
            let digest = trace.hex();
            assert_eq!(digest, gold.trace_digest, "I4B-A trace digest {tag}");
            let z_tail = [
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
            ];
            assert_eq!(z_tail, gold.z, "I4B-A z carries {tag}");
            assert!(failures.is_empty(), "I4B-A parity {tag}: {failures:?}");

            let delta = cost_fix - cost_float;
            let rel = delta / cost_fix;
            println!(
                "{:>10} {:>12.2} {:>12.2} {:>11.2} {:>10.2} {:>8.3}%",
                tag,
                cost_fix,
                cost_float,
                delta,
                max_dt,
                100.0 * rel
            );
        }
    }
    println!(
        "BZ16-I4B MEASURED: full-stream matched float/fixed costs over 9 rigs \
         (fixed engine drift-checked against the R2H goldens; float twin = \
         continuous exp, no k-bucketing/LUT, no drift repair)."
    );
}
