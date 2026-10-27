# Research-paper claim validation (manifold measurements vs published claims)

Every entry: a published claim, our measurement, and the verdict. Sources
are cited; the repo reproduces each measurement (scripts/ + docs/).

## 1. Electron-event representation (PMC7467176, bioRxiv 466795 / IUCrJ 2020)

| claim | our measurement | verdict |
| --- | --- | --- |
| EER RLE (7-bit/subpixel) stores counting-dose movies far below dense formats | EMPIAR-10943 Falcon 4i wire = 3.93 MiB/frame vs 16 MiB dense-u8 render (4.1×) | CONFIRMED |
| Shannon bound: frames compress further only when event density p ≠ 0.5 (H(p) < 1) | measured p = 0.52 on the 7 e/px/s super-res render → H ≈ 1: NO headroom at that dose/render | CONFIRMED (and explains why our sparse-event arm cannot beat the wire there; lossless record) |
| u=4 subpixels cost extra 2·log2(u) bits/electron | explains the high render density that defeated the arm at high dose | CONFIRMED |
| low-dose regime: wire ≪ dense (the regime EER exists for) | PENDING probe: EMPIAR-11213 (553-frame fractionated movies, ~0.37 MB/frame wire) fetch in flight | — |

## 2. Precision requirements and data compression in CryoEM/CryoET (PMC9645247, JSB 2022, Fluty & Ludtke)

| claim | our disposition | verdict |
| --- | --- | --- |
| 5 bits sufficient for raw cryoEM data, 8–12 for intermediates/finals (uncertainty-propagation argument) | outside manifold-codec's contract: bit-exact lossless only. This paper DEFINES the boundary for a separately labeled scientific mode | NOT IN SCOPE (deliberate) |
| pair lossy bit-truncation (front) + lossless compression (back) | viable future research mode: our MNF8 could be the lossless BACK under an explicit scientific label; gated on a decision, not implemented | DOCUMENTED |

## 3. JPEG-LS / LOCO-I prediction class (Weinberger et al., IEEE TIP 2000)

| claim | our measurement | verdict |
| --- | --- | --- |
| MED prediction is strong on photographic content | 20/20 files, avg −21% vs raw+zlib-9 (the shipped FLAG_PRED_MED) | CONFIRMED |
| content-dependence: no single predictor dominates | plane-prediction wins flat-UI; MED wins photos (v12 ablation) | CONFIRMED |

## 4. Zstandard (RFC 8878)

| claim | our measurement | verdict |
| --- | --- | --- |
| zstd ≥ zlib as a general replacement | on PREDICTION RESIDUALS: wins (backends v13, −68% mid-rate); on smooth gradient literals: LOSES to zlib-9 at every level 3–22 | PARTIAL: residual statistics, not general reputation, decide |

## 5. Low-rank / PCA (Eckart–Young; the 'manifold hypothesis', Narayanan et al. 2015)

| claim | our measurement | verdict |
| --- | --- | --- |
| variance-optimal rank k reconstruction | the math holds; as a LOSSLESS predictor the quantized-basis + integer-synthesis pipeline inflates residuals 131–191% | CONFIRMED as math, FAILED as codec (retention gate; docs/subspace-v01.md) |

## 6. Dimensional-folding hypotheses (the recent proposal docs)

| claim | our measurement | verdict |
| --- | --- | --- |
| reversible 3D→2D reindexing loses no information | bijections verified exact | CONFIRMED (trivially) |
| folding enables compression | measured 1.0× best case, up to 7× worse; fold = reordering, redundancy = mechanism | REJECTED as a mechanism |
| structural repeats/phases are the real wins | slice-rule 0.48×, slice-phase 0.26× (synthetic) | CONFIRMED (candidate items pending real stacks) |

**Method note:** every 'CONFIRMED' row links to a measurement that ran
inside this repo with the same-coder parity and exactness assertions the
retention gate requires, not to vendor numbers.
