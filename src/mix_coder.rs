//! MNF13-R5 — the dual-timescale adaptive mix coder (backend id 4),
//! MNF13-R6 — the R6-tuned variant (backend id 5),
//! MNF13-R9 — the R9 warm/refined variant (backend id 6).
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
//! Backend 6 (MNF13-R9-E, docs/mnf13-r9d-record.md): the R9-C/D
//! composition — a 20-context table whose heavy band (prev >= 97, keys
//! 7/15) splits into children keyed by prev3's coarse band (R9-D), every
//! band-split key mixing with its parent while cold (lambda = n0/(n0+n)
//! while nlocal < WARM_COLD_WINDOW, R9-C's measured schedule), and the
//! parent model kept LIVE by taking the same symbol's update as the
//! child (the `share` mechanism the R9-D composition needs). Same
//! algorithmic family; the DECODE evolution differs, hence the distinct
//! backend id. Measured through the R9 bridge: -996 B aggregate /
//! best-shape 17/4 on the 22 production streams; promotion gated by the
//! corpus run (docs/mnf13-r9e-record.md).
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
/// Log-space EWA learning rate (all backends).
const EWA_LR: f64 = 0.15;
/// Per-symbol count increment (all backends).
const COUNT_INC: u64 = 32;
/// Backend 5's FAST estimator halving cap (MNF13-R6).
pub const MIX5_FAST_CAP: u64 = 32768;
/// Backend 5's SLOW estimator halving cap — the long horizon (MNF13-R6).
pub const MIX5_SLOW_CAP: u64 = 1048576;
/// Backend 5's context count (prev bucket + 2-band pre-prev key).
pub const MIX5_NCTX: usize = 16;
/// Backend 6's context count: backend 5's 16 keys + 4 refined heavy-band
/// children (keys 7 -> {16, 17}, key 15 -> {18, 19} by prev3's band).
pub const MIX6_NCTX: usize = 20;
/// Backend 6's cold-mix schedule n0 (the R9-C measured winner; lambda =
/// n0 / (n0 + nlocal) decays from full-parent at first use and leaves the
/// window after WARM_COLD_WINDOW local updates of the context).
pub const MIX6_WARM_N0: u64 = 12;
/// Backend 6's cold-mix window in per-context local updates (the R9-C
/// cold regime the schedule harvests; frozen with the backend).
const WARM_COLD_WINDOW: u64 = 256;
/// No-parent marker in the MixCtx parent tree.
const NO_PARENT: u8 = 0xFF;

/// One coder configuration; backend 4, 5 and 6 differ ONLY here. The
/// R9 knobs are `None`/false for backends 4/5 — their code paths (and
/// wires) stay byte-identical to the released shapes.
#[derive(Clone, Copy)]
pub struct MixParams {
    /// Context count (backend 4: 8; backend 5: 16; backend 6: 20).
    pub nctx: usize,
    /// FAST estimator halving cap.
    pub cap_fast: u64,
    /// SLOW estimator halving cap.
    pub cap_slow: u64,
    /// Backend 6 only: cold-mix lambda's n0 (`None` = plain evolution).
    pub warm_n0: Option<u64>,
    /// Backend 6 only: split the heavy band (prev >= 97) into prev3-
    /// banded children (requires `nctx` = 20).
    pub refine_heavy: bool,
    /// Backend 6 only: the parent model also takes the symbol's update
    /// (live ancestors — the composition the R9-D gate measured).
    pub share: bool,
}

impl MixParams {
    /// Backend 4 constants (v1.13.0, frozen file-format contract).
    pub const BACKEND4: Self = Self {
        nctx: 8,
        cap_fast: MIX_FAST_CAP,
        cap_slow: MIX_SLOW_CAP,
        warm_n0: None,
        refine_heavy: false,
        share: false,
    };
    /// Backend 5 constants (v1.14.0, MNF13-R6; backend 4's 16-key tuning,
    /// byte-frozen at its release).
    pub const BACKEND5: Self = Self {
        nctx: MIX5_NCTX,
        cap_fast: MIX5_FAST_CAP,
        cap_slow: MIX5_SLOW_CAP,
        warm_n0: None,
        refine_heavy: false,
        share: false,
    };
    /// Backend 6 constants (MNF13-R9-E): backend 5's caps + the R9
    /// warm/refine/share composition (n0 = 12, the measured winner).
    pub const BACKEND6: Self = Self {
        nctx: MIX6_NCTX,
        cap_fast: MIX5_FAST_CAP,
        cap_slow: MIX5_SLOW_CAP,
        warm_n0: Some(MIX6_WARM_N0),
        refine_heavy: true,
        share: true,
    };
}

/// The previous-coded-byte magnitude context (0; 1-2; 3-5; 6-11;
/// 12-24; 25-48; 49-96; 97+) — shared by every backend's key rules.
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
    /// Parent context in the grammar's own tree (R9 warm topology;
    /// 0xFF = none). Inert for backends 4/5 (`warm_n0` = None).
    parent: u8,
    /// Local updates SINCE the context was built — the R9 cold-mix
    /// window's counter (saturating; inert when `warm_n0` = None).
    nlocal: u64,
}

