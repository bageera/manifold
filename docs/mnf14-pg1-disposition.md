# MNF14-PG1 — Projective RGB geometry discovery (disposition: DENIED)

Work-order memo: RGB(x,y) = R(P_theta(x,y), L_phi, M_psi) — test whether a
projective coordinate map + simple color law in (s,t) explains an image
more compactly than identity-space fields, with compression-aware selection
(IF3's theta*-form applied to the projection class). Instruments:
`pg1_probe.py` (honest-transport full arm matrix incl. real photos with an
estimator H) + `pg1_ladder.py` (q12-quantized transport + the 256/512/1024
scale ladder; copied to `crates/mnf-mans/scripts/mnf14_pg1_{probe,ladder}.py`).

## Verdict

**DENIED at every measured scale, with an explicit scope of closure.**
The projective arms win NOWHERE — not on constructed home-turf content,
not on the four real photographs.

## Mechanism class

Continuous-coordinate quantized MODEL (color law fitted in projected
(s,t) coordinates, transported as quantized parameters) + integer-exact
residual. Same family as the windowed-geometry probe (DENIED) and CF1
(radial fields, corpus-denied): a real description class, a model floor
that never undercuts the causal predictor.

## Measured (complete records: 28-B frame, zstd-19, exact recon asserted)

### Honest-transport draft (fp64 law coefs; per-file, 512² photos)

| image     | A_med  | B_affine | C_proj | D_curved | E_caware (pick) |
|-----------|--------|----------|--------|----------|-----------------|
| astronaut | 405,842| 405,564  | 405,340| 405,411  | 405,322 (root)  |
| coffee    | 439,143| 443,637  | 447,437| 450,382  | 447,419 (root)  |
| chelsea   | 331,079| 331,751  | 339,452| 337,165  | 339,434 (root)  |
| hubble    | 486,037| 486,461  | 486,479| 486,357  | 486,461 (root)  |

C (projective; the corner-identity DLT estimator = honest but weak) loses
or ties B everywhere; E's (s,t)-quadrant split never fires. Note: this
instrument's MED path (python, per-pixel walker) measures a few k above
IF2's vectorized baseline — within-process comparisons only (same-pipe
rule); the ARMS share the walk.

Method-honesty note: the FIRST E draft counted the split/root comparison
on a sign-folded magnitude plane (`|err−f| % 256`) instead of the honest
whole-plane residual — a noise-class false win (+4k) exposed it; every E
number above is the POST-FIX honest form (pred = clip(pred_med + law),
residual = (px − pred) mod 256, wire = CD-G of that).

### q12-quantized scale ladder (the memo's scale-invariance point)

Law coefs i16@Q12 (6 B/ch-law), H i16@Q12 (18 B); F2 = striped planar
scene viewed through the quantized true homography (the mechanism's own
fixture); F1 = the same scene with identity projection (control); F3 =
noise (negative control):

| fixture        | n    | A_med | A_xy_law | C_proj(q12) | D_quad | E (pick) |
|----------------|------|-------|----------|-------------|--------|----------|
| F1 (identity)  | 256  | 144   | 280      | 360         | 200    | 360 (root)|
| F1             | 512  | 222   | 412      | 260         | 278    | 260 (root)|
| F1             | 1024 | 257   | 295      | 295         | 313    | 295 (root)|
| F2 (persp.)    | 256  | 58    | 194      | 271         | 114    | 271 (root)|
| F2             | 512  | 74    | 209      | 112         | 130    | 112 (root)|
| F2             | 1024 | 146   | 184      | 184         | 202    | 184 (root)|
| F3 (noise)     | 256  | 196,655| 196,693 | 196,693     | 196,711| 196,693   |
| F3             | 512  | 786,491| 786,529 | 786,529     | 786,547| 786,529   |
| F3             | 1024 | 3,145,841| 3,145,879| 3,145,879 | 3,145,897| 3,145,879 |

The DECISIVE row set = F2: the projection map DOES express the scene
(the fitted law's residual ≈ 0), but MED's residual on the rasterized
stripes is ALREADY ≈ 0 and costs ZERO transport — the model's fixed tax
(qH + coefs + deg/q bytes ≈ 130-160 B at this transport) loses to a
free predictor at every scale, even though both grow flat (exponent
≈ 0 vs the noise control's 2.0 — the law class would scale-invariance-
win only against a residual stream that is NOT already cheap).

## Why it loses (the standing floor, now measured a third way)

The quantized-continuous model floor (parameter transport + quantization
error at the wire lattice) exceeds causal prediction's residual floor
wherever the causal predictor is already near-zero on the content. On
real photographs the fitted field removes thousands of residual bytes
but pays the same fixed tax and, per IF2/IF3, the fitted-plane error
spreads into the residual stream — aggregate loses. This is CF1's
corpus-control verdict plus the windowed-geometry floor, restated in
projective coordinates.

## Scope of closure

CLOSED by this instrument:
- projective color laws with an H known/trivially-estimated (identity-
  anchored DLT) on striped-planar fixtures under true perspective;
- the scale-invariance claim at 256/512/1024 with quantized transport;
- compression-aware (s,t)-quadrant law selection on those same fixtures
  and on real photos (E's split never fired post-fix);
- fp64-transport and q12-transport variants (both lose).

OPEN (not closed — the honest next rungs, in order):
1. **Content-estimated H**: my estimator anchors to the identity (corner
   DLT on canonical grid points); a real homography ESTIMATED from
   content correspondences (feature matching) is the untested half —
   the mechanism requires a content with a GENUINE projective warp
   relative to its color field (a tilted photograph of flat art, a
   document scan at an angle).
2. A **projective-content corpus class**: per CF1's gate question —
   does the class exist in the corpus? The current 30-file corpus
   contains none; a corpus that did would re-open arms C/E directly.
3. **Projected-residual prediction**: warp-ADAPTIVE prediction (MED
   evaluated along the projected flow rather than raster axes) is a
   mechanism class this instrument did not touch — identity-space
   residual prediction under a known P; only worth opening if (1)
   finds content-estimated-H wins.

Read before any 'projection/homography/warp-corrected' proposal.