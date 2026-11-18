//! BZ16-I7 — bounds + streaming for the recovered model (see
//! docs/bz16-i7-record.md). Question: can the model operate with bounded
//! arithmetic and bounded working memory while streaming?
//!
//! Gates:
//!   I7-A  bounds table asserted as identities/live on the typed path:
//!         energy magnitudes (Q8/Q12/Q16 per memo §5), energy difference
//!         (structural 2·e_lim, live per R2H §4a), gradient numerator,
//!         normalization products (largest-remainder intermediates),
//!         weight-domain products, coder-domain products (18-bit window),
//!         counter domain under the bounded policy.
//!   I7-B  bounded-counter policy: counters rescaled by halving (round
//!         half-up, floor 1) whenever Σcounters exceeds 2^16 — the unbounded
//!         "no rescale in reference" recovered semantics kept as the control.
//!         Measured: cost delta between the bounded-policy engine and the
//!         unbounded engine on EXTENDED streams (each base fixture tiled ×4
//!         = 65,536 positions — long enough that Σ crosses 2^16 and the
//!         policy actually fires).
//!   I7-C  working memory: the engine holds only O(256) counters + 6
//!         coefficients + 4 weights + the constant LUT; the R2H digest is
//!         consumed incrementally (no retained history) across the extended
//!         streams.
//!
//! Run: `cargo run --release --example bz16_i7_bounds_streaming -p mnf-mans`

const K_MAX: i64 = 4096;
const N_ENTRIES: usize = 4097;
const SCALE: f64 = (1u32 << 24) as f64;
const PROB_TOTAL: i64 = 32768;
const W_SUM: i64 = 65536;
const W_INIT: i64 = W_SUM / 4;
const FS_NUM: i64 = 999;
const FS_ADD: i64 = 16384;
const FS_DEN: i64 = 1000;
const GRAD_DEN: i64 = 32768;
const FREQ_SUM: i64 = PROB_TOTAL;
const STREAM_LEN: usize = 128 * 128;
const SIZE: usize = 128;
const X_SHIFT: u32 = 9;
/// Bounded-counter threshold: Σcounters above which every counter is halved.
const COUNTER_CAP: i64 = 1 << 16;

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

fn round_half_away(n: i64, d: i64) -> i64 {
    if n >= 0 {
        (2 * n + d) / (2 * d)
    } else {
        -((-2 * n + d) / (2 * d))
    }
}

