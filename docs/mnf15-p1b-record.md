# MNF15-P1-B — exact portfolio search optimization: record-stream construction reuse landed

**Status**: IMPLEMENTED + all gates green. The pick now builds every
candidate record stream ONCE per (predictor, layout, reference, coder-proxy)
pair and compresses that shared stream under each of its backends; the
winner is unchanged (byte-identical corpus). Wire: untouched. Decoder:
untouched. Backend ids: untouched.

**B1 contract (the precondition this line proved)**: at fixed
(predictor, layout, reference) the record stream depends only on the coder
PROXY — `proxy_cost(coder, …)` drives every per-tile mode choice (modes
0-8, refs) — so the four `Coder::Zstd(19)`-proxy backends (mix6, mix5,
mix4, zstd-19) share ONE stream and the zlib arm (Coder::Zlib proxy)
selects its own. The zlib split is a MEASURED fact of the shipped design,
not a limitation: the idealized 160→32 constructions (5 backends × 32
record arms) is honestly **160→64**; the token wire carries NO proxy at
all, so 40→8. Per Maximum encode: constructions 200 → **72 (−64%)**.

## Instruments

- `docs/mnf15-p1b-candidates.sha` — B2/B3 gate `p1b_candidate_identity`
  (in `src/mnf8.rs`): all 200 Maximum candidates + the pick, enumerated in
  the ORIGINAL backend-outer order, FNV-1a 64-bit, generated at the
  pre-refactor vintage (P1B_UPDATE=1) and asserted (the gate passed
  against the committed manifest byte-exactly, 312.8 s).
- `docs/mnf15-p1b-baseline.sha` — pre-refactor pick digests (30 files,
  CLI-sha256, total 810,816 asserted; same-machine wall in
  `.research/p1b/picks_old.csv`, 1,127.9 s).
- `docs/mnf15-p1b-balanced.sha` — pre-refactor balanced digests (30
  files, 832,791 B total).
- `crates/mnf-mans/scripts/mnf15_p1b_verify.py` — B4/B5/B6: post-refactor
  CLI re-encode of both profiles with `/usr/bin/time -l` RSS; asserts
  every digest against the two committed manifests; wrote
  `docs/mnf15-p1b-gate.log` (committed).
- `p1b_build_code_split` (B5 instrument, retained): matched-environment
  construction-vs-coding attribution on gradient.

## Measured (all gate rows)

| Gate | Result |
|------|--------|
| B2/B3 candidate + tie-rank identity | manifest reproduced byte-exactly (3 files × 200 arms + pick), 0 regenerated |
| B4 Maximum identity | **30/30 sha256 identical, total 810,816 = the anchor** |
| B5 split (gradient, one core, test binary) | 72 constructions **39.28 s** in a **61.46 s** pick = **64% construction / 36% coding** |
| B5 corpus wall | post-refactor 710.2 s vs 1,127.9 s same-machine baseline = **−37.0%** (vs the recorded 1,145.3 s: −38.0%) |
| B6 balanced identity | 30/30 sha256 identical (832,791 B) |
| B6 balanced timing (matched, idle machine, back-to-back) | 165.92 s → **164.17 s (−1.1%)**, peak RSS 97.6 → 98.1 MB — no regression; Balanced's surface is one proxy class, so the reuse gain is small there (see [mnf15-p1b-acceptance.log](mnf15-p1b-acceptance.log)) |
| B7 external (hubble, wire-decoded round trip) | maximum **byte-identical** (1,452,400 B; 585.9 s idle, RSS 136.5 MB) and balanced **byte-identical** (1,502,259 B; 152.6 s) vs the v1.15 reference wires — law selections preserved inside the compared bytes |
| B6 RSS | corpus peak **100.6 MB** (was 96.1 = +4.7%); worst per-file delta vs the r9e column +13.2 MB (gradient 100.6 vs 87.4) |
| Robustness | fuzz 525 trials, 0 panics, 1 accepted mutation diffed to the recorded benign class |
| Release suite | 91 tests / 0 failed (release profile) + doc `-D warnings` clean |

Selection-identity construction: the pick walks representation-first
(kind × planar × reference, proxy-class build once, then the class's
backends; token arms per cap × reference) while every candidate keeps its
ORIGINAL enumeration rank (`backend_idx*40 + arm_idx`, token arms at
32..39) and the winner is min over `(length, rank)` — equal sizes keep
the earlier original candidate, byte-for-byte the old backend-outer
loop's first-strictly-smaller semantics. No rank bookkeeping beyond the
formula; the identity gate proves it empirically.

## Performance attribution (the B5 question)

- The construction share is **~64%** of Maximum search on the
  representative fixture — matching the corpus-level −37.0%: eliminating
  the redundant zstd-proxy rebuilds (3 of 4 per flags-set, i.e. −96 of
  160 record builds) plus the token-wire rebuilds (−32 of 40) removes
  most of the construction work, and the residual is coding.
- Timing provenance note: the corpus comparison is
  same-machine/one-machine wall (710.2 vs 1,127.9, both captured within
  hours apart on identical builds' discipline — the matched comparator
  rule); the RECORDED 1,145.3 s (R9-E, Nov 4 binary) is quoted for
  continuity, not precision, and R9-F's hubble ~950 s/arm is a different
  asset on a different machine state — neither transfers as an absolute.
- The 30% target (801.7 s) is BEATEN at the matched-environment
  measurement: 801.7 s = −28.9% from 1,127.9; measured −37.0% (710.2).
  A below-target result was pre-authorized as retentive; this is
  above-target.

## Next attribution (P1-C vs P1-D)

- B5's split says construction is the majority of MAXIMUM search, and
  its largest remaining non-shared piece is the zlib proxy class (its 32
  streams are exclusive to one backend that wins 1/30 files). P1-C's
  lower-bound rule now has its target surface quantified; the
  construction-vs-coding line for the mix backends (coding side) belongs
  to P1-D's cost split, which this record does not yet subdivide.
- P1-C/P1-D remain CLOSED pending a work order; this record's scope is
  the reuse landing.

## Scope

Corpus-scoped (30-file production corpus, maximum + balanced profiles,
release CLI). The external hubble surface IS covered: acceptance re-encode
from the wire-decoded pixels (lossless round trip) reproduced BOTH
reference wires byte-identically (maximum 1,452,400 B, balanced
1,502,259 B) — see [mnf15-p1b-acceptance.log](mnf15-p1b-acceptance.log).
One machine; the B6 balanced timing and all walls in the acceptance log
are idle-machine back-to-back measurements. RSS measured via
/usr/bin/time -l (bytes→MB as on this macOS build; the r9e
column remains the committed comparator). The B5 split instrument's
absolute seconds are measured inside the test binary — the honest
corpus-level wall lives in mnf15-p1b-verify's log; the instrument's ROLE
is the construction/coding RATIO, which is what the record quotes.
