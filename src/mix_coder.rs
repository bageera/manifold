//! MNF13-R5 — the dual-timescale adaptive mix coder (backend id 4).
//!
//! Per-context FAST (halving cap 4,096) and SLOW (cap 65,536)
//! adaptive estimators over the full 256-symbol alphabet, mixed by a
//! log-space EWA weight updated per symbol from the coded symbol —
//! identical evolution on both sides (zero model transport; the
//! wire = payload bytes only). ConstricTON 0.5's stream range coder
//! carries the fixed-point probabilities (2^24 scale, no fp in the
//! wire data path). The exact-inverse contract is asserted by the
//! conformance suite (`tests/mnf8_conformance.rs`) and the corpus
//! gate.
//!
//! Wire (v1.13.0): MNF8 backend id 4. The payload the backend field
//! addresses = the mix-coded residual byte stream in the file's own
//! coded order; every context key recomputes from already-coded
//! bytes (the previous coded residual byte, bucketed to 8 keys), so
//! the decoder needs no side tables and rejects malformed inputs
//! with errors (never panics).
//!
//! Measured (docs/mnf13-r5.md): −1.80% complete corpus at the v1.12
//! pick level, wins 19/19 photographic record files, zero
//! regressions elsewhere (the pick keeps every prior arm).

use constriction::stream::{
    model::ContiguousCategoricalEntropyModel, queue::DefaultRangeDecoder,
    queue::DefaultRangeEncoder, Decode, Encode,
};

/// Fixed-point probability scale (2^24).
const SCALE: u64 = 1 << 24;
/// Fast estimator's halving cap.
pub const MIX_FAST_CAP: u64 = 4096;
/// Slow estimator's halving cap.
pub const MIX_SLOW_CAP: u64 = 65536;
/// EWA learning rate (log space).
const EWA_LR: f64 = 0.15;
/// Per-symbol count increment.
const COUNT_INC: u64 = 32;

/// The previous-coded-byte context: 8 magnitude buckets of the
/// previous coded byte (0; 1-2; 3-5; 6-11; 12-24; 25-48; 49-96; 97+).
#[inline]
pub fn mix_ctx_bucket(prev: u8) -> u8 {
    match prev {
        0 => 0,
        1..=2 => 1,
        3..=5 => 2,
        6..=11 => 3,
        12..=24 => 4,
        25..=48 => 5,
        49..=96 => 6,
        _ => 7,
    }
}

/// One context's dual estimator.
#[derive(Clone)]
struct MixCtx {
    fast: Vec<u64>,
    slow: Vec<u64>,
    log_w: f64,
}

impl MixCtx {
    fn new() -> Self {
        Self {
            fast: vec![1; 256],
            slow: vec![1; 256],
            log_w: 0.0,
        }
    }

    /// Fixed-point probability vector over the FULL alphabet; every
    /// symbol >= 1 (all must stay decodable); total exactly SCALE.
    fn probs(&self) -> Vec<u32> {
        let tf = self.fast.iter().sum::<u64>() as f64;
        let ts = self.slow.iter().sum::<u64>() as f64;
        let wf = 1.0 / (1.0 + (-self.log_w).exp());
        let mut p: Vec<u64> = self
            .fast
            .iter()
            .zip(&self.slow)
            .map(|(&cf, &cs)| {
                ((wf * (cf as f64 / tf) + (1.0 - wf) * (cs as f64 / ts)) * SCALE as f64) as u64
            })
            .collect();
        // rebalance: remainder to slot 0; every zero slot <= 1 stolen
        // from the densest entry (two passes, no double borrow)
        let sum: u64 = p.iter().sum();
        p[0] += SCALE - sum;
        let mut dense = 0usize;
        for (i, &v) in p.iter().enumerate() {
            if v > p[dense] {
                dense = i;
            }
        }
        let zeros = p.iter().filter(|&&v| v == 0).count();
        p[dense] = p[dense].max(1 + zeros as u64);
        for v in p.iter_mut() {
            if *v == 0 {
                *v = 1;
            }
        }
        p.into_iter().map(|v| v as u32).collect()
    }

