# MNF17-P1-B — mix-family stage decomposition and layer identity census

**Status**: B1/B2/B3 closed against the authorized instrument (env-gated
cfg(test); production untouched). Cohort = the P1-A 8 informative files,
all 1,216 mix-family evaluate calls (record + token arms, backends
4/5/6/7). The v1.17.0 exhaustive selector remains the oracle; the census
run's per-arm wires are the production encodes themselves.

**Instrument**: `src/mix_coder.rs → #[cfg(test)] mod p1b` research twins
plus `src/mnf8.rs → p1b_decomp` driver
(`MNF17_P1B_OUT=<csv> cargo test --release p1b_decomp -- --ignored`).
Twins = **verbatim copies of the production encode loops with only the
`encode_symbols` call removed** (the writer never feeds back into
walk/ctx/key evolution — verified by reading both paths), so
`wall(full) − wall(twin)` is the writer's exclusive cost. Per-arms rows:
459/1,216 flagged `twin > full` (timer noise; median overrun 0.7%,
worst 69.8% on one row) — excluded from stage ratios, kept for totals
and identity counting. Zero rows show negative writer attribution after
exclusion: the decomposition reconciles.

## B1 — exclusive stage attribution

| Stage | Share of mix-encode wall |
| --- | --- |
| Probability-model layer (`probs_with` + 256-word fixed-point build + `step`/`share_step`) | **≈ 99%** |
| Entropy writer (`encode_symbols`, range-coded) | **0.8–1.1%** |
| Context prep (nctx allocs + parent tree) | ≈ 0.0% |
| Seal + framing (`into_compressed`) | ≈ 0.0% |
| Grammar walk (`LaneWalk`, backend 7 only) | **0.075 s = 0.1% of b7's full cost** |

Totals over all rows: mix4 55.45 s, mix5 54.81 s, mix6 57.57 s,
mix7 52.86 s — **220.7 s** of mix-family evaluate on the cohort.

**The range coder is not the cost. The per-byte categorical model
construction is.** Each byte builds a full 256-entry fixed-point
distribution (`probs_with` mixing + `ContiguousCategoricalEntropyModel::
from_nonzero_fixed_point_probabilities`) to code one symbol.

## B2 — layer-specific identity

- No cross-backend layer sharing exists. Backends 4/5/6 carry **no
  classes layer at all** (byte-context conditioning
  `mix_ctx_key(prev, prev2, prev3)`); backend 7's LaneWalk classes layer
  exists but costs 0.1% — nothing worth sharing.
- The writer is shared code but ≈1% cost; the model layer is per-backend
  state (nctx 8/16/20/40, distinct caps/parents/schedules) — different
  trajectories by construction.
- The user's hypothesis "the four mix backends are repeatedly solving
  portions of the same mathematical problem" is **refuted at the layer
  level**: what the family shares is code, not computation. The shared
  record-stream construction + relabels already live at the pick level
  (P1-C, shipped) and cost ~0%.

## B3 — counterfactual memoization ledger

Input-identity memo (`backend, data identity` — same input provably
yields same output, so reuse is verify-free once keyed by a strong hash):

| Backend | Arms | Unique inputs | Redundant arms | Redundant cost |
| --- | --- | --- | --- | --- |
| mix4 | 320 | 256 | 64 | 8.67 s (15.6%) |
| mix5 | 320 | 256 | 64 | 8.71 s (15.9%) |
| mix6 | 320 | 256 | 64 | 9.10 s (15.8%) |
| mix7 | 256 | 208 | 48 | 7.31 s (13.8%) |
| **all** | 1,216 | 937 | 384 | **33.79 s (15.3%)** |

Deduction is per-backend (params differ ⇒ no cross-backend reuse).
Key cost: FNV ≈ 0.08 s, sha256 ≈ 0.13 s over the cohort's arm bytes —
net **≈ 15.3 points**, concentrated entirely on degenerate inputs
(identical raws across kinds/layouts/REFs); photo files sit at ~0%
(screenshot 0 redundant arms), matching the P1-A duplicate distribution.
Memory = cached payload bytes for the 937 unique inputs.

## Implications for the authorized gates

- **P1-C (shared computational graph)**: the sharing prize is not
  across backends — it is *inside* one arm's evaluation: the per-byte
  256-word model build. Any P1-C prototype must therefore attack
  single-symbol coding without materializing the full categorical
  vector (cumulative/cdf lookup on the fly). That is an algorithmic
  change — currently HOLD, requires its own exactness proof.
- **P1-D (certified elimination)**: the declined pool remains the
  dominant opportunity; the census adds that an incumbent established
  by zstd19+zlib (9.6 s) faces mix arms whose cost is 99% model work —
  bound machinery that can terminate early inside the model loop is
  the only mechanism that touches the 99% share.
- Memoization of class walks (b7) is worthless (0.1%); REF relabel
  memoization was already rejected.

## Method honesty

- Timing = single-machine `Instant` walls; stage ratios from the 757
  clean rows, totals over all 1,216; the twin-vs-parts sum reconciles
  per call (`t_full − prep − loop − seal ≥ 0` on every clean row).
- The twin removes ONLY the writer call; everything else (including the
  model build and its allocation) stays, so "model layer ≈ 99%" includes
  `step`/`share_step`/key/rotations — the writer is the only separated
  stage besides prep/seal/walk.
- 8-file cohort: not corpus-wide; rates carry the degenerate-cohort bias
  of P1-A (recorded, not extrapolated).
- fmt + clippy clean; payload hash `682e23165304b050` re-verified after
  the instrument edits.
