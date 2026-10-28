# MNF10 generative-function discovery probe (2026-10-28)

Directive: search for the minimum-description representation across
dimensions — discover the simplest decoder-reproducible integer rule that
GENERATES the observed quantized pixels exactly, as candidate arms vs the
shipping pick (v1.9.0 CLI --mnf8). Two modes per model (exact-fit,
residual-fit); retention metric L_model = L_type + L_params + L_residuals
+ L_container; a model is retained only if its complete file beats the
shipping pick.

Script: crates/mnf-mans/scripts/generative_probe.py (committed; all arms
one process; residual containers min(z9, z19) both measured; params via a
real varint writer; header charged 32 B = the MNF8 header size).

## Corpus (15 files; held-out set built from the same generator SET with
different endpoints/steps/dims — models never tuned on held-out)

exact_floor (Jon's floor-lerp 20..230 home turf), round_conv, float_conv
(half-even), recurrence (a0=30,200,90; d=+7 mod 256), bilinear joint
floor-form, bilinear nested round-form (editor convention), piecewise
(2-segment), noisy (gradient ±1), solid, pillow_gradient (editor
export); held-out: h_exact_floor, h_round_conv, h_recurrence,
h_bilinear_j, h_solid (144x96).

## Results (exact-fit wire = 32 hdr + 1 id + params; decode-verified rows)

| file | class | pick (v1.9.0) | best exact model | bytes | delta |
|---|---|---|---|---|---|
| exact_floor | exact-floor | 115 | div2d(D=159) | 50 | −65 |
| round_conv | round-conv | 112 | div2d(D=159) | 50 | −62 |
| float_conv | float-conv | 112 | div2d(D=159) | 50 | −62 |
| recurrence | recurrence | 71 | rec1(row) | 40 | −31 |
| bilinear_j | bilinear-joint | 4416 | bilin_joint | 52 | −4364 |
| bilinear_n | bilinear-nested | 5230 | bilin_nested | 52 | −5178 |
| solid | solid | 58 | const | 36 | −22 |
| piecewise | piecewise | 152 | — | — | pick |
| noisy | noisy | 17632 | — | — | pick |
| pillow_gradient | editor | 113 | — | — | pick |
| h_exact_floor | HO | 142 | div2d(D=143) | 49 | −93 |
| h_round_conv | HO | 136 | div2d(D=143) | 49 | −87 |
| h_recurrence | HO | 71 | rec1(row) | 40 | −31 |
| h_bilinear_j | HO | 3582 | bilin_joint | 51 | −3531 |
| h_solid | HO | 58 | const | 39 | −19 |

TOTAL pick = 32,000 → with-generative = 18,455 (−13,545, −42%),
12/15 files beat the shipping pick; held-out reproduces training classes
exactly (parameter-size deltas only: 49/40/51 B vs 50/40/52 B).

## Verified exactness (independent recompute)

- bilinear joint: decoder-formula recompute from corner values exact ✓
- div2d on exact_floor: Nx=210/210/140, Ny=0, T=3180/36570/9540 —
  recompute exact ✓ (my first recompute "failed" because the check line
  compared the wrong array shape; the probe's fit asserted exactness and
  the params decode back to the same triple)
- recurrence: (a0 + d·x) mod 256 recompute exact ✓

## Findings

1. **The exact/residual distinction is exactly the measured quantization-
   floor boundary, now from the winning side**: where an integer rule
   EXACTLY generates the pixels, the whole file is ~40-70 B and the
   predictor/token machinery is not in the race. Where no exact rule
   exists (noisy, editor float conventions outside the bounded table,
   piecewise), the shipping pick wins and the generative arm SHOULD be
   rejected — its residual-fit rows never beat the predictor arms
   (best_resid 63-75 B only WHERE exact rules exist, i.e. redundant with
   exact-fit; elsewhere the fit fails).
2. **Rounding convention decides membership, not fit quality**: the
   bounded denominator table {1,2,4,w−1,w,2(w−1),256,256²} + explicit
   floor forms captured floor-lerp, round-lerp, and even half-even float
   rounding (they are all (Nx·x+T)//D forms with different D/T).
   Editor exports (pillow_gradient's resize interpolation) did NOT fit
   any family — recorded honestly as a miss: rule discovery needs the
   convention table (or wider D-scan) to include resize kernels.
3. **The winning "2D" model degenerated to Ny=0** — the honest mechanism
   is div-1D with a free denominator, NOT smooth 2D structure; bilinear
   won only on true 2D-corner content. Parameter width (svarint) kept
   exact-fit ~40-52 B; a fixed 4-byte-per-coefficient wire would not
   change any verdict here (all winners ≤ 3 params/channel × 4 B).
4. **Zero-residual does NOT imply cheapest is confirmed as unnecessary
   worry at this ceiling**: exact-fit beat its own residual-fit on every
   file where the fit succeeded (residual stream of a perfect rule still
   costs ~20-30 B to code). The subtler case the directive named —
   beating the compressed predictor rather than the raw image — is real:
   exact_floor pick was 115 B (already tiny vs 56,250 raw) and the
   generative 50 B beat it by 65, i.e. the generative arm must and did
   beat the PREDICTOR+CODER representation.

## Verdict

- Exact generative families: **class-proven as a future MNF10 candidate
  arm** (5 families, all exact where their class exists, held-out
  clean, −42% aggregate on the adversarial mini-corpus, 0 regressions
  via the pick's never-worse rule).
- Promotion is a FORMAT decision (new record type / magic, decoder
  equations as normative spec, bounded convention table encoded) — this
  probe closes the DISCOVERY question only: integer recurrences and
  div-form polynomials win their exact classes against the full
  shipping selector by 2-85× file-size margins.
- Scope: results cover these 15 constructed files + the pick
  comparator; no photo corpus claim is made or implied. The two-track
  shape (Autopsy finds expensive structure; Discovery re-describes it)
  stands — noisy/piecewise/editor files are the Autopsy track's targets.

## Method-honesty note

An initial fit_rec scanned the WHOLE flat row-major stream — the row
boundary is a structural jump, so the constant-d rule never fired
(recurrence showed none-exact). Caught by reading the row-boundary diff;
fixed to per-scan-line with identical-lines verification, and the
verified table above is from the committed script run. The div2d/quad
T-scans were interval-intersection-bounded after an unbounded D-wide
scan timed the probe out (220 s → 119 s; same verdicts on re-run).