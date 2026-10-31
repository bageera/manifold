//! MNF14-IF4 — per-tile mathematical law arms (MNF8 record modes 6/7).
//!
//! Compression-aware law DISCOVERY at the tile level: every 16×16 tile
//! competes {literal (0/3), atlas refs (1/2/4/5), LAW-PLANAR (6),
//! LAW-BILINEAR (7)} at complete-wire cost — mode byte + the tile's own
//! u8 law params + its residual over the law. Production backend
//! competition untouched.
//!
//! Wire per law record: [1 B mode][law params][n B residual], where the
//! law params are u8/i8-quantized and the law is decoder-reproducible
//! from the shipped bytes alone (no side tables, no rerun of discovery).
//! Residual = (X − pred) mod 256 — lossless regardless of law quality.
//!
//! Law semantics (per channel, integer-exact):
//!   planar (6):   pred(x,y) = a_c + ((b_c·x + c_c·y + t_c) >> 5)
//!   bilinear (7): pred(x,y) = 2-D lerp of 4 corner values (u8)
//! with x,y IN-TILE. `b_c`/`c_c` transport at 1/32-pixel granularity
//! (i8 = ±3.97 rates), `t_c` = 5-bit rounding bias.

/// One fitted per-channel planar law (u8/i8 wire form).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanarLaw {
    /// [a, b, c] per channel; b/c at 1/32-pixel granularity (i8).
    pub coef: [[i8; 3]; 3],
    /// 5-bit rounding bias per channel (<<3 into the u8 byte).
    pub t: [u8; 3],
}

/// One fitted per-channel bilinear law (the tile's 4 corner values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BilinearLaw {
    /// [v00, v10, v01, v11] per channel (u8 direct).
    pub corners: [[u8; 4]; 3],
}

/// Law params' wire byte counts.
pub const PLANAR_LAW_BYTES: usize = 9 + 3;
/// Bilinear law params' wire byte count.
pub const BILINEAR_LAW_BYTES: usize = 12;

/// Planar prediction at in-tile (x, y), channel c — integer exact.
///
/// Wire layout: coef i8 = [a, b, c] with b/c at 1/64-pixel granularity
/// (transported value = rate·32), t's byte = the 5-bit bias <<3.
/// pred = a + (b·x + c·y + t) >> 6 (b in 1/64 units).
#[inline]
pub fn planar_pred(law: &PlanarLaw, x: u32, y: u32, c: usize) -> i64 {
    let k = law.coef[c];
    let t = (law.t[c] >> 3) as i64;
    i64::from(k[0]) + ((i64::from(k[1]) * i64::from(x) + i64::from(k[2]) * i64::from(y) + t) >> 5)
}

/// Bilinear prediction at in-tile (x, y) on a tw×th lattice.
#[inline]
pub fn bilinear_pred(law: &BilinearLaw, x: u32, y: u32, tw: u32, th: u32, c: usize) -> i64 {
    let c4 = law.corners[c];
    if tw <= 1 || th <= 1 {
        return i64::from(c4[0]);
    }
    let fx = (i64::from(x) << 8) / i64::from(tw - 1); // 0..=256
    let fy = (i64::from(y) << 8) / i64::from(th - 1);
    let top = i64::from(c4[0]) + (((i64::from(c4[1]) - i64::from(c4[0])) * fx) >> 8);
    let bot = i64::from(c4[2]) + (((i64::from(c4[3]) - i64::from(c4[2])) * fx) >> 8);
    top + (((bot - top) * fy) >> 8)
}

/// Serialize a planar law to its wire bytes (9 coef + 3 bias).
pub fn planar_wire(law: &PlanarLaw) -> Vec<u8> {
    let mut out = Vec::with_capacity(PLANAR_LAW_BYTES);
    for ch in law.coef {
        for v in ch {
            out.push(v.to_le_bytes()[0]);
        }
    }
    for b in law.t {
        out.push(b);
    }
    out
}

/// Parse planar law bytes back. Errors on short input (Err never panic).
pub fn planar_unwire(buf: &[u8]) -> Result<PlanarLaw, crate::error::Error> {
    if buf.len() < PLANAR_LAW_BYTES {
        return Err(crate::error::Error::fmt_msg("planar law truncated"));
    }
    let mut coef = [[0i8; 3]; 3];
    let mut p = 0usize;
    for ch in coef.iter_mut() {
        for v in ch.iter_mut() {
            *v = buf[p] as i8;
            p += 1;
        }
    }
    let mut t = [0u8; 3];
    for v in t.iter_mut() {
        *v = buf[p] & 0b11111000; // 5-bit field, <<3 alignment
        p += 1;
    }
    Ok(PlanarLaw { coef, t })
}

/// Serialize a bilinear law (12 corner bytes).
pub fn bilinear_wire(law: &BilinearLaw) -> Vec<u8> {
    let mut out = Vec::with_capacity(BILINEAR_LAW_BYTES);
    for ch in law.corners {
        for v in ch {
            out.push(v);
        }
    }
    out
}

/// Parse bilinear law bytes. Errors on short input.
pub fn bilinear_unwire(buf: &[u8]) -> Result<BilinearLaw, crate::error::Error> {
    if buf.len() < BILINEAR_LAW_BYTES {
        return Err(crate::error::Error::fmt_msg("bilinear law truncated"));
    }
    let mut corners = [[0u8; 4]; 3];
    let mut p = 0usize;
    for ch in corners.iter_mut() {
        for v in ch.iter_mut() {
            *v = buf[p];
            p += 1;
        }
    }
    Ok(BilinearLaw { corners })
}

