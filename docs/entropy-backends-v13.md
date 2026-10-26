# Entropy Backends for Manifold — Stage 1 Ablation (MNF8 research, findings)

Status: research only; frozen formats (MNF5/MNF6/MNF7) untouched. This is
track 1 of the MNF8 research proposal: Entropy Backend and Residual
Probability Modeling. Stage 2 (integrated predictor-selection under each
backend) and the custom residual coder (zero-run + Rice, contextual rANS)
remain queued — this document records the completed stage-1 backend matrix.

## Protocol

Identical serialized tile streams (literal / MED-residual / MED+geom
residual), regenerated exactly as in the v1.2 ablation matrix, fed to every
backend in one process, wall time and byte counts recorded, stream
roundtrips asserted exact for every backend (ALL OK). 10 corpus images ×
3 predictor arms. zlib-9 is the baseline column.

Backends: zlib-9, zstd 3/9/19/22+LDM, brotli q5/9/11, xz -6/-9e.
Cross-check: python-zstandard vs zstd CLI -19 agreed within 1 byte
(230,715 vs 230,716) — library parity confirmed.

## Results (best backend per stream, bytes vs zlib-9)

| stream | zlib-9 | best | delta |
|---|---|---|---|
| gradient · literal | 219,782 | xz-6 54,084 | **−75.4%** |
| gradient · med | 4,321 | xz-6 1,852 | −57.1% |
| noise · all arms | 196,935 | brotli 196,869 | −0.03% (incompressible) |
| repeat · literal | 3,894 | brotli-11 2,510 | −35.5% |
| repeat · med | 1,950 | brotli-11 540 | **−72.3%** |
| screenshot · literal | 3,099 | brotli-9 867 | −72.0% |
| screenshot · med | 3,007 | zstd-22 962 | **−68.0%** |
| solid · literal | 512 | brotli 22 | −95.7% |
| blob · literal | 24,285 | xz-6 10,292 | −57.6% |
| blob · med | 10,886 | xz-9e 8,760 | −19.5% |
| blocks · med | 3,344 | zstd-22 2,732 | −18.3% |
| document · med | 1,928 | zstd-22 1,313 | −31.9% |
| plaid · literal | 108,184 | xz-6 87,100 | −19.5% |
| plaid · med | 63,374 | xz-6 45,936 | **−27.5%** |
| poster · literal | 1,716 | brotli-11 946 | −44.9% |
| poster · med | 3,710 | zstd-19 2,476 | −33.3% |

Full per-backend matrix: run
`python3 crates/mnf-subspace/scripts/entropy_backends.py` (stdout is the
canonical report; ~25 s).

## Findings

1. **No backend dominates.** xz-6 wins smooth/low-entropy streams
   (gradient, plaid, blob), brotli-11 wins structured-repetition
   (repeat, poster, solid) and small streams, zstd-19/22 wins mid-rate
   residual streams (screenshot-med, blocks, document, poster-med). The
   review's suspicion is confirmed in both directions: the best
   general-purpose compressor for Manifold is stream-dependent — exactly
   what a `maximum`-profile encoder (measure, emit smallest) resolves,
   and what a custom residual coder could beat outright.

2. **The gradient anomaly is the headline gotcha.** zstd LOST to zlib-9
   on the gradient literal stream (230,715 vs 219,782, +5%) at every
   level 3→22 with LDM — CLI and library agree, and it is real: the
   smooth gradient has no exact repeat matches (each pixel row shifts
   deterministically), so LZ77 finds nothing and zstd emitted
   store-blocks. zlib-9's Huffman layer still compressed the narrow
   delta distribution; brotli's context modeling and xz's range coder
   both crushed it (−57% / −75%). Two consequences:
   - Backend choice must be measured per-stream; zstd is NOT a safe
     default for prediction-residual data despite winning benchmarks on
     general corpora.
   - A cheap byte-level delta/spatial predictor before ANY backend is
     worth 20–230× on smooth content (delta-preprocessed gradient:
     zstd-19 998 B, brotli-11 743 B). This is the quantitative case for
     MED-in-front-of-the-entropy-coder in MNF8 — and for the custom
     residual probability model, which formalizes what the delta hack
     does ad hoc.

3. **Brotli's built-in dictionary is a free win on tiny/solid
   streams** (solid: 22 B vs zstd-25/zlib-213). Caveat for fair
   comparison: the shipped encoder would need the dictionary shipped in
   the decoder (brotli includes a ~100 KB static dictionary) — size
   accounting MUST include that cost when brotli is a format option.
   The Python binding cannot disable it; a Rust integration (brotli
   crate) should measure with/without.

4. **xz decompression is the wrong trade for a codec** despite winning
   byte counts: liblzma/xz decompression is slow relative to zstd, and
   Manifold's decoder contract is bounded-memory streaming decode. xz
   stays the archival-comparison column, not a shipping backend.

