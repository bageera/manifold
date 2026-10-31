# MNF14-ID2 — Flat-field arm (MNF8 record mode 8): shipped, gated, record

## What shipped

The flat-field inverse-discovery arm: **per-tile constant field**
(record mode 8). Per tile the encoder competes the tile's per-channel
MEDIAN field against literal/predictor/reference/YCoCg/IF4-laws at
complete-record cost; the decoder replays
`recon = clip-free mod-256 sum of const params + residual` with residual
= (X − const) mod 256. Mode 8 is the "prediction-origin" candidate —
the ID1 memo's flat-field arm, and the first fixture-proven
exact-generative-inverse family (ID1-A) admitted to production
competition. FIVE-SITE slice completed per ADR-010 (e): encoder sweep
+ stats dispatch in `mnf8.rs`, the decode walk in `mnf6.rs`, and BOTH
`channel_diff_relabel` record branches + `channel_diff_unrelabel`
(3-B const params copied; the RESIDUAL body relabels like mode 0 —
planar ordering included — because const residuals are exact
per-pixel values just like mode-0 bodies). The wire-granularity
freeze test (`const_law_recon_exact` in laws.rs: 5 laws tests green)
pins zero-residual behavior on uniform tiles and mod-256 lossless on
noisy ones.

## Gates (all green, release profile, mode 8 in the pick)

- fmt / clippy `-D warnings` clean (scope `-p manifold-codec`)
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean
- Full release suite: **86 tests, 0 failed** across 7 suites
  (`/tmp/id2_full_test.log`), including `cd_pick_never_worse` and
  `mnf8_maximum_never_loses_to_mnf7` — the pick with mode 8 is never
  worse than the pre-mode-8 pick by construction and the pins prove
  the comparator integrity
- Fuzz 525 trials: rejected 524, accepted 1 (the known
  valid-different-file class), **0 panics** (`/tmp/id2_fuzz.log`)
- CLI verify: document golden **byte-identical** to the mix5 golden
  (the arm correctly declines on non-sky content — the fixture-deg
  detector from the skill's consumer records), exact
- **30-file corpus gate: 812,180 = BYTE-IDENTICAL to the v1.14
  anchor** — all 30 rows identical, 0 regressions
  (`/tmp/sizes_id2.csv`, checked digit-for-digit)

## The photographic gate: hubble deep field (the named class)

The firing-surface rule (ADR-010-i) named this class BEFORE the arm:
black-sky + sparse-star content, where MED's edge-sense turns the
dithered dark field into a ±noise plane (incompressible) while a
per-tile MEDIAN keeps each tile's residual centered on the tile's own
origin (zstd's RLE layer then crushes the long zero-runs).

Measured through the SHIPPED framing end-to-end (release CLI,
`--mnf8=maximum`, exact reconstruction verified):

| fixture | pre-mode-8 pick | mode-8 pick | delta |
|---------|----------------:|------------:|------:|
| hubble_deep_field.jpg (1000×872, image-crate JPEG decode) | 1,479,904 | **1,452,400** | **−27,504 (−1.86%)** |

Isolation-estimate accounting (kept separate per the R1 rule):
on a 512² crop of the CLI's own decoded pixels the python const arm
measured 464,903 vs MED 479,390 = −3.0%, with the per-tile census at
832/1024 const wins; the shipped-pick number above is SMALLER than
the isolation number exactly as R1 predicts (the pick's framing +
competition costs the arm its marginal tiles). The two numbers
answer different questions and are both recorded.

## Scope note (the corpus tie)

The 30-file corpus gate lands all-tie while the arm demonstrably
fires on a real photographic class OUTSIDE the corpus (hubble −1.86%).
ADR-010's revert-on-tie rule targets arms that fire NOWHERE (the
D8+shears case) where a dead flag costs variant-encode work forever;
mode 8 fires on its named class with zero corpus downside and no new
flags-field space (the mode byte is per-tile grammar, not a flag
bit). Decision: RETAINED. If later corpus extensions ever include
sky-class content, the arm is already priced in; nothing to
re-admit.

## Provenance

- Instrument (isolation): `/private/tmp/mnf14_id1/id2_hubble_isolation.py`
  (one process; canonical MED via pred_map; crop512 of the CLI's own
  decoded hubble pixels; honest 28-B frame + zstd-19 + CD-G)
- End-to-end: release CLI encode + verify on the JPEG fixture
  (NOTE: the historical anchor's hubble row, where present in earlier
  size reports, used OTHER fixtures — the memo's caution about
  mixed scopes is correct: the 812,180 anchor never contained
  hubble; the 1.45-MB measurement is the standalone-fixture record)
- The 812,180 v1.14 anchor remains the corpus comparator exactly as
  the work order specified — mode 8's corpus contribution is 0 B on
  this corpus, and NO inference of corpus-level reduction is drawn
  from the standalone fixture win

## What this is (in the discovery-engine arc)

ID1 proved the engine can invert exact-generative integers (76/74 B);
ADR-010 proved the pick is the engine; ID2 is the first fixture-
independent photographic class where a member of an exact-generative
vocabulary (per-tile constant fields) wins through the shipped
production machinery. The ID1-B conclusion — compositions reduce to
per-region laws under the existing per-tile record grammar — now has
its second measured confirmation: dispatch + economics compose with a
ONE-parameter law family. The next rungs per the memo's priority:
(2) the tile-scale div-form family (vx, vy, T, D) riding the same
competition sites, and (3) a larger symbolic vocabulary ONLY after
those establish production value.