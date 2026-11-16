# MNF16-X1-L — Consolidated production freeze record (backend 7)

Backend 7 = the native production integration of the X1-L L1 lane-class
mechanism (40 frozen contexts, fresh lane children keyed base+20, exact
research adaptation schedule, zero transmitted classification).
Spec records: `docs/mnf16-x1l-record.md` (discovery), R4 (`…/x1l-r4-record.md`),
R6.2 closure (`…/x1l-r62-record.md`). This record consolidates R6.3–R6.12.

## 1. Frozen invariants (measured)

| Invariant | Value | Verification |
| --- | --- | --- |
| Disabled portfolio | 810,816 B | v1.16.0 binary re-encodes all 30: per-file digests identical, no mismatches |
| Enabled portfolio | 768,303 B (−5.24%) | all 30 serialized blobs SHA-identical across the instrumented and cleaned vintages |
| Aggregate savings | 42,513 B | 42,276 archived (41,708 reference + 568 sibling-candidate) + 237 non-archived (plaid alone; document/screenshot sit inside the archived 22) |
| Selections | 23 × backend 7, 6 × zstd-1, 1 × MNF9-class | 29 MNF8 + 1 MNF9; backend 7 won across 9 distinct flag triples |
| Per-file regressions | 0 | none in either phase |
| Reconstruction | 60/60 exact | both phases, every file, pixel-exact |
| Wire changes | 0 | both arms digest-equal (not merely size-equal) |
| Byte-identity after cleanup | preserved | forest payload `682e23165304b050` re-verified after fmt AND lint fixes |

## 2. Performance (R6.7–R6.9, measured, cleaned candidate)

| Metric | Value |
| --- | --- |
| R_enc = T_new/T_baseline | 1.197 (527.4 s vs 440.7 s per 30) |
| R_dec = dec_new/dec_baseline | 1.042 (2.47 s vs 2.37 s) |
| E_saving | ≈490 B per additional encode-second |
| Run-to-run variability | forest 17.6–17.9 s; Hubble 435.7/438.6 s with byte-identical output |
| Peak RSS (encode) | 97.7–97.9 MB backend-7 maximum (below the historical 108.6 MB portfolio anchor; noted: historical anchor, not a matched per-file comparison) |
| Peak RSS (decode) | 2.8 MB corpus; 17.4 MB Hubble |

Hubble (separate stress case, not folded into corpus totals): **1,438,848 B,
backend 7, flag triple 0x45 (left + interleaved + REF 1)**, encode wall
435.7–438.6 s, decode 2.5 s, encode RSS 127.6 MB (largest observed peak; not a
proven upper bound), decode RSS 17.4 MB, repeat wire byte-identical,
reconstruction digest exact (`06ec02b845343304`). vs v1.16.0 1,452,400
(−13,552) and the R5 research reference 1,439,748 (−900).

Unmeasured/disclosed (not inferred): a matched Hubble baseline encode timing
(the v1.16.0 comparative slowdown was not executed), matched per-file RSS
against v1.16.0, and a directly timed production cost decomposition. The
three-cost reading (discovery-dominated +86.8 s; single-arm entropy encode
≈0.2 s from R4's 1.01× native measurement; decode +4.2%) therefore remains an
R4-supported estimate, not independently timed production attribution.

## 3. Compatibility (R6.10, measured)

- v1.16.0 binary reproduces the frozen disabled corpus exactly (30/30 digests).
- The backend-7 decoder decodes all 30 legacy containers exactly (30/30),
  plus backend-7 containers through the CLI and the C-ABI
  (`manifold_decode`, size-query idiom, fresh process, C link smoke PASS).
- A v1.16.0 decoder rejects backend-7 containers cleanly:
  "invalid stream: unknown backend 7", rc 2, no panic.
- Public Rust API: additive only vs v1.16.0 (no removals/changes).
- MNF8 and MNF9 container classes preserved.

## 4. Robustness (R6.11, measured)

- Backend-7-specific sweep (truncations, header-word nudges, first-64-B
  record-header edits, across b7/b1/MNF9 containers): 437 cases, **0 panics,
  0 timeouts**, peak decode RSS 50.4 MB; every backend-7 container mutation
  rejected cleanly (252/252); b7 grammar walk strictly bounds-checked.
- Standing release fuzz (`scripts/fuzz.py`, maximum-profile bases incl.
  backend-7 containers): **900 trials, 900 rejected, 0 accepted, 0 panics**.
- Determinism: repeated encodes byte-identical (30/30 ×2 phases, Hubble).
- R4's 60-case robustness gate and the R6.2-C synthetic test retained.

Disclosed pre-existing corruption classes (NOT regressions: the v1.16.0
decoder behaves identically — verified by direct comparison): (1) an MNF8
backend-1 container tolerates a flags-byte edit to an arm-accepting value and
decodes different pixels deterministically; (2) MNF9 containers carry no
header/record integrity mechanism, so dims/record-byte corruption decodes to
different images within the Limits ceiling. The wire format provides no
checksum; these corruptions are therefore not claimed detectable. Fixing
them = changes to frozen legacy decode paths → deferred to a separately
controlled hardening gate, not silently altered in this release.

## 5. Production integration gates (R6.12, measured)

`make gate` (fmt-check, clippy -D warnings, full test suites incl. release,
rustdoc -D warnings): **OK**. One test required alignment with the frozen
design: `cd_pick_never_worse`'s old-baseline matrix now mirrors production
candidate availability (backend 7 has no token arm — token wires lack record
framing). Test-only change; byte-identity re-verified after every fmt/lint
fix. Package publication not executed (release HOLD).

## 6. Authorization state

- R6.1–R6.12: CLOSED (all PASS or disclosed).
- Commit/push, v1.17.0 version bump, crates.io publication: ON HOLD pending
  the final release disposition.
- Deferred: selector-optimization research (zero-regret candidate
  elimination), MNF8/MNF9 integrity-mechanism hardening.

Evidence manifests: `research/results/x1l_r6_corpus.json`,
`x1l_r67_freeze.json`, `x1l_r67_rss.json`, `x1l_r611_fuzz.json`,
`r6_2_evidence/SHA256SUMS`.