    fn model(&self) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
        ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            self.probs(),
            false,
        )
        .ok()
    }

    fn step(&mut self, s: u8) {
        let si = usize::from(s);
        let tf = self.fast.iter().sum::<u64>() as f64;
        let ts = self.slow.iter().sum::<u64>() as f64;
        let pf = (self.fast[si] as f64 / tf).max(1e-9);
        let ps = (self.slow[si] as f64 / ts).max(1e-9);
        self.log_w = (self.log_w + EWA_LR * (pf.ln() - ps.ln())).clamp(-20.0, 20.0);
        self.fast[si] += COUNT_INC;
        self.slow[si] += COUNT_INC;
        if self.fast.iter().sum::<u64>() > MIX_FAST_CAP {
            for c in &mut self.fast {
                *c = (*c + 1) >> 1;
            }
        }
        if self.slow.iter().sum::<u64>() > MIX_SLOW_CAP {
            for c in &mut self.slow {
                *c = (*c + 1) >> 1;
            }
        }
    }
}

/// The mix coder: encode `data` (the residual bytes in coded order).
/// Zero transport; the model evolves from the bytes themselves.
pub fn mix_encode(data: &[u8]) -> Vec<u8> {
    let mut ctxs: Vec<MixCtx> = (0..8).map(|_| MixCtx::new()).collect();
    let mut enc = DefaultRangeEncoder::new();
    let mut prev = 0u8;
    for &b in data {
        let c = mix_ctx_bucket(prev);
        let m = ctxs[usize::from(c)]
            .model()
            .expect("mix model with all-nonzero probabilities is always buildable");
        enc.encode_symbols(std::iter::once((usize::from(b), &m)))
            .expect("mix encode symbol");
        ctxs[usize::from(c)].step(b);
        prev = b;
    }
    let compressed = enc.into_compressed().expect("mix seal");
    // The wire = [u32 stream_len][LE words]; the stream length is
    // charged transport (honest) — the range coder cannot recover it
    // from the frame alone.
    let mut out = Vec::with_capacity(compressed.len() * 4 + 4);
    out.extend_from_slice(
        &u32::try_from(data.len())
            .expect("stream len fits u32")
            .to_le_bytes(),
    );
    for w in compressed {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// The inverse: decode the wire back to its stream bytes. Errors,
/// never panics, on malformed input.
pub fn mix_decode(data: &[u8], limit: usize) -> Result<Vec<u8>, crate::error::Error> {
    // payload = [u32 stream_len][LE words...]
    if data.len() < 4 || !(data.len() - 4).is_multiple_of(4) {
        return Err(crate::error::Error::fmt_msg("mix payload not word-aligned"));
    }
    let stream_len = usize::try_from(u32::from_le_bytes(data[..4].try_into().expect("4 bytes")))
        .map_err(|_| crate::error::Error::fmt_msg("mix stream length"))?;
    if stream_len > limit {
        return Err(crate::error::Error::fmt_msg("bomb"));
    }
    let mut words = Vec::with_capacity((data.len() - 4) / 4);
    for chunk in data[4..].chunks_exact(4) {
        words.push(u32::from_le_bytes(chunk.try_into().expect("4 bytes")));
    }
    let mut dec = DefaultRangeDecoder::from_compressed(words)
        .map_err(|e| crate::error::Error::fmt_msg(format!("mix decoder open: {e:?}")))?;
    let mut ctxs: Vec<MixCtx> = (0..8).map(|_| MixCtx::new()).collect();
    let mut out = Vec::with_capacity(stream_len);
    let mut prev = 0u8;
    for _i in 0..stream_len {
        let c = usize::from(mix_ctx_bucket(prev));
        let m = ctxs[c]
            .model()
            .ok_or_else(|| crate::error::Error::fmt_msg("mix model build failed"))?;
        let s = dec
            .decode_symbols(std::iter::once(&m))
            .next()
            .ok_or_else(|| crate::error::Error::fmt_msg("mix stream exhausted"))?
            .map_err(|e| crate::error::Error::fmt_msg(format!("mix decode symbol: {e:?}")))?;
        let s = u8::try_from(s)
            .map_err(|_| crate::error::Error::fmt_msg("mix symbol out of byte range"))?;
        ctxs[c].step(s);
        out.push(s);
        prev = s;
    }
    Ok(out)
}