/// Discovery seeds (fitted to the TILE's real pixels — never re-derived
/// from a raster model): plane fit and corner seed.
pub fn discovery_seeds(tile: &[u8], tw: u32, th: u32) -> (PlanarLaw, BilinearLaw) {
    (
        planar_seed_tile(tile, tw, th),
        bilinear_seed_tile(tile, tw, th),
    )
}

/// Plane fit onto the wire lattice directly: a = channel min, b/c from
/// the end-point deltas at 1/64 granularity, t = 0 (deterministic seed;
/// the encoder's refinement sweeps dt/db/dc AROUND this).
fn planar_seed_tile(tile: &[u8], tw: u32, th: u32) -> PlanarLaw {
    let mut coef = [[0i8; 3]; 3];
    let mut t = [0u8; 3];
    for c in 0..3usize {
        let at = |x: i32, y: i32| i64::from(tile[((y as u32) * tw + x as u32) as usize * 3 + c]);
        let a = at(0, 0);
        // end-point rates: (p(x=tw-1) - a) / (tw-1) at 1/64 granularity
        let dx = if tw > 1 {
            (at(tw as i32 - 1, 0) - a) * 32 / i64::from(tw - 1)
        } else {
            0
        };
        let dy = if th > 1 {
            (at(0, th as i32 - 1) - a) * 32 / i64::from(th - 1)
        } else {
            0
        };
        coef[c] = [
            i64::clamp(a, -128, 127) as i8, // a clamps into i8 range
            i64::clamp(dx, -128, 127) as i8,
            i64::clamp(dy, -128, 127) as i8,
        ];
        t[c] = 0;
    }
    PlanarLaw { coef, t }
}

/// Bilinear seed: the tile's own 4 corner pixels per channel.
fn bilinear_seed_tile(tile: &[u8], tw: u32, th: u32) -> BilinearLaw {
    let idx = |x: u32, y: u32| ((y * tw + x) * 3) as usize;
    let mut corners = [[0u8; 4]; 3];
    for c in 0..3usize {
        corners[c] = [
            tile[idx(0, 0) + c],
            tile[idx(tw - 1, 0) + c],
            tile[idx(0, th - 1) + c],
            tile[idx(tw - 1, th - 1) + c],
        ];
    }
    BilinearLaw { corners }
}

/// The residual plane of a tile under a law (mod-256, exact).
pub fn law_residuals(
    tile: &[u8],
    tw: u32,
    th: u32,
    law_pred: &dyn Fn(u32, u32, usize) -> i64,
) -> Vec<u8> {
    let n = (tw * th * 3) as usize;
    let mut r = vec![0u8; n];
    for y in 0..th {
        for x in 0..tw {
            for c in 0..3u32 {
                let k = ((y * tw + x) * 3 + c) as usize;
                r[k] = (i64::from(tile[k]) - law_pred(x, y, c as usize)).rem_euclid(256) as u8;
            }
        }
    }
    r
}

/// The recon of a tile from its law + residual (exact by induction).
pub fn law_recon(
    tw: u32,
    th: u32,
    law_pred: &dyn Fn(u32, u32, usize) -> i64,
    resid: &[u8],
) -> Vec<u8> {
    let n = (tw * th * 3) as usize;
    let mut out = vec![0u8; n];
    for y in 0..th {
        for x in 0..tw {
            for c in 0..3u32 {
                let k = ((y * tw + x) * 3 + c) as usize;
                out[k] = (law_pred(x, y, c as usize) + i64::from(resid[k])).rem_euclid(256) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_roundtrip_planar() {
        let law = PlanarLaw {
            coef: [[100, 3, -2], [90, 1, 1], [120, -1, 2]],
            t: [0, 8, 16],
        };
        let buf = planar_wire(&law);
        assert_eq!(buf.len(), PLANAR_LAW_BYTES);
        assert_eq!(planar_unwire(&buf).unwrap(), law);
    }

    #[test]
    fn wire_roundtrip_bilinear() {
        let law = BilinearLaw {
            corners: [[10, 20, 30, 40], [5, 6, 7, 8], [100, 99, 98, 97]],
        };
        let buf = bilinear_wire(&law);
        assert_eq!(buf.len(), BILINEAR_LAW_BYTES);
        assert_eq!(bilinear_unwire(&buf).unwrap(), law);
    }

    #[test]
    fn law_recon_roundtrip_exact() {
        let law = PlanarLaw {
            coef: [[80, 2, 1], [80, 2, 1], [80, 2, 1]],
            t: [0, 0, 0],
        };
        let tile: Vec<u8> = (0..16 * 16 * 3).map(|i| ((i * 13) % 251) as u8).collect();
        let resid = law_residuals(&tile, 16, 16, &|x, y, c| planar_pred(&law, x, y, c));
        assert_eq!(
            law_recon(16, 16, &|x, y, c| planar_pred(&law, x, y, c), &resid),
            tile
        );
    }

    #[test]
    fn planar_seed_fits_an_actual_plane() {
        // a real plane tile (80 + 2x + y): the seed's own predict must
        // reproduce it EXACTLY in-tile (rates land on the lattice).
        let mut tile = vec![0u8; 16 * 16 * 3];
        for y in 0..16u32 {
            for x in 0..16u32 {
                let v = (80u32 + 2 * x + y).clamp(0, 255);
                for c in 0..3u32 {
                    let k = ((y * 16 + x) * 3 + c) as usize;
                    tile[k] = v as u8;
                }
            }
        }
        let seed = planar_seed_tile(&tile, 16, 16);
        for y in 0..16u32 {
            for x in 0..16u32 {
                for c in 0..3usize {
                    let p = planar_pred(&seed, x, y, c);
                    assert_eq!(
                        p,
                        80i64 + (((2 * 32) * i64::from(x) + 32 * i64::from(y)) >> 5),
                        "({x},{y},{c})"
                    );
                }
            }
        }
    }
}
