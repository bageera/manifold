# MNF17-P1-D6 — Production integration record

Scope: integrate P1-D certified termination (docs/mnf17-p1d2d3d45-record.md) into
the production selector of `encode_mnf8` with matched contemporaneous
performance measurement. Version bump, tagging and publication remain on HOLD
per the authorization ledger; this document records the integrated machine.

- D6-A implementation vintage: commit `2281db4` (GL twin `1b9fd43`,
  patch-id `0c55fb694f230618`), on top of the v1.17.0 normative baseline.
- Authorization: D6 production integration + matched performance verification;
  preserve the exhaustive oracle, the certified bound, the original candidate
  ranking semantics and every byte of v1.17.0.

## D6-A — What changed (and what did not)

`src/mix_coder.rs`:

- `MixBudget { Complete(Vec<u8>) | Dominated { at } }` and
  `mix_encode_with_budget(params, data, geom, incumbent)` — the production
  budget encoder. Its completed payload is **byte-identical** to
  `mix_encode_with` / `mix_encode_with_lane` (same encoder construction, same
  model sequence, same context selection, same seal).
- D0 bound evaluation: `B(t) = 32 + 4 + 4·num_words(t)` = HEADER8 + len-leader
  + 4 per ranged word (the exact accounting of the pinned constriction 0.5.0
  queues), checked every **256** symbols with u64 checked arithmetic and a
  conservative no-elimination on overflow. Termination rule: `B(t) >
  incumbent` (strict `>`); equal crossing never eliminates (the rank tie).
  `Dominated { at }` carries the 1-based symbol count actually encoded; no
  fabricated payload or cost exists anywhere in the path.

`src/mnf8.rs` (`encode_mnf8_metrics` pick loop, restructured):

- Cheap incumbents first: per (predictor, planar) pair the zstd19 arms
  (4 references) then the zlib arms (4 references) **complete** before the
  pair's 16 mix arms. Streams are representation-first builds exactly as
  before (P1-B): one zstd-proxy stream shared by zstd19 + the mix backends,
  one zlib-proxy stream for zlib, one `build_record_stream` pass each.
- Mix arms run `mix_encode_with_budget` against the live incumbent
  (`best` = the actual best completed description so far). Completion updates
  `best` exactly as the pre-D6 loop did; domination takes no part in
  selection.
- **Original ranks are independent of the execution order**: selection is
  still the lexicographic minimum over `(size, rank)` with the untouched
  bidx·40 + arm·4 + ref ranking; token arms and the MNF9 whole-frame
  composition arm (strict-smaller, tie keeps the MNF8 pick) are untouched.
- D6 counters (`d6_take_stats`, process-local): `dominated`, `completed`,
  `dominated_symbols`, `completed_symbols`, `dominated_streams`,
  `dominated_position[4]`. Instruments, never consulted for output; printed
  only with `MNF17_P1D6_STATS=1` (CLI) and there is an env-gated
  `MNF17_P1D6_EXHAUSTIVE=1` instrument that forces every mix arm to
  completion (byte-neutral by the D0 certificate, used by the parity test).

Not changed: entropy mathematics, the decoder, the wire format, the public
API, candidate availability, tie-breaking, profiles, the CFG-test exhaustive
`mod oracle`, cfg(test) instruments of D0–D5, and every byte of the frozen
baseline (D6-B proves this end to end).

Not introduced (per authorization): memoization, heuristic pruning, adaptive
check intervals.

## D6-B — Production correctness gate

Executed by `crates/mnf-mans/scripts/final_parity.py` on the D6-A release
build (interpreter `env -u PYTHONPATH /tmp/s1venv/bin/python`, log
`/tmp/d6_chain.log`):

