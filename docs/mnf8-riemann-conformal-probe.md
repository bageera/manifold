# MNF8-Riemann probe: conformal square→disk + anisotropic tiebreaks — measured, CONFORMAL DENIED / ANISOTROPIC MARGINAL (2026-10-27)

The proposal: test Riemann's mapping theorem as a codec foundation —
square→disk conformal guidance (`d_f(p,q) = |f(p)−f(q)|`) for predictor
selection and atlas-reference ranking, against an anisotropic
(image-dependent metric) variant, both under the derivability contract
(fixed-point chart coordinates / precomputed integer tables).

## What the math and the measurements say

### Conformal hypothesis — DENIED (structural + measured)

1. **Prediction-improvement row is analytically a no-op.** Conformal maps
   preserve angles: at the 1-px causal scale, every neighbor direction is
   rotated and scaled by the same complex factor `f'(z)`, so the ORDER of
   `d_f(W), d_f(N), d_f(NW), d_f(NE)` is identical at every pixel —
   prediction along `d_f` cannot differ from Cartesian prediction. The
   pulled-back metric `g_ij(z) = |f'(z)|²δ_ij` is isotropic pointwise
   (the proposal itself notes this); a causal predictor only sees
   pointwise geometry.
2. **The conformal machinery was built and verified** to eliminate the
   "maybe the map isn't really angle-preserving at pixel scale" escape:
   Schwarz–Christoffel `g(w) = ∫(1−w⁴)^(−1/2)` integrated to the lemma
   constant (1.31103 corner scale, edge-mid/corner = 1/√2, 4-fold
   symmetry, inverse residual < 1e-2 lem units). No numerical artifact
   hides the no-op.
3. **The only conformal lever is scale variation, and the atlas window
   never sees it.** kappa = |f'| ranges 1.0001 at 16-px offsets, 1.002 at
   32 px, 1.03 at 64 px — the MNF8 atlas ranks candidates inside a
   last-128-tile window (mostly ≤16 tiles away), where `d_f` is a
   monotone stretch of the Euclidean distance within 0.2%. Simulated
   encoder tiebreak (real files, alpha swept, per-image best): conformal
   distance penalty = **−0.11…−1.00% on 4 photos, +2.3…+7.0% on 3
   heldout** — and the control experiment (Euclidean distance penalty at
   the same alphas) lands within ±0.2% of the conformal variant on every
   file (forest: conf −142 B at α=3000 vs eucl +173 B at α=1000 — both
   directions appear at different weights; the "win" is weight-tuning
   noise). A rank-preserving monotone warp of the distance cannot produce
   a repeatable ranking gain. **Not retained.**

### Anisotropic hypothesis — marginal, one real signal class

Gradient-orientation tiebreak (`α·|sin(θ_i − θ_j)|`, orientation from the
tile's mean gradient — same grammar as MNF8 sig_dist, atlas window
unchanged, +1 B/image alpha metadata, encoder-side rate-aware α choice):

| file | base | best (α per-image) | delta |
|---|---|---|---|
| autumn | 51,836 | 51,836 (α=0) | +0.00% |
| dusk | 37,790 | 37,779 | −0.03% |
| forest | 71,717 | 71,717 (α=0) | +0.00% |
| ocean | 57,428 | 57,416 | −0.02% |
| blob (held) | 18,877 | 18,877 (α=0) | +0.00% |
| blocks (held) | 2,062 | 2,061 | −0.05% |
| document (held) | 1,380 | 1,304 (α=120k) | **−5.51%** |
| screenshot | 1,817 | 1,801 | −0.88% |
| plaid_192 | 106,591 | 106,591 (α=0) | +0.00% |
| noise 256² | 196,674 | 196,674 (α=0) | +0.00% |
| repeat 256² | 990 | 990 (α=0) | +0.00% |
| gradient | 3,052 | 3,052 (α=0) | +0.00% |

The rate-aware mechanism (pick α per file, or refuse) costs +1 B and
NEVER loses: worst case the encoder writes α=0 (+1 B). But the sum over
these 12 files is −747 B of which −736 B is one file (document, a
text/screenshot class where reference orientation signal is genuinely
predictive). Photos don't move (−0.0…−0.03%). For format admission, the
bar is the full 30-file corpus without regression and a repeatable win;
a single document-class file (−5.5%) plus screenshot (−0.88%) is
suggestive but not established at this probe's fidelity.

### Verdicts

- **Conformal-guided prediction/reference selection: NOT RETAINED**
  (analytic no-op + measured noise-vs-euclidean control). The 1851
  theorem is safe as mathematics; as codec geometry it reduces to an
  isotropic scale warp, which the atlas window spans too little of to
  matter.
- **Anisotropic orientation tiebreak: NOT RETAINED as a format change
  now**; documented as the one candidate with a real (document-class)
  effect, worth retesting only if a future corpus gains text/screenshot
  weight. The mechanism is MNF8-compatible (encoder-side, +1 B, rate-
  aware) if it ever clears the gate.

Probe script: this document is the record; simulation method and all
numbers above are reproducible (criterion-free python, series-based
conformal inverse).

Relation to earlier probes: this is the fifth geometry-family denial
(scan-order, metric-traversal, metric-warp predictor, D8+shears Rust
gate, conformal coordinates). The consistent mechanism lesson: **geometry
that preserves or warps the grid does not add bits to the coder; only
content-conditional structure (orientation on text, edges in MED,
references in the atlas) has so far paid measurable bytes.**