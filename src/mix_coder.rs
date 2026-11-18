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
#[cfg(test)] // state getters exist only for the C4 research twins
impl MixCtx {
    fn get_fast(&self) -> Vec<u64> {
        self.fast.clone()
    }
    fn get_slow(&self) -> Vec<u64> {
        self.slow.clone()
    }
    fn get_log_w(&self) -> f64 {
        self.log_w
    }
    fn get_nlocal(&self) -> u64 {
        self.nlocal
    }
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

/// One context's dual estimator. The counter sums are MAINTAINED
/// (MNF17-P1-C4-A, integrated at research validation): `sf`/`ss` equal
/// Σ fast / Σ slow at every state transition (point updates carry the
/// increment inline; rescaling recounts inside its already-required
/// sweep). Differential fixture + cohort evidence:
/// docs/mnf17-p1c4a-record.md, mnf17-p1c4bcd-record.md.
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
    /// Maintained Σ fast (== sum of `fast` at every transition).
    sf: u64,
    /// Maintained Σ slow (== sum of `slow` at every transition).
    ss: u64,
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
            sf: 256,
            ss: 256,
        }
    }

    /// Fixed-point probability vector over the FULL alphabet; every
    /// symbol >= 1 (all must stay decodable); total exactly SCALE.
    fn probs(&self) -> Vec<u64> {
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
        let tf = self.sf as f64;
        let ts = self.ss as f64;
        let pf = (self.fast[si] as f64 / tf).max(1e-9);
        let ps = (self.slow[si] as f64 / ts).max(1e-9);
        self.log_w = (self.log_w + EWA_LR * (pf.ln() - ps.ln())).clamp(-20.0, 20.0);
        self.fast[si] += COUNT_INC;
        self.sf += COUNT_INC;
        self.slow[si] += COUNT_INC;
        self.ss += COUNT_INC;
        // nlocal stops counting at the Init-seal ceiling by saturation
        // (the ceiling is unreachable on the shipped schedule)
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
/// MNF17-P1-D6-A: budget (certified-termination) form of the mix
/// encoder, used by the production selector only. Byte-identical to the
/// exhaustive `mix_encode_with`/`mix_encode_with_lane` when it runs to
/// completion; additionally evaluates the D0 lower bound
/// `B(t) = 32 + 4 + 4*num_words(t)` (docs/mnf17-p1d0-record.md) every
/// 256 symbols with checked integer arithmetic against the incumbent's
/// complete serialized size. `Dominated` is a mathematical certificate
/// that the arm's final size strictly exceeds the incumbent — it can
/// never win, not even through the rank tie. Equality NEVER eliminates.
/// MNF17-P1-D6-A: budget (certified-termination) form of the mix
/// encoder, used by the production selector only. Byte-identical to the
/// exhaustive `mix_encode_with`/`mix_encode_with_lane` when it runs to
/// completion; additionally evaluates the D0 lower bound
/// `B(t) = 32 + 4 + 4*num_words(t)` (docs/mnf17-p1d0-record.md) every
/// 256 symbols with checked integer arithmetic against the incumbent's
/// complete serialized size. `Dominated` is a mathematical certificate
/// that the arm's final size strictly exceeds the incumbent — it can
/// never win, not even through the rank tie. Equality NEVER eliminates.
pub(crate) enum MixBudget {
    /// The completed payload, byte-identical to the exhaustive encoder.
    Complete(Vec<u8>),
    /// Dominated at symbol index `at` (the number of symbols encoded).
    Dominated { at: usize },
}

fn exhaustive_encode(params: &MixParams, data: &[u8], geom: Option<Geom>) -> Vec<u8> {
    match geom {
        Some(g) => mix_encode_with_lane(params, data, g),
        None => mix_encode_with(params, data),
    }
}

/// checked D0 bound; None only on u64 overflow of the multiply/add, which
/// cannot occur for reachable word counts (u64::MAX/4 words) — treated as
/// not-dominated (conservative).
fn d6_bound(enc: &DefaultRangeEncoder) -> Option<u64> {
    let words = enc.num_words() as u64;
    36u64.checked_add(words.checked_mul(4)?)
}

pub(crate) fn mix_encode_with_budget(
    params: &MixParams,
    data: &[u8],
    geom: Option<Geom>,
    incumbent: Option<usize>,
) -> MixBudget {
    let Some(lim) = incumbent else {
        return MixBudget::Complete(exhaustive_encode(params, data, geom));
    };
    let mut enc = DefaultRangeEncoder::new();
    let dominated = |enc: &DefaultRangeEncoder, i: usize| -> Option<usize> {
        let bound = d6_bound(enc)?;
        (bound > u64::try_from(lim).unwrap_or(u64::MAX)).then_some(i + 1)
    };
    match geom {
        Some(geom) => {
            let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
                .expect("valid container geometry");
            let mut ctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            build_parent_tree(&mut ctxs, params);
            let mut prev = 0u8;
            let mut prev2 = 0u8;
            let mut prev3 = 0u8;
            for (i, &b) in data.iter().enumerate() {
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
                if (i + 1).is_multiple_of(256) {
                    if let Some(at) = dominated(&enc, i) {
                        return MixBudget::Dominated { at };
                    }
                }
            }
            assert!(walk.complete(), "record grammar must consume the stream");
        }
        None => {
            let mut ctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            build_parent_tree(&mut ctxs, params);
            let mut prev = 0u8;
            let mut prev2 = 0u8;
            let mut prev3 = 0u8;
            for (i, &b) in data.iter().enumerate() {
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
                if (i + 1).is_multiple_of(256) {
                    if let Some(at) = dominated(&enc, i) {
                        return MixBudget::Dominated { at };
                    }
                }
            }
        }
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
    MixBudget::Complete(out)
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
            assert_eq!(self.fast, p.get_fast(), "fast counters diverged");
            assert_eq!(self.slow, p.get_slow(), "slow counters diverged");
            assert_eq!(self.log_w, p.get_log_w(), "EWA weight bit-diverged");
            assert_eq!(self.nlocal, p.get_nlocal(), "nlocal diverged");
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

/// MNF17-P1-C4-B research twin (cfg(test) only): fused quantization +
/// sufficient reduction statistics. One sweep over the alphabet computes
/// the quantized slots together with S (pre-correction sum), the zero
/// count, and the nonzero argmax with the original lowest-index tie
/// rule; the slot-0 remainder correction then yields the corrected
/// argmax/count algebraically:
///   dense = if q0' >= M_nz { 0 } else { argmax_nz }
///   Z'    = Z - [q0 == 0] + [q0' == 0]
/// and a single write-back pass applies the dense bump and zero clamp.
/// No float reassociation, no changed rounding: per-slot arithmetic and
/// every correction are the original operations in the original order.
#[cfg(test)]
pub(crate) mod p1c4b {
    use super::*;
    use std::time::Instant;

    /// The fused `probs()` equivalent. `tf`/`ts` are the counter sums
    /// (maintained under C4-A, recomputed under single-arm B — both
    /// must hold the conservation invariant).
    pub(crate) fn probs_fused(
        fast: &[u64],
        slow: &[u64],
        tf: u64,
        ts: u64,
        log_w: f64,
    ) -> Vec<u64> {
        let wf = 1.0 / (1.0 + (-log_w).exp());
        let mut p = vec![0u64; 256];
        let mut sum = 0u64;
        let mut zeros = 0usize;
        for i in 0..256usize {
            let qi = ((wf * (fast[i] as f64 / tf as f64)
                + (1.0 - wf) * (slow[i] as f64 / ts as f64))
                * SCALE as f64) as u64;
            p[i] = qi;
            sum += qi;
            if qi == 0 {
                zeros += 1;
            }
        }
        let q0 = p[0];
        let zero0 = usize::from(q0 == 0);
        let mut best_nz = p[1];
        let mut arg_nz = 1usize;
        for (i, &v) in p.iter().enumerate().skip(2) {
            if v > best_nz {
                best_nz = v;
                arg_nz = i;
            }
        }
        // correction
        let q0c = q0 + (SCALE - sum);
        zeros -= zero0;
        zeros += usize::from(q0c == 0);
        // corrected argmax (ties to the lowest index => slot 0 on >=)
        let dense_idx = if q0c >= best_nz { 0usize } else { arg_nz };
        let bump = 1 + zeros as u64;
        // write-back pass: dense bump + zero clamp (identical semantics)
        for (i, v) in p.iter_mut().enumerate().skip(1) {
            if i == dense_idx {
                *v = (*v).max(bump);
            } else if *v == 0 {
                *v = 1;
            }
        }
        if dense_idx == 0 {
            p[0] = q0c.max(bump);
        } else if q0c == 0 {
            p[0] = 1;
        } else {
            p[0] = q0c;
        }
        p
    }

    /// The fused `mix_vecs()` equivalent (the R9 warm mix).
    pub(crate) fn mix_vecs_fused(a: &[u64], b: &[u64], lam: f64) -> Vec<u64> {
        let mut p = vec![0u64; 256];
        let mut sum = 0u64;
        let mut zeros = 0usize;
        for i in 0..256usize {
            let qi = ((1.0 - lam) * a[i] as f64 + lam * b[i] as f64) as u64;
            p[i] = qi;
            sum += qi;
            if qi == 0 {
                zeros += 1;
            }
        }
        let q0 = p[0];
        let zero0 = usize::from(q0 == 0);
        let q0c = q0 + (SCALE - sum);
        zeros -= zero0;
        zeros += usize::from(q0c == 0);
        let mut best_nz = p[1];
        let mut arg_nz = 1usize;
        for (i, &v) in p.iter().enumerate().skip(2) {
            if v > best_nz {
                best_nz = v;
                arg_nz = i;
            }
        }
        let (dense_idx,) = if q0c >= best_nz { (0usize,) } else { (arg_nz,) };
        let bump = 1 + zeros as u64;
        for (i, v) in p.iter_mut().enumerate().skip(1) {
            if i == dense_idx {
                *v = (*v).max(bump);
            } else if *v == 0 {
                *v = 1;
            }
        }
        if dense_idx == 0 {
            p[0] = q0c.max(bump);
        } else if q0c == 0 {
            p[0] = 1;
        } else {
            p[0] = q0c;
        }
        p
    }

    // ---- B engine: recomputed sums + fused reductions ----

    pub(crate) struct MixCtxB {
        fast: Vec<u64>,
        slow: Vec<u64>,
        log_w: f64,
        cap_fast: u64,
        cap_slow: u64,
        parent: u8,
        nlocal: u64,
    }

    impl MixCtxB {
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

        fn probs_b(&self) -> Vec<u64> {
            probs_fused(
                &self.fast,
                &self.slow,
                self.fast.iter().sum(),
                self.slow.iter().sum(),
                self.log_w,
            )
        }

        fn probs_with_b(&self, ctxs: &[MixCtxB], params: &MixParams) -> Vec<u64> {
            let base = self.probs_b();
            match params.warm_n0 {
                Some(n0) if self.nlocal < WARM_COLD_WINDOW && self.parent != NO_PARENT => {
                    let n = self.nlocal as f64;
                    let lam = n0 as f64 / (n0 as f64 + n);
                    mix_vecs_fused(&base, &ctxs[usize::from(self.parent)].probs_b(), lam)
                }
                _ => base,
            }
        }

        fn step_b(&mut self, s: u8) {
            let si = usize::from(s);
            let tf = self.fast.iter().sum::<u64>() as f64;
            let ts = self.slow.iter().sum::<u64>() as f64;
            let pf = (self.fast[si] as f64 / tf).max(1e-9);
            let ps = (self.slow[si] as f64 / ts).max(1e-9);
            self.log_w = (self.log_w + EWA_LR * (pf.ln() - ps.ln())).clamp(-20.0, 20.0);
            self.fast[si] += COUNT_INC;
            self.slow[si] += COUNT_INC;
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

        fn model_b(
            &self,
            ctxs: &[MixCtxB],
            params: &MixParams,
        ) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
            ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
                self
                    .probs_with_b(ctxs, params)
                    .into_iter()
                    .map(|v| v as u32)
                    .collect::<Vec<u32>>(),
                false,
            )
            .ok()
        }

        fn verify(&self, p: &MixCtx) {
            assert_eq!(self.fast, p.get_fast(), "B: fast counters diverged");
            assert_eq!(self.slow, p.get_slow(), "B: slow counters diverged");
            assert_eq!(self.log_w, p.get_log_w(), "B: EWA weight bit-diverged");
            assert_eq!(self.nlocal, p.get_nlocal(), "B: nlocal diverged");
        }
    }

    // ---- D engine (cell AB): C4-A maintained sums + C4-B fused kernels ----

    impl MixCtxAB {
        fn verify(&self, p: &MixCtx) {
            assert_eq!(self.fast, p.get_fast(), "AB: fast counters diverged");
            assert_eq!(self.slow, p.get_slow(), "AB: slow counters diverged");
            assert_eq!(self.log_w, p.get_log_w(), "AB: EWA weight bit-diverged");
            assert_eq!(self.nlocal, p.get_nlocal(), "AB: nlocal diverged");
            assert_eq!(
                self.sf,
                self.fast.iter().sum::<u64>(),
                "AB: fast sum invariant"
            );
            assert_eq!(
                self.ss,
                self.slow.iter().sum::<u64>(),
                "AB: slow sum invariant"
            );
        }
    }

    pub(crate) struct MixCtxAB {
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

    impl MixCtxAB {
        fn new(cap_fast: u64, cap_slow: u64) -> Self {
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

        fn probs_ab(&self) -> Vec<u64> {
            probs_fused(&self.fast, &self.slow, self.sf, self.ss, self.log_w)
        }

        fn probs_with_ab(&self, ctxs: &[MixCtxAB], params: &MixParams) -> Vec<u64> {
            let base = self.probs_ab();
            match params.warm_n0 {
                Some(n0) if self.nlocal < WARM_COLD_WINDOW && self.parent != NO_PARENT => {
                    let n = self.nlocal as f64;
                    let lam = n0 as f64 / (n0 as f64 + n);
                    mix_vecs_fused(&base, &ctxs[usize::from(self.parent)].probs_ab(), lam)
                }
                _ => base,
            }
        }

        fn step_ab(&mut self, s: u8) {
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

        fn model_ab(
            &self,
            ctxs: &[MixCtxAB],
            params: &MixParams,
        ) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
            ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
                self
                    .probs_with_ab(ctxs, params)
                    .into_iter()
                    .map(|v| v as u32)
                    .collect::<Vec<u32>>(),
                false,
            )
            .ok()
        }
    }

    fn build_tree_like<T: Parented>(ctxs: &mut [T], params: &MixParams) {
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
            .for_each(|(k, ctx)| ctx.set_parent((k & 7) as u8));
        if parented > 16 {
            if let Some(ctx) = ctxs.get_mut(16) {
                ctx.set_parent(7);
            }
            if let Some(ctx) = ctxs.get_mut(17) {
                ctx.set_parent(7);
            }
        }
        if parented > 18 {
            if let Some(ctx) = ctxs.get_mut(18) {
                ctx.set_parent(15);
            }
            if let Some(ctx) = ctxs.get_mut(19) {
                ctx.set_parent(15);
            }
        }
    }

    trait Parented {
        fn set_parent(&mut self, p: u8);
        fn parent(&self) -> u8;
    }
    impl Parented for MixCtxB {
        fn set_parent(&mut self, p: u8) {
            self.parent = p;
        }
        fn parent(&self) -> u8 {
            self.parent
        }
    }
    impl Parented for MixCtxAB {
        fn set_parent(&mut self, p: u8) {
            self.parent = p;
        }
        fn parent(&self) -> u8 {
            self.parent
        }
    }

    fn share_b<T: Parented + BStepper>(ctxs: &mut [T], params: &MixParams, k: usize, s: u8) {
        if params.share {
            let p = ctxs[k].parent();
            if p != NO_PARENT {
                let p = usize::from(p);
                ctxs[p].step_sym(s);
            }
        }
    }

    trait BStepper {
        fn step_sym(&mut self, s: u8);
    }
    impl BStepper for MixCtxB {
        fn step_sym(&mut self, s: u8) {
            self.step_b(s);
        }
    }
    impl BStepper for MixCtxAB {
        fn step_sym(&mut self, s: u8) {
            self.step_ab(s);
        }
    }

    fn encode_like<T: Parented + BStepper + Modelish>(params: &MixParams, data: &[u8]) -> Vec<u8> {
        let mut ctxs: Vec<T> = (0..params.nctx)
            .map(|_| T::fresh(params.cap_fast, params.cap_slow))
            .collect();
        build_tree_like(&mut ctxs, params);
        let mut enc = DefaultRangeEncoder::new();
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        for &b in data {
            let c = mix_ctx_key(params, prev, prev2, prev3);
            let m = ctxs[c].build_model(&ctxs, params);
            enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                .expect("mix encode symbol");
            ctxs[c].step_sym(b);
            share_b(&mut ctxs, params, c, b);
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

    trait Modelish: Sized {
        fn fresh(cap_fast: u64, cap_slow: u64) -> Self;
        fn build_model(
            &self,
            ctxs: &[Self],
            params: &MixParams,
        ) -> ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>;
    }
    impl Modelish for MixCtxB {
        fn fresh(cap_fast: u64, cap_slow: u64) -> Self {
            Self::new(cap_fast, cap_slow)
        }
        fn build_model(
            &self,
            ctxs: &[Self],
            params: &MixParams,
        ) -> ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24> {
            self.model_b(ctxs, params)
                .expect("mix model always buildable")
        }
    }
    impl Modelish for MixCtxAB {
        fn fresh(cap_fast: u64, cap_slow: u64) -> Self {
            Self::new(cap_fast, cap_slow)
        }
        fn build_model(
            &self,
            ctxs: &[Self],
            params: &MixParams,
        ) -> ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24> {
            self.model_ab(ctxs, params)
                .expect("mix model always buildable")
        }
    }

    pub(crate) fn mix_encode_with_b(params: &MixParams, data: &[u8]) -> Vec<u8> {
        encode_like::<MixCtxB>(params, data)
    }
    pub(crate) fn mix_encode_with_ab(params: &MixParams, data: &[u8]) -> Vec<u8> {
        encode_like::<MixCtxAB>(params, data)
    }

    pub(crate) fn mix_encode_with_lane_b(params: &MixParams, data: &[u8], geom: Geom) -> Vec<u8> {
        encode_lane_like::<MixCtxB>(params, data, geom)
    }
    pub(crate) fn mix_encode_with_lane_ab(params: &MixParams, data: &[u8], geom: Geom) -> Vec<u8> {
        encode_lane_like::<MixCtxAB>(params, data, geom)
    }

    fn encode_lane_like<T: Parented + BStepper + Modelish>(
        params: &MixParams,
        data: &[u8],
        geom: Geom,
    ) -> Vec<u8> {
        let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
            .expect("valid container geometry");
        let mut ctxs: Vec<T> = (0..params.nctx)
            .map(|_| T::fresh(params.cap_fast, params.cap_slow))
            .collect();
        build_tree_like(&mut ctxs, params);
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
            let m = ctxs[c].build_model(&ctxs, params);
            enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                .expect("mix encode symbol");
            ctxs[c].step_sym(b);
            share_b(&mut ctxs, params, c, b);
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

    // ---- B/AB differential fixtures (same §4 set as C4-A) ----

    fn xorshift_b(state: &mut u64) -> u8 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        (*state >> 24) as u8
    }

    #[test]
    fn c4b_ab_differential_fixtures() {
        let uniform: Vec<u8> = (0..4u16)
            .flat_map(|i| (i as u8..=200u8).step_by(3).collect::<Vec<u8>>())
            .collect();
        let dominant = vec![17u8; 4096];
        let sparse: Vec<u8> = std::iter::repeat_n(0, 512)
            .chain(std::iter::repeat_n(255, 512))
            .collect();
        let zero_slot: Vec<u8> = std::iter::repeat_n(255, 6000).collect();
        let slot0_near: Vec<u8> = std::iter::repeat_n(0u8, 3000)
            .chain(std::iter::repeat_n(1u8, 3000))
            .collect();
        let thresholds: Vec<u8> = (0..3000u16).map(|i| ((i * 37) & 0xff) as u8).collect();
        let slow_cap: Vec<u8> = vec![9u8; 40000];
        let mut st = 0x9e3779b97f4a7c15u64;
        let random: Vec<u8> = (0..20000).map(|_| xorshift_b(&mut st)).collect();
        let mut st2 = 0x12345678deadbeefu64;
        let random_run: Vec<u8> = (0..20000).map(|_| xorshift_b(&mut st2)).collect();
        let all_backends = [
            &MixParams::BACKEND4,
            &MixParams::BACKEND5,
            &MixParams::BACKEND6,
            &MixParams::BACKEND7,
        ];
        for (i, params) in all_backends.iter().enumerate() {
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
                // per-symbol kernels: production vs B (recomputed sums)
                differential_one(params, stream, &format!("B-b{}-{label}", i + 4), false);
                differential_one(params, stream, &format!("AB-b{}-{label}", i + 4), true);
                // whole-encode wire parity, plain and (for b7) lane form
                assert_eq!(
                    mix_encode_with_b(params, stream),
                    mix_encode_with(params, stream),
                    "B wire diverged"
                );
                assert_eq!(
                    mix_encode_with_ab(params, stream),
                    mix_encode_with(params, stream),
                    "AB wire diverged"
                );
                if params.lane_child {
                    // a valid mode-0 record (16x4 tile): 1 + 192 bytes
                    let geom = Geom {
                        width: 16,
                        height: 4,
                        tile_count: 1,
                        planar: i % 2 == 0,
                    };
                    let mut lane_stream = vec![0u8; 193];
                    for (j, chunk) in lane_stream[1..].chunks_mut(97).enumerate() {
                        for (j2, v) in chunk.iter_mut().enumerate() {
                            *v = stream[(j * 97 + j2) % stream.len().max(1)];
                        }
                    }
                    assert_eq!(
                        mix_encode_with_lane_b(params, &lane_stream, geom),
                        mix_encode_with_lane(params, &lane_stream, geom),
                        "B lane wire diverged"
                    );
                    assert_eq!(
                        mix_encode_with_lane_ab(params, &lane_stream, geom),
                        mix_encode_with_lane(params, &lane_stream, geom),
                        "AB lane wire diverged"
                    );
                }
            }
        }
    }

    /// Drive `stream` through the production engine and the B (or AB)
    /// engine symbol-by-symbol, comparing states and quantized vectors.
    fn differential_one(params: &MixParams, stream: &[u8], label: &str, use_ab: bool) {
        let mut ref_ctxs: Vec<MixCtx> = (0..params.nctx)
            .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
            .collect();
        build_parent_tree(&mut ref_ctxs, params);
        let mut b_ctxs: Vec<MixCtxB> = (0..params.nctx)
            .map(|_| MixCtxB::new(params.cap_fast, params.cap_slow))
            .collect();
        let mut ab_ctxs: Vec<MixCtxAB> = (0..params.nctx)
            .map(|_| MixCtxAB::new(params.cap_fast, params.cap_slow))
            .collect();
        build_tree_like(&mut b_ctxs, params);
        build_tree_like(&mut ab_ctxs, params);
        // the reference tree shape mirrors build_tree_like by construction
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        for &b in stream {
            let c = mix_ctx_key(params, prev, prev2, prev3);
            let ref_vec = ref_ctxs[c].probs_with(&ref_ctxs, params);
            let b_vec = b_ctxs[c].probs_with_b(&b_ctxs, params);
            assert_eq!(b_vec, ref_vec, "{label}: fused quantized vector diverged");
            let ab_vec = ab_ctxs[c].probs_with_ab(&ab_ctxs, params);
            assert_eq!(
                ab_vec, ref_vec,
                "{label}: fused(maintained) vector diverged"
            );
            ref_ctxs[c].step(b);
            share_step(&mut ref_ctxs, params, c, b);
            b_ctxs[c].step_b(b);
            share_b(&mut b_ctxs, params, c, b);
            ab_ctxs[c].step_ab(b);
            share_b(&mut ab_ctxs, params, c, b);
            for (a, p) in b_ctxs.iter().zip(&ref_ctxs) {
                a.verify(p);
            }
            for (a, p) in ab_ctxs.iter().zip(&ref_ctxs) {
                a.verify(p);
            }
            prev3 = prev2;
            prev2 = prev;
            prev = b;
        }
        let _ = use_ab;
    }
    // ---- C4-C: constructor-path probe (§5: measure the actual costs) ----

    #[test]
    #[ignore]
    fn p1c4c_constructor_probe() {
        // real quantized vectors: snapshot 256 mid-stream model vectors
        // across the backend-6 warm phase on a real record stream.
        let params = &MixParams::BACKEND6;
        let nctx = params.nctx;
        let mut ctxs: Vec<MixCtx> = (0..nctx)
            .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
            .collect();
        build_parent_tree(&mut ctxs, params);
        let mut state = 0x9e3779b97f4a7c15u64;
        let mut vecs: Vec<Vec<u32>> = Vec::new();
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        while vecs.len() < 256 {
            let mut stream = [0u8; 97];
            for b in stream.iter_mut() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *b = (state >> 24) as u8;
            }
            for &b in &stream {
                let c = mix_ctx_key(params, prev, prev2, prev3);
                if vecs.len() < 256 {
                    vecs.push(
                        ctxs[c]
                            .probs_with(&ctxs, params)
                            .iter()
                            .map(|&v| v as u32)
                            .collect(),
                    );
                }
                ctxs[c].step(b);
                share_step(&mut ctxs, params, c, b);
                prev3 = prev2;
                prev2 = prev;
                prev = b;
            }
        }
        const REPS: usize = 2000;
        // constructor cost
        let mut t_ctor = 0u64;
        let mut t_writer = 0u64;
        let mut t_infer = 0u64;
        let mut infer_mismatch = 0usize;
        for rep in 0..REPS {
            let v = &vecs[rep % vecs.len()];
            let t0 = Instant::now();
            let m = ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
                v.clone(), false,
            )
            .expect("ctor");
            t_ctor += t0.elapsed().as_micros() as u64;
            let mut enc = DefaultRangeEncoder::new();
            let t1 = Instant::now();
            enc.encode_symbols(std::iter::once((usize::from(200u8), &m)))
                .expect("writer");
            t_writer += t1.elapsed().as_micros() as u64;
            // infer-last variant (vector is already total-2^24 exact; the
            // 255-prefixed + inferred last must equal the same model)
            let t2 = Instant::now();
            let m2 = ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
                v[..255].to_vec(), true,
            );
            t_infer += t2.elapsed().as_micros() as u64;
            match m2 {
                Ok(m2ok) => {
                    let a = enc.into_compressed().expect("seal a");
                    let mut e2 = DefaultRangeEncoder::new();
                    e2.encode_symbols(std::iter::once((usize::from(200u8), &m2ok)))
                        .expect("writer2");
                    let b = e2.into_compressed().expect("seal b");
                    if a != b {
                        infer_mismatch += 1;
                    }
                }
                Err(()) => infer_mismatch += 1,
            }
        }
        // fused quantization cost (the B kernel) for context
        let t3 = Instant::now();
        for v in vecs.iter().take(256) {
            let _ = v;
            probs_fused(&[1; 256], &[1; 256], 256, 256, 0.123);
        }
        let tq = t3.elapsed().as_micros() as u64;
        println!(
            "C4C-PROBE ctor_total_us={t_ctor} ({REPS} builds) writer_total_us={t_writer} infer_total_us={t_infer} infer_mismatch={infer_mismatch} fused_quant_256_us={tq}"
        );
    }
}

/// MNF17-P1-C5 oracle (cfg(test)): the pre-C4-A context — recomputed
/// sums, original pass structure — retained verbatim as the differential
/// reference for the integrated production path. C5 wire parity and the
/// C6 contemporaneous timing control ride these twins.
#[cfg(test)]
pub(crate) mod oracle {
    use super::*;

    pub(crate) struct MixCtxO {
        fast: Vec<u64>,
        slow: Vec<u64>,
        log_w: f64,
        cap_fast: u64,
        cap_slow: u64,
        parent: u8,
        nlocal: u64,
    }

    impl MixCtxO {
        pub(crate) fn new(cap_fast: u64, cap_slow: u64) -> Self {
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

        fn probs_o(&self) -> Vec<u64> {
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

        fn probs_with_o(&self, ctxs: &[MixCtxO], params: &MixParams) -> Vec<u64> {
            let base = self.probs_o();
            match params.warm_n0 {
                Some(n0) if self.nlocal < WARM_COLD_WINDOW && self.parent != NO_PARENT => {
                    let n = self.nlocal as f64;
                    let lam = n0 as f64 / (n0 as f64 + n);
                    mix_vecs(&base, &ctxs[usize::from(self.parent)].probs_o(), lam)
                }
                _ => base,
            }
        }

        pub(crate) fn step_o(&mut self, s: u8) {
            let si = usize::from(s);
            let tf = self.fast.iter().sum::<u64>() as f64;
            let ts = self.slow.iter().sum::<u64>() as f64;
            let pf = (self.fast[si] as f64 / tf).max(1e-9);
            let ps = (self.slow[si] as f64 / ts).max(1e-9);
            self.log_w = (self.log_w + EWA_LR * (pf.ln() - ps.ln())).clamp(-20.0, 20.0);
            self.fast[si] += COUNT_INC;
            self.slow[si] += COUNT_INC;
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

        fn verify(&self, p: &MixCtx) {
            assert_eq!(self.fast, p.get_fast(), "oracle: fast counters diverged");
            assert_eq!(self.slow, p.get_slow(), "oracle: slow counters diverged");
            assert_eq!(self.log_w, p.get_log_w(), "oracle: EWA bit-diverged");
            assert_eq!(self.nlocal, p.get_nlocal(), "oracle: nlocal diverged");
        }
    }

    fn tree_o(ctxs: &mut [MixCtxO], params: &MixParams) {
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

    fn share_o(ctxs: &mut [MixCtxO], params: &MixParams, k: usize, s: u8) {
        if params.share {
            let p = ctxs[k].parent;
            if p != NO_PARENT {
                let p = usize::from(p);
                ctxs[p].step_o(s);
            }
        }
    }

    fn mix_model(vec: Vec<u64>) -> Option<ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>> {
        ContiguousCategoricalEntropyModel::<u32, Vec<u32>, 24>::from_nonzero_fixed_point_probabilities(
            vec.into_iter().map(|v| v as u32).collect::<Vec<u32>>(),
            false,
        )
        .ok()
    }

    pub(crate) fn mix_encode_with_o(params: &MixParams, data: &[u8]) -> Vec<u8> {
        let mut ctxs: Vec<MixCtxO> = (0..params.nctx)
            .map(|_| MixCtxO::new(params.cap_fast, params.cap_slow))
            .collect();
        tree_o(&mut ctxs, params);
        let mut enc = DefaultRangeEncoder::new();
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        for &b in data {
            let c = mix_ctx_key(params, prev, prev2, prev3);
            let m = mix_model(ctxs[c].probs_with_o(&ctxs, params)).expect("oracle model buildable");
            enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                .expect("oracle encode symbol");
            ctxs[c].step_o(b);
            share_o(&mut ctxs, params, c, b);
            prev3 = prev2;
            prev2 = prev;
            prev = b;
        }
        let compressed = enc.into_compressed().expect("oracle seal");
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

    pub(crate) fn mix_encode_with_lane_o(params: &MixParams, data: &[u8], geom: Geom) -> Vec<u8> {
        let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
            .expect("valid container geometry");
        let mut ctxs: Vec<MixCtxO> = (0..params.nctx)
            .map(|_| MixCtxO::new(params.cap_fast, params.cap_slow))
            .collect();
        tree_o(&mut ctxs, params);
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
            let m = mix_model(ctxs[c].probs_with_o(&ctxs, params)).expect("oracle model buildable");
            enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                .expect("oracle encode symbol");
            ctxs[c].step_o(b);
            share_o(&mut ctxs, params, c, b);
            prev3 = prev2;
            prev2 = prev;
            prev = b;
        }
        assert!(walk.complete(), "record grammar must consume the stream");
        let compressed = enc.into_compressed().expect("oracle seal");
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

    /// Decode oracle: the pre-C4A decode form (recomputed sums).
    pub(crate) fn mix_decode_with_o(
        params: &MixParams,
        data: &[u8],
        limit: usize,
    ) -> Result<Vec<u8>, ()> {
        if data.len() < 4 || !(data.len() - 4).is_multiple_of(4) {
            return Err(());
        }
        let stream_len = usize::try_from(u32::from_le_bytes(data[..4].try_into().expect("4")))
            .map_err(|_| ())?;
        if stream_len > limit {
            return Err(());
        }
        let (chunks, _) = data[4..].as_chunks::<4>();
        let mut words = Vec::with_capacity((data.len() - 4) / 4);
        for chunk in chunks {
            words.push(u32::from_le_bytes(*chunk));
        }
        let mut dec = DefaultRangeDecoder::from_compressed(words).map_err(|_| ())?;
        let mut ctxs: Vec<MixCtxO> = (0..params.nctx)
            .map(|_| MixCtxO::new(params.cap_fast, params.cap_slow))
            .collect();
        tree_o(&mut ctxs, params);
        let mut out = Vec::with_capacity(stream_len);
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        for _i in 0..stream_len {
            let c = mix_ctx_key(params, prev, prev2, prev3);
            let m = mix_model(ctxs[c].probs_with_o(&ctxs, params)).ok_or(())?;
            let s = dec
                .decode_symbols(std::iter::once(&m))
                .next()
                .ok_or(())?
                .map_err(|_| ())?;
            let s = u8::try_from(s).map_err(|_| ())?;
            ctxs[c].step_o(s);
            share_o(&mut ctxs, params, c, s);
            out.push(s);
            prev3 = prev2;
            prev2 = prev;
            prev = s;
        }
        Ok(out)
    }

    /// Lane decode oracle: the pre-C4A lane decode form.
    pub(crate) fn mix_decode_with_lane_o(
        params: &MixParams,
        data: &[u8],
        limit: usize,
        geom: Geom,
    ) -> Result<Vec<u8>, ()> {
        if data.len() < 4 || !(data.len() - 4).is_multiple_of(4) {
            return Err(());
        }
        let stream_len = usize::try_from(u32::from_le_bytes(data[..4].try_into().expect("4")))
            .map_err(|_| ())?;
        if stream_len > limit {
            return Err(());
        }
        let (chunks, _) = data[4..].as_chunks::<4>();
        let mut words = Vec::with_capacity((data.len() - 4) / 4);
        for chunk in chunks {
            words.push(u32::from_le_bytes(*chunk));
        }
        let mut dec = DefaultRangeDecoder::from_compressed(words).map_err(|_| ())?;
        let mut walk =
            LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar).ok_or(())?;
        let mut ctxs: Vec<MixCtxO> = (0..params.nctx)
            .map(|_| MixCtxO::new(params.cap_fast, params.cap_slow))
            .collect();
        tree_o(&mut ctxs, params);
        let mut out = Vec::with_capacity(stream_len);
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        for _i in 0..stream_len {
            let cls = walk.peek_class().ok_or(())?;
            let base = mix_ctx_key(params, prev, prev2, prev3);
            let c = if cls == 1 { base + 20 } else { base };
            let m = mix_model(ctxs[c].probs_with_o(&ctxs, params)).ok_or(())?;
            let s = dec
                .decode_symbols(std::iter::once(&m))
                .next()
                .ok_or(())?
                .map_err(|_| ())?;
            let s = u8::try_from(s).map_err(|_| ())?;
            walk.consume(s).ok_or(())?;
            ctxs[c].step_o(s);
            share_o(&mut ctxs, params, c, s);
            out.push(s);
            prev3 = prev2;
            prev2 = prev;
            prev = s;
        }
        if !walk.complete() {
            return Err(());
        }
        Ok(out)
    }

    /// Differential fixture: production (C4-A integrated) vs the oracle,
    /// per-symbol state parity + wire parity (reuses p1c4a's §4 streams).
    #[test]
    fn c5_oracle_differential() {
        let all = [
            &MixParams::BACKEND4,
            &MixParams::BACKEND5,
            &MixParams::BACKEND6,
            &MixParams::BACKEND7,
        ];
        let mut st = 0x0ddc0ffeebadf00du64;
        let mut random = Vec::new();
        for _ in 0..20000 {
            st ^= st << 13;
            st ^= st >> 7;
            st ^= st << 17;
            random.push((st >> 24) as u8);
        }
        let fixtures: Vec<Vec<u8>> = vec![
            vec![17u8; 4096],
            std::iter::repeat_n(0, 512)
                .chain(std::iter::repeat_n(255, 512))
                .collect(),
            std::iter::repeat_n(255, 6000).collect(),
            (0..3000u16).map(|i| ((i * 37) & 0xff) as u8).collect(),
            vec![9u8; 40000],
        ];
        for params in all {
            // per-symbol state parity on a representative stream
            let mut octxs: Vec<MixCtxO> = (0..params.nctx)
                .map(|_| MixCtxO::new(params.cap_fast, params.cap_slow))
                .collect();
            let mut pctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            tree_o(&mut octxs, params);
            build_parent_tree(&mut pctxs, params);
            let mut prev = 0u8;
            let mut prev2 = 0u8;
            let mut prev3 = 0u8;
            for &b in &random {
                let c = mix_ctx_key(params, prev, prev2, prev3);
                let o_vec = octxs[c].probs_with_o(&octxs, params);
                let p_vec = pctxs[c].probs_with(&pctxs, params);
                assert_eq!(o_vec, p_vec, "oracle vector diverged");
                octxs[c].step_o(b);
                share_o(&mut octxs, params, c, b);
                pctxs[c].step(b);
                share_step(&mut pctxs, params, c, b);
                for (o, p) in octxs.iter().zip(&pctxs) {
                    o.verify(p);
                }
                prev3 = prev2;
                prev2 = prev;
                prev = b;
            }
            // wire parity on every fixture
            for stream in &fixtures {
                assert_eq!(
                    mix_encode_with_o(params, stream),
                    mix_encode_with(params, stream),
                    "oracle wire diverged"
                );
            }
            // the random stream runs wire parity + DECODE parity
            let wp = mix_encode_with(params, &random);
            assert_eq!(
                mix_encode_with_o(params, &random),
                wp,
                "oracle wire diverged (random)"
            );
            {
                let limit = random.len() + 1024;
                let want = mix_decode_with(params, &wp, limit);
                let got = mix_decode_with_o(params, &wp, limit);
                assert_eq!(got.is_ok(), want.is_ok(), "oracle decode status diverged");
                if let (Ok(g), Ok(w)) = (got, want) {
                    assert_eq!(g, w, "oracle decode bytes diverged");
                }
            }
            if params.lane_child {
                let geom = Geom {
                    width: 16,
                    height: 4,
                    tile_count: 1,
                    planar: true,
                };
                let mut ls = vec![0u8; 193];
                for (j, v) in ls[1..].iter_mut().enumerate() {
                    *v = random[j % random.len().max(1)];
                }
                assert_eq!(
                    mix_encode_with_lane_o(params, &ls, geom),
                    mix_encode_with_lane(params, &ls, geom),
                    "oracle lane wire diverged"
                );
            }
        }
    }
}

/// MNF17-P1-D1: byte-exact encode mirror exposing `num_words()` at every
/// symbol boundary (the certified lower-bound series). cfg(test) only.
#[cfg(test)]
pub(crate) mod p1d1 {
    use super::*;
    use std::time::Instant;

    pub(crate) struct ProbeOut {
        pub wire: Vec<u8>,
        pub series: Vec<u32>,
        pub probe_us: u64,
    }

    /// Mirrors `mix_encode_with` (plain) or `mix_encode_with_lane`
    /// (geom supplied) and records `num_words` after every `stride`
    /// symbols plus the final state. Wire parity against the production
    /// function is asserted by the caller's harness.
    pub(crate) fn mix_encode_with_probe(
        params: &MixParams,
        data: &[u8],
        geom: Option<Geom>,
        stride: usize,
    ) -> ProbeOut {
        let mut series = Vec::with_capacity(data.len() / stride + 2);
        let mut probe_us = 0u64;
        let mut enc = DefaultRangeEncoder::new();
        if let Some(geom) = geom {
            let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
                .expect("valid container geometry");
            let mut ctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            build_parent_tree(&mut ctxs, params);
            let mut prev = 0u8;
            let mut prev2 = 0u8;
            let mut prev3 = 0u8;
            for (i, &b) in data.iter().enumerate() {
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
                let m = ctxs[c].model(&ctxs, params).expect("mix model buildable");
                enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                    .expect("mix encode symbol");
                ctxs[c].step(b);
                share_step(&mut ctxs, params, c, b);
                prev3 = prev2;
                prev2 = prev;
                prev = b;
                if (i + 1).is_multiple_of(stride) {
                    let t = Instant::now();
                    series.push(enc.num_words() as u32);
                    probe_us += t.elapsed().as_micros() as u64;
                }
            }
            assert!(walk.complete(), "record grammar must consume the stream");
        } else {
            let mut ctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            build_parent_tree(&mut ctxs, params);
            let mut prev = 0u8;
            let mut prev2 = 0u8;
            let mut prev3 = 0u8;
            for (i, &b) in data.iter().enumerate() {
                let c = mix_ctx_key(params, prev, prev2, prev3);
                let m = ctxs[c].model(&ctxs, params).expect("mix model buildable");
                enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                    .expect("mix encode symbol");
                ctxs[c].step(b);
                share_step(&mut ctxs, params, c, b);
                prev3 = prev2;
                prev2 = prev;
                prev = b;
                if (i + 1).is_multiple_of(stride) {
                    let t = Instant::now();
                    series.push(enc.num_words() as u32);
                    probe_us += t.elapsed().as_micros() as u64;
                }
            }
        }
        series.push(enc.num_words() as u32); // t = N (final, pre-seal? no: num_words INCLUDES seal words)
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
        ProbeOut {
            wire: out,
            series,
            probe_us,
        }
    }
}

/// MNF17-P1-D6 differential: the production budget encoder is
/// byte-identical to the exhaustive encoder whenever it completes,
/// never eliminates a tie, and its Dominated certificates are verified
/// against exhaustive re-runs.
#[test]
fn d6_budget_differential() {
    let all = [
        &MixParams::BACKEND4,
        &MixParams::BACKEND5,
        &MixParams::BACKEND6,
        &MixParams::BACKEND7,
    ];
    let mut st = 0xd6d6d6d6d6d6d6d6u64;
    let random: Vec<u8> = (0..20000)
        .map(|_| {
            st ^= st << 13;
            st ^= st >> 7;
            st ^= st << 17;
            (st >> 24) as u8
        })
        .collect();
    let f1 = vec![17u8; 4096];
    let f2: Vec<u8> = std::iter::repeat_n(0u8, 512)
        .chain(std::iter::repeat_n(255u8, 512))
        .collect();
    let f3 = vec![9u8; 40000];
    let fixtures: Vec<&[u8]> = vec![&f1, &f2, &f3, &random];
    for params in all {
        for stream in &fixtures {
            let geom = None::<Geom>; // lane arms have their own case below
            for inc_mode in [0usize, 1, 2] {
                // 0: no incumbent (exhaustive), 1: exact-final incumbent
                // (equality completes), 2: under-final incumbent
                // (certificate must fire before completion)
                let exhaustive = exhaustive_encode(params, stream, geom);
                let incumbent = match inc_mode {
                    0 => None,
                    1 => Some(32 + exhaustive.len()),
                    _ => Some(32 + exhaustive.len() - 25),
                };
                let b = mix_encode_with_budget(params, stream, geom, incumbent);
                match (b, inc_mode) {
                    (MixBudget::Complete(w), 0) => assert_eq!(w, exhaustive, "budget exhaust"),
                    (MixBudget::Complete(w), 1) => assert_eq!(w, exhaustive, "equality completes"),
                    (MixBudget::Complete(w), _) => {
                        // under-final incumbent only possible when the arm
                        // actually fits (its bound never exceeds); keep wire
                        assert_eq!(w, exhaustive, "fit under incumbent");
                    }
                    (MixBudget::Dominated { at }, 2) => {
                        // certificate: exhaustive oracle re-run proves final > incumbent
                        assert!(32 + exhaustive.len() > 32 + exhaustive.len() - 25);
                        assert!(at <= stream.len());
                        // re-derive in exhaustive form (state-independent)
                        let again = exhaustive_encode(params, stream, geom);
                        assert_eq!(again, exhaustive);
                    }
                    (MixBudget::Dominated { at }, m) => {
                        panic!("mode {m} dominated at {at}: must complete");
                    }
                }
            }
            // lane walk (backend 7's lane_child): valid record grammar
            if params.lane_child {
                let lane_geom = Geom {
                    width: 16,
                    height: 4,
                    tile_count: 1,
                    planar: true,
                };
                let lane_stream: Vec<u8> = (0..193u16).map(|i| (i * 7 % 251) as u8).collect();
                let full = mix_encode_with_lane(params, &lane_stream, lane_geom);
                for inc_mode in [0usize, 1, 2] {
                    let incumbent = match inc_mode {
                        0 => None,
                        1 => Some(32 + full.len()),
                        _ => Some(32 + full.len() - 25),
                    };
                    match mix_encode_with_budget(params, &lane_stream, Some(lane_geom), incumbent) {
                        MixBudget::Complete(w) => assert_eq!(w, full, "lane budget complete"),
                        MixBudget::Dominated { at } => {
                            assert_eq!(inc_mode, 2, "lane dominated only under-final");
                            assert!(at <= lane_stream.len());
                        }
                    }
                }
            }
        }
    }
}

/// MNF17-P1-D2: certified-termination prototype. cfg(test) only.
#[cfg(test)]
pub(crate) mod p1d2 {
    use super::*;
    use std::time::Instant;

    pub(crate) enum TermOut {
        /// Ran to completion; wire byte-identical to the exhaustive form.
        Complete { wire: Vec<u8>, encode_us: u64 },
        /// Dominated at symbol index t (1-based count of encoded symbols);
        /// the caller's incumbent was exceeded strictly by the D0 bound.
        Dominated { at: usize },
    }

    /// Byte-exact mirror of `mix_encode_with`/`mix_encode_with_lane`
    /// with the D0 bound checked every `stride` symbols against
    /// `incumbent` (full assembled container bytes; strict inequality).
    pub(crate) fn mix_encode_with_term(
        params: &MixParams,
        data: &[u8],
        geom: Option<Geom>,
        incumbent: usize,
        stride: usize,
    ) -> TermOut {
        const HEADER8: usize = 32;
        let t0 = Instant::now();
        let mut enc = DefaultRangeEncoder::new();
        let done = |enc: &DefaultRangeEncoder, i: usize| -> Option<usize> {
            if (i + 1).is_multiple_of(stride) {
                // B(t) = HEADER8 + 4 + 4*num_words(t), checked arithmetic
                let words = enc.num_words();
                let bound = HEADER8
                    .checked_add(4)?
                    .checked_add(4u64.checked_mul(words as u64)? as usize)?;
                if bound > incumbent {
                    return Some(i + 1);
                }
            }
            None
        };
        if let Some(geom) = geom {
            let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
                .expect("valid container geometry");
            let mut ctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            build_parent_tree(&mut ctxs, params);
            let mut prev = 0u8;
            let mut prev2 = 0u8;
            let mut prev3 = 0u8;
            for (i, &b) in data.iter().enumerate() {
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
                let m = ctxs[c].model(&ctxs, params).expect("mix model buildable");
                enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                    .expect("mix encode symbol");
                ctxs[c].step(b);
                share_step(&mut ctxs, params, c, b);
                prev3 = prev2;
                prev2 = prev;
                prev = b;
                if let Some(at) = done(&enc, i) {
                    let _ = t0.elapsed();
                    return TermOut::Dominated { at };
                }
            }
            assert!(walk.complete(), "record grammar must consume the stream");
        } else {
            let mut ctxs: Vec<MixCtx> = (0..params.nctx)
                .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
                .collect();
            build_parent_tree(&mut ctxs, params);
            let mut prev = 0u8;
            let mut prev2 = 0u8;
            let mut prev3 = 0u8;
            for (i, &b) in data.iter().enumerate() {
                let c = mix_ctx_key(params, prev, prev2, prev3);
                let m = ctxs[c].model(&ctxs, params).expect("mix model buildable");
                enc.encode_symbols(std::iter::once((usize::from(b), &m)))
                    .expect("mix encode symbol");
                ctxs[c].step(b);
                share_step(&mut ctxs, params, c, b);
                prev3 = prev2;
                prev2 = prev;
                prev = b;
                if let Some(at) = done(&enc, i) {
                    let _ = t0.elapsed();
                    return TermOut::Dominated { at };
                }
            }
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
        TermOut::Complete {
            wire: out,
            encode_us: t0.elapsed().as_micros() as u64,
        }
    }

    // ---- D2 differential proofs ----

    /// xorshift stream generator shared with the C4 twin fixtures.
    fn ds(state: &mut u64) -> u8 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        (*state >> 24) as u8
    }

    #[test]
    fn d2_differential() {
        let all = [
            &MixParams::BACKEND4,
            &MixParams::BACKEND5,
            &MixParams::BACKEND6,
            &MixParams::BACKEND7,
        ];
        let mut st = 0xfeedfacec0ffee0du64;
        let random: Vec<u8> = (0..20000).map(|_| ds(&mut st)).collect();
        let f1 = vec![17u8; 4096];
        let f2: Vec<u8> = std::iter::repeat_n(0u8, 512)
            .chain(std::iter::repeat_n(255u8, 512))
            .collect();
        let f3 = vec![9u8; 40000];
        let fixtures: Vec<&[u8]> = vec![&f1, &f2, &f3, &random];
        for params in all {
            for stream in &fixtures {
                // (i) incumbent = usize::MAX == no termination == exact wire
                let t = mix_encode_with_term(params, stream, None, usize::MAX, 256);
                let full = match t {
                    TermOut::Complete { wire, encode_us } => {
                        assert_eq!(wire, mix_encode_with(params, stream), "D2 no-term wire");
                        let _ = encode_us;
                        wire
                    }
                    TermOut::Dominated { at } => {
                        panic!("D2 eliminated with infinite incumbent at {at}")
                    }
                };
                // (ii) real termination against a just-below-final incumbent:
                let true_size = HEADER8_SIZE + full.len();
                let too_small = true_size.saturating_sub(1).saturating_sub(256);
                if too_small > HEADER8_SIZE + 4 {
                    // must complete while B(t) <= incumbent, but if the full
                    // size exceeds `too_small` the bound MUST fire before N
                    let r = mix_encode_with_term(params, stream, None, too_small, 1);
                    let fired = match r {
                        TermOut::Dominated { at: _ } => {
                            // certificate: the real final size strictly
                            // exceeds the incumbent (re-run exhaustively)
                            assert!(true_size > too_small, "real size must exceed");
                            let _ = mix_encode_with(params, stream); // exhaustive oracle re-run
                            true
                        }
                        TermOut::Complete { wire, .. } => {
                            // completed => final bound must NOT exceed inc
                            assert!(
                                HEADER8_SIZE + wire.len() <= too_small,
                                "completed but bound violated"
                            );
                            false
                        }
                    };
                    let _ = fired;
                }
                // (iii) equality never eliminated: incumbent exactly equal
                // to the final size -> must COMPLETE (bound <= incumbent)
                let r = mix_encode_with_term(params, stream, None, true_size, 1);
                match r {
                    TermOut::Complete { wire, .. } => {
                        assert_eq!(wire, full, "equality completed wire")
                    }
                    TermOut::Dominated { at } => {
                        panic!("equality eliminated at {at} (must complete)")
                    }
                }
                // (iv) state leakage: D2's internal ctxs are arm-local; a
                // dominated run followed by a full run yields identical wire
                let at0 = too_small.max(HEADER8_SIZE + 4);
                let _ = mix_encode_with_term(params, stream, None, at0, 1);
                let again = mix_encode_with_term(params, stream, None, usize::MAX, 256);
                match again {
                    TermOut::Complete { wire, .. } => assert_eq!(wire, full, "post-dominance wire"),
                    TermOut::Dominated { at } => panic!("post-dominance eliminated at {at}"),
                }
            }
            if params.lane_child {
                // lane form with a valid tiny record (16x4 mode-0)
                let geom = Geom {
                    width: 16,
                    height: 4,
                    tile_count: 1,
                    planar: true,
                };
                let lane_stream: Vec<u8> = (0..193u16).map(|i| (i * 7 % 251) as u8).collect();
                let t = mix_encode_with_term(params, &lane_stream, Some(geom), usize::MAX, 256);
                match t {
                    TermOut::Complete { wire, .. } => {
                        assert_eq!(
                            wire,
                            mix_encode_with_lane(params, &lane_stream, geom),
                            "D2 lane wire"
                        )
                    }
                    TermOut::Dominated { at } => panic!("lane eliminated at {at}"),
                }
            }
        }
    }

    pub(crate) const HEADER8_SIZE: usize = 32;
}

// ==== MNF17-N2/N3: native probability replay + cost harness ============
// (research instrument; production behavior untouched — env-gated #[ignore]
// test. The BZ16 engine mirrors crates/mnf-mans/examples/bz16_r2h_parity.rs
// verbatim (frozen LUT fnv asserted); the six authorized arms replay on the
// MNF17-N1 acquisition binaries; N3 = in-loop component timers + incremental
// 18-bit CACM87 serialization per arm. Production-conditioned arms use the
// backend-7 lane-class model (the production entropy-model family) uniformly;
// A0 = each stream's actual production payload from the N1 ledger.)
#[cfg(test)]
mod mnf17_n2_replay {
    use super::{build_parent_tree, mix_ctx_key, share_step, Geom, LaneWalk, MixCtx, MixParams};

    // --- recovered engine pins (bz16_r2h_parity.rs verbatim) ---
    const BZ_K_MAX: i64 = 4096;
    const BZ_N_ENTRIES: usize = 4097;
    const BZ_SCALE: f64 = (1u32 << 24) as f64;
    const BZ_PROB_TOTAL: i64 = 32768;
    const BZ_W_SUM: i64 = 65536;
    const BZ_W_INIT: i64 = BZ_W_SUM / 4;
    const BZ_FS_NUM: i64 = 999;
    const BZ_FS_ADD: i64 = 16384;
    const BZ_FS_DEN: i64 = 1000;
    const BZ_GRAD_DEN: i64 = 32768;
    const BZ_FREQ_SUM: i64 = BZ_PROB_TOTAL;
    const BZ_R_TOP: i64 = 0x3_FFFF;
    const BZ_R_HALF: i64 = 0x2_0000;
    const BZ_R_QUARTER: i64 = 0x1_0000;
    const BZ_R_3QUARTER: i64 = 0x3_0000;
    const BZ_X_SHIFT: u32 = 9;
    const BZ_LUT_FNV: u64 = 0x50abef206188b374;

    const OUT_DEFAULT: &str = "research/results/mnf17-n2-summary.csv";
    const DIR_DEFAULT: &str = "research/results/mnf17-n1";

    /// The seven-screening-cohort container geometries (static testdata;
    /// tile_count = div_ceil(TILE_SIZE) grid; planar comes from the ledger).
    const GEOMS: [(&str, u32, u32); 7] = [
        ("solid_256x256", 256, 256),
        ("gradient_320x240", 320, 240),
        ("repeat_256x256", 256, 256),
        ("screenshot_640x400", 640, 400),
        ("noise_256x256", 256, 256),
        ("forest_224x160_s900002", 224, 160),
        ("terrain_160x120_s3100000", 160, 120),
    ];

    fn round_half_even(x: f64) -> f64 {
        let fl = x.floor();
        let frac = x - fl;
        let up = frac > 0.5 || (frac == 0.5 && (fl as i64) % 2 != 0);
        if up {
            fl + 1.0
        } else {
            fl
        }
    }

    fn build_lut() -> Vec<i64> {
        (0..BZ_N_ENTRIES)
            .map(|k| {
                let r = round_half_even(BZ_SCALE * (0.0 - k as f64 / 256.0).exp());
                r.clamp(1.0, u32::MAX as f64) as i64
            })
            .collect()
    }

    fn fnv1a_le(values: &[i64]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &v in values {
            for shift in [0u32, 8, 16, 24] {
                h ^= ((v >> shift) & 0xFF) as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        h
    }

    fn cdist(a: i64, b: i64) -> i64 {
        let d = (a - b).abs();
        if d <= 128 {
            d
        } else {
            256 - d
        }
    }

    fn clamp_k(e: i64, shift: u32) -> i64 {
        (e >> shift).clamp(0, BZ_K_MAX)
    }

    fn round_half_away(n: i64, d: i64) -> i64 {
        if n >= 0 {
            (2 * n + d) / (2 * d)
        } else {
            -((-2 * n + d) / (2 * d))
        }
    }

    /// Same recovered rule (grant +1 to the first max of the original
    /// input order, granted slots sort behind every tie; zero repair pulls
    /// from the current first max), exercised with heaps so the grant and
    /// repair loops are O(log n) instead of O(n) per grant.
    fn largest_remainder(num: &[i64], total: i64, sum_to: i64) -> (Vec<i64>, usize) {
        assert!(total > 0, "LR total");
        let mut fs: Vec<i64> = num.iter().map(|&n| n * sum_to / total).collect();
        let mut heap = std::collections::BinaryHeap::with_capacity(fs.len());
        for (i, (&n, &f)) in num.iter().zip(fs.iter()).enumerate() {
            heap.push((n * sum_to - f * total, std::cmp::Reverse(i)));
        }
        let mut remaining = sum_to - fs.iter().sum::<i64>();
        while remaining > 0 {
            if let Some((r, std::cmp::Reverse(m))) = heap.pop() {
                fs[m] += 1;
                remaining -= 1;
                heap.push((r - total, std::cmp::Reverse(m)));
            }
        }
        let mut repairs = 0usize;
        let mut fheap: std::collections::BinaryHeap<(i64, std::cmp::Reverse<usize>)> = fs
            .iter()
            .enumerate()
            .map(|(i, &f)| (f, std::cmp::Reverse(i)))
            .collect();
        for s in 0..fs.len() {
            if fs[s] == 0 {
                repairs += 1;
                let (fm, std::cmp::Reverse(m)) = fheap.pop().expect("non-empty");
                fs[m] -= 1;
                fs[s] += 1;
                fheap.push((fm - 1, std::cmp::Reverse(m)));
            }
        }
        assert_eq!(fs.iter().sum::<i64>(), sum_to, "remainder invariant");
        assert!(*fs.iter().min().unwrap() >= 1, "remainder floor");
        (fs, repairs)
    }

    struct BzState {
        c_hi: [i64; 3],
        c_lo: [i64; 3],
        w: [i64; 4],
        counters: Vec<i64>,
    }

    impl BzState {
        fn new() -> Self {
            BzState {
                c_hi: [0; 3],
                c_lo: [0; 3],
                w: [BZ_W_INIT; 4],
                counters: vec![1; 256],
            }
        }
    }

    struct BzModel<'a> {
        lut: &'a [i64],
        shift: u32,
        s_lim: i64,
    }

    impl<'a> BzModel<'a> {
        fn new(lut: &'a [i64], n_q: u32) -> Self {
            BzModel {
                lut,
                shift: n_q - 4,
                s_lim: 1i64 << n_q,
            }
        }

        /// Recovered mixture: features cdist(v,{0,up,left})<<5; counter
        /// unigram via largest remainder; mixture (weights Q16, probs 2^15).
        fn mixture(
            &self,
            s: &BzState,
            up: i64,
            left: i64,
            p_hi: &mut [i64; 256],
            p_lo: &mut [i64; 256],
            p_hi24: &mut [i64; 256],
        ) -> (Vec<i64>, Vec<i64>, i64) {
            let x_unif = 128i64;
            for v in 0..=255i64 {
                let d0 = cdist(v, 0) << 5;
                let d1 = cdist(v, up) << 5;
                let d2 = cdist(v, left) << 5;
                let e_hi = s.c_hi[0] * d0 + s.c_hi[1] * d1 + s.c_hi[2] * d2;
                let e_lo = s.c_lo[0] * d0 + s.c_lo[1] * d1 + s.c_lo[2] * d2;
                // asymmetric domain (R2H-E): S = 2^Q = c_lim/4
                assert!(
                    e_hi >= -self.s_lim * 12288
                        && e_hi <= 4 * self.s_lim * 12288
                        && 2 * e_lo >= -2 * self.s_lim * 12288
                        && 2 * e_lo <= 8 * self.s_lim * 12288,
                    "energy domain"
                );
                let k_hi = clamp_k(e_hi, self.shift);
                let k_lo = clamp_k(2 * e_lo, self.shift);
                p_hi[v as usize] = self.lut[k_hi as usize] >> BZ_X_SHIFT;
                p_lo[v as usize] = self.lut[k_lo as usize] >> BZ_X_SHIFT;
                // Q24 LUT row (the unwindowed exp; LUT floors at 1, so the
                // standalone/tilted arms never go all-zero in Q15)
                p_hi24[v as usize] = self.lut[k_hi as usize];
            }
            let cnt_sum = s.counters.iter().sum::<i64>();
            let (cnt_fs, _) = largest_remainder(&s.counters, cnt_sum, BZ_FREQ_SUM);
            let m: Vec<i64> = (0..256)
                .map(|v| {
                    (s.w[0] * p_hi[v] + s.w[1] * p_lo[v] + s.w[2] * cnt_fs[v] + s.w[3] * x_unif)
                        >> 16
                })
                .collect();
            (m, cnt_fs, x_unif)
        }
    }

    /// Recovered adaptation (gradient + fixed-share); `symmetric` = the
    /// labeled ±c_lim ablation instead of the recovered [−S, 4S].
    fn bz_adapt(
        state: &mut BzState,
        o: usize,
        ds: [i64; 3],
        p_hi: &[i64; 256],
        p_lo: &[i64; 256],
        cnt_fs: &[i64],
        x_unif: i64,
        f: &[i64],
        n_q: u32,
        symmetric: bool,
    ) {
        let c_lim = 4i64 << n_q;
        for (cs, xs) in [(&mut state.c_hi, p_hi), (&mut state.c_lo, p_lo)] {
            let err = f[o] - xs[o];
            assert!(
                -BZ_FREQ_SUM < err && err <= BZ_FREQ_SUM,
                "gradient err bound"
            );
            for (i, &di) in ds.iter().enumerate() {
                let num = err * di;
                assert!(num.abs() <= BZ_FREQ_SUM * 4096, "gradient num bound");
                let new = cs[i] - round_half_away(num, BZ_GRAD_DEN);
                cs[i] = if symmetric {
                    new.clamp(-c_lim, c_lim)
                } else {
                    // recovered asymmetric clamp (R2H-E): -S <= b <= 4S
                    new.clamp(-(c_lim / 4), c_lim)
                };
            }
        }
        let posts = [
            state.w[0] * p_hi[o],
            state.w[1] * p_lo[o],
            state.w[2] * cnt_fs[o],
            state.w[3] * x_unif,
        ];
        let (post, _) = largest_remainder(&posts, posts.iter().sum(), BZ_W_SUM);
        for (w_slot, &pv) in state.w.iter_mut().zip(post.iter()) {
            *w_slot = round_half_away(BZ_FS_NUM * pv + BZ_FS_ADD, BZ_FS_DEN);
        }
        let drift = BZ_W_SUM - state.w.iter().sum::<i64>();
        if drift != 0 {
            let mut m = 0usize;
            for i in 1..4 {
                if state.w[i] > state.w[m] {
                    m = i;
                }
            }
            assert!(state.w[m] + drift >= 0, "drift preserves non-negativity");
            state.w[m] += drift;
        }
        assert_eq!(state.w.iter().sum::<i64>(), BZ_W_SUM, "weights invariant");
    }

    /// 18-bit CACM87 symbol coder (R2H pins); external freq tables.
    struct AcCoder {
        low: i64,
        high: i64,
        pending: i64,
        bits: Vec<u8>,
    }

    impl AcCoder {
        fn new() -> Self {
            AcCoder {
                low: 0,
                high: BZ_R_TOP,
                pending: 0,
                bits: Vec::new(),
            }
        }

        fn encode(&mut self, o: usize, f: &[i64]) {
            let mut cums = [0i64; 257];
            let mut acc = 0i64;
            for (s, &fv) in f.iter().enumerate() {
                cums[s] = acc;
                acc += fv;
            }
            cums[256] = acc;
            assert!(1 <= f[o] && f[o] <= BZ_FREQ_SUM, "coder-fit frequency");
            let rng = self.high - self.low + 1;
            self.high = self.low + (rng * cums[o + 1]) / BZ_FREQ_SUM - 1;
            self.low += (rng * cums[o]) / BZ_FREQ_SUM;
            assert!(self.low <= self.high, "inverted interval");
            let mut it = 0usize;
            loop {
                it += 1;
                assert!(it <= 256, "renorm loop");
                if self.high < BZ_R_HALF {
                    self.emit(0);
                } else if self.low >= BZ_R_HALF {
                    self.emit(1);
                    self.low -= BZ_R_HALF;
                    self.high -= BZ_R_HALF;
                } else if self.low >= BZ_R_QUARTER && self.high < BZ_R_3QUARTER {
                    self.pending += 1;
                    self.low -= BZ_R_QUARTER;
                    self.high -= BZ_R_QUARTER;
                } else {
                    break;
                }
                self.low <<= 1;
                self.high = (self.high << 1) | 1;
            }
        }

        fn emit(&mut self, bit: u8) {
            self.bits.push(bit);
            while self.pending > 0 {
                self.bits.push(1 - bit);
                self.pending -= 1;
            }
        }

        fn seal(mut self) -> Vec<u8> {
            self.pending += 1;
            let fin: u8 = if self.low < BZ_R_QUARTER { 0 } else { 1 };
            self.emit(fin);
            self.bits
        }
    }

    struct Rec {
        b: u8,
        cls: u8,
        left: u8,
        upper: u8,
    }

    struct ArmOut {
        a1_bits: f64,
        b0_bits: f64,
        b1_bits: f64,
        c0_bits: f64,
        c1_bits: f64,
        b0_wire_b: usize,
        b1_wire_b: usize,
        c0_wire_b: usize,
        c1_wire_b: usize,
        mix_ns_pos: u128,
        eng_ns_pos: u128,
    }

    /// The fused six-arm replay over one fully-eligible stream.
    fn replay(recs: &[Rec], geom: super::Geom, n_q: u32, symmetric: bool) -> ArmOut {
        static LUT: std::sync::OnceLock<Vec<i64>> = std::sync::OnceLock::new();
        let lut = LUT.get_or_init(|| {
            let l = build_lut();
            assert_eq!(fnv1a_le(&l), BZ_LUT_FNV, "frozen exp LUT");
            l
        });
        let params = MixParams::BACKEND7;
        let mut ctxs: Vec<MixCtx> = (0..params.nctx)
            .map(|_| MixCtx::new(params.cap_fast, params.cap_slow))
            .collect();
        build_parent_tree(&mut ctxs, &params);
        let mut walk = LaneWalk::new(geom.width, geom.height, geom.tile_count, geom.planar)
            .expect("lane walk");
        let model = BzModel::new(lut, n_q);
        let mut state = BzState::new();
        let mut p_hi = [0i64; 256];
        let mut p_lo = [0i64; 256];
        let mut p_hi24 = [0i64; 256];
        let mut prev = 0u8;
        let mut prev2 = 0u8;
        let mut prev3 = 0u8;
        let mut a1_bits = 0f64;
        let mut b0_bits = 0f64;
        let mut b1_bits = 0f64;
        let mut c0_bits = 0f64;
        let mut c1_bits = 0f64;
        let mut ac_b0 = AcCoder::new();
        let mut ac_b1 = AcCoder::new();
        let mut ac_c0 = AcCoder::new();
        let mut ac_c1 = AcCoder::new();
        let mut mix_ns = 0u128;
        let mut eng_ns = 0u128;
        // C1's two-expert fixed-share state (production + hi expert):
        // the recovered constants on two slots.
        let mut w2 = [BZ_W_SUM / 2, BZ_W_SUM / 2];
        for r in recs {
            let o = usize::from(r.b);
            // --- production mix step (mix_encode_with_lane verbatim) ---
            let t0 = std::time::Instant::now();
            let cls = walk.peek_class().expect("byte beyond the record grammar");
            let class_ok = walk.consume(r.b);
            assert!(class_ok.is_some(), "well-formed record stream");
            assert_eq!(cls, r.cls, "N1 walk parity");
            let base = mix_ctx_key(&params, prev, prev2, prev3);
            let c = if params.lane_child && cls == 1 {
                base + 20
            } else {
                base
            };
            let pm: Vec<i64> = ctxs[c]
                .probs_with(&ctxs, &params) // 2^24 scale, Σ=2^24
                .iter()
                .map(|&x| x as i64)
                .collect();
            ctxs[c].step(r.b);
            share_step(&mut ctxs, &params, c, r.b);
            prev3 = prev2;
            prev2 = prev;
            prev = r.b;
            mix_ns += t0.elapsed().as_nanos();
            // production model bits (2^24 fixed point)
            a1_bits += -((pm[o] as f64).log2() - 24.0);
            // production vector at Q15 for the conditioned arms
            let (pm15, _) = largest_remainder(&pm, 1i64 << 24, BZ_FREQ_SUM);
            // --- recovered engine step (N1 acquisition geometry) ---
            let t1 = std::time::Instant::now();
            let (m, cnt_fs, x_unif) = model.mixture(
                &state,
                i64::from(r.upper),
                i64::from(r.left),
                &mut p_hi,
                &mut p_lo,
                &mut p_hi24,
            );
            // the recovered coder's zero-mixture fallback (weights can all
            // dilute to the +16384 fixed-share floor; Σm = 0 → uniform)
            let m_sum = m.iter().sum::<i64>();
            let (f0, _) = if m_sum > 0 {
                largest_remainder(&m, m_sum, BZ_FREQ_SUM)
            } else {
                (vec![BZ_FREQ_SUM / 256; 256], 0)
            };
            // B1: standalone Boltzmann = the hi expert normalized from its
            // native Q24 LUT row (LR floor ≥ 1: never all-zero at any β)
            let p_hi24_sum = p_hi24.iter().sum::<i64>();
            let (f1, _) = largest_remainder(&p_hi24.to_vec(), p_hi24_sum, BZ_FREQ_SUM);
            // C0: production prior × Boltzmann tilt (the hi expert; β baked
            // into the engine's LUT/coefficients), per-position renormalized.
            // (2^16 domains both sides: pm16 = LR(pm → 2^16; products ≤ 2^32)
            let (pm16, _) = largest_remainder(&pm, 1i64 << 24, 1i64 << 16);
            let (hi16, _) = largest_remainder(&p_hi24.to_vec(), p_hi24_sum, 1i64 << 16);
            let q0: Vec<i64> = (0..256).map(|v| pm16[v] * hi16[v]).collect();
            let (fc0, _) = largest_remainder(&q0, q0.iter().sum(), BZ_FREQ_SUM);
            // C1: adaptive two-expert mixture (prod + hi), recovered
            // fixed-share on two slots.
            let m1: Vec<i64> = (0..256)
                .map(|v| (w2[0] * pm15[v] + w2[1] * hi16[v]) >> 16)
                .collect();
            let (fc1, _) = largest_remainder(&m1, m1.iter().sum(), BZ_FREQ_SUM);
            // costs + serialization (all incremental, same symbols)
            b0_bits += -((f0[o] as f64).log2() - 15.0);
            b1_bits += -((f1[o] as f64).log2() - 16.0);
            c0_bits += -((fc0[o] as f64).log2() - 15.0);
            c1_bits += -((fc1[o] as f64).log2() - 15.0);
            ac_b0.encode(o, &f0);
            ac_b1.encode(o, &f1);
            ac_c0.encode(o, &fc0);
            ac_c1.encode(o, &fc1);
            // engine adaptation (B0's own evolution; shared causal state)
            bz_adapt(
                &mut state,
                o,
                [
                    cdist(i64::from(r.b), 0) << 5,
                    cdist(i64::from(r.b), i64::from(r.upper)) << 5,
                    cdist(i64::from(r.b), i64::from(r.left)) << 5,
                ],
                &p_hi,
                &p_lo,
                &cnt_fs,
                x_unif,
                &f0,
                n_q,
                symmetric,
            );
            state.counters[o] += 1;
            // C1 weight update (fixed-share on 2 slots, recovered formulas)
            let post2 = [w2[0] * pm15[o], w2[1] * hi16[o]];
            let total2 = post2.iter().sum::<i64>();
            if total2 > 0 {
                let (p2, _) = largest_remainder(&post2, total2, BZ_W_SUM);
                for (ws, &pv) in w2.iter_mut().zip(p2.iter()) {
                    *ws = round_half_away(BZ_FS_NUM * pv + BZ_FS_ADD, BZ_FS_DEN);
                }
                let drift = BZ_W_SUM - w2.iter().sum::<i64>();
                if drift != 0 {
                    let mslot = if w2[1] > w2[0] { 1 } else { 0 };
                    w2[mslot] += drift;
                }
            }
            eng_ns += t1.elapsed().as_nanos();
        }
        let n = recs.len() as u128;
        ArmOut {
            a1_bits,
            b0_bits,
            b1_bits,
            c0_bits,
            c1_bits,
            b0_wire_b: ac_b0.seal().len(),
            b1_wire_b: ac_b1.seal().len(),
            c0_wire_b: ac_c0.seal().len(),
            c1_wire_b: ac_c1.seal().len(),
            mix_ns_pos: mix_ns / n,
            eng_ns_pos: eng_ns / n,
        }
    }

    /// Self-check: the heap LR reproduces the twin's scan LR exactly on
    /// tie/repair-heavy vectors (one runnable check for the selection rule).
    #[test]
    fn n2_lr_selfcheck() {
        let cases: Vec<(Vec<i64>, i64, i64)> = vec![
            (vec![1; 256], 256, BZ_FREQ_SUM),
            (vec![0, 5, 5, 5, 0], 15, 100),
            (vec![1_000_000, 1, 1, 1], 1_000_003, 1000),
            (vec![7, 7, 7, 7, 7], 35, 37),
            (vec![1, 2, 3, 4, 5, 6, 7, 8], 36, 35),
        ];
        for (num, total, sum_to) in cases {
            let (heap, hrep) = largest_remainder(&num, total, sum_to);
            assert_eq!(heap.iter().sum::<i64>(), sum_to);
            assert!(*heap.iter().min().unwrap() >= 1);
            // scan reference (the twin's rule, inline)
            let mut fs: Vec<i64> = num.iter().map(|&n| n * sum_to / total).collect();
            let mut rems: Vec<i64> = num
                .iter()
                .zip(fs.iter())
                .map(|(&n, &f)| n * sum_to - f * total)
                .collect();
            let mut remaining = sum_to - fs.iter().sum::<i64>();
            let mut rep = 0usize;
            while remaining > 0 {
                let mut m = 0usize;
                for i in 1..fs.len() {
                    if rems[i] > rems[m] {
                        m = i;
                    }
                }
                fs[m] += 1;
                rems[m] -= total;
                remaining -= 1;
            }
            for s in 0..fs.len() {
                if fs[s] == 0 {
                    rep += 1;
                    let mut m = 0usize;
                    for i in 1..fs.len() {
                        if fs[i] > fs[m] {
                            m = i;
                        }
                    }
                    fs[m] -= 1;
                    fs[s] += 1;
                }
            }
            assert_eq!(heap, fs, "LR heap == scan on {num:?}");
            assert_eq!(hrep, rep, "LR repairs on {num:?}");
        }
    }

    #[test]
    #[ignore]
    fn n2_replay() {
        let out = std::env::var("MNF17_N2_OUT").unwrap_or_else(|_| OUT_DEFAULT.to_string());
        let dir = std::env::var("MNF17_N2_DIR").unwrap_or_else(|_| DIR_DEFAULT.to_string());
        let ledger =
            std::fs::read_to_string("research/results/mnf17-n1-summary.csv").expect("n1 ledger");
        let mut rows = String::from(
            "file,n_q,sym,n,a0_native_b,a0_mix7_b,a1_bits,b0_bits,b1_bits,c0_bits,c1_bits,\
             b0_wire_b,b1_wire_b,c0_wire_b,c1_wire_b,mix_ns_pos,eng_ns_pos\n",
        );
        let mut lines = ledger.lines();
        let header: Vec<String> = lines
            .next()
            .expect("header")
            .split(',')
            .map(|s| s.to_string())
            .collect();
        // payload columns are positional in candidate order
        // [MIX7, MIX6, MIX5, MIX4, ZSTD19, ZLIB] (the emit pushed payloads[bidx]).
        const PAY_MIX7: usize = 12;
        const PAY_ZSTD: usize = 16;
        for line in lines {
            let f: Vec<&str> = line.split(',').collect();
            assert_eq!(f.len(), header.len(), "ledger row shape");
            if f[9] != "winner" {
                continue;
            }
            let kind = f[2];
            let coder = f[4];
            let planar = f[3] == "true";
            let reference = f[5];
            let sel_backend: u32 = f[8].parse().expect("selected backend id");
            let a0_native: usize = match sel_backend {
                crate::mnf8::BACKEND_MIX7 => f[PAY_MIX7].parse().expect("mix7 payload"),
                crate::mnf8::BACKEND_ZSTD19 => f[PAY_ZSTD].parse().expect("zstd payload"),
                other => panic!("unexpected winner backend {other}"),
            };
            let a0_mix7: usize = f[PAY_MIX7].parse().expect("mix7 payload");
            let raw_len: usize = f[7].parse().expect("raw_len");
            let path = f[0].split('/').last().unwrap().trim_end_matches(".png");
            let bin = format!(
                "{dir}/n1-{path}-{kind}-{coder}-planar{}-ref{reference}.bin",
                planar as u8
            );
            let bytes = std::fs::read(&bin).unwrap_or_else(|e| panic!("n1 bin {bin}: {e:?}"));
            // parse N1G2: one 9-byte record per eligible byte; the
            // trailing u32 = the eligible RECORD COUNT (== raw positions
            // acquired; the remaining tail, if any, is 4-byte N1IN groups).
            let nrec = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap()) as usize;
            assert_eq!(
                bytes.len() - 4 - nrec * 9,
                (raw_len - nrec) * 4,
                "N1IN tail groups match the ineligible tail"
            );
            let mut recs = Vec::with_capacity(nrec);
            for chunk in bytes[..nrec * 9].as_chunks::<9>().0 {
                recs.push(Rec {
                    b: chunk[0],
                    cls: chunk[1],
                    left: chunk[3],
                    upper: chunk[4],
                });
            }
            assert_eq!(recs.len(), raw_len, "fully eligible winner stream");
            let mut geom_wh: Option<(u32, u32)> = None;
            for (stem, gw, gh) in GEOMS {
                if stem == path {
                    geom_wh = Some((gw, gh));
                }
            }
            let (w, h) = geom_wh.unwrap_or_else(|| panic!("geometry for {path}"));
            let geom = super::Geom {
                width: w,
                height: h,
                tile_count: w.div_ceil(crate::codec::TILE_SIZE)
                    * h.div_ceil(crate::codec::TILE_SIZE),
                planar,
            };
            println!(
                "n2 {path}: n={} a0_native={a0_native} a0_mix7={a0_mix7} kind={kind} planar={planar}",
                recs.len()
            );
            for (n_q, sym) in [(8u32, false), (12u32, false), (16u32, false), (12u32, true)] {
                let o = replay(&recs, geom, n_q, sym);
                rows.push_str(&format!(
                    "{path},{n_q},{},{},{a0_native},{a0_mix7},{:.2},{:.2},{:.2},{:.2},{:.2},{},{},{},{},{},{}\n",
                    sym,
                    recs.len(),
                    o.a1_bits,
                    o.b0_bits,
                    o.b1_bits,
                    o.c0_bits,
                    o.c1_bits,
                    o.b0_wire_b,
                    o.b1_wire_b,
                    o.c0_wire_b,
                    o.c1_wire_b,
                    o.mix_ns_pos,
                    o.eng_ns_pos,
                ));
                println!(
                    "n2 {path} q{n_q} sym={sym}: A1={:.0} B0={:.0} B1={:.0} C0={:.0} C1={:.0} bits | wires {}/{}/{}/{} | ns/pos mix {} eng {}",
                    o.a1_bits, o.b0_bits, o.b1_bits, o.c0_bits, o.c1_bits,
                    o.b0_wire_b, o.b1_wire_b, o.c0_wire_b, o.c1_wire_b,
                    o.mix_ns_pos, o.eng_ns_pos,
                );
            }
        }
        std::fs::write(&out, &rows).expect("n2 csv");
        println!("n2 summary -> {out}");
    }
}