5. **Roundtrip exactness held for every backend on every stream**
   (including --ultra -22 with LDM) — backend pluggability is safe from
   a losslessness standpoint; the format work is a backend-identifier
   field plus per-backend decoder bounds (zstd window/checksum, brotli
   dictionary handling, xz memory ceiling), all specified before any
   new magic.

## Decision (research gate)

- Keep zlib-9 as the control arm and zstd(-19) as the leading
  replacement candidate for a `balanced` profile; brotli(-9/-11) joins
  the `maximum` profile's measured set; xz is measured but not
  recommended for the codec path.
- Next: stage 2 (integrated: re-run predictor selection under each of
  the two shipping candidates, selector metadata in the size
  accounting), then the custom residual coder (zero-run + Rice and
  contextual rANS via constriction) benchmarked against the same frozen
  streams. Any format change (new magic, backend field) only after the
  retention gate passes on the held-out corpus.

---

# Stage 2 — Integrated selection + custom residual coder arms (2026-10-25)

Status: research only; frozen formats untouched. Stage-2 script:
`crates/mnf-subspace/scripts/entropy_backends_stage2.py`.

Protocol: per tile, candidates (literal / exact ref / D8 geom) are costed
through the backend being measured (the `maximum`-profile simulation) with a
cheap MAD shortlist (8 of ≤128x8) paying the real backend calls; the stream
then codes with that same backend; selector metadata included. Custom arms:
zero-run + adaptive Rice (mag+sign), and planar R/G/B split coded as three
separate streams under zstd-19.

## Results (bytes; 10-image corpus)

| image | zlib-9 | zstd-19 | brotli-9 | brotli-11 | xz-6 | rice | planar(zstd) |
|---|---|---|---|---|---|---|---|
| gradient | 6,243 | 3,909 | 4,022 | **3,432** | 3,536 | 239,065 | 2,823 |
| noise | 196,935 | 196,879 | 196,869 | = | 196,936 | 442,976 | 196,686 |
| repeat | 1,865 | 990 | 976 | **913** | 1,048 | 103,334 | 1,192 |
| screenshot | 3,038 | 1,498 | **1,334** | 1,529 | 1,648 | 15,633 | 3,139 |
| solid | 511 | 47 | **29** | 33 | 176 | 133 | 258 |
| blob | 10,886 | **8,801** | 10,598 | 9,086 | 8,812 | 22,632 | 9,357 |
| blocks | 3,344 | **2,992** | 3,057 | 3,085 | 3,292 | 13,065 | 4,432 |
| document | 2,134 | **1,829** | 1,990 | 1,853 | 1,860 | 14,957 | 3,279 |
| plaid | 61,975 | 58,698 | 60,058 | 50,453 | **45,944** | 132,319 | 38,424 |
| poster | 3,569 | 2,747 | 3,061 | **2,721** | 2,892 | 74,129 | 6,897 |

Mode-utilization shifts with the backend: e.g. gradient picks 286 exact
references under zlib-9 but mostly literals under zstd/brotli; poster flips
from 27 literals to 90 exact references under brotli. The review's premise is
confirmed: the cheapest predictor/mode is BACKEND-DEPENDENT (selection must
run inside the encoder per backend, never via a fixed proxy).

Interleaved vs planar matters per backend: planar + zstd-19 cuts the
gradient's integrated stream 3,909 → 2,823 B (−28%) but COSTS blob +
5% and screenshot + 107%. Another measured, content-dependent flag for a
`maximum` profile.

## Custom residual coder (first arms — honest losses)

- Zero-run + adaptive Rice (mag+sign per tile): loses on every image with
  tile repetition (repeat: 103 kB vs 0.9-1.9 kB for LZ-family) — a pure
  per-symbol model cannot express "copy the previous tile body". Wins only
  solid (133 B — it is a run coder's home turf).
- Order-0/order-1 empirical entropy check (vectorized, framed streams
  including mode bytes): order-0 LOSES to zlib-9 broadly (screenshot 23.9 kB
  vs 3.0 kB); order-1 (prev-magnitude bucket x slot-kind, 8 contexts, table
  cost included) closes a lot (7.8 kB) but still loses. Conclusion: LZ's
  string-matching layer is doing real work on tile-structured residuals; a
  competitive custom coder must be MIXED — run-length/LZ layer for repeated
  tile bodies + rANS with contextual magnitude model for the rest
  (the JPEG-LS architecture shape: runs + Golomb for the non-run symbols).
  The constriction-based interleaved-rANS track inherits this requirement.

## Stage-2 decisions for MNF8 (pending formats)

- `balanced`: zstd-19, interleaved, no predictor-selection change (safe,
  never loses to zlib-9 in the matrix).
- `maximum` simulation results: backend-dependence of mode selection is real
  (utilization tables above); xz stays archival (wins plaid only).
- Rice/rANS arms need the mixed design; stage 1's byte-delta hack (998 B on
  gradient) also beats every integrated arm — spatial modeling in front of
  ANY coder remains the dominant lever.