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