// Layer B: quantized integer basis (the only thing a decoder ever sees).
// B: D x k fixed-point i16, scale s: predictor computes (Bq) >> s.
// mu: integer mean patch (i32 accumulator-safe).

#[derive(Debug, Clone)]
pub struct QuantBasis {
    pub d: usize,
    pub k: usize,
    pub scale: u32,
    pub mu: Vec<i32>,     // d values (pre-clamp offsets around 128)
    pub b: Vec<Vec<i16>>, // k directions, each d entries
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantError {
    /// Basis entry out of i16 fixed-point range at this scale.
    EntryOutOfRange { direction: usize, value: f64 },
    /// Rank requested exceeds discoverable subspace.
    RankTooHigh { requested: usize, discovered: usize },
}

/// Quantize fp directions (normalized) + mean into i16 fixed point.
/// Direction entries in [-1,1] => q14 scale (s=14) keeps |v*16384| <= 16384.
pub fn quantize(
    dirs: &[Vec<f64>],
    mean: &[f64],
    sv: &[f64],
    k: usize,
    scale: u32,
) -> Result<QuantBasis, QuantError> {
    let d = mean.len();
    if k > dirs.len() {
        return Err(QuantError::RankTooHigh {
            requested: k,
            discovered: dirs.len(),
        });
    }
    let s = (1i64 << scale) as f64;
    let mut b = Vec::with_capacity(k);
    for (dir_idx, dir) in dirs.iter().enumerate().take(k) {
        if dir.len() != d {
            break; // discovery did not fill this component: usable rank < k
        }
        if sv.get(dir_idx).copied().unwrap_or(0.0) < 1e-9 {
            break; // exhausted spectrum: fewer usable directions than k
        }
        let mut row = Vec::with_capacity(d);
        for v in dir {
            let q = v * s;
            if !q.is_finite() || q.abs() > 32767.0 {
                return Err(QuantError::EntryOutOfRange {
                    direction: dir_idx,
                    value: q,
                });
            }
            // deterministic rounding: half away from zero
            row.push(if q >= 0.0 {
                (q + 0.5).floor() as i16
            } else {
                (q - 0.5).ceil() as i16
            });
        }
        b.push(row);
    }
    // mu: fp mean biased by -128 for fixed-point offset arithmetic
    let mu = mean
        .iter()
        .map(|m| {
            let q = m * s;
            if q >= 0.0 {
                (q + 0.5).floor() as i32
            } else {
                (q - 0.5).ceil() as i32
            }
        })
        .collect();
    Ok(QuantBasis {
        d,
        k: b.len(),
        scale,
        mu,
        b,
    })
}
