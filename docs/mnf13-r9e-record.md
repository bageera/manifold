# MNF13-R9-E — backend 6 (the R9 warm/refined mix coder) joins the pick: measured record

Status: MEASURED + INTEGRATED · backend id 6 = the R9-C/D composition
(backend 5's family + the heavy-band prev3 refinement, the R9-C cold-mix
schedule with the corrected form, and the live-ancestor `share` rule),
wired into the shipped pick at complete-serialized cost per file, gated
on the FULL 30-file production corpus against the shipped portfolio's
actual output. All gates green; fuzz clean; byte parity with the
measuring instrument proven 22/22.

## What shipped (one implementation, three constant sets)

`MixParams{BACKEND4, BACKEND5, BACKEND6}` — byte-frozen v1.13/v1.14
evolutions (verified: the knobs are inert for 4/5, wires unchanged) and
backend 6:

- **20 contexts**: backend 5's 16 keys + 4 refined heavy-band children —
  keys 7 (prev ≥ 97, band 0) → {16, 17} and key 15 (band 1) → {18, 19},
  split by prev3's coarse band (the R9-D distinct-children form; the
  merged-children design these measurements started from measured LOSES
  and is not shipped).
- **Cold-mix schedule**: λ = n0/(n0+n) with n0 = 12 while nlocal < 256,
  mixing into the grammar-tree parent (parent of band key k = k & 7;
  children's parents are base keys 7/15).
- **Live ancestors (`share`)**: the parent also takes the symbol's
  update — the mechanism the R9-D composition needs (frozen parents
  measured −456 vs −996 aggregate on the bridge).

Corrected-schedule note (method honesty): the R9-C record described the
schedule as λ = 2^N/(2^N+n); the committed bridge actually implements
λ = N/(N+n) with N parsed from the mode token — the MEASURED numbers
correspond to the code, and this record's n0 = 12 carries the code's
form. Doc-prose-vs-code mismatch caught before the backend froze.

## Parity with the measuring instrument (promotion-critical)

The shipped `mix_encode_with(BACKEND6)` is byte-equal to the R9 bridge's
`refineA-warm2p12-share` wire on all 22 production streams, and equals
the recorded per-file lengths (`r9e_parity.rs` driver, 0 mismatches).
The instrument is not a proxy — the shipped coder IS the measured one.

## Corpus gate (the pre-registered bar: complete-file, zero regressions)

The shipped portfolio's ACTUAL output per file, comparator re-swept in
the same run from the pre-backend-6 binary (HEAD 1baf847):

| | total | vs comparator |
|---|---:|---|
| old binary (1baf847, backend ≤ 5) | **812,180** | = the v1.14 release portfolio (sanity anchor) |
| new (backend 6 in the pick) | **810,816** | **−1,364 B (−0.168%)** |

- Per file: **14 wins, 0 losses, 16 ties** — zero regressions; every
  file's decode verified exact through the CLI.
- Backend 6 fires 15/30 (13 photographic streams + document + one
  genuine equal-size tie, autumn s900001: b5 payload == b6 payload =
  18,748 B). screenshot declines backend 6 (+4 on the fixed-stream
  comparison) and keeps mix5 — the min() self-gating the pick by
  construction.
- Reconciliation vs the fixed-stream bridge estimate (−1,076): the gap
  is EXACTLY −288 on 4 files (forest224s900002/ocean/sand/slate192) —
  the pick measures backend × predictor × planar × REF JOINTLY (32 arm
  combos per backend), and mix6-favoring combos elsewhere in the matrix
  beat the shipped arm's flags word. The instrument held flags fixed;
  the gate measured the real production surface.
- Encode time + peak memory reported per file, never collapsed into the
  decision (`.research/r9/r9e_rows.csv`): maximum-profile encode
  870 s → 1145 s summed (median 22.8 → 30.1 s) — the 5th sweep arm plus
  mix6's per-symbol mix/parent overhead. The Balanced profile (zstd-19
  only) is unchanged, so the default consumer path pays nothing.

## Mechanism note (why the win concentrates where it does)

The win is the heavy band's prev3 conditional structure the R9-D floor
measured (−14.2 kB omniscient), harvested causally at 7–10 % with the
R9-C cold-mix covering the children's adaptation: −0.2..−1.0 kB per
stream class on 13 of 22 photographic streams, zero effect elsewhere
(the contexts never fire on synthetic/low-entropy records). Consistent
with every R9 round: conditional structure is real; per-file stability
comes from composition (cold-mix + live parents), and the pick absorbs
what a fixed mechanism cannot.

## Gates

- Unit: backends 4/5 byte-frozen knobs-inert pin; backend 6 roundtrip +
  distinctness; malformed Err-never-panic.
- Conformance: new golden `mnf8_mix6_document.*` at the CURRENT measured
  pick (LEFT|REF=3|mix6, flags 0xe0, stream_len 168,234) via the
  committed generator `scripts/gen_mnf8_mix6_fixture.py`; the v1.14
  mix5 golden retained as a valid mix5 wire-form record with its
  stale "current pick" wording corrected (the dethrone rule: the
  invariant asserted, the pick claim recomputed).
- fuzz 300 trials 0 panics; full `make gate` (fmt, clippy -D warnings
  on the fresh 1.99.1 toolchain, release tests, rustdoc) green.

## Boundaries

- The R9-E measure covers the 30-file production corpus at the release
  CLI; hubble-class external assets follow the R9-A wire-decode
  convention and stay outside this sweep.
- No version bump / tag / publish in this commit: the release decision
  (lib 1.15.0 + capi + the new README benchmark row) follows the normal
  release review with this record as the gate evidence.
- The R9-C prose-formula correction (λ = N/(N+n)) is recorded here AND
  now matches the frozen backend-6 constants; any future schedule
  change is a NEW backend id by the standing rule.