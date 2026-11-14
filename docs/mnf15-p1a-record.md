# MNF15-P1-A — per-file encoder search-cost attribution (recorded-inputs join)

**Status**: measurement line opener for the P1 proposal ("Exact portfolio
search optimization": same pick, same bytes, less work). No encoding was
re-run; every number here derives from committed records and the recorded
per-file CSV, through a committed instrument.

**Instruments**: `crates/mnf-mans/scripts/mnf15_p1a_join.py` reads
`.research/r9/r9e_rows.csv` (the R9-E corpus-gate per-file table: release-CLI
encode+verify per file, old = pre-backend-6 binary `1baf847`, new =
v1.15.0 `586370e`), asserts the record's anchors byte-exactly, and emits
`docs/mnf15-p1a-decision-table.csv` (per file) + `docs/mnf15-p1a-gate.log`
(this run's summary, committed beside the record).

## Method-honesty notes

- The proposal's derived figure "202 seconds per additional kilobyte" divides
  1,364 B by 1024 as 1.364 kB; the measured aggregate is **206.5 s per kB
  saved** (+275.1 s ÷ 1.332 kB). Same conclusion, corrected constant.
- The per-file CSV records the file-level PICK (winning backend + sizes +
  wall + RSS), not per-candidate timings. "Winning vs losing candidates" is
  therefore decided at FILE level: the 15 files whose winning arm IS backend 6
  ("mix6-won", including the byte-equal tie autumn s900001) versus the 15
  files whose pick is another backend ("mix6-lost" — backend 6's 40 arms were
  fully evaluated and declined by the min()).
- Wall times are summed sequential per-file CLI walls on one machine
  (macOS, one core per R9-F's record); hubble timings are recorded separately
  and are NOT transferred pixel-scaled (R9-F: "sizes, not times, are the
  R9-F claim").
- Rounding deltas are reported from the CSV's seconds fields (3 decimals).

## Inputs verified against the records (no reruns)

| Claim                          | Source                                    | Verified |
|--------------------------------|-------------------------------------------|----------|
| 812,180 → 810,816 B (−1,364)   | R9-E record § corpus gate                 | asserted |
| encode 870 → 1,145 s summed    | R9-E record § corpus gate / CSV           | 870.2 / 1145.3 |
| mix6 fires 15/30               | R9-E record / CSV `pick_backend`          | 15 (incl. the tie) |
| candidate matrix 32 rec + 8 tok per backend; Balanced 40; Maximum 160→200 | `src/mnf8.rs` `encode_mnf8` (`:232–283`): 4 predictors × 2 layouts × 4 REF record arms + 2 caps × 4 REF token arms; `backend_candidates()` = 5 backends on Maximum (v1.14: 4 + zstd19/zlib split), Balanced = zstd19 only | code-matched |
| ID4: 3,341 record tiles all mode 0 (corpus) | `.research/r9/id4_census.csv` 30 production rows | exact |
| R9-F hubble +0 both profiles; ~950 s max / ~40 s bal, one core | R9-F record | quoted |

## Measured (the P1-A question)

**The added search time concentrates on candidates that do not ship.**

- Corpus: +275.1 s (+31.6%) buys −1,364 B (−0.168%) = **206.5 s/kB**.
- Split by the file-level pick:
  - **mix6-won (15 files): +104.5 s** for all of **−1,364 B** (78.5 s/kB on
    that side), four files carry the win via the joint backend×arm sweep
    (largest single win dusk s3100002 −288 B at 33.7 s/kB);
  - **mix6-lost (15 files): +170.6 s** for **0 B** — **62.0% of the added
    corpus time buys nothing at the file level**.
- The top time penalties are all mix6-lost files: screenshot +42.5 s (backend
  5 kept, bytes ±0), noise +16.5 s, repeat +14.1 s, solid +10.5 s, plaid
  +9.9 s (all backend 1/0 picks, bytes ±0).
- Added time is pixel-scaled, not class-scaled: corr(pixels, Δtime) = 0.97
  (n = 29; plaid's filename carries no dims), ≈ **219 s per megapixel** of
  added Maximum-profile search — consistent with R9-F's hubble ~950 s
  (1,000×872 + 3,465 tiles) where backend 6 also fires zero.
- Memory: measured and NOT a bottleneck on this corpus — peak RSS
  95.5 → 96.1 MB corpus-max, per-file Δmax 1.9 MB. The proposal's
  "Undetermined" memory row is resolved for the 30-file corpus (external
  assets unrecorded; see Boundaries).
- Backend win distribution (30 files): mix6 15, mix5 7, zstd19 7, zlib 1 —
  every backend still wins files; pruning by class would need the joint
  structure intact (R9-E's reconciliation already showed backend choice moves
  the winning predictor/planar/REF combo).

## What this assigns to the P1 rungs

- **P1-C (exact early termination)** — the mix6-lost class (+170.6 s, 0 B)
  is the target surface. The pick's min() needs every candidate's complete
  size, so pruning must be by PROVEN lower bounds, not estimates; the
  container/backend boundary is the place where a bound could be honest:
  `encode_stream_coder` builds the backend-independent record stream and then
  hands it to one of five backends (`src/mnf8.rs:147–155`), so a
  complete-size accumulator per coding stage is structurally available.
- **P1-B (invariant computation reuse)** — the same record stream is rebuilt
  5× per (predictor, planar, REF) combo, once per backend; all five backends
  consume `buf` (`:147–155`). Proving record-stream identity across backends
  for fixed flags is P1-B's first gate; identity ⇒ 4 of 5 rebuilds are pure
  waste. Predictor planes likewise depend on (kind, planar) only.
- **P1-D (backend execution efficiency)** — mix6's per-symbol mix + live
  parent updates are inside `mix_encode`; R9-E names "the 5th sweep arm plus
  mix6's per-symbol mix/parent overhead" without splitting the two; P1-A's
  file-level data cannot split them either. P1-D's entry measurement is that
  split.
- **Heuristic pruning** stays a separate-profile proposal, unchanged by this
  record: the pick's joint measurement is what the evidence protects.
- Compression/new-model work stays frozen at the v1.15.0 anchor: R9-F (+0
  external) and ID4 (corpus mode floor; hubble's 277 non-mode-0 tiles are the
  external scope) already close those directions for this corpus class.
- The 30% target (1,145 → ~802 s) is a proposal target, now pointed at the
  measured 170.6 s lost-class + rebuild-waste surface; P1-A does not validate
  it.

## Verdict

P1-A stands as a measurement record: **62% of the added v1.15 Maximum-search
time is spent on files where backend 6 is evaluated and declines, and the
added time scales with pixels (219 s/Mpx) because arm evaluation — not arm
wins — consumes it.** The economics of discovery, not the models, are the
measured bottleneck; memory is measured-absent on this corpus. The next
rung is P1-B's identity proof, with P1-C's lower-bound rule designed against
the same file-level class this record isolates.

## Scope

Corpus-scoped (the 30-file production corpus at the release CLI, maximum
profile; Balanced unchanged by construction). Candidate-level/per-arm timing
attribution does not exist in the recorded CSV and belongs to P1-B/P1-C
instruments. External-class (hubble) RSS was not recorded by the R9-F
instrument; only its wall times and byte anchors are cited. One machine, no
performance isolation (per R9-F's own caveat).
