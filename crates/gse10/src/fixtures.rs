//! Deterministic fixture grids (GSE-09 reproduction family, Rust side).
//!
//! RNG = CPython-compatible MT19937 (the `mt19937` crate) + a faithful
//! hand-roll of CPython's getrandbits/_randbelow/randint so the SAME
//! seed produces byte-identical fixture grids in the Rust gate and the
//! committed Python reproduction (docs/gse09/prototype_reproduction.py).
//! Cross-language: the storage matrix can run in either harness.

/// CPython `random.Random(seed)`-compatible stream (int seeds < 2^32).
pub struct PythonRng {
    inner: mt19937::MT19937,
}

impl PythonRng {
    pub fn new(seed: u64) -> PythonRng {
        // CPython random_seed: init_by_array with the int's 32-bit words,
        // little-endian; for seeds < u32::MAX the key is just [seed].
        assert!(seed <= u32::MAX as u64, "python-rng: u32 seeds only");
        PythonRng {
            inner: mt19937::MT19937::new_with_slice_seed(&[seed as u32]),
        }
    }

    fn next_u32(&mut self) -> u32 {
        use rand_core::TryRng;
        self.inner.try_next_u32().expect("mt19937 infallible")
    }

    /// CPython Random.getrandbits(k): words = ceil(k/32), word i = bits
    /// [32i, 32i+31), last word keeps the HIGH (k mod 32) bits. Errors,
    /// never panics; k >= 1 required.
    pub fn getrandbits(&mut self, k: u32) -> Result<u64, &'static str> {
        if k == 0 || k > 64 {
            return Err("getrandbits: k in 1..=64 supported");
        }
        let words = k.div_ceil(32) as usize;
        let mut acc: u64 = 0;
        let mut krest = k;
        for i in 0..words {
            let mut r = self.next_u32();
            if krest < 32 {
                r >>= 32 - krest; // drop the LEAST significant bits
            }
            acc |= (r as u64) << (32 * i);
            krest = krest.saturating_sub(32);
        }
        Ok(acc)
    }

    /// CPython Random._randbelow_with_getrandbits(n) — randrange body.
    /// Errors on n == 0 (python raises ValueError).
    pub fn randbelow(&mut self, n: usize) -> Result<usize, &'static str> {
        if n == 0 {
            return Err("randbelow: n > 0 required");
        }
        let k = usize::BITS - n.leading_zeros(); // n.bit_length()
        loop {
            let r = self.getrandbits(k)?;
            if (r as usize) < n {
                return Ok(r as usize);
            }
        }
    }

    /// randrange(n) — single-arg form.
    #[allow(dead_code)] // exercised by gates; kept as the python-mirror API
    pub fn randrange(&mut self, n: usize) -> Result<usize, &'static str> {
        self.randbelow(n)
    }

    /// randint(a, b) = a + randrange(b - a + 1).
    pub fn randint(&mut self, a: i32, b: i32) -> Result<i32, &'static str> {
        if b < a {
            return Err("randint: b >= a required");
        }
        Ok(a + self.randbelow((b - a + 1) as usize)? as i32)
    }

    /// randrange(256) — CPython path: _randbelow(256) = getrandbits(9)
    /// with rejection (k = 9 because 256.bit_length() == 9), NOT a bare
    /// 8-bit draw. Call order must match the Python harness.
    pub fn byte(&mut self) -> u8 {
        self.randbelow(256).expect("n=256") as u8
    }
}

/// Fixture family (mirrors docs/gse09/prototype_reproduction.py shapes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Constant,
    Mathematical,
    SparseEdits,
    Mixed,
    SmoothNoisy,
    Random,
    /// planar base + `edits` exceptions inside the hotspot rect
    /// (56..64, 56..64) — the dense-exception query adversary.
    DenseHotspot(u32),
}

pub const KIND_NAMES: [&str; 7] = [
    "constant",
    "mathematical",
    "sparse_edits",
    "mixed",
    "smooth_noisy",
    "random",
    "dense_hotspot",
];

