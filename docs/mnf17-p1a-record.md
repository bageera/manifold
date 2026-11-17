# MNF17-P1-A — backend-7 pipeline equivalence-class census (first measurement)

**Status**: opener for MNF17-P1 ("computational portfolio optimization":
find the same m\*, spend less work). Proposal adopted from the user's
2026-11-16 direction note: zero-regret optimization — identical serialized
output, identical reconstruction, no size increase.

**Instrument**: `src/mnf8.rs → #[cfg(test)] mod mnf17_p1a_census`
(env-gated + `#[ignore]`: `MNF17_P1A_OUT=<csv> cargo test --release
mnf17_p1a_census -- --ignored --nocapture`). Replays the production
Maximum candidate matrix with the shared-construction discipline of the
real pick (one `build_record_stream` per (kind, layout, proxy class) +
3 O(len) relabels + per-backend evaluate), hashes every layer identity,
and times everything. **Winner reconciliation: 8/8 files — the replayed
loop picks the same backend and the same byte count as the production
pick.** The release gate is green with the instrument present.

**Census files** (8): solid, gradient, repeat, heldout/plaid, noise,
forest 224×160, dusk 160×120, screenshot 640×400 — the degenerate cohort
that carries v1.15-P1-D's known identical-probe rates plus the
backend-7-winning photo cohort (b7 picked on 4/8; zstd19 on 4/8,
zlib 0). Per-file table: `docs/mnf17-p1a-decision-table.csv`.

## Layer composition (measured, single-threaded wall)

| Layer | Time | Share | Redundancy found |
| --- | --- | --- | --- |
| Stream builds (incl. per-tile sweep probes) | 28.3 s | 12% | 128 builds → **75** unique raw0 streams |
| REF relabels | 42.6 ms | ~0% | 512 → **261** unique (49% redundant, but worthless to memoize) |
| Candidate evaluate (payload encodes) | 222.1 s | 88% | 1,856 evals → **34.4 s** of byte-identical duplicates (16%) |

Evaluate split by backend: mix6 54.6 s, mix4 53.2 s, mix5 52.5 s,
**mix7 52.0 s** (256 record arms), zstd19 6.1 s, zlib 3.5 s — the four
mix backends are **95.6% of evaluate cost**.

## Identity layers, per the R6.2 tuple (H_body, H_classes, geom, config, model)

- **Stream layer**: 53/128 builds emit byte-identical raw0 streams
  (degenerate inputs produce identical streams across kinds/layouts).
- **Classes layer**: not yet separately hashed (the walk fold computes
  the sequence identity but the prototype emitted only
  `classes_walked <bool>`; cost sits inside each backend's evaluate).
  Unmeasured quantity → P1-B instrument.
- **Payload layer**: duplicate-payload evaluations (same ident, distinct
  arms) = the measured equivalence-memo ceiling: **16.0% of evaluate**
  (solid 11.3 s, repeat 6.2 s, gradient 5.6 s, forest 5.0 s;
  screenshot = 0 — every arm unique).
- **D1 (shipped probe memo)**: measured live in the replay sweep,
  999,855/1,940,041 = **34.0 corpus hit rate**, saturating on the
  degenerate cohort (solid 99.9%, repeat 99.2%) while photos sit at
  1.8–5.4% and screenshot at 37.7%.

## Ceilings recorded (counterfactual upper bounds, not claims)

- **Perfect-bound ceiling** (all non-winning arms never evaluated):
  221.3 s of 222.1 s — definitionally ~100%; the realizable share
  depends on bound tightness and incumbent order (P1-D).
- **Equivalence-memo ceiling** (skip arms whose computation identity
  hit the cache): 34.4 s of evaluate (16%) + redundant stream builds.
- The declined pool per file: screenshot 77.3 s, noise 31.4 s,
  gradient 27.7 s, repeat 21.7 s, solid 20.7 s — i.e. the degenerate
  files where a compressed backend wins carry the large declined pools,
  exactly where an early incumbent (zstd19+zlib = 9.6 s) would empower
  bound pruning of the four mix backends.

## Method honesty

- Walls are single-machine `Instant` sums; the pick pass was executed
  twice per file (sizes/D1, then wall) + once as the arm replay —
  instrumentation overhead included.
- The token-arms row family is backend-7-free by design (record framing).
- The classes-layer split inside `mix_encode_with_lane` is an open
  measurement (P1-B) — no claim is made about walk-vs-lane-encode cost.
- No production path changed: the census is cfg(test), zero warnings,
  `make gate` OK with the instrument in tree; the forest payload hash
  `682e23165304b050` re-verified after formatting.

## P1-B/C/D implications

1. **P1-B (walk/classes instrument)**: hash the lane-class sequence per
   (raw, layout) inside the sweep and time the walk split.
2. **P1-C (sharing)**: the mix-family evaluate = one full encode per
   (backend, raw, layout) with no cross-backend reuse — a shared
   per-candidate cost decomposition is the lever above the 16%.
3. **P1-D (bounds)**: the declined pool is 99%+ of evaluate on every
   file; a certified lower bound tighter than "incumbent size" turns
   screenshot's 77 s (the largest pool) into the first target. Shannon
   estimates remain excluded as unsound for wire-size bounds.
