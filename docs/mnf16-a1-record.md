# MNF16-A1 record (2026-11-15) — acquisition-signature screening (R0–R2)

First discovery program on the T1 substrate. Frozen feature
definitions (scripts/mnf16_a1.py docstring): A1 2×2 coordinate phase
(ids 0..3), A2 8×8 coordinate phase (ids 0..63), A3 block-boundary
membership (ids 0..1), A4 within-file permutation of A2 (seed 17),
A0 = the shipped production context (trace keys). R0: every feature is
target-independent BY CONSTRUCTION — operands are the decoder-known
(y, x) of residual bytes via the record walk; framing bytes carry no
phase and fall back to the production model (the frozen X1 wire rule).

## R2 screening (T2 population = the pre-registered 5-file screening.lock; X1 estimator conventions)

| Arm | marginal bits | prequential gain | shuffled control | content (real − shuffle) |
| --- | --- | --- | --- | --- |
| A1 2×2 phase | 753 | −477.3 B | −470.5 B | −6.8 B |
| A2 8×8 phase | 8,566 | **−17,400.5 B** | −17,891.1 B | +490.6 B |
| A3 block edges | 201 | −125.1 B | −160.1 B | +35.0 B |
| A4 (control) | 8,014 | −17,891.1 B | −17,889.2 B | −1.9 B |

## Synthetic audit-path controls (explicit mix-path diagnostics, SEPARATE from natural production selection)

Micro record streams (one interleaved mode-0 16×16 record) coded
directly by the audit — the production picker is not involved, which
answers the limitation X1 hit when the Maximum pick chose backend 1:

- positive (residuals engineered JPEG-grid-phase-dependent): A2 gain
  **+96.6 B**, its own shuffle −473.4 B → the instrument DETECTS real
  phase-conditioned structure when it exists;
- negative (iid white residuals): A2 gain **−314.6 B** → no phantom gain.

## Findings

1. The instrument is validated both ways by the constructed controls.
2. On natural production streams the tested acquisition family has
   only trace marginal information (753–8.6k bits over ~5 files ≈
   0.09–1.07 kB) and C6 absorbs essentially all of it; conditional
   content vs controls is noise-scale (A1 −6.8, A3 +35.0) or
   −net-negative (A2's +491 B content sits under ~18 kB of
   64-slot fragmentation).
3. Per the pre-registered stopping rule: no arm advances to R3
   (adaptive coder arms). The screening negative is recorded with the
   retained caveat from the acceptance note: the 5-file manifest is
   length-selected, not content-selected, so this does not disprove
   acquisition signatures on untested image histories — but no
   candidate earns further execution under the tested definitions.

## Disposition

A1 R0–R2 closed negative. No R3. A1's synthetic-control path (micro
audit-path streams) is retained as the reusable pattern for future
content-domain controls. Acquisition signatures remain "relatively
unexplored" as a domain; the tested coordinate-phase family is denied
economically. X1-L R4 passes (docs/mnf16-x1l-r4-record.md); X1-L R5
(production portfolio accounting) remains gated on a promotion
decision. Artifacts: research/results/a1_screening.json;
measurement-only commits — production untouched, anchor 810,816 B.
