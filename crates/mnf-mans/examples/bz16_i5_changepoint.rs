//! BZ16-I5 — change-point evaluation (revised methodology; see
//! docs/bz16-i5-record.md). Question: does fixed-share adaptation recover
//! efficiently when the underlying symbol distribution changes?
//!
//! Streams: two 8192+8192 concatenations with the distribution change at
//! t* = 8192 — camera→brick (downward cost shift) and brick→camera
//! (upward shift). Engine: the recovered fixed-point pipeline (the same
//! code I4B anchored against the R2H goldens), Q12 primary; Q8/Q16 as
//! ablation controls.
//! Metrics per rig:
//!   steady_before  mean b/sym over [t*−512, t*)
//!   steady_after   mean b/sym over [t*+1024, t*+1536) (post-transient)
//!   join transient mean/max b/sym over [t*, t*+256)
//!   recovery R     first t > t* whose trailing-64-position cost mean
//!                  is within 5% of steady_after
//!   restart diff   Σcost over [t*, t*+recovery+512) for the adapted engine
//!                  minus the same sum for an engine seeded FRESH at t*
//!                  (negative => carried state helps, positive => it resists)
//! Fixed-share α sensitivity in the float domain (α ∈ {0.999 recovered,
//! 0.99, 0.9}) on the upward-shift stream.
//!
//! Run: `cargo run --release --example bz16_i5_changepoint -p mnf-mans`

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
const T_STAR: usize = 8192;
const HALF: usize = 8192;

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

