# MNF15-P1-D D1 — exact probe-result memoization (body + tile layers)

**Status**: IMPLEMENTED + all gates green. The per-pick `ProbeCache` turns
`proxy_cost`'s redundant compression probes into exact lookups: identity
is `(probe configuration, full input bytes)` decided by **memcmp** — the
FNV-1a 64 hash only locates the bucket and can never establish identity.
The search space, candidate ranks, tie-breaking, wire format and decoder
are untouched; every returned cost is the true compression length of that
input under that configuration, cached or not ([docs/mnf15-p1d-gate.log](mnf15-p1d-gate.log)).

## The two layers

- **D1a body memo** — every sweep probe (`lit`, m3, atlas exact/color,
  flat/law bodies, bilinear shifts) routes through
  (configuration, body) → cost, memcmp-decided, 8 MiB body budget
  (48 B/entry conservative charge), per-pick lifetime, fall-back to the
  ordinary probe on exhaustion.
- **D1b tile memo** — the laws+flat segment is a pure function of
  (tile data, planar, probe configuration): no predictor residual (it
  consumes `tile.data`, not the med body), no tile position,
  kind-independent. One ~1.7 KB entry replaces ~103 stored probe bodies
  (~84 KB) for every sweep sharing the key; 8 MiB tile budget. Its
  winner re-enters the ORIGINAL strict-less candidate order
  (lit → m3 → law_p → law_b → flat), so ties keep the earlier candidate
  exactly as the sequential code did — verified by case analysis and by
  the byte gates below.

## Measured (matched environment, idle machine)

| Gate | Result |
|------|--------|
| C1 identity manifest | committed 200-candidate manifest reproduced byte-exactly (278.9 s) |
| C2 corpus | 30/30 Maximum + 30/30 Balanced digests identical, **810,816 = the anchor** |
| C3 hubble | byte-identical (1,452,400 B); wall 383.2 s vs 376.9 (+1.7% — recorded) |
| C4 fuzz | 525 trials, 0 panics |
| C5 D1 unit gates | forced-collision, θ-separation, budget-exhaustion — all green |
| Wall (B5) | Maximum **428.3 s** (481.1 → −11.0%); cumulative **1,127.9 → 428.3 = −62.0% (2.63×)** |
| Wall (Balanced) | **39.87 s** (56.38 → −29.3%); cumulative −76.0% (**4.16×**) |
| RSS (B6) | corpus peak **107.9 MB** (+11.7 over P1-C — the conservative 16 MiB budget, 8+8) |
| Executions | screenshot 1.89 M → 676.6 K (−64.2%); gradient −29.3%; autumn −28.5% |

## The budget knob (evidence for Jon's reserved call)

D0's aggregate ceiling (9.82× fewer compressor calls) assumes the FULL
distinct-body sets stay cached — ~55 K bodies on screenshot alone
(~36–40 MB). Within the conservative 16 MiB budget the residual executing
mass is atlas-derived bodies (~560 K of 676 K screenshot executions; the
atlas set depends on the run-state, so it is body-memo territory, not
tile-memo). The `exhausted` counters in `p1d_probe_census` quantify the
gap per fixture; raising the budget is a one-constant change whose RSS
price is measured above (roughly +1 MB RSS per +1 MiB budget). The
8+8 MiB split kept the RSS delta to +11.7 MB while capturing the
laws-lattice mass outright.

## Next (D2 has since landed — see [mnf15-p1d2-gate.log](mnf15-p1d2-gate.log))

D2 shipped the same day: the zstd-3 probe context is reused across a
pick's misses (`SessionOnly` reset per body — the zio writer's own
concatenated-frame directive — via `raw::Encoder::with_context`), zlib
probes stayed FRESH after the attribution instrument measured reuse
neutral (~0.99x). Gates: parity over the adversarial sequence (the gate
caught and fixed a recycled-sink append bug), manifest byte-exact,
corpus 30/30 both profiles, hubble byte-identical at 363.3 s (-5.2%,
reversing D1's open +1.7% with matched evidence), fuzz 525/0. Measured:
Maximum 428.3 -> 414.5 s (-3.2%); cumulative **1,127.9 -> 414.5 s =
-63.2% (2.72x)**; Balanced 39.87 -> 28.92 s (-27.4%); cumulative
**-82.6% (5.74x)**; RSS 108.6 MB (+0.7).
