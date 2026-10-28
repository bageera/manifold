# MNF10 two-track opener: Bit-Cost Autopsy + Generative Discovery (2026-10-28)

## The Autopsy readout (independent walk of the shipped v1.9.0 pick; scripts/autopsy.py, docs/mnf10-autopsy-gate.csv)

30-file corpus, wire sizes = the shipping pick (TOTAL file = 951,455):

| bucket | files | readout |
|---|---|---|
| record mode-0 streams (PRED_LEFT/PLANE/MED) | 24/30 | byte bulk = per-tile MED/LEFT residual bodies inside z19: photos dominate (stream 57-107 kB/file, files 15-62 kB) |
| token wires (FLAG_TOKENS) | 4/30 | gradient 254 B; screenshot 678 B (3,044 near + 36 match + 19 lit; stream 12.5 kB -> 678); document 923 B (806 near + 18 lit); blocks 1,277 B |
| overhead/consumed mismatch | 0 | walk consumes exactly the decompressed stream on all 30 (grammar-accounting exact) |

Where the bytes actually are: photographic residual bodies (the LZ/
predictor endgame — Discovery's exact families do not apply there
without a shared-model layer) and the few token wires where the near
form (ADR-005) is already doing the work the Discovery families probe.

## Autopsy -> Discovery hand-off

Discovery probes the exact-rule question per file's residual plane (or
the source itself). The pick measures and retains. The two-track
conjecture: Autopsy names the expensive structure; Discovery re-
describes it when an exact integer rule generates it. Files WITH exact
rules (the probe corpus: ramps, recurrences, corner blends, segment
ramps) re-described at 39-81 B; files without (photos, noise, resize
kernels) keep the pick — recorded honestly as the boundary of track 2.

## Disposition

- The Autopsy instrumentation is committed (scripts/autopsy.py + gate
  CSV) and reproduces the walk exactly (consumed == stream on every
  file; a near-token off-by-one desync in the walker was caught by the
  consumed-check and fixed before any number was quoted).
- Next: MNF10 promotion ADR (docs/mnf8-adr-007-mnf10-genfunc.md) +
  mnf-mans/src/mnf10.rs prototype (real decoder, 5/5 tests).
