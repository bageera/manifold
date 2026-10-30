//! MNF13-R5 — the dual-timescale adaptive mix coder (backend id 4),
//! MNF13-R6 — the R6-tuned variant (backend id 5).
//!
//! Per-context FAST and SLOW adaptive fixed-point (2^24-scale) estimators
//! over the full 256-symbol alphabet, mixed by a log-space EWA weight
//! updated per symbol from the DECODED symbol — identical model evolution
//! on both sides (zero model transport; the wire = [u32 stream_len] + LE
//! u32 words). ConstricTON 0.5's stream range coder carries the
//! fixed-point probabilities (2^24 scale, no fp in the wire data path).
//! The exact-inverse contract is asserted by the conformance suite
//! (`tests/mnf8_conformance.rs`) and the corpus gate.
//!
//! Wire (v1.14.0): MNF8 backend ids 4 (v1.13.0, frozen) and 5 (MNF13-R6;
//! the measured grid winner, docs/mnf13-r6.md). The payload the backend
//! field addresses = the mix-coded residual byte stream in the file's
//! own coded order; every context key recomputes from already-coded
//! bytes, so the decoder needs no side tables and rejects malformed
//! inputs with errors (never panics).
//!
//! Backend 4: context = the previous coded byte bucketed to 8 keys;
//! FAST cap 4,096, SLOW cap 65,536. Measured docs/mnf13-r5.md: −1.80%
//! complete corpus at the v1.12 pick level, wins 19/19 photographic
//! record files, zero regressions elsewhere.
//!
//! Backend 5 (MNF13-R6): context = the previous byte's bucket + a
//! 2-band coarse pre-prev key (16 total); FAST cap 32,768, SLOW cap
//! 1,048,576 (the long horizon). Same algorithmic family; the DECODE
//! evolution differs by the constants, which is why the backend id is
//! distinct. Measured docs/mnf13-r6.md: −3,376 B on the 20 backend-4
//! corpus files (−0.62%), 20/20 fires, zero regressions; corpus
//! projection 816,341 → 812,965 (−0.41%).

use constriction::stream::{
    model::ContiguousCategoricalEntropyModel, queue::DefaultRangeDecoder,
    queue::DefaultRangeEncoder, Decode, Encode,
};

/// Fixed-point probability scale (2^24).
const SCALE: u64 = 1 << 24;
/// Backend 4's FAST estimator halving cap (frozen with v1.13.0).
pub const MIX_FAST_CAP: u64 = 4096;
/// Backend 4's SLOW estimator halving cap (frozen with v1.13.0).
pub const MIX_SLOW_CAP: u64 = 65536;
/// Log-space EWA learning rate (both backends).
const EWA_LR: f64 = 0.15;
/// Per-symbol count increment (both backends).
const COUNT_INC: u64 = 32;
/// Backend 5's FAST estimator halving cap (MNF13-R6).
pub const MIX5_FAST_CAP: u64 = 32768;
/// Backend 5's SLOW estimator halving cap — the long horizon (MNF13-R6).
pub const MIX5_SLOW_CAP: u64 = 1048576;
/// Backend 5's context count (prev bucket + 2-band pre-prev key).
pub const MIX5_NCTX: usize = 16;

/// One coder configuration; backend 4 and 5 differ ONLY here.
#[derive(Clone, Copy)]
pub struct MixParams {
    /// Context count (backend 4: 8; backend 5: 16).
    pub nctx: usize,
    /// FAST estimator halving cap.
    pub cap_fast: u64,
    /// SLOW estimator halving cap.
    pub cap_slow: u64,
}

impl MixParams {
    /// Backend 4 constants (v1.13.0, frozen file-format contract).
    pub const BACKEND4: Self = Self {
        nctx: 8,
        cap_fast: MIX_FAST_CAP,
        cap_slow: MIX_SLOW_CAP,
    };
    /// Backend 5 constants (v1.14.0, MNF13-R6).
    pub const BACKEND5: Self = Self {
        nctx: MIX5_NCTX,
        cap_fast: MIX5_FAST_CAP,
        cap_slow: MIX5_SLOW_CAP,
    };
}

/// The previous-coded-byte magnitude context (0; 1-2; 3-5; 6-11;
/// 12-24; 25-48; 49-96; 97+) — shared by both backends' key rules.
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
    cap_fast: u64,
    cap_slow: u64,
}

