# MNF16-X1 record (2026-11-15) — conditional cross-channel information discovery

Scope authorized: X1-0 (availability census) + T0–T5 validation and
conditional-information measurement of X1-A (earlier-lane residual
magnitude) and X1-B (earlier-lane residual sign) only. X3 (joint) is a
free byproduct of the same operands and is reported, not searched. No
wire work, no new backend, no parameter search: the feature spec, edges
[0]/[1–3]/[4–15]/[16–63]/[>=64], sign classes, aggregate rule and
control seeds are frozen in the instrument docstring
(scripts/mnf16_x1.py).

## T0–T4 validation (the first extractor built under the contract)

T0 — dependency audit (literal, in the docstring): operands =
{wire[y, x, lane′] for lane′ < lane at the SAME pixel}; no current
byte, no future bytes, no raster, no cross-pixel reads.
T1 — exhaustive target substitution: 5,767,168/5,767,168 invariances
across 22 files + control (1024 random symbols × 256 values each).
T2 — suffix overwrite (0xAA at N/2 and 3N/4) and within-window
residual permutation: features identical for all prefix symbols.
T3 — prefix-only interface: the reference extractor reads ONLY a
`PrefixView` keyed (y, x, lane) whose `earlier()` refuses lane ≥ cur;
framing bytes (record mode/meta) are not residual operands and carry
no view entries at all.
T4 — decoder parity: the reference (view) path and the vectorized fast
path agree EXACTLY on every stream byte of every file (feature bytes
and availability masks).

## X1-0 availability census

22/30 production files censused (mix backends 5/6 with a mix payload;
8 skipped as be0/be1/token — no record stream). 2,542,946 stream
bytes, 1,693,056 with an admissible same-pixel earlier lane. The
availability pattern is EXACTLY the grammar's: 66.67% of residual
bytes (lanes 1 and 2 of every record) in every layout (planar and
interleaved), every REF (0–3), every backend, every populated backend
context. Mode-0 residuals dominate the population. L6 (per-byte model
cost, S3-0's corpus anchor) = 583,366 B.

## T5 conditional information (frozen S2 conventions: 80/20 split, prequential)

| Arm (all else frozen) | corpus gain | content beyond availability (gain − X4b) | files positive |
| --- | --- | --- | --- |
| availability split ALONE | **+3,989.1 B (0.684% of L6)** | — | — |
| X1-A magnitude | +3,533.8 B | **+74.4 B** | 12/22 |
| X1-B sign | +3,878.7 B | **+296.3 B** | 14/22 |
| X3 joint | +3,591.5 B | +497.3 B | 19/22 |
| X4 global shuffle | +2,282–2,306 B | control | — |

X4b = feature values permuted among admissible positions (availability
fixed) — its gain vs base isolates the availability partition's
residual contribution, so `gain − X4b` = the feature's CONTENT at fixed
availability.

Marginal levels: I(S;A) = 15,356 bits (A), 44,895 bits (B), 49,062
bits (J) corpus-wide — the production context plus the availability
partition absorb nearly all of it; the conditional content is 0.01–0.1%
of the marginal.

## Findings

1. **X1-A denied**: earlier-lane residual magnitude adds +74 B
   corpus-wide (0.013% of L6; 12/22 files, noise-scale) on top of the
   availability partition, and the full arm nets LESS than the plain
   availability split (−455 B fragmentation).
2. **X1-B denied as an X1 addition**: sign carries the largest content
   (+296 B, 14/22 files) but the real sign-conditioned arm still nets
   −110 B BELOW the availability split alone. Adding any content class
   to the partition costs more (model fragmentation) than the content
   pays.
3. **X3 denied**: same pattern.
4. **The lane-availability partition is the one live candidate**: +3,989
   B estimator-level (0.684% of L6), zero extraction cost (it is record
   grammar metadata the decoder already knows), target-independent by
   construction (passes T0–T4 trivially — the lane class derives from
   the record walk, never from any residual). It is a structural
   finding X1 surfaced as a byproduct, NOT a cross-lane-content
   confirmation. Per the S3 lesson it remains an ESTIMATOR-level number
   until measured through the actual adaptive schedule (an additive
   lane-class child mode in the r9_audit instrument) — not built under
   this authorization.

## Controls (partial, honestly limited)

- x1_chanswap (real photo, R↔G swapped): censused (backend 6, REF 2).
  Cross-lane marginals remain measurable; the intended matched-REF
  natural baseline was not produced (the natural forest pick chose
  REF 1, the swapped pick REF 2), so the break-vs-natural direction is
  NOT established at matched reference — recorded as partial.
- x1_correlated / x1_indep / x1_identical (192×128 and 288×224
  variants): the shipped Maximum pick chose backend 1 (zstd container,
  no mix coding) for every variant — the availability census has no
  mix stream to walk. Recorded as an explicit limitation; the pick
  itself indicates these content classes carry nothing the mix coder
  would be chosen for.
- The corpus-level X4 shuffle control behaves as required (real arms
  beat their own shuffles by the content deltas above).

## Disposition

X1-A/B/X3: closed as cross-lane conditional features (content
negligible, net-negative against the structural split). The
lane-availability partition advances as a deferred STRUCTURAL
candidate pending a decision on the coder-level arm. MNF16-A1
(acquisition signatures) remains queued. Production anchor 810,816 B
untouched; no production sources modified. Run artifacts:
/tmp/mnf16_x1_full.log (corpus + per-file by_mode/by_ctx),
/tmp/mnf16_x1_ctrl.log, /tmp/mnf16_x1_ctrl2.log
(/tmp/mnf16_x1_census.json holds the latest run's rows).
