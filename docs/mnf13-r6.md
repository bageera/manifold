# MNF13-R6 — adaptation & quantization ablation on the v1.13 coder (2026-10-30)

The work order's axes, measured on the shipped backend-4 coder's own
wires. Representation FROZEN: predictor, channel transforms, token
grammar untouched; only the entropy model's adaptation machinery
varies. Per-file exact-inverse asserted inside every measurement
(bridge `crates/mnf-mans/examples/r6_mix.rs`; constriction 0.5 stream
range coder; zero transport).

## Instruments & validity anchor

`scripts/…` under crates/mnf-mans: `mnf13_r6_extract.py` (residual wire
rebuild in the coder's own order — per-tile [mode][body], planar
[ref][a−ref][b−ref] planes / interleaved triplets, ADR-008 CD relabel
from the SHIPPED source semantics), `mnf13_r6_gate.py` (13 arms × the
corpus), `mnf13_r6_caps_finish.py`/`mnf13_r6_grid.py` (grid finish).
Anchor before any arm number: the extracted stream re-encoded through
the UNCHANGED shipped mix coder reproduces the shipped file payload
byte-for-byte — asserted per file, all four wire shapes (REF∈{1,3} ×
planar/interleaved × LEFT/PLANE/MED). Two extraction bugs were caught
by the anchor itself (ref-3 pair order; the per-tile mode bytes) — no
arm number predates the passing anchor.

Two instrument notes for the record:
- the CLI silently accepts `--profile maximum` as an ignored positional;
  every affected script now passes `--mnf8=maximum` (the shipped-pick
  CSV columns labeled mnf8-maximum in scripts before the fix were
  Balanced picks — scripts fixed this session, `--profile` acceptance
  is a known CLI wart to close separately);
- MNF9-magic picks and token wires carry no mix-addressable record
  stream: skip rows, credited at shipped size (the pick declined the
  mix arm there by construction).

## Gate 1 — the 13-arm ablation (20 backend-4 files; plen = shipped payloads)

Comparator = the pick's own backend-4 payload per file (framing shared
→ pure entropy-model delta). Full rows in `docs/mnf13-r6-gate.log`.

| Arm | Total (B) | Δ vs shipped | Fires |
|---|---|---|---|
| A shipped (fast 4096, slow 65536, lr .15, inc 32, 2^24, 8 ctx) | 547,600 | — | — |
| caps 1024/16384 | 554,096 | +6,496 | 0 |
| caps 1024/65536 | 547,616 | +16 | 1 |
| caps 4096/16384 | 554,092 | +6,492 | 0 |
| caps 4096/262,144 | 546,312 | −1,288 | 20 |
| caps 16384/65536 | 547,612 | +12 | 2 |
| lr .05 | 547,644 | +44 | 0 |
| lr .25 | 547,596 | −4 | 2 |
| lr .50 | 547,588 | −12 | 4 |
| inc 16 | 547,636 | +36 | 5 |
| inc 64 | 548,408 | +808 | 0 |
| scale 2^20 | 547,576 | −24 | 6 |
| ctx 16 (prev-bucket coarse joint) | 545,336 | −2,264 | 18 |

Read: the adaptation-rate compromise was already near-optimal per
component (solo sweeps ±0.08%); the two REAL levers are the SLOW cap
(lengthen the stable-content memory: −1,288 alone, fires 20/20) and
the CONTEXT RESOLUTION (16 keys over prev-bucket × coarse-pre-prev
band: −2,264, fires 18/20).

## Gate 2 — the combined grid (the two levers composed; 8 cap pairs, one process)

`docs/mnf13-r6-caps-grid.log`. The knobs are additive to −3,376:

| Arm (ctx16 + caps) | Total | Δ |
|---|---|---|
| shipped baseline | 547,600 | — |
| mix32768_1048576 ctx16 | 544,224 | **−3,376** |
| mix16384_1048576 ctx16 | 544,232 | −3,368 |
| mix16384_524288 ctx16 | 544,252 | −3,348 |
| mix4096_262144 ctx16 | 544,320 | −3,280 |

Per-file min-pick (shipped arms included — the never-worse rule) =
**544,212 (−3,388 = −0.62%) with wins on 20/20 backend-4 files**; no
per-file regression anywhere on the grid. Released-corpus projection
(the other 10 files unchanged): 816,341 → **812,965 B (−3,376 =
−0.41%; the pick-min form −3,388 = −0.41%)**.

The slope is flattening at the big-cap end (32768/1M vs 16384/524288 =
−8 B) — the knobs are within ~120 B of each other across an 8× cap
span: the R6 axes are near their own optimum, and the remaining
conditional floor (R1's −7..−8 %/file) stays open to a CONTEXT-side
(= predictor-adjacent, not adaptation-side) rung — priority-4 row's
territory, not R6's.

## Disposition per the standing gates

- Complete-file improvement on the pick's actual output: −0.62% over
  the mix-coded half of the corpus, −0.41% released-corpus, 20/20
  fires, zero regressions — PASSES the retention bar as measured.
- NOT shipped in this change: the knobs alter backend-4's decode
  evolution, so promotion = a format-semantics decision (v1.13 files
  must keep decoding exactly — same rule as every frozen surface). The
  candidate shape: backend id 5 = "R6 mix" (fast 32768, slow 1048576,
  ctx16 joint keys), the pick measuring backends 4 AND 5 per record
  arm at complete cost — per-backend constants are decoder-side
  (zero transport), so no wire change beyond the id. Next session
  rung = the four-site backend-id slice (the ADR-008/(i) recipe).
- Work-order axes with no lever left: fixed-rate mixtures (solo
  sweeps ≤ ±0.08%), count increment (32 = optimum), probability
  quantization (2^24 = optimum; 2^20 −24 but inconsistent) — measured
  and closed.

## Scope-of-closure

CLOSED: adaptation/quantization parameter space AROUND the shipped
coder (caps 1k..32k fast / 16k..1M slow, lr .05..5, inc 16..64,
prob-scale 2^20/2^24, 8 vs 16 joint contexts). OPEN: context RESOLUTION
beyond 16 keys (R0's order-1 −0.33 bits is the standing bound to aim
at), context rules using predictor neighbors rather than residual
magnitudes (predictor-side = frozen out of R6 by definition), and any
per-class backend-pair table (a selector word = wire change, same
free-policy bar as every format item).