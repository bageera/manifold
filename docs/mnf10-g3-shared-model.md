# MNF10-G3: shared-model competition (Track B, 2026-10-28)

## Fixtures (scripts/gen_g3_fixtures.py)

g3_photo.png (the sand photo, 31,345 B under the pick), g3_shared_exact.png
(one 32x32 photo-texture tile planted at 6 sites — capability fixture),
g3_shared_2pct.png (the same with 2%-corruption — the CY01 robustness
class). Both corrupted variants decode-verified through the CLI.

## Arms measured (complete serialized cost, independent decode, one coder)

| arm | shared-exact | photo control | z9 variant |
|---|---|---|---|
| A — shipping pick (v1.9.0 + MNF9 arm)  | 23,812 | 31,345 | — |
| A-rebuilt in harness (pred literals, z19) | 23,774 | 30,916 | 25,668→(*see note) |
| C — atlas exact-refs on residuals (z19) | 24,253 (4 refs fired) | 30,916 (0 refs) | — |
| C' — shared 32-block template, arbitrary sites, ZERO residual (z19) | 24,694 | — | 26,354 (z9: wins vs 32,868 by ~6.5 KB) |

Coder-parity note: harness-z19 ≈ CLI pick within ~40 B (the recorded
cross-harness boundary noise); the comparisons here are all
same-harness.

## Findings

1. **Arm C on the shipping coder LOSES**: the atlas-ref arm (+479 B) and
   the dedicated shared-template grammar (+880 B) both LOSE to the z19
   container's own LZ layer, which codes the six exact repeats for FREE.
   The residual-atlas fired only 4 of 24 tile refs — the residual block
   keyed on tile-relative residuals whose first column depends on the
   tile's left neighbor, so 20 exact-PIXEL copies have different
   residual blocks (the mechanism boundary: content-equality of residuals
   ≠ content-equality of pixels).
2. **The zstd-disabled control flips the verdict**: under zlib-9's small
   window the same shared-template arm WINS ~6.5 KB (26,354 vs 32,868) —
   the win belongs to a WEAK container's dictionary, not to the atlas
   grammar. Structural modeling vs the strong container's repetition
   machinery: the container wins on this fixture class.
3. **Capability established, practical value bounded**: a shared-model
   grammar CAN represent 6 exact translations for ~30 B of metadata
   (site coords + markers) + one 3,072-B template — but the already-
   present LZ layer does it cheaper inside z19. The practical-value
   question for G3 shifts to classes where the container's dictionary
   cannot reach: streams longer than the z19 window, distributed
   references, transformed (non-identity) repeats (Arm D), and
   corrupted-reference residuals (Arm E — the 25,500-B corrupted fixture
   stands ready).

## Disposition

- Track B's acceptance criterion confirmed as Jon stated it: a shared
  model must beat zstd INCLUDED, on the same bytes, complete cost. On
  the exact-translation fixture: DENIED at z19, PROVEN available under
  a window-constrained container.
- Arms D (transforms) and E (corrupted references) are the next
  measurements; the model family and search budget stay frozen until
  those numbers exist.
- Scope: these three 160x120 fixtures + one coding library per arm;
  photo-corpus generalization only after D/E pass.
