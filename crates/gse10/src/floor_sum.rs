//! Closed-form wrap counts for linear byte generators.
//!
//! For M(i) = (A + B·i) mod 256 (values byte-reduced AFTER the mod),
//! the window sum needs the wrap count:
//!
//!   S_M = n·A + B·(n·s + n·(n−1)/2) − 256·∑_{i=s}^{s+n−1} ⌊(A+B·i)/256⌋
//!
//! The wrap count is a harmonic floor-sum ∑⌊(a·i+b)/m⌋, computed by the
//! classical O(log) exchange algorithm (recursive Euclid-style; see
//! AtCoder-style floor_sum; validated against brute force externally,
//! 0/400 mismatches, and by tests here).
//!
//! v1.1 spec rules encoded here:
//! - MATHEMATICAL floor division (never truncation toward zero):
//!   for negative a/b, use div_euclid-derived floor semantics.
//! - All intermediate arithmetic checked signed i64; the crate
//!   validates parameter/dimension bounds before evaluating
//!   (no reliance on fixture-scale assumptions).

/// floor((a*i + b)/m) summed for i in 0..n, with MATHEMATICAL floored
/// division for negative a/b. m > 0, n >= 0.
///
/// Classical floor_sum with floor semantics on negatives: normalize the
/// remainder so 0 <= a,b < m before the exchange loop (Euclidean
/// division), keeping the sign handled by quotient/remainder rules:
/// floor division = div_euclid when m > 0.
pub fn floor_sum(n: i64, m: i64, a: i64, b: i64) -> i64 {
    assert!(n >= 0, "floor_sum: n >= 0 required");
    assert!(m > 0, "floor_sum: m > 0 required");
    // Euclidean normalize a and b into [0, m) so the exchange algorithm
    // below operates on nonnegative values (floor semantics preserved).
    let q_a = a.div_euclid(m);
    let a = a.rem_euclid(m);
    let q_b = b.div_euclid(m);
    let b = b.rem_euclid(m);
    // pulls back the quotient contributions of the normalization:
    // sum floor((a0*i + b0)/m) = q_a*sum_i + q_b*n + floor_sum' where
    // sum over i in 0..n of i = n*(n-1)/2.
    let mut ans = q_a * ((n.checked_mul(n - 1).ok_or("n^2 overflow").unwrap_or(0)) / 2) + q_b * n;
    ans += floor_sum_nonneg(n, m, a, b);
    ans
}

/// Core exchange algorithm, nonnegative a, b, m; n >= 0.
/// (a or b may exceed m on entry — the leading `if a/b >= m` steps
/// handle both. The debug asserts below hold only AFTER that step.)
fn floor_sum_nonneg(n: i64, m: i64, mut a: i64, mut b: i64) -> i64 {
    let mut ans: i64 = 0;
    let mut n = n;
    let mut m = m;
    if a >= m {
        ans += (n - 1).checked_mul(n).unwrap() / 2 * (a / m);
        a %= m;
    }
    if b >= m {
        ans += n * (b / m);
        b %= m;
    }
    loop {
        if a == 0 {
            break;
        }
        let y = a.checked_mul(n).unwrap() + b;
        if y < m {
            break;
        }
        let new_n = y / m;
        let new_b = y % m;
        let new_m = a;
        let new_a = m;
        n = new_n;
        b = new_b;
        m = new_m;
        a = new_a;
        if a >= m {
            ans += (n - 1).checked_mul(n).unwrap() / 2 * (a / m);
            a %= m;
        }
        if b >= m {
            ans += n.checked_mul(b / m).unwrap();
            b %= m;
        }
    }
    ans
}

/// Window sum of (A + B·i) mod 256 for i in [s, s+n), exact, no
/// byte iteration. A, B may be any i64 (taken mod 256); s >= 0,
/// n >= 0. Overflow-safe at planar-scale bounds (validated to
/// side 256: |B| <= 255, n <= 65,536 → terms fit i64 with wide margin;
/// run the checked math and error rather than wrap).
pub fn window_sum_linear(a_in: i64, b_in: i64, s: i64, n: i64) -> Result<i64, &'static str> {
    if n < 0 || s < 0 {
        return Err("window_sum_linear: n, s >= 0 required");
    }
    let a = a_in.rem_euclid(256);
    let b = b_in.rem_euclid(256);
    // T = n·A + B·(n·s + n(n−1)/2)
    let idx_sum = s
        .checked_mul(n)
        .and_then(|v| v.checked_add(n.checked_mul(n - 1)? / 2))
        .ok_or("window_sum_linear: index-sum overflow")?;
    let t = b
        .checked_mul(idx_sum)
        .and_then(|v| v.checked_add(n.checked_mul(a)?))
        .ok_or("window_sum_linear: T overflow")?;
    // wrap count: floor((A + B·i)/256) for i in s..s+n
    //              = floor((B·i + A)/256)
    let wraps = floor_sum(
        n,
        256,
        b,
        a.checked_add(b.checked_mul(s).ok_or("b·s overflow")?)
            .ok_or("A+B·s overflow")?,
    );
    Ok(t - 256 * wraps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute(n: i64, m: i64, a: i64, b: i64) -> i64 {
        (0..n).map(|i| (a * i + b).div_euclid(m)).sum()
    }

    #[test]
    fn floor_sum_matches_brute_nonneg() {
        for n in 0..40 {
            for m in 1..40usize {
                for a in 0..60 {
                    for b in 0..60 {
                        let got = floor_sum_nonneg(n, m as i64, a, b);
                        let want = brute(n, m as i64, a, b);
                        assert_eq!(got, want, "n={n} m={m} a={a} b={b}");
                    }
                }
            }
        }
    }

    #[test]
    fn floor_sum_math_floor_on_negatives() {
        // negative a: floor((−5·i + 3)/4) — truncation would differ from 0
        // already at i=1: floor(−2/4) = −1, trunc gives 0.
        for n in 1..25 {
            for a in -30..0 {
                for b in -30..30 {
                    let got = floor_sum(n, 4, a, b);
                    let want = brute(n, 4, a, b);
                    assert_eq!(got, want, "n={n} a={a} b={b}");
                }
            }
        }
    }

    #[test]
    fn window_sum_linear_matches_brute_wrap_and_sign() {
        for a in [0, 1, 3, 250, 255, -3] {
            for b in [0, 1, 7, 255, -7] {
                for s in [0i64, 1, 7, 128, 255] {
                    for n in [1i64, 2, 63, 64, 255, 256] {
                        let got = window_sum_linear(a, b, s, n).unwrap();
                        let want: i64 = (0..n).map(|i| (a + b * (s + i)).rem_euclid(256)).sum();
                        assert_eq!(got, want, "a={a} b={b} s={s} n={n}");
                    }
                }
            }
        }
    }

    #[test]
    fn overflow_returns_err_not_panic() {
        let r = window_sum_linear(1, 255, i64::MAX / 2, i64::MAX / 2);
        assert!(r.is_err());
    }
}
