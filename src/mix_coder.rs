//! MNF13-R5 — the dual-timescale adaptive mix coder (backend id 4),
//! MNF13-R6 — the R6-tuned variant (backend id 5),
//! MNF13-R9 — the R9 warm/refined variant (backend id 6).
//! MNF16-X1-L — the lane-class-conditioned variant (backend id 7).
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
/// Backend 7 (MNF16-X1-L): the promoted 40-context lane-class mapping
/// (20-key base + 20 fresh children; docs/mnf16-x1l-r5-record.md).
pub const MIX7_NCTX: usize = 40;
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
    /// Backend 7 only: lane-class conditioning (MNF16-X1-L, promoted at
    /// v1.17.0). Class-1 difference-lane bytes take a child context at
    /// 20 + the base key; class-0 value-lane/framing bytes keep the base
    /// key. Classes derive from the decoder-available grammar walk
    /// (`LaneWalk`); nothing is transmitted. Requires `nctx` = 40.
    pub lane_child: bool,
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
        lane_child: false,
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
        lane_child: false,
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
        lane_child: false,
    };
    /// Backend 7 constants (v1.17.0, MNF16-X1-L): backend 6's full
    /// schedule plus lane-class children (fresh, no hierarchical
    /// sharing — L2 measured half of L1 and was not promoted). The
    /// measured winner across the R0-R5 program.
    pub const BACKEND7: Self = Self {
        nctx: MIX7_NCTX,
        cap_fast: MIX5_FAST_CAP,
        cap_slow: MIX5_SLOW_CAP,
        warm_n0: Some(MIX6_WARM_N0),
        refine_heavy: true,
        share: true,
        lane_child: true,
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
    // Backend 7: the lane children (keys 20+) are FRESH — no ancestors
    // (the X1-L L1 arm's measured winner; hierarchical sharing, L2,
    // measured half of L1 and was not promoted). The base keys 8..19
    // keep the R9 warm topology.
    let parented = if params.lane_child {
        MIX6_NCTX
    } else {
        params.nctx
    };
    ctxs.iter_mut()
        .enumerate()
        .skip(8)
        .take(parented.saturating_sub(8))
        .for_each(|(k, ctx)| ctx.parent = (k & 7) as u8);
    if parented > 16 {
        ctxs[16].parent = 7;
        ctxs[17].parent = 7;
    }
    if parented > 18 {
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

/// Backend 7's incrementally maintained lane class (MNF16-X1-L, promoted
/// at v1.17.0 per docs/mnf16-x1l-r5-record.md; native per R4
/// docs/mnf16-x1l-r4-record.md). A pure function of the container
/// metadata (width, height, tile count, `planar`) and the decoded-prefix
/// grammar state — zero transmitted classification. Framing (mode/meta)
/// bytes class 0; interleaved bodies repeat (0,1,1) per pixel; planar
/// bodies stack lane-major planes (tw*th class-0 bytes then 2*tw*th
/// class-1 bytes). Byte-exact mirror of the R4 native walk that the
/// parity gate validated; `None` on malformed framing (reject, not
/// panic — the decode side maps this to a clean error).
#[derive(Clone)]
pub struct LaneWalk {
    planar: bool,
    width: u32,
    height: u32,
    grid_cols: u32,
    next_tile: u32,
    tile_count: u32,
    tw: u32,
    th: u32,
    meta_left: usize,
    body_left: u64,
    body_pos: u64,
    all_zero: bool,
}

/// The record grammar's record shape after the mode byte: (meta bytes
/// still to come, residual-block bytes), per the shipped relabel/decode
/// walk (mnf8.rs channel_diff_relabel, residual_census.py) — mode 3 =
/// the bit-packed transform record (np + 2*ceil(np*9/8), no per-lane
/// layout), modes 6/7 = the law-record constants.
fn record_shape(mode: u8, np: u64) -> Option<(usize, u64)> {
    let nb = np * 3;
    match mode {
        0 => Some((0, nb)),
        1 | 4 => Some((4, nb)),
        2 | 5 => Some((5, nb)),
        3 => {
            let n9 = (np * 9).div_ceil(8);
            Some((0, np + 2 * n9))
        }
        6 => Some((crate::laws::PLANAR_LAW_BYTES, nb)),
        7 => Some((crate::laws::BILINEAR_LAW_BYTES, nb)),
        8 => Some((3, nb)),
        _ => None, // unknown record mode: reject
    }
}

impl LaneWalk {
    /// Fresh walker; `None` on degenerate geometry.
    pub fn new(width: u32, height: u32, tile_count: u32, planar: bool) -> Option<Self> {
        if width == 0 || height == 0 || tile_count == 0 {
            return None;
        }
        Some(Self {
            planar,
            width,
            height,
            grid_cols: width.div_ceil(crate::codec::TILE_SIZE),
            next_tile: 0,
            tile_count,
            tw: 0,
            th: 0,
            meta_left: 0,
            body_left: 0,
            body_pos: 0,
            all_zero: false,
        })
    }

    /// Class of the byte at the current grammar position — computable
    /// from the decoded prefix alone (the class never depends on the
    /// byte's value: framing is structural, bodies are positional).
    /// Class 0 = value-lane-or-framing, class 1 = difference-lane (the
    /// X1 availability partition). `None` = malformed framing (a byte
    /// beyond any record).
    pub fn peek_class(&self) -> Option<u8> {
        if self.meta_left == 0 && self.body_left == 0 {
            // record boundary: the next byte is the next tile's mode byte
            if self.next_tile >= self.tile_count {
                return None; // byte beyond the grammar: malformed
            }
            return Some(0);
        }
        if self.meta_left > 0 {
            return Some(0);
        }
        // body byte (mode 3 = the bit-packed transform record: class 0
        // throughout — deterministic; no per-lane byte structure)
        if self.all_zero {
            return Some(0);
        }
        if self.planar {
            Some(if self.body_pos < u64::from(self.tw) * u64::from(self.th) {
                0
            } else {
                1
            })
        } else if self.body_pos.is_multiple_of(3) {
            Some(0)
        } else {
            Some(1)
        }
    }

    /// Feed the byte at the current position and advance. At a record
    /// boundary `b` must be a valid mode byte (it sets the meta count
    /// and the body length); `None` rejects unknown modes. A record that
    /// completes past the final tile leaves a terminal state; any byte
    /// after that peeks `None` (reject trailing data).
    pub fn consume(&mut self, b: u8) -> Option<()> {
        if self.meta_left == 0 && self.body_left == 0 {
            if self.next_tile >= self.tile_count {
                return None; // trailing byte: malformed
            }
            self.tw = crate::codec::TILE_SIZE
                .min(self.width - (self.next_tile % self.grid_cols) * crate::codec::TILE_SIZE);
            self.th = crate::codec::TILE_SIZE
                .min(self.height - (self.next_tile / self.grid_cols) * crate::codec::TILE_SIZE);
            self.next_tile += 1;
            self.body_pos = 0;
            let np = u64::from(self.tw) * u64::from(self.th);
            let (meta, body) = record_shape(b, np)?;
            self.meta_left = meta;
            self.body_left = body;
            // mode 3's bit-packed body is class-0 throughout
            self.all_zero = b == 3;
            return Some(());
        }
        if self.meta_left > 0 {
            self.meta_left -= 1;
            return Some(());
        }
        self.body_pos += 1;
        self.body_left -= 1;
        Some(())
    }

    /// True when every record has been framed and the residual block of
    /// the last record completed — the structural end check the R4
    /// walk's consumed-everything rule corresponds to.
    pub fn complete(&self) -> bool {
        self.next_tile == self.tile_count
            && self.meta_left == 0
            && self.body_left == 0
            && (self.body_pos == 0 || self.body_pos == self.body_len())
    }

    fn body_len(&self) -> u64 {
        u64::from(self.tw) * u64::from(self.th) * 3
    }
}

/// Container geometry the lane walk needs (the same fields the R4 gate
/// rode in the audit header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geom {
    /// Image width in pixels (the record grid's full extent).
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Number of record frames (= tiles; `cols * rows`).
    pub tile_count: u32,
    /// The container's layout bit (bodies planar-ordered when set).
    pub planar: bool,
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

/// Backend 7: encode with lane-class conditioning under `geom` (the
/// container metadata the walk needs). The wire shape is identical to
/// the other mix backends; the class mapping is never transmitted.
pub fn mix_encode_with_lane(params: &MixParams, data: &[u8], geom: Geom) -> Vec<u8> {
    let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
        .expect("valid container geometry");
    let mut ctxs: Vec<MixCtx> = (0..params.nctx)
        .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
        .collect();
    build_parent_tree(&mut ctxs, params);
    let mut enc = DefaultRangeEncoder::new();
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    let mut prev3 = 0u8;

    for &b in data {
        let cls = walk
            .peek_class()
            .expect("byte beyond the record grammar on the encode path");
        walk.consume(b)
            .expect("well-formed record stream on the encode path");
        let base = mix_ctx_key(params, prev, prev2, prev3);
        let c = if params.lane_child && cls == 1 {
            base + 20
        } else {
            base
        };
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
    assert!(walk.complete(), "record grammar must consume the stream");
    let compressed = enc.into_compressed().expect("mix seal");
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

/// Backend 7: the lane-conditioned inverse under `geom`. Errors, never
/// panics, on malformed input; classes rederive strictly from
/// already-coded bytes plus the container geometry.
pub fn mix_decode_with_lane(
    params: &MixParams,
    data: &[u8],
    limit: usize,
    geom: Geom,
) -> Result<Vec<u8>, crate::error::Error> {
    if !params.lane_child {
        return Err(crate::error::Error::fmt_msg(
            "lane conditioning requires backend 7 parameters",
        ));
    }
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
    let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
        .ok_or_else(|| crate::error::Error::fmt_msg("invalid container geometry"))?;
    let mut ctxs: Vec<MixCtx> = (0..params.nctx)
        .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
        .collect();
    build_parent_tree(&mut ctxs, params);
    let mut out = Vec::with_capacity(stream_len);
    let mut prev = 0u8;
    let mut prev2 = 0u8;
    let mut prev3 = 0u8;
    for _i in 0..stream_len {
        // class FIRST: computable from the decoded prefix (never from
        // the byte being decoded — the same prefix-invariance contract
        // every conditional feature passes)
        let cls = walk
            .peek_class()
            .ok_or_else(|| crate::error::Error::fmt_msg("byte beyond the record grammar"))?;
        let base = mix_ctx_key(params, prev, prev2, prev3);
        let c = if cls == 1 { base + 20 } else { base };
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
        walk.consume(s)
            .ok_or_else(|| crate::error::Error::fmt_msg("malformed record framing"))?;
        ctxs[c].step(s);
        share_step(&mut ctxs, params, c, s);
        out.push(s);
        prev3 = prev2;
        prev2 = prev;
        prev = s;
    }
    if !walk.complete() {
        return Err(crate::error::Error::fmt_msg(
            "record grammar incomplete: payload shorter than its records frame",
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R6.2-C: backend 7's wire must BYTE-MATCH the audit's reference
    /// implementation on identical (stream, classes, geometry): a 16x4
    /// planar mode-0 record through the lane walk + cold-mix + all caps.
    /// On mismatch, bisects the first divergent symbol by truncation.
    #[test]
    fn backend7_lane_wire_matches_audit() {
        let npx = 16 * 4usize;
        let np = npx * 3;
        let mut data = vec![0u8; 1 + np];
        for (i, b) in data[1..].iter_mut().enumerate() {
            *b = ((i * 7 + (i >> 3)) % 256) as u8;
        }
        let geom = crate::mix_coder::Geom {
            width: 16,
            height: 4,
            tile_count: 1,
            planar: true,
        };
        let wire = mix_encode_with_lane(&MixParams::BACKEND7, &data, geom);
        let bridge = std::path::Path::new("target/release/examples/r9_audit");
        if !bridge.exists() {
            eprintln!("audit bridge not built; skipping");
            return;
        }
        let mode = "mix32768_1048576-lr15-inc32-ctx16-refineA-warm2p12-share-lanechild";
        // per-SYMBOL classes: the mode byte itself is class 0
        let classes: Vec<u8> = std::iter::once(0u8)
            .chain(std::iter::repeat_n(0u8, npx))
            .chain(std::iter::repeat_n(1u8, 2 * npx))
            .collect();
        let run = |k: usize| -> Vec<u8> {
            let mut input = format!("{} {mode}\n", data[..1 + k].len()).into_bytes();
            input.extend_from_slice(&data[..1 + k]);
            input.extend_from_slice(&classes[..1 + k]);
            use std::io::Write;
            let mut child = std::process::Command::new(bridge)
                .arg("e")
                .arg(mode)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::inherit())
                .spawn()
                .expect("bridge spawn");
            child
                .stdin
                .take()
                .expect("stdin")
                .write_all(&input)
                .expect("bridge stdin");
            let c = child.wait_with_output().expect("bridge run");
            assert_eq!(c.status.code(), Some(0), "audit failed: {}", c.status);
            c.stdout
        };
        let ref_wire = run(data.len() - 1);
        if wire == ref_wire {
            eprintln!("R6.2-C: synthetic wire MATCHES the audit reference");
            return;
        }
        eprintln!(
            "R6.2-C: MISMATCH prod {} vs audit {} bytes on {} symbols",
            wire.len(),
            ref_wire.len(),
            data.len()
        );
        let mut lo = 1usize;
        let mut b = data.len() - 1;
        // longest K whose wires agree; report the first divergent symbol
        while lo < b {
            let mid = (lo + b).div_ceil(2);
            let pw = mix_encode_with_lane(&MixParams::BACKEND7, &data[..1 + mid], geom);
            let aw = run(mid);
            if pw == aw {
                lo = mid;
            } else {
                b = mid - 1;
            }
        }
        eprintln!(
            "R6.2-C: first divergent SYMBOL = {} (wires agree through {lo})",
            lo + 1
        );
        eprintln!(
            "  prod[:{}] = {} B | audit[:{}] = {} B",
            lo + 2,
            mix_encode_with_lane(&MixParams::BACKEND7, &data[..lo + 2], geom).len(),
            lo + 2,
            run(lo + 1).len()
        );
        panic!("backend 7 wire != audit reference (see R6.2-C report)");
    }

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

/// MNF17-P1-B1/B2 research twin (cfg(test) only: excluded from release
/// builds). Replays mix_encode_with(_lane) byte-for-byte WITHOUT the
/// entropy writer (encode_symbols), timing and folding each stage.
/// The writer never feeds back into walk/ctxs/key evolution (verified by
/// reading both encode paths: the enc output is not an input anywhere),
/// so t_full - t_twin is an exact exclusive-writer attribution.
/// Sampled folds index layer identities (indexes only; verify on hit —
/// hash equality is never treated as proof): key/class per byte, model
/// vector sampled at first/last word.
/// MNF17-P1-B1 research twins (cfg(test) only). Verbatim copies of
/// `mix_encode_with` / `mix_encode_with_lane` with the entropy-writer
/// call (`enc.encode_symbols`) REMOVED and nothing else changed — no
/// folds, no branches, no counters — so wall(twin) is wall(full) minus
/// the writer's exclusive cost (the writer never feeds back into
/// walk/ctx/key evolution; verified by reading both encode paths).
/// Identity work lives in the caller, outside these timed bodies.
#[cfg(test)]
pub(crate) mod p1b {
    use super::*;
    use std::time::Instant;

    fn timed_prep<T>(f: impl FnOnce() -> T) -> (T, u64) {
        let t = Instant::now();
        let v = f();
        (v, t.elapsed().as_micros() as u64)
    }

    /// backend 4/5/6 form minus writer; returns (prep_us, loop_us, seal_us)
    pub(crate) fn nowriter(params: &MixParams, data: &[u8]) -> (u64, u64, u64) {
        let (mut ctxs, prep_us) = timed_prep(|| {
            let mut ctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            build_parent_tree(&mut ctxs, params);
            ctxs
        });
        let enc = DefaultRangeEncoder::new();
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        let t1 = Instant::now();
        for &b in data {
            let c = mix_ctx_key(params, prev, prev2, prev3);
            let m = ctxs[c]
                .model(&ctxs, params)
                .expect("mix model is always buildable");
            drop(m);
            ctxs[c].step(b);
            share_step(&mut ctxs, params, c, b);
            prev3 = prev2;
            prev2 = prev;
            prev = b;
        }
        let loop_us = t1.elapsed().as_micros() as u64;
        let t2 = Instant::now();
        let _ = enc.into_compressed();
        (prep_us, loop_us, t2.elapsed().as_micros() as u64)
    }

    /// backend 7 form minus writer; returns (prep_us, loop_us, seal_us)
    pub(crate) fn nowriter_lane(params: &MixParams, data: &[u8], geom: Geom) -> (u64, u64, u64) {
        let (parts, prep_us) = timed_prep(|| {
            let walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
                .expect("valid container geometry");
            let mut ctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            build_parent_tree(&mut ctxs, params);
            (walk, ctxs)
        });
        let (mut walk, mut ctxs) = parts;
        let enc = DefaultRangeEncoder::new();
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        let t1 = Instant::now();
        for &b in data {
            let cls = walk
                .peek_class()
                .expect("byte beyond the record grammar on the encode path");
            walk.consume(b)
                .expect("well-formed record stream on the encode path");
            let base = mix_ctx_key(params, prev, prev2, prev3);
            let c = if params.lane_child && cls == 1 {
                base + 20
            } else {
                base
            };
            let m = ctxs[c]
                .model(&ctxs, params)
                .expect("mix model is always buildable");
            drop(m);
            ctxs[c].step(b);
            share_step(&mut ctxs, params, c, b);
            prev3 = prev2;
            prev2 = prev;
            prev = b;
        }
        let loop_us = t1.elapsed().as_micros() as u64;
        let t2 = Instant::now();
        let _ = enc.into_compressed();
        (prep_us, loop_us, t2.elapsed().as_micros() as u64)
    }
}

/// MNF17-P1-C4-A research twin (cfg(test) only): the mix coder's model
/// loop with incrementally MAINTAINED counter sums (`sf`/`ss`) replacing
/// the per-symbol full-vector reductions. Every mutation site of
/// `fast`/`slow` tracks its sum inline; rescaling recounts inside its
/// own sweep. Invariant under test: (sf, ss) == (Σ fast, Σ slow) at
/// every symbol, for every backend configuration and fixture. The
/// quantized vector, its corrections and the coder calls are otherwise
/// byte-identical; wire byte-equality against the production encoders
/// is asserted by the differential tests.
#[cfg(test)]
pub(crate) mod p1c4a {
    use super::*;

    pub(crate) struct MixCtxA {
        fast: Vec<u64>,
        slow: Vec<u64>,
        log_w: f64,
        cap_fast: u64,
        cap_slow: u64,
        parent: u8,
        nlocal: u64,
        sf: u64,
        ss: u64,
    }

    impl MixCtxA {
        pub(crate) fn new(cap_fast: u64, cap_slow: u64) -> Self {
            Self {
                fast: vec![1; 256],
                slow: vec![1; 256],
                log_w: 0.0,
                cap_fast,
                cap_slow,
                parent: NO_PARENT,
                nlocal: 0,
                sf: 256,
                ss: 256,
            }
        }

        /// Same arithmetic as `MixCtx::probs`, reading the maintained sums.
        pub(crate) fn probs_a(&self) -> Vec<u64> {
            let tf = self.sf as f64;
            let ts = self.ss as f64;
            let wf = 1.0 / (1.0 + (-self.log_w).exp());
            let mut p: Vec<u64> = self
                .fast
                .iter()
                .zip(&self.slow)
                .map(|(&cf, &cs)| {
                    ((wf * (cf as f64 / tf) + (1.0 - wf) * (cs as f64 / ts)) * SCALE as f64) as u64
                })
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

        /// Same arithmetic as `MixCtx::probs_with` (the R9 warm mix),
        /// building the base from the maintained sums.
        pub(crate) fn probs_with_a(&self, ctxs: &[MixCtxA], params: &MixParams) -> Vec<u64> {
            let base = self.probs_a();
            match params.warm_n0 {
                Some(n0) if self.nlocal < WARM_COLD_WINDOW && self.parent != NO_PARENT => {
                    let n = self.nlocal as f64;
                    let lam = n0 as f64 / (n0 as f64 + n);
                    mix_vecs(&base, &ctxs[usize::from(self.parent)].probs_a(), lam)
                }
                _ => base,
            }
        }

        pub(crate) fn step_a(&mut self, s: u8) {
            let si = usize::from(s);
            let pf = (self.fast[si] as f64 / self.sf as f64).max(1e-9);
            let ps = (self.slow[si] as f64 / self.ss as f64).max(1e-9);
            self.log_w = (self.log_w + EWA_LR * (pf.ln() - ps.ln())).clamp(-20.0, 20.0);
            self.fast[si] += COUNT_INC;
            self.sf += COUNT_INC;
            self.slow[si] += COUNT_INC;
            self.ss += COUNT_INC;
            self.nlocal = self.nlocal.saturating_add(1);
            if self.sf > self.cap_fast {
                let mut acc = 0u64;
                for c in &mut self.fast {
                    *c = (*c + 1) >> 1;
                    acc += *c;
                }
                self.sf = acc;
            }
            if self.ss > self.cap_slow {
                let mut acc = 0u64;
                for c in &mut self.slow {
                    *c = (*c + 1) >> 1;
                    acc += *c;
                }
                self.ss = acc;
            }
        }

        pub(crate) fn model_a(
            &self,
            ctxs: &[MixCtxA],
            params: &MixParams,
        ) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
            let p32: Vec<u32> = self
                .probs_with_a(ctxs, params)
                .into_iter()
                .map(|v| v as u32)
                .collect();
            ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
                p32, false,
            )
            .ok()
        }

        /// Full-256-vector parity + state parity vs the production ctx.
        fn verify(&self, p: &MixCtx) {
            assert_eq!(self.fast, *p_fast(p), "fast counters diverged");
            assert_eq!(self.slow, *p_slow(p), "slow counters diverged");
            assert_eq!(self.log_w, p_log_w(p), "EWA weight bit-diverged");
            assert_eq!(self.nlocal, p_nlocal(p), "nlocal diverged");
            assert_eq!(
                self.sf,
                self.fast.iter().sum::<u64>(),
                "maintained fast sum violated the conservation invariant"
            );
            assert_eq!(
                self.ss,
                self.slow.iter().sum::<u64>(),
                "maintained slow sum violated the conservation invariant"
            );
        }
    }

    // private MixCtx field accessors for the twin (same module tree)
    fn p_fast(c: &MixCtx) -> &Vec<u64> {
        &c.fast
    }
    fn p_slow(c: &MixCtx) -> &Vec<u64> {
        &c.slow
    }
    fn p_log_w(c: &MixCtx) -> f64 {
        c.log_w
    }
    fn p_nlocal(c: &MixCtx) -> u64 {
        c.nlocal
    }

    /// Verbatim copy of `build_parent_tree` for the A contexts.
    fn build_parent_tree_a(ctxs: &mut [MixCtxA], params: &MixParams) {
        if params.warm_n0.is_none() {
            return;
        }
        let parented = if params.lane_child {
            MIX6_NCTX
        } else {
            params.nctx
        };
        ctxs.iter_mut()
            .enumerate()
            .skip(8)
            .take(parented.saturating_sub(8))
            .for_each(|(k, ctx)| ctx.parent = (k & 7) as u8);
        if parented > 16 {
            ctxs[16].parent = 7;
            ctxs[17].parent = 7;
        }
        if parented > 18 {
            ctxs[18].parent = 15;
            ctxs[19].parent = 15;
        }
    }

    /// Verbatim copy of `share_step` for the A contexts.
    fn share_step_a(ctxs: &mut [MixCtxA], params: &MixParams, k: usize, s: u8) {
        if params.share {
            let p = ctxs[k].parent;
            if p != NO_PARENT {
                let p = usize::from(p);
                ctxs[p].step_a(s);
            }
        }
    }

    /// The C4-A encode: production `mix_encode_with` with A contexts.
    pub(crate) fn mix_encode_with_a(params: &MixParams, data: &[u8]) -> Vec<u8> {
        let mut ctxs: Vec<MixCtxA> = (0..params.nctx)
            .map(|_| MixCtxA::new(params.cap_fast, params.cap_slow))
            .collect();
        build_parent_tree_a(&mut ctxs, params);
        let mut enc = DefaultRangeEncoder::new();
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        for &b in data {
            let c = mix_ctx_key(params, prev, prev2, prev3);
            let m = ctxs[c]
                .model_a(&ctxs, params)
                .expect("mix model with all-nonzero probabilities is always buildable");
            enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                .expect("mix encode symbol");
            ctxs[c].step_a(b);
            share_step_a(&mut ctxs, params, c, b);
            prev3 = prev2;
            prev2 = prev;
            prev = b;
        }
        let compressed = enc.into_compressed().expect("mix seal");
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

    /// The C4-A backend-7 form (production `mix_encode_with_lane`).
    pub(crate) fn mix_encode_with_lane_a(params: &MixParams, data: &[u8], geom: Geom) -> Vec<u8> {
        let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
            .expect("valid container geometry");
        let mut ctxs: Vec<MixCtxA> = (0..params.nctx)
            .map(|_| MixCtxA::new(params.cap_fast, params.cap_slow))
            .collect();
        build_parent_tree_a(&mut ctxs, params);
        let mut enc = DefaultRangeEncoder::new();
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        for &b in data {
            let cls = walk
                .peek_class()
                .expect("byte beyond the record grammar on the encode path");
            walk.consume(b)
                .expect("well-formed record stream on the encode path");
            let base = mix_ctx_key(params, prev, prev2, prev3);
            let c = if params.lane_child && cls == 1 {
                base + 20
            } else {
                base
            };
            let m = ctxs[c]
                .model_a(&ctxs, params)
                .expect("mix model with all-nonzero probabilities is always buildable");
            enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                .expect("mix encode symbol");
            ctxs[c].step_a(b);
            share_step_a(&mut ctxs, params, c, b);
            prev3 = prev2;
            prev2 = prev;
            prev = b;
        }
        assert!(walk.complete(), "record grammar must consume the stream");
        let compressed = enc.into_compressed().expect("mix seal");
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

    // ---- differential fixtures (§4 of the authorization) ----

    fn xorshift(state: &mut u64) -> u8 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        (*state >> 24) as u8
    }

    const ALL_BACKENDS: [&MixParams; 4] = [
        &MixParams::BACKEND4,
        &MixParams::BACKEND5,
        &MixParams::BACKEND6,
        &MixParams::BACKEND7,
    ];

    /// Drive a symbol sequence through BOTH engines symbol-by-symbol,
    /// asserting per-symbol: maintained-sum invariant, 256/2 state parity,
    /// and 256/256 quantized-vector parity (warm form when applicable).
    fn differential(params: &MixParams, stream: &[u8], label: &str) {
        let mut ref_ctxs: Vec<MixCtx> = (0..params.nctx)
            .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
            .collect();
        build_parent_tree(&mut ref_ctxs, params);
        let mut a_ctxs: Vec<MixCtxA> = (0..params.nctx)
            .map(|_| MixCtxA::new(params.cap_fast, params.cap_slow))
            .collect();
        build_parent_tree_a(&mut a_ctxs, params);
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        for &b in stream {
            let c = mix_ctx_key(params, prev, prev2, prev3);
            let ref_vec = ref_ctxs[c].probs_with(&ref_ctxs, params);
            let a_vec = a_ctxs[c].probs_with_a(&a_ctxs, params);
            assert_eq!(a_vec, ref_vec, "{label}: quantized vector 256/256 diverged");
            ref_ctxs[c].step(b);
            share_step(&mut ref_ctxs, params, c, b);
            a_ctxs[c].step_a(b);
            share_step_a(&mut a_ctxs, params, c, b);
            for (a, p) in a_ctxs.iter().zip(&ref_ctxs) {
                a.verify(p);
            }
            prev3 = prev2;
            prev2 = prev;
            prev = b;
        }
    }

    fn wire_parity(params: &MixParams, stream: &[u8], label: &str) {
        let a = mix_encode_with_a(params, stream);
        let p = mix_encode_with(params, stream);
        assert_eq!(a, p, "{label}: C4-A wire diverged from production");
    }

    #[test]
    fn c4a_differential_fixtures() {
        // §4 boundary fixtures; each exercised on all four backends.
        let uniform: Vec<u8> = (0..4u16)
            .flat_map(|i| (i as u8..=200u8).step_by(3).collect::<Vec<u8>>())
            .collect();
        let dominant = vec![17u8; 4096]; // extreme concentration + EWA clamp
        let sparse: Vec<u8> = std::iter::repeat_n(0, 512)
            .chain(std::iter::repeat_n(255, 512))
            .collect();
        let zero_slot: Vec<u8> = std::iter::repeat_n(255, 6000).collect(); // slot 0 empty; remainder+dense interplay
        let slot0_near: Vec<u8> = std::iter::repeat_n(0u8, 3000)
            .chain(std::iter::repeat_n(1u8, 3000))
            .collect(); // slot-0 argmax races
        let thresholds: Vec<u8> = (0..3000u16).map(|i| ((i * 37) & 0xff) as u8).collect(); // fast cap crossings
        let slow_cap: Vec<u8> = vec![9u8; 40000]; // slow cap halvings
        let mut st = 0x9e3779b97f4a7c15u64;
        let random: Vec<u8> = (0..20000).map(|_| xorshift(&mut st)).collect();
        let mut st2 = 0x12345678deadbeefu64;
        let random_run: Vec<u8> = (0..20000).map(|_| xorshift(&mut st2)).collect();
        for (i, params) in ALL_BACKENDS.iter().enumerate() {
            for (label, stream) in [
                ("uniform", &uniform),
                ("dominant", &dominant),
                ("sparse", &sparse),
                ("zero_slot", &zero_slot),
                ("slot0_near", &slot0_near),
                ("thresholds", &thresholds),
                ("slow_cap", &slow_cap),
                ("random_a", &random),
                ("random_b", &random_run),
            ] {
                differential(params, stream, &format!("b{}-{label}", i + 4));
                wire_parity(params, stream, &format!("b{}-{label}", i + 4));
            }
        }
    }
}
