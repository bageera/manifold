# MNF13-R9-F — the released pick's external surface: hubble-class boundary sweep

Measurement record closing the boundary R9-E's own record names: its
corpus gate covers the 30-file production corpus only, and
"hubble-class external assets follow the R9-A wire-decode convention
and stay outside this sweep." R9-F runs the same old-vs-new release-CLI
gate ACROSS that boundary, then re-censuses the external-class pick
with the ID4 instrument to ask the ADR-010 question on the outside
class: does the record-grammar floor verdict survive off-corpus?

Instruments: `crates/mnf-mans/scripts/mnf13_r9f_ext_surface.py`
(sweep, reusing the R9-E gate's run/verify/backend-of mechanics) +
`scripts/residual_census.py` (the ID4 census, invoked on the external
truth directly). Old binary = 1baf847 (pre-backend-6) at
`/tmp/r9e-old-target/release/manifold`; new = the v1.15.0 tree
(586370e) at `target/release/manifold`. Both binaries' output verified
exact (`verify` rc=0) per arm.

## Measured (single external-class asset: hubble, 1000×872)

The decoded-truth PNG is the same pixels the ID4/R9-A censuses audited
(1000×872, 3,465 tiles). Wire-level outputs:

| profile  | old (1baf847) | new (v1.15.0) | delta | new pick |
| -------- | ------------- | ------------- | ----- | -------- |
| maximum  | 1,452,400     | 1,452,400     | +0    | backend 5 (flags 0x45) |
| balanced | 1,502,259     | 1,502,259     | +0    | unchanged pick |

- **Backend 6 does not fire on the external surface.** The v1.15 pick
  is byte-identical to the v1.14 pick on hubble in both profiles: the
  cold-mix λ-schedule and live parents buy zero there, exactly as on
  the 16 corpus files where mix6 does not fire (R9-E: 16 ties).
- **Anchor reproduction:** the fresh old-binary maximum output IS the
  committed ID4 hub wire byte-for-byte
  (`/tmp/id4_files/hub_hubble_deep_field.jpg.max.mnf8`,
  `id4_anchor_match=True`), and it equals the 1,452,400 external
  anchor R9-A staged for complete-file gating. Two independent
  instrument generations agree on the byte level.
- Encode wall reality (measured in-pass, one core): hubble maximum =
  ~950 s per arm; balanced ≈ 40 s. The R9-E corpus encode-time report
  does not transfer pixel-scaled to the external asset; sizes, not
  times, are the R9-F claim.

## Census re-walk (the ID4 instrument on the external truth)

`R9F_CENSUS_ONLY=1` leg of the same instrument; the v1.15 CLI's own
maximum wire walked from raw stream bytes (zlib container), with the
walker's full desync + wire-exactness assertions:

- `mode_hist: {0: 3188, 8: 146, 7: 91, 6: 20, 1: 19, 4: 1}` —
  **3,465 tiles, 277 non-mode-0 (8.0%)**, spread across the shipped
  record modes {8: 146, 7: 91, 6: 20, 1: 19, 4: 1}. This is EXACTLY the composition
  R9-A recorded for hubble ("277 law/const/ref tiles"): the v1.15
  pick reproduces the v1.14 external pick structurally, tile by tile,
  on top of the byte-equal sizes — three independent instrument
  generations (R9-A audit, ID4 census, R9-F sweep) agree.
- The ID4 in-corpus verdict ("3,341 record tiles, all mode 0") is
  CORPUS-SCOPED, as its own record's title states: on the external
  class the shipped record vocabulary DOES fire — the pick's
  discovery engine reaches for the law/const/ref arms exactly where
  the entropy classes justify them (edge 3,091 tiles / mixed 370 /
  texture-heavy 4, all nz_mean ≈ 0.99).
- `regret: −49,859` = maximum 1,452,400 − balanced 1,502,259:
  reconciles the sweep's row bytes exactly through the census
  instrument's own accounting.
- `validated: 3,334` = every mode-0 and mode-8 tile reconstructed from
  stream bytes and asserted == source pixels (3,188 + 146; modes 6/7
  are length-walked only, per the ID4 convention). Zero failures.

## Verdict

The R9-E corpus verdict does not extend, and does not need to: the
pick's min() construction already absorbs every non-firing arm by its
own trial cost, and R9-F measures that absorption on the outside class
— zero byte movement, pick unchanged, both anchors reproduced exact.
The external surface's value for the record grammar remains the
standing one (R9-A: hubble is the in-instrument example of non-mode-0
record tiles; the census leg records the current mix), and any byte
change there would have been a NEW-backend-id decision, which the
measurement forecloses: there is none to take.

## Gates

- Per-arm `verify` exit 0 (encode→decode→exact) on both binaries,
  both profiles (the sweep asserts it inline).
- Wire anchors: 1,452,400 (R9-A external anchor) and the ID4 committed
  hub wire — both reproduced byte-equal by fresh runs.
- Python instruments only; no lib/Cargo change → `make gate` n/a;
  py_compile clean on both touched scripts.

## Boundaries

- The external surface is single-asset (hubble): "hubble-class" is
  exactly the class the repo's records name, and no second external
  fixture exists outside testdata on this machine. The claim is
  scoped to the class the records name, not to all conceivable
  external imagery.
- Encode time measured in-pass without perf isolation (two binaries
  sharing the machine); never a decision input, sizes are.
- /tmp wires (`/tmp/r9f_*.mnf8`) are regenerables; the record carries
  the numbers and the two byte-anchors that make regenerability
  checkable.
