# MNF17-P1-C4-A — incrementally maintained counter sums

**Status**: C4-A exactness gates passed and performance measured on the
research twin (`#[cfg(test)] mod p1c4a`, production untouched).
Authorization: P1-C4-A AUTHORIZED (research-only).

## Result

| | Production | C4-A maintained sums | Net |
| --- | --- | --- | --- |
| 192 record arms (3 files × 4 backends × 16 arms, medians of 3) | 58.22 s | 50.47 s | **1.154× (13.3% saved)** |
| per-backend | 14.4–14.7 s | 12.4–12.8 s | 1.146–1.157× (12.8–13.6% saved) |

Uniform across files (solid/forest/screenshot) and backends — the
counter-sum reductions exist in every arm's hot path, and the gain is
not concentrated on degenerate inputs, matching the authorization's
expectation for a per-symbol lever.

## Exactness evidence

- **Differential fixture suite** (`c4a_differential_fixtures`): 9 §4
  fixtures (uniform, single-dominant to the EWA clamp, sparse,
  slot-0-empty remainder interplay, slot-0/near-argmax races, fast-cap
  crossings, slow-cap rescaling over 40 k symbols, two 20 k-symbol
  random trajectories) × 4 backend configurations × every symbol:
  maintained sums equal the true sums, 512 counter entries bit-equal,
  `log_w` bit-equal, `nlocal` equal, **quantized 256-vector 256/256
  identical**, and the full **serialized wires byte-identical**
  (`mix_encode_with[_lane]_a` ≡ production on every fixture).
- **Cohort replay**: all 192 arms assert `c4a wire == production wire`
  and cross-repeat determinism. Zero divergences.
- The invariant's mechanism is trivially conservative: the sums are
  updated at every mutation site (including parent `share_step`) and
  recounted inside rescale sweeps; halvings are rare (fast cap 4096/32768
  per `+= 32`, slow cap 65536/1048576), so the recount cost is negligible.

## Pass budget effect

`step` (per symbol): 2 Σ passes (log-W) + 2 Σ passes (cap checks) →
0 passes hot path, recount only on cap-crossing. `probs`: 2 Σ passes
(tf/ts) → 0. Total hot-path pass count drop ≈ 4 of ~8–13 per symbol;
the measured 13.3% net is the observed truth (below the idealized
per-pass arithmetic because the quantization/correction passes — the
dominant cost — remain untouched, as designed for this arm).

## Dispositions

- C4-A: **measured 1.154×, exactness clean.** Retained as the base for
  C4-B/C/D.
- C4-B (fused quantization + global reductions), C4-C (direct cdf
  construction), and C4-D (A+B+C) continue per the authorization;
  B's correction-order proof obligations (slot-0 argmax/zero-status,
  tie-breaking, u64 boundaries) will be certified by the same
  differential harness before any timing.
- C5–C7 corpus validation rides C4-D (CONDITIONAL; production
  integration HOLD).

Data: `/tmp/mnf17_p1c4a_rows.csv` (rows: file, backend, kind, planar,
stream len, prod/c4a wall µs, lengths, wire fnv16 pairs — all equal).