| requirement | acceptance | measured |
| --- | --- | --- |
| actual production selector vs exhaustive oracle | identical | `d6_pick_parity` (flat/gradient/random: terminated pick == `MNF17_P1D6_EXHAUSTIVE` pick, byte + backend) and FINAL_PARITY PASS against the frozen manifests |
| disabled corpus | 810,816 B | 810,816 B, 0 mismatches |
| enabled corpus | 768,303 B | 768,303 B, 0 mismatches |
| serialized-wire parity | 60/60 | 60/60 recon |
| Hubble wire | 1,438,848 B, identical digest | 1,438,848 B, sha16 `b2a8a10915239c99` |
| Hubble reconstruction | exact | PASS |
| held-out fixtures | zero wire differences | blob/blocks/plaid/poster included in the 30-file manifest sweep, 0 mismatches |
| MNF9 whole-frame selection | preserved | solid_256x256 → MNF9 36 B (`[mnf9] exact rule discovered: 36 B (mnf8 arm 62 B)`), FINAL_PARITY composition path unchanged |
| atlas / geometry configurations | preserved | atlas=true, geom=false CLI defaults; the geom-sensitive Hubble arm reproduces the shipped 4-B-marginal winner |
| all compression profiles | existing semantics preserved | full `make gate` green: fmt, clippy (no warnings), tests (B4 + B6 pick digests, C5 `c5_oracle_differential`, D0–D5 instruments), docs |
| release gate | green, zero warnings | GATE: OK (`/tmp/d6_gate.log`) |

In-repo differential tests added with D6-A:

- `d6_budget_differential` — the budget encoder is byte-identical to the
  exhaustive encoder on completion (four backends × degenerate + structured +
  20,000-random fixtures, plain and lane/walk forms), an exact-final incumbent
  always COMPLETES (the equality rule), and under-final incumbents produce
  only genuine certificates (each verified against an exhaustive re-run; no
  state leaks between runs).
- `d6_pick_parity` — the integrated production selector with certified
  termination picks the byte-identical winner as the same selector with the
  exhaustive instrument forced (`MNF17_P1D6_EXHAUSTIVE=1`) on
  flat / gradient / pseudo-random synthetic images (flat: many equal-size
  rank ties; random: expected weak termination, still exact).

## D6-C — Matched contemporaneous performance (three arm builds)

All arms: release builds, same machine, same 30-file corpus + Hubble,
`/usr/bin/time -l` per encode. Arm A = v1.17.0 tag, arm B = `0366a02`
(v1.17.0 + C4-A + C6, no termination), arm C = D6-A HEAD. User time = CPU.

| arm | corpus (30 files) user s | real s | vs A |
| --- | --- | --- | --- |
| A — original v1.17.0 (no C4-A, no termination) | 520.5 | 529.1 | 1.00× |
| B — current production (+C4-A) | 458.1 | 470.4 | 1.14× |
| C — D6 production candidate (+C4-A + certified termination) | 322.8 | 331.8 | **1.61×** |

Separate attribution: C4-A = A→B × 1.136; certified termination = B→C ×
1.420; cumulative = 1.61× (multiplicative decomposition, nothing summed).
Measurements are matched contemporaneous runs of the actual shipped encoder
entry points (CLI `encode … --mnf8=maximum`), not research harnesses.

Per-arm encodes (user s, selected):

| file | A | B | C | C/A |
| --- | --- | --- | --- | --- |
| repeat_256x256 | 21.49 | 18.98 | 1.13 | 0.05× |
| solid_256x256 | 20.60 | 18.05 | 2.30 | 0.11× |
| gradient_320x240 | 28.94 | 25.84 | 3.52 | 0.12× |
| poster_208x144 | 11.44 | 9.98 | 1.71 | 0.15× |
| screenshot_640x400 | 79.79 | 69.36 | 20.61 | 0.26× |
| blocks_192x128 | 9.63 | 8.53 | 4.73 | 0.49× |
| document_200x280 | 18.56 | 16.30 | 9.72 | 0.52× |
| heldout blob_224x160 | 15.54 | 13.87 | 9.13 | 0.59× |
| (representative photo) forest_224x160_s3100005 | 18.38 | 15.32 | 14.94 | 0.81× |
| noise_256x256 | 33.10 | 28.85 | 29.37 | 0.89× |

Hubble (separate stress measurement, 1000×872 = 872,000 pixels):

| arm | encode user s | vs A |
| --- | --- | --- |
| A | 434.2 | 1.00× |
| B | 377.8 | 1.15× |
| C | 375.5 | 1.16× |

