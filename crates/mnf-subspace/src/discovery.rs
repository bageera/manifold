// Layer A: continuous subspace discovery (encoder-side, fp allowed).
// Power-iteration SVD is enough for rank-k<=32 on 768-dim patches and
// keeps the crate dependency-free.

/// X: N x D row-major training matrix (patches or residuals), centered internally.
/// Returns (k principal directions D x k, fp means D, singular values k),
/// directions normalized, sign-canonical (largest-abs component positive).
pub fn pca(x: &[Vec<f64>], k: usize) -> (Vec<Vec<f64>>, Vec<f64>, Vec<f64>) {
    let n = x.len();
    assert!(n > 0, "no training data");
    let d = x[0].len();
    let mut mean = vec![0.0; d];
    for row in x {
        for (m, v) in mean.iter_mut().zip(row) {
            *m += v / n as f64;
        }
    }
    // comp-major output: k rows of d (comp = row)
    let mut c: Vec<Vec<f64>> = (0..k).map(|_| vec![0.0; d]).collect();
    // power iteration with deflation (Gram-Schmidt orthogonalization)
    let mut sub = x
        .iter()
        .map(|row| {
            row.iter()
                .zip(&mean)
                .map(|(v, m)| v - m)
                .collect::<Vec<f64>>()
        })
        .collect::<Vec<_>>();
    let mut sv = vec![0.0; k];
    for comp in 0..k {
        // start vector: row sums of covariance (deterministic)
        let mut v = vec![0.0; d];
        for row in &sub {
            for (vv, r) in v.iter_mut().zip(row) {
                *vv += r;
            }
        }
        let norm: f64 = v.iter().map(|a| a * a).sum::<f64>().sqrt();
        if norm < 1e-12 {
            break;
        }
        for vv in &mut v {
            *vv /= norm;
        }
        for _it in 0..64 {
            // w = C^T v  (d x n times n x d => accumulate)
            let mut w = vec![0.0; d];
            for row in &sub {
                let dot: f64 = row.iter().zip(&v).map(|(a, b)| a * b).sum();
                for (ww, r) in w.iter_mut().zip(row) {
                    *ww += dot * r;
                }
            }
            // orthogonalize against previous components
            for prev in c.iter().take(comp) {
                let dot: f64 = w.iter().zip(prev).map(|(a, b)| a * b).sum();
                for (ww, p) in w.iter_mut().zip(prev) {
                    *ww -= dot * p;
                }
            }
            let n2: f64 = w.iter().map(|a| a * a).sum::<f64>().sqrt();
            if n2 < 1e-12 {
                break;
            }
            let delta: f64 = w
                .iter()
                .zip(&v)
                .map(|(a, b)| (a / n2 - b) * (a / n2 - b))
                .sum::<f64>()
                .sqrt();
            for (vv, ww) in v.iter_mut().zip(&w) {
                *vv = ww / n2;
            }
            if delta < 1e-10 {
                break;
            }
        }
        // canonical sign
        if let Some(p) = v
            .iter()
            .map(|a| a.abs())
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
        {
            if v[p] < 0.0 {
                for vv in &mut v {
                    *vv = -*vv;
                }
            }
        }
        sv[comp] = {
            let mut dot = 0.0f64;
            for row in &sub {
                let d_: f64 = row.iter().zip(&v).map(|(a, b)| a * b).sum();
                dot += d_ * d_;
            }
            dot.sqrt()
        };
        if comp >= k {
            break;
        }
        for (cc, vv) in c[comp].iter_mut().zip(&v) {
            *cc = *vv;
        }
        // deflate sub
        for row in &mut sub {
            let proj: f64 = row.iter().zip(&v).map(|(a, b)| a * b).sum();
            for (rr, vv) in row.iter_mut().zip(&v) {
                *rr -= proj * vv;
            }
        }
    }
    (c, mean, sv)
}
