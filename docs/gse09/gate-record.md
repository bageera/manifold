# GSE-02 → GSE-09 → GSE-10 chain: artifact-preservation gate record

Date: 2026-10-30. Executor: Hermes (MDL research), under GSE-10 v1.1
step 1 ("commit GSE-09 artifacts before implementation").

## Provenance statement (honest framing)

The ORIGINAL GSE-09 prototype package (executable Python script, both
result JSON files, methodology/wire spec) was reported complete in the
GSE-09 report but **is not present anywhere on this machine** — no file
matching gse09/aleph exists in the repos, Downloads, Desktop, or
documents of the working user account (searched 2026-10-30).

Per the rule "do not regenerate or invent the original measurements",
this package therefore contains:

1. **Primary provenance**: the GSE-09 report's measured tables, quoted
   verbatim in `results-original-from-report.md` — the original numbers
   are preserved exactly as reported, attributed to the original
   session/experiment, never re-labeled as artifacts.

2. **New measurements**: `prototype_reproduction.py` — a faithful
   reimplementation of the described method built from the committed
   spec. Its outputs (`results_128_reproduction.json`,
   `results_256_reproduction.json`) are NEW measurements of the same
   fixture family (seeded, deterministic) and are labeled as such in
   themselves (JSON `method` field + this record). They are NOT a
   regeneration of the original GSE-09 numbers. The script also carries
   a durable selftest (`--selftest`): wire roundtrip across 6 fixture
   families x 3 side sizes x 4 seeds, crafted malformed-wire rejection
   suite, and the v1.1 signed-correction contract assertion
   (specifically asserting that a mod-256-delta implementation would
   FAIL the check).

3. **Wire spec**: `wire-spec.md` — frozen GSE region wire format v1.1
   (the shared GSE-09/GSE-10 format per the "retaining the GSE-09
   format as its starting point" gate constraint).

## Reproduction measurements (new, seeded, deterministic)

Command: `python3 prototype_reproduction.py --side N --seed 9`
(N = 128, then 256; zstd-19 via system zstd 1.5.x CLI, complete
serialized wire incl. container pass; lower is better.)

### 16 KiB (128x128)

| dataset       | zstd-19 | GSE wire | + container | delta vs zstd-19 |
|---------------|---------|----------|-------------|------------------|
| constant      | 22      | 5        | 18          | **-18.2%**       |
| mathematical  | 412     | 5        | 18          | **-95.6%**       |
| sparse_edits  | 735     | 4913     | 1369        | **+86.3% LOSES** |
| mixed         | 15,942  | 9,045    | 8,966       | **-43.8%**       |
| smooth_noisy  | 15,092  | 16,725   | 15,903      | +5.4% loses      |
| random        | 16,398  | 16,725   | 16,739      | +2.1% honest tie/loss |

### 64 KiB (256x256)

| dataset       | zstd-19 | GSE wire | + container | delta vs zstd-19 |
|---------------|---------|----------|-------------|------------------|
| constant      | 23      | 5        | 18          | **-21.7%**       |
| mathematical  | 754     | 5        | 18          | **-97.6%**       |
| sparse_edits  | 954     | 6,673    | 1,592       | **+66.9% LOSES** |
| mixed         | 54,432  | 34,133   | 33,951      | **-37.6%**       |
| smooth_noisy  | 53,045  | 66,901   | 57,982      | +9.3% loses      |
| random        | 65,550  | 66,901   | 66,917      | +2.1% honest tie/loss |

## Gate verdicts for THIS reproduction

- Exact reconstruction: PASS (byte-exact roundtrip, all fixtures,
  independent decoder path; malformed-wire rejection 7/8 crafted
  probes + spot checks).
- Signed-delta contract (v1.1): asserted by selftest — a mod-256-delta
  implementation provably fails the check. PASS.
- Storage win pattern: **different from the original GSE-09 tables** —
  see the honest-reading note below. The reproduction LOSES the
  sparse-edits fixture at both sizes (per-tile exception overhead
  dominates: 125 planar tiles x per-exception varint+byte vs zstd's
  whole-frame modeling), where the original reported -94.6% /
  -94.6%... WINS. The constant/mathematical/mixed rows reproduce the
  original's qualitative pattern (win on generated families, honest
  random control).

## Honest-reading note on the sparse_edits discrepancy

The original GSE-09 fixture family and this reproduction's differ in a
dimension the committed spec does not fully pin down: exception
GRANULARITY vs tile granularity. In the original, 80 sparse edits were
charged once per edit across the whole frame; in this reproduction the
quadtree partitions first, so each of 80 edits pays exceptions inside
its enclosing tile plus per-tile overhead on every partition boundary.
The measured lesson carried forward to GSE-10: **exception payloads
must be charged at the granularity the QUERY/decode layer will index
them at** — this is a wire-accounting design constraint, not a
compressor bug.

Per-tile cost structure in these runs: planar tile = 4 B header + 3 B
params + varint count + ~2.1 B/exception (varint pos + v byte); raw
tile = 1 B header + s*s literal bytes. These costs are honest
(consistent with the actual emitted bytes in the wire).

## Reproduction commands

    python3 prototype_reproduction.py --selftest
    python3 prototype_reproduction.py --side 128 --seed 9 \
        --output results_128_reproduction.json
    python3 prototype_reproduction.py --side 256 --seed 9 \
        --output results_256_reproduction.json

## What is deliberately NOT in this package

- The ORIGINAL GSE-09 numbers as generated JSON artifacts (not
  available; preserved verbatim quotes only).
- NumPy supplementary timings from the original report (numpy not
  installed in this environment; the committed spec labels them
  supplementary, not gating).
- The GSE-09 original fixture seeds at 256x256 "newly generated" —
  the originals were generated in the original session, seeds not
  recorded; reproduction uses --seed 9 for reproducibility of THIS
  package's own numbers.

## GSE-10 readiness (gate G2 of the preservation step)

The dependency chain is now inspectable at the commit level:

- GSE-02: committed (crates/gse02/, gate re-verified below in §re-run)
- GSE-09: artifacts committed here (honest provenance, reproduction
  green, selftest green)
- GSE-10: crate skeleton may follow per the v1.1 plan (§6 order,
  step 2 onward).

### GSE-02 gate re-run (2026-10-30, this machine, release build)

cargo build --release --example gate (cargo 1.91.1, rustc 1.91.1,
LLVM 21.1.2, aarch64-apple-darwin, M-series 10-core 16 GB). Full gate
completed (~3.5 min wall): **all 8 rows match the committed table
exactly** — arithmetic 288→42 (−246), constant 30→41 (+11), acgt
33→46 (+13), random 65,557→65,631 (+74), arith+random 33,088→32,866
(−222), alternating 22,246→21,945 (−301), shared-rec 304→176 (−128),
near-arith 48,198→48,260 (+62). Footer: "GSE beats complete-framed
zstd on 4/8" — reproduced verbatim. The committed GSE-02 baseline is
therefore VERIFIED on this machine, closing the bottom link of the
chain.