// Deterministic integer synthesis (the decoder's whole world).
// xhat_c = mu_c + (sum_j B_jc * q_j) >> scale  (rounding: +half, floor)
// per canonical byte: pixel = clamp_c where mu stored as fp-mean*scale.

#[derive(Debug, Clone)]
pub enum CoordError {
    /// Coordinate exceeds dynamic range of the widest accumulator (i64).
    Overflow,
    /// Coordinates for k > basis.k
    TooManyCoords { have: usize, want: usize },
}

pub struct Predictor<'a> {
    pub basis: &'a crate::basis::QuantBasis,
}

impl<'a> Predictor<'a> {
    /// Coordinates q: i32 per component; returns synthesized patch (u8, d).
    /// All arithmetic checked/wrapping-safe in i64 accumulators.
    pub fn synthesize(&self, q: &[i32]) -> Result<Vec<u8>, CoordError> {
        let b = self.basis;
        if q.len() > b.k {
            return Err(CoordError::TooManyCoords {
                have: q.len(),
                want: b.k,
            });
        }
        if b.d > 1 << 22 {
            return Err(CoordError::Overflow); // paranoia at absurd d
        }
        let half = 1i64 << (b.scale - 1);
        let mut out = vec![0u8; b.d];
        for c in 0..b.d {
            let mut acc: i64 = b.mu[c] as i64; // mean in fixed point
            for (j, row) in b.b.iter().enumerate() {
                let qj = if j < q.len() { q[j] as i64 } else { 0 };
                if qj != 0 {
                    acc += qj * row[c] as i64; // i64 x i16 -> i64, checked by range
                }
            }
            // fixed point >> with round-half-up
            let scaled = (acc + half) >> b.scale;
            // canonical RGB byte: mu was fp-mean (0..255)*scale; clamp result
            let px = scaled.clamp(0, 255) as u8;
            out[c] = px;
        }
        Ok(out)
    }
}

/// Exact residual: r = (x - xhat) mod 256.
pub fn residual(x: &[u8], xhat: &[u8]) -> Vec<u8> {
    x.iter()
        .zip(xhat)
        .map(|(a, b)| a.wrapping_sub(*b))
        .collect()
}

/// Exact reconstruction: x = (xhat + r) mod 256 (decoder side).
pub fn reconstruct(xhat: &[u8], r: &[u8]) -> Vec<u8> {
    xhat.iter()
        .zip(r)
        .map(|(a, b)| a.wrapping_add(*b))
        .collect()
}
