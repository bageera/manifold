# MNF14-ID4 — Production residual-cost census: measured baseline (instrument)

Status: MEASURED · committed as the baseline for the entropy-research
turn (MNF13-R9 per Jon's disposition, 2026-11-01). Production codec
untouched; no wire change; the 812,180 anchor is the baseline this
census EXACTLY reconciles through its own instrument.

## The deliverable

Release-CLI encodes (`--profile maximum` AND balanced) of the 30-file
corpus + the hubble fixture; header parse + container decompress +
exact record-grammar walk; per-tile wire-exactness validation; residual
classification by the file's own predictor deltas; zlib-9 per-tile
proxy pricing (ID2 census convention). Instruments:
`scripts/residual_census.py` + `scripts/residual_census_report.py`;
data `/tmp/id4_census.csv` (regenerable from the log-equivalent run).

## Headline numbers

- **812,180 B** — maximum-profile 30-corpus total, reconciled
  BYTE-EXACT through the census instrument itself
  (`size_max = regret + size_bal` reconstructs every row; the two
  mix-winner token files carried their true maximum sizes 552/488).
- **3,341 record tiles in-corpus, all mode 0** — within this corpus not
  one tile prefers a reference, color transform, law arm, or const
  field; the pick's record grammar is at its own local floor. MNF9
  magic carries solid. Token wires: document (806 near), screenshot
  (3,044 near + 36 match).
- **6,675 tile reconstructions asserted == source pixels** — from raw
  wire bytes (predictor inverse + REF-field inverse + planar/interleave
  unpack), across maximum and balanced files and both record-token
  shapes. Grammar-walk consumed == stream length on every file.
- Residual-cost concentration (z9 proxy share by the file's own
  predictor deltas): **edge/texture class 98.1%**, mixed 1.8%,
  sparse-small 0.1%, texture-heavy 0 tiles in-corpus.
- Hubble (the corpus-outside named class, the only firing site of the
  law/reference arms): mode 8 × 146, mode 7 × 91, mode 6 × 20,
  refs × 20 — and its tile residuals are the same full-amplitude
  profile (nz_mean 0.99) as the in-corpus photos.

## What this establishes (and what it does NOT — the scientific boundary)

ESTABLISHED: the tested corpus's encoded cost is overwhelmingly
concentrated in tiles classified by large-amplitude predictor
differences, and the existing geometric/reference arms have NO measured
firing opportunity on it. Every additional geometric law has therefore
failed for a measured reason, not a lack of fitting power.

NOT YET ESTABLISHED (recorded as the boundary conditions of the next
instrument, per Jon's qualification):
1. **Irreducibility is NOT proven.** 98.1% identifies a cost
   concentration, not a Shannon lower bound. The edge class conflates
   several statistically different structures (any tile with ≥1 delta
   ≥ 32 lands there); a production entropy attribution must split it
   before any model is proposed.
2. **No direct statistical attribution of the maximum-profile
   backend-5 payloads exists yet.** The walker reads production-true
   records where the container is zlib/zstd (7 files maximum; 23 files
   walked at balanced, labeled `walk_src=balanced`, because backend-5's
   range-coder stream has no python decode); pixel-reconstruction and
   grammar-walk checks are strong on every walked file, but the
   stage-2 re-costing caveat applies to the 23: per-tile mode/predictor
   picks can move between profiles. R9-A's byte-equal extraction is
   the required fix.

## Consequences → MNF13-R9 (frozen representation, entropy models only)

The five stages, pre-registered: R9-A byte-equal extraction of
production backend-5 symbol streams (measure exactly what the shipped
coder sees); R9-B prequential probability-cost census (adaptation vs
coding overhead); R9-C R7 hierarchical context backoff on the actual
production stream; R9-D R8 higher-order context with backoff; R9-E
complete-file maximum-profile gate. The earlier R7/R8 proxy results are
candidate models only; the R1 conditional-floor estimate (−7..−8% per
photographic file) is a hypothesis. No representation or grammar edit
is licensed by ID4; every R9 verdict is complete-profile against the
812,180 baseline at the shipped pick.

## Method honesty (bugs caught mid-run, all by instrument assertions)

- CLI argv form (`encode <in> -o <out>`) — caught by argparse-exit
  assert, run 1.
- Mix-backend (4/5) payloads are range-coder streams with no python
  decode — caught decompressing blocks; fixed with the LABELED
  balanced-walk substitution above (`walk_src`), never silent (run 2).
- The wire-exactness validator CAUGHT a real instrument bug: the first
  python predictor filled the first row/column with 127; the shipped
  edge semantics (`mnf6.rs` `med_predict`, `mnf8.rs` `image_predict`)
  are (0,0)=127, top row = left, left col = ABOVE. The dbg probe
  (wire col-0 residual 64 = top neighbor) proved it; the corrected
  vectorized walk validates 6,675/6,675 tiles (run 3 → run 4).
- Token-branch size attribution: `size_max` initially copied the
  walked (possibly balanced) blob; corrected to the maximum file's
  true size; corpus total reconciled EXACT at 812,180 post-fix.

## Provenance

- Binary: `target/release/manifold` (fresh `--features image-io`
  build, commit tree 9222bef-era; post-ID2 grammar).
- Fixtures: testdata PNGs (lossless truth) + hubble via the CLI's own
  JPEG decode (`/tmp/hf1_hubble_decoded.png`) — the same truth source
  ID2's isolation used.
- Per-file rows carry flags, backend, walk_src, mode hist, token tags,
  class stats (tiles / z9 / nz_mean), validated-tile count, sizes and
  regret (maximum − balanced; corpus regret −20,611 = mix6's share).