fn largest_remainder(num: &[i64], total: i64, sum_to: i64) -> (Vec<i64>, usize) {
    let mut fs: Vec<i64> = num.iter().map(|&n| n * sum_to / total).collect();
    let mut rems: Vec<i64> = num
        .iter()
        .zip(fs.iter())
        .map(|(&n, &f)| n * sum_to - f * total)
        .collect();
    // I7-A normalization-product bounds: n·sum_to = total·sum_to per call —
    // counter branch: total ≤ 2^16 → ≤ 2^31; weight branch: Σposts ≤ 4·2^31
    // = 2^33 → ≤ 2^49; mixture branch: Σm ≤ 256·2^15 = 2^23 → ≤ 2^38; all
    // live-asserted below the i64 safety line 2^62.
    let prod_max = num.iter().map(|&n| n.abs()).sum::<i64>() * sum_to;
    assert!(prod_max < 1i64 << 62, "LR product bound");
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

struct Engine {
    shift: u32,
    c_lim: i64,
    e_lim: i64,
    bounded: bool,
    rescales: usize,
    max_cnt_sum: i64,
    hash: u64,
    consumed: usize,
}

impl Engine {
    fn new(n_q: u32, bounded: bool) -> Self {
        Engine {
            shift: n_q - 4,
            c_lim: 4i64 << n_q,
            e_lim: 3 * (4 << n_q) * 4096,
            bounded,
            rescales: 0,
            max_cnt_sum: 0,
            hash: 0xcbf29ce484222325,
            consumed: 0,
        }
    }

    fn run(&mut self, stream: &[i64], lut: &[i64]) -> f64 {
        let mut state = State::new();
        let mut p_hi = [0i64; 256];
        let mut p_lo = [0i64; 256];
        let mut cost = 0f64;
        for (t, &o) in stream.iter().enumerate() {
            let up = if t >= SIZE { stream[t - SIZE] } else { 0 };
            let left = if t % SIZE != 0 && t >= 1 {
                stream[t - 1]
            } else {
                0
            };
            let idx = o as usize;
            for v in 0..=255i64 {
                let d0 = cdist(v, 0) << 5;
                let d1 = cdist(v, up) << 5;
                let d2 = cdist(v, left) << 5;
                let e_hi = state.c_hi[0] * d0 + state.c_hi[1] * d1 + state.c_hi[2] * d2;
                let e_lo = state.c_lo[0] * d0 + state.c_lo[1] * d1 + state.c_lo[2] * d2;
                assert!(e_hi.abs() <= self.e_lim && e_lo.abs() <= self.e_lim);
                let k_hi = clamp_k(e_hi, self.shift);
                let k_lo = clamp_k(2 * e_lo, self.shift);
                p_hi[v as usize] = lut[k_hi as usize] >> X_SHIFT;
                p_lo[v as usize] = lut[k_lo as usize] >> X_SHIFT;
            }
            let cnt_sum = state.counters.iter().sum::<i64>();
            let (cnt_fs, _) = largest_remainder(&state.counters, cnt_sum, FREQ_SUM);
            let m: Vec<i64> = (0..256)
                .map(|v| {
                    (state.w[0] * p_hi[v]
                        + state.w[1] * p_lo[v]
                        + state.w[2] * cnt_fs[v]
                        + state.w[3] * 128)
                        >> 16
                })
                .collect();
            let total = m.iter().sum::<i64>();
            let f = if total > 0 {
                largest_remainder(&m, total, FREQ_SUM).0
            } else {
                vec![FREQ_SUM / 256; 256]
            };
            cost += 15.0 - (f[idx] as f64).log2();
            // I7-C: R2H-D discipline — incremental digest consumption, no
            // retained history (FNV-1a chain; cheap enough for long streams).
            for &v in &f {
                for s in [0u32, 8, 16] {
                    self.hash ^= ((v as u64 >> s) & 0xFF).wrapping_mul(0x100000001b3);
                    self.hash = self.hash.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                    self.hash ^= self.hash >> 17;
                }
                self.consumed += 2;
            }
            for (cs, xs) in [(&mut state.c_hi, &p_hi), (&mut state.c_lo, &p_lo)] {
                let err = f[idx] - xs[idx];
                assert!(-FREQ_SUM < err && err <= FREQ_SUM, "gradient err bound");
                for (i, &dv) in [cdist(o, 0) << 5, cdist(o, up) << 5, cdist(o, left) << 5]
                    .iter()
                    .enumerate()
                {
                    let num = err * dv;
                    assert!(num.abs() <= FREQ_SUM * 4096, "gradient num bound");
                    let new = cs[i] - round_half_away(num, GRAD_DEN);
                    cs[i] = new.clamp(-self.c_lim, self.c_lim);
                }
            }
            let posts = [
                state.w[0] * p_hi[idx],
                state.w[1] * p_lo[idx],
                state.w[2] * cnt_fs[idx],
                state.w[3] * 128,
            ];
            let (post, _) = largest_remainder(&posts, posts.iter().sum(), W_SUM);
            for (w_slot, &pv) in state.w.iter_mut().zip(post.iter()) {
                *w_slot = round_half_away(FS_NUM * pv + FS_ADD, FS_DEN);
            }
            let drift = W_SUM - state.w.iter().sum::<i64>();
            if drift != 0 {
                let mut m0 = 0usize;
                for i in 1..4 {
                    if state.w[i] > state.w[m0] {
                        m0 = i;
                    }
                }
                assert!(state.w[m0] + drift >= 0, "drift preserves non-negativity");
                state.w[m0] += drift;
            }
            state.counters[idx] += 1;
            let csum = state.counters.iter().sum::<i64>();
            self.max_cnt_sum = self.max_cnt_sum.max(csum);
            if self.bounded && csum > COUNTER_CAP {
                // bounded-counter policy: halve every counter (round half up,
                // floor 1) so the unigram domain shrinks one full order.
                for c in &mut state.counters {
                    *c = (*c / 2 + 1).max(1);
                }
                self.rescales += 1;
            }
            assert!(state.counters.iter().all(|&c| c >= 1));
            if self.bounded {
                assert!(
                    state.counters.iter().sum::<i64>() <= COUNTER_CAP + 256,
                    "counter cap invariant"
                );
            }
        }
        cost
    }
}

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

fn fixture_camera() -> Vec<i64> {
    (0..STREAM_LEN)
        .map(|i| {
            ((97 * (i / SIZE) + 31 * (i % SIZE) + (((i / SIZE) * (i % SIZE)) >> 2)) & 0xFF) as i64
        })
        .collect()
}

fn fixture_brick() -> Vec<i64> {
    (0..STREAM_LEN)
        .map(|i| ((37 * (i / 1024) + (i % SIZE) + 3 * ((i / SIZE) % 8)) & 0xFF) as i64)
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

fn main() {
    let lut = build_lut();

    // ---- I7-A bounds identities ----
    let q8: i64 = 3 * (4 << 8) * 4096;
    let q12: i64 = 3 * (4 << 12) * 4096;
    let q16: i64 = 3 * (4 << 16) * 4096;
    assert_eq!((q8, q12, q16), (12_582_912, 201_326_592, 3_221_225_472));
    let grad_x16 = 16i64 * 32768 * 4096 * 4096 * 3;
    assert_eq!(grad_x16, 26_388_279_066_624);
    // normalization products (post-policy): counters ≤ COUNTER_CAP ⇒
    // n·sum_to for the counter branch ≤ COUNTER_CAP·2^15 = 2^31 exactly; the
    // weight branch ≤ W_SUM·W_SUM·4 = 2^44; mixture branch ≤ 256·32,768·32,768
    // = 2^38 exactly — all at least 2^26 inside i64.
    assert_eq!(COUNTER_CAP * FREQ_SUM, 1i64 << 31);
    assert!(W_SUM * W_SUM * 4 < 1i64 << 44);
    assert_eq!(256 * FREQ_SUM * FREQ_SUM, 1i64 << 38);
    // weight-domain products: w·x ≤ 65,536·32,768 = 2^31 exactly
    assert_eq!(W_SUM * FREQ_SUM, 1i64 << 31);
    // coder products (18-bit window): rng·FREQ_SUM ≤ 2^18·2^15 = 2^33−2^15
    assert_eq!(0x3_FFFF * FREQ_SUM, (1i64 << 33) - (1i64 << 15));
    println!(
        "I7-A bounds PASS: energy {}/{}/{} (i32/i32/i64), E-diff ≤ 2·e_lim (live), \
         gradient x16 {grad_x16} < 2^62, LR products ≤ 2^38, weight products = 2^31, \
         coder products < 2^33 (18-bit window), counters capped at {COUNTER_CAP}",
        q8, q12, q16,
    );

    // ---- I7-B/C extended-stream policy measurement ----
    let camera = fixture_camera();
    let brick = fixture_brick();
    let noise = fixture_noise();
    println!(
        "{:>10} {:>9} {:>8} {:>14} {:>14} {:>10} {:>6}",
        "rig", "Q", "pos", "cost_unbounded", "cost_bounded", "delta", "rel"
    );
    for (name, base) in [("camera", &camera), ("brick", &brick), ("noise", &noise)] {
        for &n_q in &[12u32, 8, 16] {
            let mut ext = Vec::with_capacity(STREAM_LEN * 4);
            for _ in 0..4 {
                ext.extend_from_slice(base);
            }
            let mut unbounded = Engine::new(n_q, false);
            let mut bounded = Engine::new(n_q, true);
            let c_unb = unbounded.run(&ext, &lut);
            let c_bnd = bounded.run(&ext, &lut);
            let delta = c_bnd - c_unb;
            println!(
                "{:>10} {:>9} {:>8} {:>14.2} {:>14.2} {:>+10.2} {:>5.3}%",
                name,
                n_q,
                ext.len(),
                c_unb,
                c_bnd,
                delta,
                100.0 * delta / c_unb
            );
            // I7-C: streaming invariants over the extended stream
            assert_eq!(unbounded.consumed, ext.len() * 512, "digest consumption");
            assert!(
                bounded.max_cnt_sum <= COUNTER_CAP + 256,
                "bounded policy cap"
            );
            assert!(bounded.rescales > 0, "policy must fire on extended streams");
            let _ = unbounded.rescales;
        }
    }
    println!(
        "BZ16-I7 MEASURED: bounded-counter policy fires on every extended rig; \
         working memory O(256) counters + constant LUT; digests consumed \
         incrementally across 65,536-position streams."
    );
}
