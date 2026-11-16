# MNF16-X1-L R5 record (2026-11-15) — complete production portfolio competition

R4 passed (docs/mnf16-x1l-r4-record.md); R5 adds the frozen L1
candidate to the complete v1.16.0 Maximum portfolio through the
experimental integration path and runs the full competition. The
experimental representation = the shipped MNF8 container with the mix
payload replaced by the L1-coded entropy stream and the backend
discriminator set to 0xFFFF — OUTSIDE the shipped 0..7 stable namespace
(R5-experimental isolation); the lane-class mapping is wire-internal
(native per R4, no transmitted classes).

## Gates

| Gate | Result |
| --- | --- |
| R5.1 backends frozen | PASS — no production source modified; the shipped binaries, wire format and namespace untouched |
| R5.2 candidate frozen | PASS — the arm set and mode strings are exactly the recorded X1-L spec (context 20+k for class 1, base keys for class 0; shipped lr/inc/caps) |
| R5.3 identity (arm disabled) | PASS — the disabled-arm corpus sum = **810,816 B EXACTLY** (30-file reproduction: 22 trace-bound mix files + 7 be1 files + 1 MNF9 exact-generative file, sizes re-encoded deterministically) |
| R5.4 full portfolio competition | PASS — 30 files: **22/22 mix files flip to L1** (every experimental size < shipped); all 8 non-mix files keep their shipped representation (7 be1 + 1 MNF9; no mix payload exists for them to re-code at instrument level — recorded limitation) |
| R5.5 exact reconstruction | PASS — the L1-selected streams decode exactly (the R4 parity/roundtrip chain: fresh decode-side models evolved from decoded symbols, asserted in every native audit run; container semantics unchanged) |
| R5.6 per-file selection + accounting | PASS — 30-row table in research/results/x1l_r5.json; wire accounting complete (entropy incl framing + 4-byte payload word + 32-byte header) |
| R5.7 balanced identity + performance | PASS (by construction) — no balanced-path changes; the 28.92 s / balanced outputs untouched |
| R5.8 Hubble complete-file | PASS — shipped 1,452,400 B vs experimental 1,439,748 B = **−12,652 B** (identical to the recorded research wire saving; the assembly formula 32 + 4 + entropy verified against the surviving shipped wire byte-exactly) |

## The portfolio result

| Portfolio | Bytes |
| --- | --- |
| v1.16.0 shipped corpus (30 files) | **810,816** (the anchor, reproduced exactly — R5.3 identity) |
| L1-enabled experimental corpus | **769,108** |
| Portfolio gain | **+41,708 B (5.14%)** |

The complete measured research saving (~7.15% of mix payloads) carries
into the portfolio at full value: 5.14% of the whole corpus, because
the non-mix files keep their representations. The 75%-retention
sensitivity bound does not bind — retention is 100% (the research
entropy wire and the competition wire are the same measurement; the
container overhead is the identical 32+4 bytes both sides).

## R5.9 performance

- Shipped path: unchanged by construction (R5.1) — the standing
  references stand: Maximum 414.5 s, balanced 28.92 s, RSS 108.6 MB.
  The deterministic re-encodes this experiment ran (the 8 non-mix
  files + reconciliation) reused the recorded production picks.
- Instrument native costs (R4/R5 measurements): native audit + wire
  ~0.2-0.5 s per corpus file, ~6 s Hubble-class, ~98 MB RSS — the
  lane-walk share measured at 1.01x in R4.
- Marginal cost per KiB saved (instrument path): < 11 s for 40.7 KiB
  (< 0.27 s/KiB). The PRODUCTION integration timing (evaluating
  lanechild variants inside the shipped pick, sharing record
  construction per the P1 invariance principles) is pre-promotion work
  and is NOT estimated here.

## Limitations (recorded)

- 8 non-mix files keep their shipped representation: a full production
  integration could also evaluate mix-L1 for files whose pick chose
  zstd/zlib — that requires evaluating record streams the shipped
  encoder does not store, and is pre-promotion work.
- Backend 0xFFFF is an experimental discriminator only; a permanent
  identifier (and the corresponding release-engineering pass) waits for
  the promotion decision.
- MNF9 files (exact-generative class) are out of scope for the mix
  family by construction.

## Disposition

R5 PASSES with the portfolio at 769,108 B vs the 810,816 anchor.
X1-L is a promotion candidate: a compression-focused v1.17.0 release
would carry the lanechild context mapping as a new (named) backend for
the mix family, with the release suite, packaging, FFI and registry
verification per the release contract, and the production-pick timing
measurement as the remaining pre-release work. No production source,
backend identifier or release action has been executed by this record;
the promotion decision is not made here. Artifacts:
research/results/x1l_r5.json (per-file table).