/// One full pass of the recovered engine over `stream`, emitting the
/// per-position ideal cost in b/sym units.
fn run_engine(stream: &[i64], n_q: u32, lut: &[i64]) -> Vec<f64> {
    let shift = n_q - 4;
    let c_lim = 4i64 << n_q;
    let mut state = State::new();
    let mut p_hi = [0i64; 256];
    let mut p_lo = [0i64; 256];
    let mut out = Vec::with_capacity(stream.len());
    for (t, &o) in stream.iter().enumerate() {
        let up = if t >= SIZE { stream[t - SIZE] } else { 0 };
        let left = if t % SIZE != 0 && t >= 1 {
            stream[t - 1]
        } else {
            0
        };
        for v in 0..=255i64 {
            let d0 = cdist(v, 0) << 5;
            let d1 = cdist(v, up) << 5;
            let d2 = cdist(v, left) << 5;
            let e_hi = state.c_hi[0] * d0 + state.c_hi[1] * d1 + state.c_hi[2] * d2;
            let e_lo = state.c_lo[0] * d0 + state.c_lo[1] * d1 + state.c_lo[2] * d2;
            // asymmetric domain: hi in [-S, 4S]·Σd, lo doubled
            let s_lim = 3 * c_lim / 4;
            assert!(
                e_hi >= -s_lim * 12288
                    && e_hi <= 4 * s_lim * 12288
                    && 2 * e_lo >= -2 * s_lim * 12288
                    && 2 * e_lo <= 8 * s_lim * 12288
            );
            let k_hi = clamp_k(e_hi, shift);
            let k_lo = clamp_k(2 * e_lo, shift);
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
        out.push(15.0 - (f[o as usize] as f64).log2());
        let idx = o as usize;
        for (cs, xs) in [(&mut state.c_hi, &p_hi), (&mut state.c_lo, &p_lo)] {
            let err = f[idx] - xs[idx];
            for (i, &dv) in [cdist(o, 0) << 5, cdist(o, up) << 5, cdist(o, left) << 5]
                .iter()
                .enumerate()
            {
                let new = cs[i] - round_half_away(err * dv, GRAD_DEN);
                // recovered asymmetric clamp: -S <= b <= 4S, S = 3<<n_q
                cs[i] = new.clamp(-(3 * c_lim / 4), 3 * c_lim);
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
            state.w[m0] += drift;
        }
        state.counters[idx] += 1;
    }
    out
}

/// Float-domain engine for the fixed-share α sensitivity ablation
/// (w' = α·posterior + (1−α)·W/4; α = 0.999 is the recovered equivalent).
#[allow(clippy::needless_range_loop)]
fn run_engine_float(stream: &[i64], n_q: u32, alpha: f64) -> Vec<f64> {
    let b = (2.0f64).powi(-(n_q as i32 + 4));
    let b2 = 2.0 * b;
    let c_lim = (4i64 << n_q) as f64;
    let mut c_hi = [0f64; 3];
    let mut c_lo = [0f64; 3];
    let mut w = [W_INIT as f64; 4];
    let mut counters = vec![1.0f64; 256];
    let mut out = Vec::with_capacity(stream.len());
    for (t, &o) in stream.iter().enumerate() {
        let up = if t >= SIZE { stream[t - SIZE] } else { 0 };
        let left = if t % SIZE != 0 && t >= 1 {
            stream[t - 1]
        } else {
            0
        };
        let idx = o as usize;
        let mut xs = [[0f64; 256]; 2];
        let mut ds = [[0f64; 3]; 2];
        for v in 0..=255i64 {
            let d0 = (cdist(v, 0) << 5) as f64;
            let d1 = (cdist(v, up) << 5) as f64;
            let d2 = (cdist(v, left) << 5) as f64;
            for (row, cs) in [&c_hi, &c_lo].iter().enumerate() {
                let e = cs[0] * d0 + cs[1] * d1 + cs[2] * d2;
                let beta_b = if row == 0 { b } else { b2 };
                xs[row][v as usize] = 32768.0 * (-(beta_b * e).max(0.0)).exp();
                if row == 0 {
                    ds[0] = [d0, d1, d2];
                }
            }
        }
        ds[1] = ds[0];
        let c_sum: f64 = counters.iter().sum();
        let cnt: Vec<f64> = (0..256)
            .map(|v| 32768.0 * counters[v as usize] / c_sum)
            .collect();
        let m: Vec<f64> = (0..256)
            .map(|v| (w[0] * xs[0][v] + w[1] * xs[1][v] + w[2] * cnt[v] + w[3] * 128.0) / 65536.0)
            .collect();
        let mut m_int: Vec<i64> = m.iter().map(|&v| (v + 0.5).floor() as i64).collect();
        let total: i64 = m_int.iter().sum();
        let f = if total > 0 {
            largest_remainder(&m_int, total, FREQ_SUM).0
        } else {
            m_int = vec![FREQ_SUM / 256; 256];
            m_int.clone()
        };
        out.push(15.0 - (f[idx] as f64).log2());
        for (row, cs) in [&mut c_hi, &mut c_lo].iter_mut().enumerate() {
            let err = f[idx] as f64 - xs[row][idx];
            for (i, &dv) in ds[row].iter().enumerate() {
                let new = cs[i] - err * dv / GRAD_DEN as f64;
                // recovered asymmetric clamp: -S <= b <= 4S
                cs[i] = new.clamp(-0.75 * c_lim, 3.0 * c_lim);
            }
        }
        let post = [
            w[0] * xs[0][idx],
            w[1] * xs[1][idx],
            w[2] * cnt[idx],
            w[3] * 128.0,
        ];
        let psum: f64 = post.iter().sum();
        for (w_slot, &pv) in w.iter_mut().zip(post.iter()) {
            *w_slot = alpha * pv / psum * W_SUM as f64 + (1.0 - alpha) * W_SUM as f64 / 4.0;
        }
        counters[idx] += 1.0;
    }
    out
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn main() {
    let lut = build_lut();
    let camera = fixture_camera();
    let brick = fixture_brick();
    let noise = fixture_noise();
    let _ = noise;
    let joins: Vec<(String, Vec<i64>, bool)> = vec![
        (
            "camera->brick".into(),
            {
                let mut s = camera.clone();
                s.extend_from_slice(&brick[..HALF]);
                let _ = s.len();
                s[..T_STAR + HALF].to_vec()
            },
            false,
        ),
        (
            "brick->camera".into(),
            {
                let mut s = brick.clone();
                s.extend_from_slice(&camera[..HALF]);
                s.to_vec()
            },
            true,
        ),
    ];

    println!(
        "{:>14} {:>6} {:>9} {:>9} {:>9} {:>9} {:>10} {:>7}",
        "rig", "Q", "before", "after", "join_max", "recover", "adapt_x", "restart"
    );
    for (name, stream, up_shift) in &joins {
        for n_q in [12u32, 8, 16] {
            let cost = run_engine(stream, n_q, &lut);
            let before = mean(&cost[T_STAR - 512..T_STAR]);
            let after = mean(&cost[T_STAR + 1024..T_STAR + 1536]);
            let join_max = cost[T_STAR..T_STAR + 256]
                .iter()
                .cloned()
                .fold(0.0f64, f64::max);
            // recovery: trailing-64 mean within 5% of the post-change steady
            let mut recovery = 0usize;
            for k in 0..HALF - 64 {
                let seg = &cost[T_STAR + k..T_STAR + k + 64];
                let mu = mean(seg);
                if (mu - after).abs() <= 0.05 * after {
                    recovery = k + 64;
                    break;
                }
            }
            if recovery == 0 {
                recovery = HALF; // never returned within 5% in half a stream
            }
            // restart reference: fresh engine over the same post-* window
            let fresh = run_engine(&stream[T_STAR..], n_q, &lut);
            let window = recovery.min(HALF) + 512;
            let adapted_sum: f64 = cost[T_STAR..T_STAR + window].iter().sum();
            let fresh_sum: f64 = fresh[..window].iter().sum();
            let restart = adapted_sum - fresh_sum;
            println!(
                "{:>14} {:>6} {:>9.3} {:>9.3} {:>9.3} {:>10} {:>10.1} {:>7.1}",
                name, n_q, before, after, join_max, recovery, window as f64, restart
            );
            let _ = up_shift;
        }
    }

    // fixed-share α sensitivity (float domain), upward-shift stream, Q12
    println!("\nalpha sensitivity (float engine, brick->camera Q12):");
    for &alpha in &[0.999f64, 0.99, 0.9] {
        let cost = run_engine_float(&joins[1].1, 12, alpha);
        let after = mean(&cost[T_STAR + 1024..T_STAR + 1536]);
        let mut recovery = 0usize;
        for k in 0..HALF - 64 {
            let seg = &cost[T_STAR + k..T_STAR + k + 64];
            let mu = mean(seg);
            if (mu - after).abs() <= 0.05 * after {
                recovery = k + 64;
                break;
            }
        }
        if recovery == 0 {
            recovery = HALF;
        }
        let join_max = cost[T_STAR..T_STAR + 256]
            .iter()
            .cloned()
            .fold(0.0f64, f64::max);
        println!(
            "  alpha={alpha:>5}: join_max={join_max:.3} b/sym, recovery={recovery}, steady_after={after:.3}"
        );
    }
    println!(
        "BZ16-I5 MEASURED: change-point recovery of the recovered fixed-share \
         engine over two distribution joints (Q12 primary, Q8/Q16 controls) \
         + alpha sensitivity."
    );
}