Hubble's certified termination saves ~0.6% of its encode time despite the
95.31% candidate-domination rate — the certificate fires late on
photograph-like content (the D1 census predicted 8–24% symbol-work on
photos; Hubble's arms die in their final strata). This is exactly the
candidate-vs-computation distinction the authorization asked D6 to
establish: **candidate rates are not time.**

Decode (same byte-identical containers, three decoders):

| arm | Hubble decode user s |
| --- | --- |
| A (v1.17.0 decoder) | 2.40 |
| B (C4-A decoder) | 2.06 |
| C (= B's decoder) | 2.12 |

Decode is unaffected by D6 (identical decoder; C−B = run noise) and the
already-shipped C4-A decode gain (1.14×) is confirmed contemporaneously.

Peak RSS (encode, `/usr/bin/time -l maximum resident`): arm A ≈ B ≈ C at
~31 MB (mixed), decode ≤ 18 MB. The D6 pick loop keeps the same
representation-first one-stream-per-class discipline; `raw.len()` of the
held zstd-proxy stream plus the live zlib stream are simultaneously live
per pair (bounded, as before, by the corpus's stream shapes); no corpus-level
or Hubble RSS regression was measured.

## D6 candidate and symbol accounting (MNF17_P1D6_STATS)

Per-file mix-arm disposition across the 30-file corpus + Hubble
(`/tmp/d6_counters.log`) — production execution order (per-pair cheap first),
so incumbent strength differs from the D3 research order:

- Corpus (30 files): 3,840 mix arms → 3,530 dominated, 310
  completed. Candidate rate: 91.9%.
- Mix symbols actually run: 281,079,156; dominated arms' full streams would have
  required 480,825,632; avoided = 199,746,476 (41.5 of would-be mix
  symbol work).
- This is the computational (not merely candidate) elimination and it is —
  as the authorization anticipated — smaller than the D3 research harness's
  61.5% because the D6-A production integration orders cheap incumbents per
  pair rather than globally, and the corpus wall is dominated by
  photograph-like streams where the certificate fires late (measured
  honestly by the B→C 1.420× wall reduction above, which is the end-to-end
  truth including these late firings).
- Per-file dominated/completed examples: gradient 128/0 (symbols run
  1,325,568), solid 128/0, repeat 128/0, poster 128/0, screenshot
  125/128 (26,154,267 symbols run); noise 128/0 with symbols run ≈ would-run (0% avoided — the one
  class with no certified savings; its +2% wall vs arm B is the bound-check
  overhead, ≈ 0.5 s).
- Hubble: 125/128 dominated, symbols run 317,001,180 (~2.48M per arm) vs streams 335,627,248 (~2.62M per arm)
  (5.5% avoided).

## §7 reporting reconciliations (recorded, not re-researched)

1. Ordering control 177.1 s is 1.00× against the contemporaneous exhaustive
   control and 1.09× against the earlier 192.2 s census. Contemporaneous
   claims use 2.60× (D3 vs control); the 2.82× figure is only
   census-relative.
2. Hubble's "~1.3M" in the D2–D5 record referred to the mix-arm record
   stream scale, not pixels: Hubble is 872,000 pixels; its mix-arm stream is
   ~2.6M symbols (pinned exactly in the counters above — the earlier
   wording was imprecise).

## Honest production frontier (measured)

The §6 projection (229.1 s, 2.30×) was a research-path extrapolation and is
NOT what production measures. The measured D6 frontier is 322.8 s user time
(1.61× vs v1.17.0; 1.42× incremental over C4-A). The cause is the same one
the authorization flagged: the mix-family census factor (85%) counted mix
evaluation in the research harness, while the production wall includes whole
record-stream builds, cheap backend evaluations and the photograph-heavy
workload where the certificate recovers little symbol work. The measured
number is the frontier; no further projection is asserted.

## Reproduction

- Arm A/B/C binaries and logs: `/tmp/d6arm_{A,B,C}/manifold`,
  `/tmp/d6_chain.log`, `/tmp/d6_armC.log`, `/tmp/d6_decode.log`,
  `/tmp/d6_counters.log`; chain driver `/tmp/d6_chain.sh` (the arm-C script
  corrected the `cp` target directory bug: `mkdir -p` before install).
- Counters: `MNF17_P1D6_STATS=1 target/release/manifold encode <file> -o
  /tmp/out --mnf8=maximum`.
- Exhaustive instrument: `MNF17_P1D6_EXHAUSTIVE=1` (byte-neutral).
- Full gate: `cargo fmt --all && make gate` → GATE OK.
- FINAL_PARITY PASS on the D6-A tree: JSON in `/tmp/d6_chain.log`.
