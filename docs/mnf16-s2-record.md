# MNF16-S2 record (2026-11-15) — production conditional attribution through the R9 bridge

Stage 2 of the MNF16-S1 program, per Jon's contract and three decision
gates. Instrument: `scripts/mnf16_s2_bridge.py` (reuses
`mnf13_r9a_gate.extract_stream/bridge/bucket/static_floor` and the
stage-1 feature machinery with stage-1's frozen bucket edges — the R9
extraction research is NOT reopened). Scope: every corpus file whose
Maximum pick is a mix payload (backend 4/5/6), wire-decoded through the
shipped decoder's own symbol stream (`bridge 'p'`), per-symbol
production contexts recomputed from `r9_audit.rs key_of` (backend 6 =
`mix32768_1048576-lr15-inc32-ctx16-refineA-warm2p12-share`, 20
contexts; backend 5 = 16-key rule; backend 4 = 8-key rule).

## Gate 1 — production identity (PASS)

All 22 mix-payload files: symbol stream parses (record-law-aware walk,
`_walk_check`), per-symbol -log2(p) reproduces with cost vs the shipped
range-coded payload within the rANS container overhead
(`coder_overhead_B` 1.1-7.3 B across all files; adapt regret vs the
static production floor 88-736 B — R9-era magnitudes). Context
populations sensible (heavy-band children 16-19 fire on photographic
files; base keys dominate on flat).

## Gate 2 — decoder causality (PASS, by construction + walk)

Features are same-plane earlier positions of the same decode stream.
The annotated (mode-0 tile) symbols carry their full-stream indices;
every feature read (west/north/nw within plane; ±8 gradient) resolves to
a strictly earlier index in the bridge-decoded stream — i.e. the exact
order the shipped entropy decoder consumes. No raster-only assumption.

## Gate 3 — incremental information (mixed; G8 advances, D negative)

Arms (static conditional-information layer, 80/20 causal prequential,
N = 2,539,584 production symbols; S2-5 on the backend-6 subset,
N = 1,225,536):

| Arm | in-sample | prequential | verdict |
| --- | --- | --- | --- |
| S2-1 G8 (global) | 30,171 B (0.095 mb8/sym) | **+18,460 B (+0.058)** | positive, per-file fragile (−0.65…+1.86) |
| S2-2 disagreement | 12,429 B (+0.039) | **−19,594 B (−0.062)** | NEGATIVE — C6 already captures it |
| S2-3 joint | 44,546 B | −53,422 B | negative (double fragmentation) |
| S2-4 shuffled control | 2,769 B | −23,216 B | control behaves (no false signal) |
| S2-5 G8 heavy-band split | (+0.055 avg) | **+8,594 B (+0.056)** | positive on EVERY b6 file (0.38…0.61 per file) |

Readings:

1. G8's information survives the production join: +18,460 B prequential
   over the production context C6 (vs stage 1's weak-context
   +290,408 B — the production model already absorbs most of the
   coarse-scale signal; the residual is genuine).
2. The heavy-band-split form (S2-5, splicing the four G8 children only
   into the already-large heavy contexts — the r9d refinement
   architecture) is the stable shape: positive prequential on every
   backend-6 file measured. Global 20×8 splicing loses through
   adaptation overhead on small files — Jon's S2-5 hypothesis confirmed
   by measurement.
3. Predictor disagreement's stage-1 signal dies under the production
   context (−0.062 prequential): the 20-context prev-symbol schedule
   already carries it. HONEST NEGATIVE, recorded as such.
4. The distribution-preserving shuffled control is negative — the G8
   gain is spatial, not a statistical-pooling artifact.

## Population honesty

Mode-0-tile symbols only (as stage 1; law-mode tile residuals are per-
pixel but semantically law-predicted — deferred). The four stage-1
skips remain scoped exclusions (token wires; solid container framing).
Backend-1 picks (gradient/blob/plaid/poster/noise/repeat/solid) are
zstd-container files = no mix symbols to audit.

## Disposition (Jon's table)

- G8 cross-scale gradient: primary candidate → advances to the
  coder-level prequential arm (a `r9_audit` extension mode splicing the
  S2-5 heavy children with per-symbol G8 features through the actual
  adaptive mix schedule) — the wire-accounting rung before any format
  claim. The Hubble holdout runs there (frozen edges, single shot).
- Disagreement D: recorded prequential-negative against C6. No backend.
- Joint: negative (fragmentation). S2-5's split is the form to carry.
- No production wire changes; v1.16.0 anchors untouched; no new backend
  identifiers.

Run artifacts: `/tmp/mnf16_s2_full.log` (per-file rows + AGG),
instrument `scripts/mnf16_s2_bridge.py`.
