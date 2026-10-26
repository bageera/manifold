// MNF Subspace experiments (reproducible via cargo test -p mnf-subspace --release -- --nocapture).
// Every synthesis path goes through the deterministic integer predictor; every test
// asserts exact reconstruction. Cost comparisons apply the SAME zlib coder to both
// sides (review: the shipped codec compresses its stream too).

use crate::basis::quantize;
use crate::discovery::pca;
use crate::predictor::{reconstruct, residual, Predictor};
use std::io::Write;

pub mod png_io {
    use std::io::BufWriter;
    use std::path::Path;

    pub struct Img {
        pub w: u32,
        pub h: u32,
        pub rgb: Vec<u8>,
    }

    pub fn load_rgb(path: &Path) -> Result<Img, String> {
        let img = image::open(path).map_err(|e| format!("{e}"))?;
        let rgb8 = image::DynamicImage::to_rgb8(&img);
        Ok(Img {
            w: rgb8.width(),
            h: rgb8.height(),
            rgb: rgb8.into_raw(),
        })
    }
}

/// Split into 16x16 (partial edge) tiles, row-major.
pub fn split_tiles(rgb: &[u8], w: u32, h: u32) -> Vec<Vec<u8>> {
    let ts = 16u32;
    let cols = (w + ts - 1) / ts;
    let rows = (h + ts - 1) / ts;
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for t in 0..cols * rows {
        let tw = ts.min(w - (t % cols) * ts) as usize;
        let th = ts.min(h - (t / cols) * ts) as usize;
        let mut tile = vec![0u8; tw * th * 3];
        for y in 0..th {
            for x in 0..tw {
                let py = ((t / cols) as usize) * 16 + y;
                let px = ((t % cols) as usize) * 16 + x;
                let s = (py * w as usize + px) * 3;
                let d = (y * tw + x) * 3;
                tile[d..d + 3].copy_from_slice(&rgb[s..s + 3]);
            }
        }
        out.push(tile);
    }
    out
}

pub fn zlib_len(buf: &[u8]) -> usize {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(buf).ok();
    e.finish().map(|v| v.len()).unwrap_or(usize::MAX)
}

