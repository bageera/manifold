# Step-5 chart candidates — measured verdicts (2026-10-28)

Three candidates entered through the standard pattern (mechanism,
metadata counted, exactness asserted, strongest control in the table).
Harness: `crates/mnf-mans/scripts/charts_probe.py` (one process, one
coder per arm, log preserved). Corpus: the 30-file corpus.

| file | plane-z9 | tokens | 2D-tokens | orient-vs-std |
|---|---|---|---|---|
| (30 files, totals) | 1,076,739 | 1,178,876 | 1,180,494 | +0 |

Per-file rate-aware readout (the pick = min of the candidates offered):

- pick WITHOUT 2D-tokens: **1,073,613** (tokens+plane per-file min)
- pick WITH 2D-tokens (column-layout candidate, +1 B flag): **1,073,037**
  (−576 B from three files: screenshot −11.2%, document −18.5%,
  poster −8.4% — text-ish classes where repeats live in COLUMNS and the
  raster-only matcher misses them)

## Verdicts

1. **Gradient-orientation tiebreak (candidate B): DENIED — the surface
   does not exist.** Zero non-flat ties in document and screenshot
   residuals (the classes where the −5.5% effect was recorded): after
   the clamp and the mod-256 residual, `pa == pb`-type adjacent ties
   occur at 0.00% of pixels. The recorded effect was an ENCODER-cost
   effect (measured in the conformal probe's specific harness), not a
   residual-size effect; on the retained cost model it never fires.
   Closed with evidence; no format item.
2. **2D/token-column candidate (candidate A'): RETAINED as a pick
   candidate, not a format item.** The token wire grammar is already
   fully 2D (dx, dy) — the "chart" here is only a LAYOUT CANDIDATE
   (column-major re-layout of the residual plane before the matcher)
   with a +1 B layout flag. Measured −576 B/30-file with wins exactly
   in the text/screen classes; absorbed into the token arm as a per-file
   layout choice when the token arm is promoted through a format step.
   No new record grammar — never needs MNF9 magic.
3. **Arm F (per-region class scoring): MEASURED, retained as
   instrumentation.** The per-file class split is now visible in every
   gate (the 5 classes where tokens fire, 2 where rANS fires, 23
   plane-only) — the mechanism the cryo-branch contracts called "per-
   region class scoring" already exists as the rate-aware pick's
   honest per-file readout. A per-REGION (vs per-FILE) pick adds
   granularity only where classes mix within one image; measured
   across this corpus, no file's winning class varies by region
   enough to justify per-region metadata (the mixed classes are
   noise-dominated where no arm but plane wins anywhere). Recorded;
   revisit only if a mixed-class corpus appears.

## What this means for the format family

The 2.0 shape after this probe batch is unchanged from v1.6.0's: token
arm (now with the column-layout candidate) + plane arm + z19/z9
containers, all behind the rate-aware pick. The genuinely new record
grammar (shared charts F_k with transition maps) still has NO measured
candidate that beats the existing arms on this corpus — six denials +
this batch's two non-firings stand between the ambition and the wire.
The manifold-native step waits for a class of content where a shared
model beats these controls after metadata; the cryo-EM arm P contract
(translation-group coordinates) remains the one place the precondition
holds by construction.