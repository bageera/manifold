# Riemannian Atlas — experiment-contract review (2026-10-27)

Status: proposal review, not implementation. The proposal (raster →
image-dependent metric tensor → geodesic charts → fold/atlas → residual)
is mathematically coherent and its own caution is correct: the fold is
reordering, the wins must come from prediction + atlas + residual. The
repo's measured record positions each sub-arm precisely.

## What the record already decides

**1. The metric-tensor idea is a re-discovery of our predictor problem.**
The proposed metric `g = I₂ + λJᵀJ` makes distance depend on local image
gradients — that is EXACTLY what edge-sense prediction (LOCO-I MED) does
in the value domain, and what `Sig` ranking does in the atlas. v12
measured both: MED = the single biggest lever (−98% gradient class), and
its failure mode is content-dependence (loses to plane on flat-UI). A
Riemannian scan-order derived from `J` will, at best, reproduce MED's
edge-following behavior one level up. The 3am question: what does the
metric give that MED doesn't already? Candidate honest answer: a
DETERMINISTIC scan order that adapts to curves (bottle-label case). That
is a traversal arm, not a geometry arm.

**2. Traversal arms are measured dead.** The dimensional-fold probe
measured exactly the "intelligent path" hypothesis the proposal's §3
describes: naive fold, Morton (locality-preserving), z-order — against
prediction arms. Every reordering arm ≤ 1.0× and often 4–7× worse;
on photo-like content the best fold (0.74×) still LOSES to plain
vertical-delta prediction (0.55×). Mechanism: bijections preserve bytes,
LZ exploits byte-locality, and adaptive paths pay `L_path`. The proposal's
caution ("not automatically superior; metadata may exceed savings") is the
measured outcome. A geodesic scan inherits this verdict UNLESS it changes
the predictor's context geometry, not the scan — see arm R2 below.

**3. "Shared geometric atlas" is the strongest sub-arm — and it's arm P.**
Matching structurally similar charts across distant regions = the torus/
slice-phase mechanism (0.26× on cyclic content, phases recovered exactly)
transplanted to learned charts. The measured precondition stands: shared
models pay when the structure is genuinely shared (periodic slices,
multi-view), and lose on photographs (20/20). The atlas-on-residuals ≈ 0
result (v12, finding 2) also bounds what chart-matching can add after a
real predictor. The Riemannian framing adds a vocabulary, not a new
mechanism, unless charts make previously-unsimilar regions similar.

## Contract for the one arm worth measuring (R2: metric-warp predictor)

Not a traversal. Keep the raster scan; warp the PREDICTOR'S NEIGHBORHOOD
by the metric: the context pixel for position (x,y) is g-geodesic-nearest
rather than raster-north/west. Determinism contract: the warp is quantized
to integer offsets per tile (2 bits/tile max), so the decoder recomputes it
bit-exactly with zero fp. Costs: 2 bits/tile id + predictor arithmetic
(unchanged). Baseline: MED alone; success: beats MED on the curved-content
class (bottle-label synthetic) by > transport noise WITH the 2-bit cost,
WITHOUT losing flat-UI/smooth classes beyond noise.

Everything else (geodesic path encoding, curvature charting, chart-atlas)
inherits the measured dead verdicts and needs neither code nor time.