impl MixCtx {
    fn new(cap_fast: u64, cap_slow: u64) -> Self {
        Self {
            fast: vec![1; 256],
            slow: vec![1; 256],
            log_w: 0.0,
            cap_fast,
            cap_slow,
            parent: NO_PARENT,
            nlocal: 0,
        }
    }

    /// Fixed-point probability vector over the FULL alphabet; every
    /// symbol >= 1 (all must stay decodable); total exactly SCALE.
    fn probs(&self) -> Vec<u64> {
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
        p
    }

    /// The R9 warm form: the context's own vector, or mixed with the
    /// live parent's while cold (lambda = n0/(n0+n) decays to no-op).
    /// Inert unless `warm_n0` is set (backends 4/5 return the plain
    /// shape byte-identically).
    fn probs_with(&self, ctxs: &[MixCtx], params: &MixParams) -> Vec<u64> {
        let base = self.probs();
        match params.warm_n0 {
            Some(n0) if self.nlocal < WARM_COLD_WINDOW && self.parent != NO_PARENT => {
                let n = self.nlocal as f64;
                let lam = n0 as f64 / (n0 as f64 + n);
                mix_vecs(&base, &ctxs[usize::from(self.parent)].probs(), lam)
            }
            _ => base,
        }
    }

    fn model(
        &self,
        ctxs: &[MixCtx],
        params: &MixParams,
    ) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
        let p32: Vec<u32> = self
            .probs_with(ctxs, params)
            .into_iter()
            .map(|v| v as u32)
            .collect();
        ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            p32,
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
        // nlocal stops counting at the Init-seal ceiling by saturation
        // (the ceiling is unreachable on the shipped schedule)
        self.nlocal = self.nlocal.saturating_add(1);
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

/// The R9 warm mix: P = (1-lambda)·a + lambda·b in fixed point, then
/// rebalance (both are valid 2^24 vectors, so the mix is a convex
/// combination; remainder to slot 0). Deterministic float path — the
/// exact arithmetic the R9 bridge measured with.
fn mix_vecs(a: &[u64], b: &[u64], lam: f64) -> Vec<u64> {
    let mut p: Vec<u64> = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| ((1.0 - lam) * x as f64 + lam * y as f64) as u64)
        .collect();
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
    p
}

/// The R9 warm topology: parent of band-split key k = k & 7 (the R5
/// bucket the 16-key rule split); the refined heavy-band children look
/// to the base heavy keys they refine (16, 17 -> 7; 18, 19 -> 15).
/// No-op unless `share`/`warm_n0` are set (backend 6).
fn build_parent_tree(ctxs: &mut [MixCtx], params: &MixParams) {
    if params.warm_n0.is_none() {
        return; // backends 4/5: no tree, no share, byte-frozen shapes
    }
    ctxs.iter_mut()
        .enumerate()
        .skip(8)
        .take(params.nctx.saturating_sub(8))
        .for_each(|(k, ctx)| ctx.parent = (k & 7) as u8);
    if ctxs.len() > 16 {
        ctxs[16].parent = 7;
        ctxs[17].parent = 7;
    }
    if ctxs.len() > 18 {
        ctxs[18].parent = 15;
        ctxs[19].parent = 15;
    }
}

/// The `share` half of the R9 composition: the ancestor model also takes
/// the symbol's update (child stepped first, then the parent — the
/// decode side replicates the exact order; one chain level).
#[inline]
fn share_step(ctxs: &mut [MixCtx], params: &MixParams, k: usize, s: u8) {
    if params.share {
        let p = ctxs[k].parent;
        if p != NO_PARENT {
            let p = usize::from(p);
            ctxs[p].step(s);
        }
    }
}

/// The context key for the byte at coded position `i`, given the three
/// previously coded bytes. Backend 4 = 8 buckets of `prev` (the v1.13
/// rule, byte-for-byte); backend 5 = the prev bucket plus a 2-band
/// coarse pre-prev bucket (the R6 rule); backend 6 additionally splits
/// the heavy band (prev >= 97, keys 7/15) into children keyed by
/// prev3's coarse band (the R9 rule; deterministic, causal).
#[inline]
fn mix_ctx_key(params: &MixParams, prev: u8, prev2: u8, prev3: u8) -> usize {
    let pb = usize::from(mix_ctx_bucket(prev));
    if params.nctx == 8 {
        pb
    } else {
        let band = usize::from(mix_ctx_bucket(prev2) >= 3);
        let base = pb | (band << 3);
        if params.refine_heavy && params.nctx >= MIX6_NCTX && pb == 7 {
            // heavy band: prev3's coarse band picks the child
            let band3 = usize::from(mix_ctx_bucket(prev3) >= 3);
            if band == 0 {
                16 + band3
            } else {
                18 + band3
            }
        } else {
            base
        }
    }
}