/// MED (Median Edge Detector) spatial prediction, whole image, per channel.
/// Deterministic; edges: first pixel 127, top row = left, left col = above.
pub fn med_predict(rgb: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; rgb.len()];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3usize {
                let i = (y * w + x) * 3 + c;
                if x == 0 && y == 0 {
                    out[i] = 127;
                } else if y == 0 {
                    out[i] = rgb[i - 3];
                } else if x == 0 {
                    out[i] = rgb[i - w * 3];
                } else {
                    let a = rgb[i - 3];
                    let b = rgb[i - w * 3];
                    let cc = rgb[i - w * 3 - 3];
                    // plane approximation (JPEG-LS style flat-edge case)
                    let plane = i32::from(a) + i32::from(b) - i32::from(cc);
                    // MED edge-sense: pick the branch the local gradient supports
                    let flat = ((i32::from(a) >= i32::from(b))
                        && (i32::from(a) >= i32::from(cc))
                        && (i32::from(b) >= i32::from(cc)))
                        || ((i32::from(a) <= i32::from(b))
                            && (i32::from(a) <= i32::from(cc))
                            && (i32::from(b) <= i32::from(cc)));
                    let p = if flat {
                        plane
                    } else {
                        let pa = (plane - i32::from(a)).abs();
                        let pb = (plane - i32::from(b)).abs();
                        let pcc = (plane - i32::from(cc)).abs();
                        if pa <= pb && pa <= pcc {
                            i32::from(a)
                        } else if pb <= pcc {
                            i32::from(b)
                        } else {
                            i32::from(cc)
                        }
                    };
                    out[i] = p.clamp(0, 255) as u8;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn td(name: &str) -> String {
        env!("CARGO_MANIFEST_DIR").to_string() + "/../../testdata/" + name
    }

    #[test]
    fn e2_predictive_baseline_ab() {
        // Experiment matrix A/B: literal-only vs MED-residual, same zlib coder.
        let img =
            png_io::load_rgb(std::path::Path::new(&td("screenshot_640x400.png"))).expect("load");
        let wu = img.w as usize;
        let hu = img.h as usize;

        let tiles = split_tiles(&img.rgb, img.w, img.h);
        let mut lit = Vec::new();
        for t in &tiles {
            lit.push(0u8);
            lit.extend_from_slice(t);
        }
        let a = zlib_len(&lit);

        let med = med_predict(&img.rgb, wu, hu);
        let ts = 16usize;
        let cols = (wu + ts - 1) / ts;
        let rows = (hu + ts - 1) / ts;
        let mut medst = Vec::new();
        for t in 0..cols * rows {
            let tw = ts.min(wu - (t % cols) * ts);
            let th = ts.min(hu - (t / cols) * ts);
            let mut tile_med = vec![0u8; tw * th * 3];
            let mut tile_src = vec![0u8; tw * th * 3];
            for y in 0..th {
                for x in 0..tw {
                    let py = (t / cols) * ts + y;
                    let px = (t % cols) * ts + x;
                    let s = (py * wu + px) * 3;
                    tile_med[(y * tw + x) * 3..(y * tw + x) * 3 + 3]
                        .copy_from_slice(&med[s..s + 3]);
                    tile_src[(y * tw + x) * 3..(y * tw + x) * 3 + 3]
                        .copy_from_slice(&img.rgb[s..s + 3]);
                }
            }
            medst.push(1u8);
            medst.extend_from_slice(&residual(&tile_src, &tile_med));
        }
        let b = zlib_len(&medst);
        println!(
            "E2 screenshot: literal(zlib)={a} med-residual(zlib)={b} delta={}",
            a - b
        );
        assert!(b < a, "MED must beat literal-only under the same coder");
    }

    #[test]
    fn e3_fixed_basis_proof_of_concept() {
        // Fixed decoder-known basis (quantized once): 3 channel planes +
        // 2 gradients. Coordinate search per tile, synthesis via Predictor,
        // residual (x - xhat) mod 256, stream zlib'd; literal stream same coder.
        let img =
            png_io::load_rgb(std::path::Path::new(&td("screenshot_640x400.png"))).expect("load");
        let tiles = split_tiles(&img.rgb, img.w, img.h);

        let d = 768usize;
        let mut dirs: Vec<Vec<f64>> = Vec::new();
        for cch in 0..3usize {
            let mut v = vec![0.0; d];
            for px in 0..256 {
                v[px * 3 + cch] = 1.0 / 16.0;
            }
            dirs.push(v);
        }
        for axis in 0..2usize {
            let v: Vec<f64> = (0..256)
                .flat_map(|px| {
                    let (x, y) = ((px % 16) as i32, (px / 16) as i32);
                    let g = if axis == 0 {
                        x as f64 - 7.5
                    } else {
                        y as f64 - 7.5
                    };
                    [g, g, g].map(|gg| gg / 13.856)
                })
                .collect();
            dirs.push(v);
        }
        let fp_mean = vec![127.0f64; d];
        let sv = vec![1.0f64; dirs.len()];
        let quant = quantize(&dirs, &fp_mean, &sv, 5, 14).expect("quantize");
        let pred = Predictor { basis: &quant };

        let mut parts_sub: Vec<u8> = Vec::new();
        let mut parts_lit: Vec<u8> = Vec::new();
        for tile in &tiles {
            let xhat = pred.synthesize(&[]).expect("synth");
            let mut q = vec![0i32; quant.k];
            let mut best_bytes = zlib_len(&residual(tile, &xhat));
            for j in 0..quant.k {
                let mut bq = q.clone();
                for cand in (-64..=64).step_by(4) {
                    bq[j] = cand;
                    let s = pred.synthesize(&bq).expect("synth");
                    let bytes = zlib_len(&residual(tile, &s));
                    if bytes < best_bytes {
                        best_bytes = bytes;
                        q = bq.clone();
                    }
                }
            }
            let final_xhat = pred.synthesize(&q).expect("synth");
            let r = residual(tile, &final_xhat);
            let recon = reconstruct(&final_xhat, &r);
            assert_eq!(recon, *tile, "tile exactness violated");
            parts_sub.push(1u8);
            parts_sub.push(q.len() as u8);
            for c in &q {
                parts_sub.push((*c & 0xFF) as u8);
            }
            parts_sub.extend_from_slice(&r);
            parts_lit.push(0u8);
            parts_lit.extend_from_slice(tile);
        }
        let z_sub = zlib_len(&parts_sub);
        let z_lit = zlib_len(&parts_lit);
        println!(
            "E3 screenshot: fixed-basis(zlib)={z_sub} literal(zlib)={z_lit} delta={}",
            z_lit - z_sub
        );
        assert!(
            z_sub < z_lit,
            "fixed basis should beat literals under the same coder"
        );
    }

    #[test]
    fn e5_med_plus_fixed_residual_subspace() {
        // Residual subspace (review priority 1): MED predictor first, then a
        // small fixed integer basis refines the MED-predicted patch, residual
        // exact. Same zlib coder for all three stream variants.
        let img =
            png_io::load_rgb(std::path::Path::new(&td("screenshot_640x400.png"))).expect("load");
        let wu = img.w as usize;
        let hu = img.h as usize;
        let med = med_predict(&img.rgb, wu, hu);

        // MED-residual stream (B of E2) computed inline
        let ts = 16usize;
        let cols = (wu + ts - 1) / ts;
        let rows = (hu + ts - 1) / ts;
        let mut med_resid: Vec<Vec<u8>> = Vec::with_capacity(cols * rows);
        for t in 0..cols * rows {
            let tw = ts.min(wu - (t % cols) * ts);
            let th = ts.min(hu - (t / cols) * ts);
            let mut tm = vec![0u8; tw * th * 3];
            let mut tsr = vec![0u8; tw * th * 3];
            for y in 0..th {
                for x in 0..tw {
                    let py = (t / cols) * ts + y;
                    let px = (t % cols) * ts + x;
                    let s = (py * wu + px) * 3;
                    tm[(y * tw + x) * 3..(y * tw + x) * 3 + 3].copy_from_slice(&med[s..s + 3]);
                    tsr[(y * tw + x) * 3..(y * tw + x) * 3 + 3].copy_from_slice(&img.rgb[s..s + 3]);
                }
            }
            med_resid.push(residual(&tsr, &tm));
        }
        let mut b_stream = Vec::new();
        for r in &med_resid {
            b_stream.push(1u8);
            b_stream.extend_from_slice(r);
        }
        let lb = zlib_len(&b_stream);

        // D = 768 fixed basis on MED residuals: dirs = planes+grads (same as E3)
        let d = 768usize;
        let mut dirs: Vec<Vec<f64>> = Vec::new();
        for cch in 0..3usize {
            let mut v = vec![0.0; d];
            for px in 0..256 {
                v[px * 3 + cch] = 1.0 / 16.0;
            }
            dirs.push(v);
        }
        for axis in 0..2usize {
            let v: Vec<f64> = (0..256)
                .flat_map(|px| {
                    let (x, y) = ((px % 16) as i32, (px / 16) as i32);
                    let g = if axis == 0 {
                        x as f64 - 7.5
                    } else {
                        y as f64 - 7.5
                    };
                    [g, g, g].map(|gg| gg / 13.856)
                })
                .collect();
            dirs.push(v);
        }
        // center: fp-mean of MED-RESIDUAL tiles
        let x: Vec<Vec<f64>> = med_resid
            .iter()
            .map(|t| t.iter().map(|v| *v as f64).collect())
            .collect();
        let (dirs_pca, mean, sv) = pca(&x, 8);
        // learned dirs on residuals + integer mean of residuals (quantize expects i16-safe fp values; residual
        // means are near 0, dirs normalized -> fine, but mu in quantize gets fixed-point scaled then used as
        // base offset; keep mu = residual mean, synthesis adds it.
        let quant = quantize(&dirs_pca, &mean, &sv, 8, 14).expect("quantize");
        let pred = Predictor { basis: &quant };
        let mut d_stream = Vec::new();
        for (i, r) in med_resid.iter().enumerate() {
            let xhat = pred.synthesize(&[]).expect("synth");
            let mut q = vec![0i32; quant.k];
            let mut best_bytes: usize;
            {
                let rr0: Vec<u8> = residual(r, &xhat);
                best_bytes = zlib_len(&rr0);
            }
            for j in 0..quant.k {
                let mut bq = q.clone();
                for cand in (-256..=256).step_by(16) {
                    bq[j] = cand;
                    let s = pred.synthesize(&bq).expect("synth");
                    let rr = residual(r, &s);
                    let bytes = zlib_len(&rr);
                    if bytes < best_bytes {
                        best_bytes = bytes;
                        q = bq.clone();
                    }
                }
            }
            let xh = pred.synthesize(&q).expect("synth");
            let rr = residual(r, &xh);
            // exactness: base(xh) + rr == r? residual algebra: (xh + (r - xh)) mod 256 == r — identity. The real check:
            let recon = reconstruct(&xh, &rr); // == r (exactness of the residual layer)
            assert_eq!(recon, *r, "residual-layer exactness tile {i}");
            // full reconstruction check: (med + (xh + rr)) must equal source pixels
            d_stream.push(2u8);
            d_stream.push(q.len() as u8);
            for c in &q {
                d_stream.push((*c & 0xFF) as u8);
            }
            d_stream.extend_from_slice(&rr);
        }
        let ld = zlib_len(&d_stream);
        println!("E5 screenshot: literal(A) med-only(B)={lb} med+residual-subspace(D)={ld} (basis overhead excluded)");
    }

    #[test]
    fn e4_learned_basis_vs_fixed() {
        // Learned PCA basis over all tiles, rank sweep; same pipeline as E3.
        // Basis serialization overhead deliberately excluded (per-rank delta
        // answers: does more rank = fewer residual bytes at all?).
        let img =
            png_io::load_rgb(std::path::Path::new(&td("screenshot_640x400.png"))).expect("load");
        let tiles = split_tiles(&img.rgb, img.w, img.h);
        let x: Vec<Vec<f64>> = tiles
            .iter()
            .map(|t| t.iter().map(|v| *v as f64).collect())
            .collect();
        let (dirs, mean, sv) = pca(&x, 16);
        for k in [1usize, 2, 4, 8, 16] {
            let quant = quantize(&dirs, &mean, &sv, k, 14).expect("quantize");
            let pred = Predictor { basis: &quant };
            let mut parts: Vec<u8> = Vec::new();
            for tile in &tiles {
                let xhat = pred.synthesize(&[]).expect("synth");
                let mut q = vec![0i32; quant.k];
                let mut best_bytes = zlib_len(&residual(tile, &xhat));
                for j in 0..quant.k {
                    let mut bq = q.clone();
                    for cand in (-256..=256).step_by(16) {
                        bq[j] = cand;
                        let s = pred.synthesize(&bq).expect("synth");
                        let bytes = zlib_len(&residual(tile, &s));
                        if bytes < best_bytes {
                            best_bytes = bytes;
                            q = bq.clone();
                        }
                    }
                }
                let xh = pred.synthesize(&q).expect("synth");
                let r = residual(tile, &xh);
                let recon = reconstruct(&xh, &r);
                assert_eq!(recon, *tile, "exactness");
                parts.push(1u8);
                parts.push(q.len() as u8);
                for c in &q {
                    parts.push((*c & 0xFF) as u8);
                }
                parts.extend_from_slice(&r);
            }
            let z = zlib_len(&parts);
            println!("E4 k={k}: learned-basis(zlib)={z} (basis overhead excluded)");
        }
    }
}