/// Row-major side×side byte grid.
pub fn make(kind: Kind, side: usize, seed: u64) -> Vec<u8> {
    let mut g = vec![0u8; side * side];
    let mut rng = PythonRng::new(seed);
    let put = |g: &mut Vec<u8>, y: usize, x: usize, v: i32| {
        g[y * side + x] = (v & 0xFF) as u8;
    };
    match kind {
        Kind::Constant => {
            for y in 0..side {
                for x in 0..side {
                    put(&mut g, y, x, 77);
                }
            }
        }
        Kind::Mathematical | Kind::SparseEdits | Kind::DenseHotspot(_) => {
            // planar base (3, 7, 11) then per-kind edits
            for y in 0..side {
                for x in 0..side {
                    put(&mut g, y, x, 3 + 7 * y as i32 + 11 * x as i32);
                }
            }
            if kind == Kind::SparseEdits {
                for _ in 0..80 {
                    let y = rng.randbelow(side).expect("in-range");
                    let x = rng.randbelow(side).expect("in-range");
                    put(&mut g, y, x, rng.byte() as i32);
                }
            }
            if let Kind::DenseHotspot(n) = kind {
                // hotspot aligned 8×8 at (56..64)²
                for _ in 0..n {
                    let y = 56 + rng.randbelow(8).expect("in-range");
                    let x = 56 + rng.randbelow(8).expect("in-range");
                    put(&mut g, y, x, rng.byte() as i32);
                }
            }
        }
        Kind::Mixed => {
            let tile = side / 16;
            for ty in 0..16 {
                for tx in 0..16 {
                    if (tx + ty) % 2 == 0 {
                        let a = rng.byte() as i32;
                        let b = rng.byte() as i32;
                        let c = rng.byte() as i32;
                        for yy in ty * tile..(ty + 1) * tile {
                            for xx in tx * tile..(tx + 1) * tile {
                                put(&mut g, yy, xx, a + b * yy as i32 + c * xx as i32);
                            }
                        }
                    } else {
                        for yy in ty * tile..(ty + 1) * tile {
                            for xx in tx * tile..(tx + 1) * tile {
                                put(&mut g, yy, xx, rng.byte() as i32);
                            }
                        }
                    }
                }
            }
        }
        Kind::SmoothNoisy => {
            for y in 0..side {
                for x in 0..side {
                    let n = rng.randint(-4, 4).expect("in-range");
                    put(&mut g, y, x, 40 + 5 * y as i32 + 9 * x as i32 + n);
                }
            }
        }
        Kind::Random => {
            for y in 0..side {
                for x in 0..side {
                    g[y * side + x] = rng.byte();
                }
            }
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = make(Kind::Mixed, 128, 5);
        let b = make(Kind::Mixed, 128, 5);
        assert_eq!(a, b);
    }

    #[test]
    fn mathematical_is_exact_planar() {
        let g = make(Kind::Mathematical, 32, 7);
        for y in 0..32 {
            for x in 0..32 {
                assert_eq!(g[y * 32 + x], ((3 + 7 * y + 11 * x) & 0xFF) as u8);
            }
        }
    }

    #[test]
    fn hotspot_has_edits() {
        let g = make(Kind::DenseHotspot(64), 128, 3);
        let mut hot = 0usize;
        for y in 56..64 {
            for x in 56..64 {
                let base = (3 + 7 * y + 11 * x) & 0xFF;
                if g[y * 128 + x] != base as u8 {
                    hot += 1;
                }
            }
        }
        assert!(hot > 0, "hotspot edits landed");
    }

    /// Cross-language reproducibility: first randbelow(128) values from
    /// CPython random.Random(77) (verified interactively): [64,83,50,61,49].
    #[test]
    fn python_compat_randbelow_stream() {
        let mut l = PythonRng::new(77);
        for want in [64usize, 83, 50, 61, 49] {
            assert_eq!(l.randbelow(128).expect("in-range"), want);
        }
    }

    /// random.Random(8) first 8 randrange(256) bytes (Python-computed):
    /// [116, 189, 192, 64, 98, 22, 43, 70].
    #[test]
    fn python_compat_fixture_stream() {
        let mut rr = PythonRng::new(8);
        let first8: Vec<u8> = (0..8).map(|_| rr.byte()).collect();
        assert_eq!(
            first8,
            vec![116u8, 189, 192, 64, 98, 22, 43, 70],
            "cpython stream mismatch"
        );
    }
}