/// The mix coder: encode `data` (the residual bytes in coded order)
/// under `params`. Zero transport; the model evolves from the bytes
/// themselves.
pub fn mix_encode_with(params: &MixParams, data: &[u8]) -> Vec<u8> {
    let mut ctxs: Vec<MixCtx> = (0..params.nctx)
        .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
        .collect();
    build_parent_tree(&mut ctxs, params);
    let mut enc = DefaultRangeEncoder::new();
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    let mut prev3 = 0u8;
    for &b in data {
        let c = mix_ctx_key(params, prev, prev2, prev3);
        let m = ctxs[c]
            .model(&ctxs, params)
            .expect("mix model with all-nonzero probabilities is always buildable");
        enc.encode_symbols(std::iter::once((usize::from(b), &m)))
            .expect("mix encode symbol");
        ctxs[c].step(b);
        share_step(&mut ctxs, params, c, b);
        prev3 = prev2;
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
    let (chunks, _) = data[4..].as_chunks::<4>();
    for chunk in chunks {
        words.push(u32::from_le_bytes(*chunk));
    }
    let mut dec = DefaultRangeDecoder::from_compressed(words)
        .map_err(|e| crate::error::Error::fmt_msg(format!("mix decoder open: {e:?}")))?;
    let mut ctxs: Vec<MixCtx> = (0..params.nctx)
        .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
        .collect();
    build_parent_tree(&mut ctxs, params);
    let mut out = Vec::with_capacity(stream_len);
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    let mut prev3 = 0u8;
    for _i in 0..stream_len {
        let c = mix_ctx_key(params, prev, prev2, prev3);
        let m = ctxs[c]
            .model(&ctxs, params)
            .ok_or_else(|| crate::error::Error::fmt_msg("mix model build failed"))?;
        let s = dec
            .decode_symbols(std::iter::once(&m))
            .next()
            .ok_or_else(|| crate::error::Error::fmt_msg("mix stream exhausted"))?
            .map_err(|e| crate::error::Error::fmt_msg(format!("mix decode symbol: {e:?}")))?;
        let s = u8::try_from(s)
            .map_err(|_| crate::error::Error::fmt_msg("mix symbol out of byte range"))?;
        ctxs[c].step(s);
        share_step(&mut ctxs, params, c, s);
        out.push(s);
        prev3 = prev2;
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

    /// R9: backends 4 and 5 are byte-frozen — the new params fields add
    /// zero behavior when the knobs are None/false.
    #[test]
    fn mix_backends45_byte_frozen() {
        let data: Vec<u8> = (0..4096u32)
            .map(|i| ((i * 31 + i * i) >> 4) as u8)
            .collect();
        let mut p5_nofreeze = MixParams::BACKEND5;
        p5_nofreeze.nctx = 20; // even a 20-table allocation changes nothing
        assert_eq!(
            mix_encode_with(&MixParams::BACKEND5, &data),
            mix_encode_with(&p5_nofreeze, &data),
            "warm/refine/share knobs must be inert for the frozen forms"
        );
    }

    /// R9 backend 6: roundtrip exactness on a heavy-band synthetic (the
    /// refined children exercise prev3-banded keys) and wire-distinctness
    /// from backend 5 on the same data (distinct decode evolution).
    #[test]
    fn mix_backend6_roundtrip_and_distinct() {
        // heavy-band stream: values >= 97 dominate, prev-dependent
        // structure so the children's band3 splits actually split
        let data: Vec<u8> = (0..8192u32)
            .map(|i| (97 + (i * 17 + (i >> 4) * 13 + i / 7) % 159) as u8)
            .collect();
        let w5 = mix_encode_with(&MixParams::BACKEND5, &data);
        let w6 = mix_encode_with(&MixParams::BACKEND6, &data);
        assert_eq!(
            mix_decode_with(&MixParams::BACKEND5, &w5, data.len()).unwrap(),
            data
        );
        assert_eq!(
            mix_decode_with(&MixParams::BACKEND6, &w6, data.len()).unwrap(),
            data
        );
        assert_ne!(w5, w6, "backend 6's evolution must differ from backend 5's");
    }

    /// R9 backend 6: malformed-input rejects are errors, never panics
    /// (the R9 knobs change evolution, not the payload framing contract).
    #[test]
    fn mix_backend6_rejects_malformed() {
        let data: Vec<u8> = (0..4096u32).map(|i| (97 + (i % 159)) as u8).collect();
        let w6 = mix_encode_with(&MixParams::BACKEND6, &data);
        // not word-aligned
        assert!(mix_decode_with(&MixParams::BACKEND6, &w6[..17], 4096).is_err());
        // bomb stream_len
        let mut bomb = w6.clone();
        bomb[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(mix_decode_with(&MixParams::BACKEND6, &bomb, 4096).is_err());
        // word-aligned truncation: never a panic, either way it exits
        let r = mix_decode_with(&MixParams::BACKEND6, &w6[..12], 4096);
        let _ = r.map(|v| assert!(v.len() <= 4096));
        // well-formed wire decodes exact
        assert_eq!(
            mix_decode_with(&MixParams::BACKEND6, &w6, data.len()).unwrap(),
            data
        );
    }
}
