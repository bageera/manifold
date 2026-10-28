# Lattice ablation — Experiment B measured (2026-10-28)

The untested hypothesis from the Erdős closure's scope line: alternative
prediction-grid TOPOLOGY. All lattices keep raster causality
(decoder-reproducible), so each family is a legitimate predictor change;
the residual planes differ, everything coded (no metadata beyond the
per-file pick flag in the hybrid variant). Harness:
`crates/mnf-mans/scripts/lattice_probe.py` (v2, with the method-honesty
note below). Same-coder rule: one zlib-9 per arm. 30-file corpus.

## Arms (faithful implementations + the controls the verdict needs)

- cart-MED: (left, up, up-left), edge-sense — the shipping predictor.
- cart-plane: LOCO-I plane branch FORCED (a+b−cc always) — isolates the
  branch choice from the neighborhood choice.
- left-only: pure left predictor — the degenerate control (pins what the
  v1 buggy "hex" arm actually measured).
- tri-MED: (left, up, up-right) — the triangular 6-lattice's causal set.
- hex-MED: faithful staggered pair — above-neighbors (y−1, x−1+stagger)
  and (y−1, x+stagger), stagger = y mod 2.

**Method-honesty note (the v1 catch):** the first hex implementation was
DEGENERATE — both "above" taps read the same up-left pixel on staggered
rows, making it a left-only predictor. Its numbers (hex-min −27,317,
poster −58%) were the degenerate arm winning flat-color classes and are
NOT a lattice result; they poisoned v1's table and this record cites
only v2. Every suspicious win in this program gets a mechanism check
first; this is the third such catch (Hilbert d2xy 2026-10-27, MNF8 med
stride 2026-10-26) and the rule now has three precedents.

## Results (zlib-9 totals over 30 files)

| arm | bytes | vs cart-MED |
|---|---|---|
| cart-MED (shipping) | 1,078,338 | — |
| **cart-plane (LOCO-I forced)** | **1,029,156** | **−49,182 (−4.6%)** |
| left-only (degenerate control) | 1,048,804 | −29,534 |
| hex-MED (faithful stagger) | 1,165,268 | +86,930 |
| tri-MED (up-right pair) | 1,221,808 | +143,470 |
| erdos-r2 (algebraic coords) | = cart-MED exactly (affine no-op; analytic tie confirmed) |

Per-class splits (the honest readout): cart-plane wins screenshots/plaid
(−4.5%/−29k) and photos broadly but loses poster (+0.4%) and stone;
left-only wins poster/document (−59%/−7%) as v1's degenerate arm did;
hex/tri lose on nearly every class (the staggered/pair neighborhoods
break the vertical continuity that raster MED's up-tap exploits).

## Verdicts

1. **Alternative grid topology (hex/tri): DENIED.** Both faithful
   staggered-lattice predictors lose decisively (+8.1% / +13.3%) — the
   raster up-tap (same column above) is the single most valuable causal
   neighbor on natural images, and both alternative lattices break it.
   The Erdős-r2 algebraic arm ties exactly, confirming the analytic
   no-op (affine coordinate changes move no neighbor). Grid topology is
   closed for square-raster causal predictors on this corpus.
2. **The real measured discovery is NOT a lattice: the branch choice
   dominates the neighborhood choice.** Forcing LOCO-I's plane branch
   beats full MED by −49,182/30 (−4.6%) — larger than any lattice
   effect in either direction. MED's edge-sense pick-branch, the
   JPEG-LS heritage, is measurable dead weight on this corpus (its
   protection against extremes costs more than it saves; the wrap-
   around residual absorbs the clamping harm). This makes the adaptive
   hybrid question concrete: cart-plane as an additional per-file arm
   (1 flag bit, decoder-reproducible trivially) is a REAL candidate —
   +1 bit vs −49k. Left-only as a per-file arm likewise (poster −59%,
   document −7%, and only +1 bit against its wins); combined with
   FLAG_PRED_MED the encoder already has the pick machinery to
   measure both per file at format cost ~0 (MNF8 flag space bit 4).
3. **Adaptive hybrid (candidate 4): not denied — made concrete and
   cheap.** The block-level version from the proposal is UNNECESSARY:
   per-file suffices on this evidence (class-consistent winners), and
   per-file is what the pick machinery already is.

## Format mapping (next step, measured basis ready)

Candidate arms for MNF8 flag space: bit 4 = PRED_PLANE (plane-branch
literal bodies), bit 5 = PRED_LEFT (left-only literal bodies) — both
decode causally exactly like FLAG_PRED_MED, cost one bit each on the
wire, and join the ADR-003 rate-aware pick. Estimated aggregate win if
both fire where measured: ≈ −52k over the corpus (−4.8%) with zero new
record grammar. Retention gate to run before any format step: full
corpus per-file pick with both arms added vs v1.6.0 pick TOTAL
(1,029,925): predicted ~981k. This is MNF8-compatible, no MNF9 magic.

## Scope line (per the proposal's distinction)

- CLOSED by this record: hex/triagraph causal-lattice prediction; Erdős
  algebraic coordinates as prediction coordinates (analytic no-op,
  measured tie).
- OPEN and concretized by this record: the branch-choice family
  (plane/left as additional causal predictors) — NOT a lattice result;
  the proposal's lattice framing surfaced it as a side effect of the
  controls, which is what controls are for.