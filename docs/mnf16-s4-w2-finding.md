# MNF16-S4 W2 finding (2026-11-15) — the G8 feature is not decoder-realizable; the line closes

S4 was authorized with W2 = "encoder-generated and independently
decoder-generated G8 sequences identical" — the decoder must recover
the feature from stored bytes alone. During S4's design pass, the
W2 contract caught a defect in the S3 feature that the S3 gates did
not:

## The defect

The frozen G8 stencil is mean(|west-8 − R|, |north-8 − R|) — it READS
THE TARGET SYMBOL R. A wire decoder cannot know R's value before
decoding it, so the S3 conditioning state is not recoverable from the
wire: E's features include self-information the decoder cannot have.
The S3 causality proof validated the neighbor LOOKUP indices (strictly
earlier) but the target's own value enters the feature through the
subtraction — an untested path. The S1/S2 G8 numbers share the
definition; the defect dates to stage 1.

## The decisive measurement (strictly-past form, all else frozen)

G8' = mean(|west-8|, |north-8|) — every input strictly earlier, wire-
realizable by construction. Same edges, mapping, contexts, update
schedule, seed. Instrument: scripts/mnf16_s3_coder.py --past.

| Arm | corpus model cost | Δ vs S3-0 |
| --- | --- | --- |
| S3-0 (shipped schedules) | 583,366.1 B | — |
| S3-1 frozen G8 (not realizable) | 577,890.9 B | +5,475.2 B |
| S3-1' G8' strictly-past (realizable) | 585,449.0 B | **−2,082.9 B** |
| shuffled control | 589,854.0 B | −6,487.9 B |

Per-file: 21/22 files LOSE with the realizable form (the single +39.6 B
on forest_3100005 is noise-scale; its control is −380.6 B). The entire
S3-1 aggregate gain is attributed to target-correlated feature leakage:
a coarse function of R itself was conditioning the model coding R.

## Disposition

- S4 as authorized ("preserve G8 exactly as measured") is
  architecturally impossible under W2 — the feature cannot be
  reconstructed from the wire by definition. S4 does not proceed on
  this feature. No wire was built; the s4 harness was never used to
  measure an unrealizable arm.
- The G8 cross-scale gradient line CLOSES: the only two
  decoder-realizable features measured against production contexts —
  predictor disagreement and G8' — are both non-positive, and G8's
  recorded positives are explained by the self-dependence. The
  S1→S3 narrowing was the progressive removal of the target's own
  contribution (S2's estimator reads the full stream; S3's audit
  likewise), not the extraction of an ever-smaller real residue.
- The negatives STAND as measurements: predictor disagreement and the
  realizable G8' both fail to pay for their conditioning. Backend 6's
  20-context prev-symbol schedule is a strong local-magnitude model;
  the tested same-plane neighbor-magnitude and disagreement families
  add nothing recoverable.
- The instrument chain remains valid: every recorded number is the
  correct conditional-information measurement of the feature as
  defined at its time; the addendum states the realizability limit
  rather than retracting the arithmetic.

## What survives

1. A reusable, gate-hardened measurement pipeline (S1 census → S2
   production bridge → S3 adaptive coder arms) in which every future
   feature must pass a target-independence check BEFORE the coder arm.
   scripts/mnf16_s3_coder.py --past is that check, applied.
2. Verified negatives: disagreement denied under tested models (S2);
   same-plane neighbor-magnitude cross-scale conditioning denied
   (G8', this record).
3. The production anchors untouched: 810,816 B corpus, 414.5 s / 28.92 s
   walls, shipped backends and v1.16.0 release unchanged. No
   production source was modified by S1-S4 research.
