# Riemannian traversal probe — metric-guided ordering measured (2026-10-27)

The proposal (v0.7-successor): a metric-guided traversal
`w(p,q) = sqrt((q-p)ᵀG(p)(q-p))`, `G = I + λ∇I∇Iᵀ` — order pixels by
similar-color geodesics instead of raster/Morton/Hilbert. Tested exactly
as specified with the ONE honest caveat about derivation order stated
below. 128×128 grayscale (the proposal's own first milestone size),
per-order prediction = diff-from-previous-in-order, entropy = one coder
(zlib-7) for every arm, metadata: raster/morton/hilbert = 0 bits
(deterministic), greedy = 0 bits under the causal-derivation assumption
(the proposal's own design requirement).

**Method honesty note:** my first hilbert implementation had an
index-collapse bug (64 unique pixels of 16,384 — caught because hilbert
"winning" on random data by 18× is impossible; a permutation preserves
the byte multiset). Re-verified bijections for both morton and hilbert
before measuring anything. No verdict cites the buggy numbers.

## Results (bytes after zip of path-residuals; 16384 px)

| class | raster | morton | hilbert | metric-greedy | best |
| --- | --- | --- | --- | --- | --- |
| smooth | **3,975** | 9,269 | 8,440 | 15,060 (3.79×) | raster |
| sharp edges | 88 | 98 | 94 | **59 (0.67×)** | metric-greedy |
| plaid | **553** | 863 | 1,164 | 9,664 (17.5×) | raster |
| random | 16,395 | 16,395 | 16,397 | 16,395 (1.00×) | tie |

## Findings

1. **The metric-greedy heuristic loses on 3 of 4 classes.** Mechanism: a
   metric-guided priority queue is NOT a contiguous path — the heap jumps
   to the globally-cheapest unvisited pixel, so the sequence crosses large
   distances between similar-color islands and the order-0 path
   prediction (diff = previous element) is worse than raster on smooth
   and plaid. This is the structural reason the 2000 scan-order study
   (Hilbert ≯ raster) and the fold probe (all reorderings ≤ tie) keep
   being re-discovered: a permutation does not reduce information, and
   the coder exploits locality, not similarity-at-a-distance.

2. **The one winning class is exactly the one prediction can't reach:**
   sharp binary edges, where a jump across an edge is FREE (both sides
   uniform) and the metric-arm visits each uniform region contiguously
   (59 B vs raster 88 B). But the same win is available to a predictor
   that knows about edges (MED's edge-sense) without any permutation —
   and hilbert loses to raster even here.

3. **The causal-derivation requirement is the real blocker.** For the
   greedy order, the decoder must reproduce the encoder's order
   causally from DECODED pixels — but the encoder used TRUE pixels, so
   once prediction errors appear, decoder-side paths diverge and the
   0-bit metadata assumption fails. Making the path explicit costs
   ~2 bits/sample worst case (per-pixel out-of-order direction), which
   erases the 0.67× edge-class win (2 bits × 16k px = 4 kB ≫ 29 B saved).
   The only traversal that survives this contract is a deterministic
   spatial one — which is what raster already is.

4. **The proposal's own test-cases table already contains the verdict:**
   random = 1.00× tie for all four arms (permutation preserves the
   information), which is the folded statement of "reordering ≠
   compression".

## Verdict

REJECTED as a coding mechanism (measured): metric-guided traversal loses
to raster+prediction on smooth/plaid/random and its single edge-class win
dies against ~2 bits/sample path metadata under the derivability
contract. NOT RETAINED; no format item. If a metric-derived structure
ever earns bytes, it will be as a PREDICTOR-side hint (the R2
metric-warp-predictor contract in docs/riemannian-review.md), not a
traversal.