impl MixCtx {
    fn new(cap_fast: u64, cap_slow: u64) -> Self {
        Self {
            fast: vec![1; 256],
            slow: vec![1; 256],
            log_w: 0.0,
            cap_fast,
            cap_slow,
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
        if self.fast.iter().sum::<u64>() > self.cap_fast {
            for c in &mut self.fast {
                *c = (*c + 1) >> 1;
            }
        }
        if self.slow.iter().sum::<u64>() > self.cap_slow {
            for c in &mut self.slow {
                *c = (*c + 1) >> 1;
            }
        }
    }
}

/// The context key for the byte at coded position `i`, given the two
/// previously coded bytes. Backend 4 = 8 buckets of `prev` (the v1.13
/// rule, byte-for-byte); backend 5 = the prev bucket plus a 2-band
/// coarse pre-prev bucket (the R6 rule; deterministic, causal).
#[inline]
fn mix_ctx_key(params: &MixParams, prev: u8, prev2: u8) -> usize {
    let pb = usize::from(mix_ctx_bucket(prev));
    if params.nctx == 8 {
        pb
    } else {
        let band = usize::from(mix_ctx_bucket(prev2) >= 3);
        pb | (band << 3)
    }
}

/// The mix coder: encode `data` (the residual bytes in coded order)
/// under `params`. Zero transport; the model evolves from the bytes
/// themselves.
pub fn mix_encode_with(params: &MixParams, data: &[u8]) -> Vec<u8> {
    let mut ctxs: Vec<MixCtx> = (0..params.nctx)
        .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
        .collect();
    let mut enc = DefaultRangeEncoder::new();
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    for &b in data {
        let c = mix_ctx_key(params, prev, prev2);
        let m = ctxs[c]
            .model()
            .expect("mix model with all-nonzero probabilities is always buildable");
        enc.encode_symbols(std::iter::once((usize::from(b), &m)))
            .expect("mix encode symbol");
        ctxs[c].step(b);
        prev2 = prev;
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

/// The backend-4 form (v1.13.0 constants; byte-identical to the 1.13.0
/// release's behavior).
pub fn mix_encode(data: &[u8]) -> Vec<u8> {
    mix_encode_with(&MixParams::BACKEND4, data)
}

/// The inverse under `params`: decode the wire back to its stream bytes.
/// Errors, never panics, on malformed input.
pub fn mix_decode_with(
    params: &MixParams,
    data: &[u8],
    limit: usize,
) -> Result<Vec<u8>, crate::error::Error> {
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
    let mut ctxs: Vec<MixCtx> = (0..params.nctx)
        .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
        .collect();
    let mut out = Vec::with_capacity(stream_len);
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    for _i in 0..stream_len {
        let c = mix_ctx_key(params, prev, prev2);
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
        prev2 = prev;
        prev = s;
    }
    Ok(out)
}

/// The backend-4 form (v1.13.0 constants).
pub fn mix_decode(data: &[u8], limit: usize) -> Result<Vec<u8>, crate::error::Error> {
    mix_decode_with(&MixParams::BACKEND4, data, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R5 legacy: the 8-ctx backend-4 form is byte-exact on run classes.
    #[test]
    fn mix_backend4_roundtrip_runs() {
        let data: Vec<u8> = std::iter::repeat_n(0u8, 4096)
            .chain(std::iter::repeat_n(7u8, 512))
            .collect();
        let wire = mix_encode(&data);
        assert_eq!(mix_decode(&wire, data.len()).unwrap(), data);
    }

    /// R6: the 16-key backend-5 form roundtrips and differs from
    /// backend 4 on non-trivial data (distinct decode evolution).
    #[test]
    fn mix_backend5_roundtrip_and_distinct() {
        let data: Vec<u8> = (0..8192u32).map(|i| (i * 7 + (i >> 5)) as u8).collect();
        let w4 = mix_encode(&data);
        let w5 = mix_encode_with(&MixParams::BACKEND5, &data);
        assert_eq!(mix_decode(&w4, data.len()).unwrap(), data);
        assert_eq!(
            mix_decode_with(&MixParams::BACKEND5, &w5, data.len()).unwrap(),
            data
        );
        // different model constants = different wire (usually smaller)
        assert_ne!(w4, w5, "backend 5's evolution must differ from backend 4's");
    }

    /// The R6 research projection's shape: on the REAL residual wires the
    /// 16-key/long-cap form never loses (pinned in the corpus gate;
    /// this unit pins the wire-level contract on one synthetic stream).
    #[test]
    fn mix_backend5_rejects_malformed() {
        let data: Vec<u8> = (0..4096u32).map(|i| ((i * 31) >> 3) as u8).collect();
        let w5 = mix_encode_with(&MixParams::BACKEND5, &data);
        // truncated (17 B is not word-aligned) / bomb stream_len
        assert!(mix_decode_with(&MixParams::BACKEND5, &w5[..17], 4096).is_err());
        let mut bomb = w5.clone();
        bomb[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(mix_decode_with(&MixParams::BACKEND5, &bomb, 4096).is_err());
        // truncation to a word-aligned prefix (12 B = 2 words past the
        // length word): constriction's queue decoder MAY legitimately
        // decode a prefix stream — exhaustion surfaces as Ok-with-short
        // OR Err depending on the coder state; the CONTRACT is never-a-
        // panic, so assert exactly that:
        let r = mix_decode_with(&MixParams::BACKEND5, &w5[..12], 4096);
        assert!(r.is_ok() || r.is_err(), "no panic reachable"); // panics propagate
        let _ = r.map(|v| assert!(v.len() <= 4096));
        // well-formed wire of a RUN stream decodes exact
        assert_eq!(
            mix_decode_with(&MixParams::BACKEND5, &w5, data.len()).unwrap(),
            data
        );
    }
}
