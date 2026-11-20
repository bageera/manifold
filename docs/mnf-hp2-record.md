# MNF-HP2 — Offline probability correction record

Program: MNF-HP. Objective: the smallest mathematical correction over the
production P0 that reduces complete description length after model costs.
Python screening per the gauntlet; population = the per-image probability-
vector prefixes (contiguous 40k-row exact-model-replay prefixes from HP1's
dump; 360k held-out rows = 9 heldout-photos prefixes ≈ 61% of their streams).

Governing convention (single definition, used by every arm and picker):
gain = Σ log2(Pθ(y)/P0(y)) bits — POSITIVE = the correction codes the observed
stream for fewer bits than production P0. The first arms draft inverted this
sign (bits(new) − bits(p0) logged as "gain"), which made the λ/γ pickers
maximize damage and every number sign-flipped; caught by the γ→0 physical
impossibility check (corrections approaching the uniform distribution cannot
beat a 2.9–4.5-bit/symbol production model) plus a vec/trace alignment audit
(exact match fraction 1.0 — the substrate was sound, the convention was not).
All results below are from the corrected script
(`research/hp2/hp2_arms.py`; log `research/results/hp2-arms.log`;
CSV `research/results/hp2-model-comparison.csv`).

## 1. Population P1 — all-classes training, cross-class held-out transfer

| arm | model bytes | held-out gain | per-krow | standalone net | verdict |
|---|---|---|---|---|---|
| A0 production P0 | 0 | 0 (baseline) | — | — | — |
| A1 LR round 2^24→2^16 | 0 | −187.7 bits | −0.52 | −23.5 B | precision floor ≈ 0 (NEGATIVE, tiny) |
| A2 scalar temperature | 1 | 0.0 (γ*=1.0) | 0.00 | −1 B | NEGATIVE — no exploitable miscalibration |
| A3 per-bucket temperature | 16 | −205.8 bits | −0.57 | −41.7 B | NEGATIVE — calibration is not bucket-conditional |
| MIX-A_U2 (census-survivor table) | 294,912 | −3,421.1 bits | −9.50 | −295.3 KB | NEGATIVE — cross-class transfer fails |
| NULL (b-only table) | 4,096 | −10,533.7 bits | −29.26 | −5.4 KB | NEGATIVE |
| AX shuffled-feature | 294,912 | −7,738.0 | −21.49 | — | control confirms negative regime |
| AN random-feature | 294,912 | −16,376.3 | −45.49 | — | control |

The A2 dev curve is monotone toward γ=1.0 from BOTH sides (0.85: −1,111,
1.25: −791 dev bits): the production model is, to within this instrument,
optimally calibrated — flattening (γ<1) and sharpening (γ>1) both cost bits.
P1's λ*=0.4 was picked on dev yet harms held-out — the pooled
synthetic+photo table overfits the training mix and does not transfer.

## 2. Population P2 — class-matched photos (the HP1b survivor's economics)

Corrector: single-feature interpolated mixture on the HP1-B
equality/flatness indicators; table trained on 19 photos, evaluated on 9
heldout-photos prefixes (360k rows).

| arm | model bytes | held-out gain | per-krow | standalone net | installed-codec net |
|---|---|---|---|---|---|
| MIX-X_lu (u8 cells) | 98,304 | +1,056.2 bits | +2.93 | −98,172 B | +132 B |
| MIX-X_lu (u4 row-scaled) | 6,192 | +1,056.2 bits | +2.93 | −6,060 B | +132 B |
| MIX-X_ud / X_flat | ≈ same | +993.7 / +940.7 | +2.76 / +2.61 | negative | +124 / +118 B |

Standalone MDL: NET NEGATIVE at every precision (a ~132-byte-per-archive
gain cannot pay for its own 6–98 KB table). Installed-codec MDL (table a
shared offline asset): +132 B per archive ≈ +0.04% of the test pool payload —
positive arithmetic, but three orders below any integration threshold, and
per-image sign flips remain (HP1b full-stream: min −10.69/krow).

## 3. Accounting disclosures

- gain_bytes are ideal-bit deltas (Σ log2 ratios); finalized coder overhead
  is the production coder's, shared by A0 and all arms, so deltas stand —
  but wire-level confirmation would require HP6/HP10 Rust integration.
- Model bytes for u8/u4 are MEASURED serialized sizes (exact quantized cell
  counts); the gauntlet's ideal_bits/finalized_bits/payload split applies to
  wire confirmation, not to this screening.
- No probabilities here feed production: integration remains HOLD.

## 4. Gate disposition

Gate HP2 ("held-out predictive improvement AND positive model-inclusive
compression economics under at least one explicitly defined accounting
regime"): **FAILED.** The only positive-economics candidate (X_lu,
installed-codec) is +132 B/archive with per-image inconsistencies — no
meaningful economics; standalone fails at every precision. Per §15's rule
(a candidate that never wins gets a negative production verdict even if
mathematically interesting): **X_lu-family correctors = NEGATIVE for
production; preserved as the narrow HP1-B finding.** Temperature/calibration
arms = NEGATIVE (HP1-F closes: no exploitable miscalibration).

Consequences per the gauntlet's entry conditions: HP3 (hierarchical
organization), HP4 (reordering), HP5 (offline library), HP6 (quantization of
a surviving model), HP7 (learned long-range) — **no surviving candidate with
meaningful economics; branches remain CONDITIONAL-CLOSED pending the
operator's disposition.** HP8 (redundancy/compute) is the one branch whose
entry does not depend on HP2's outcome (P1-certified methodology applies);
HP9 spec already delivered; HP10 HOLD.

## 5. Limits

- Prefix population (61% of held-out streams) for vector-backed arms;
  full-stream census equivalents reported in HP1b where they differ
  (+5.11/krow full vs +2.93/krow prefix for X_lu).
- Single-feature tables by design; no interaction/feature-combination arm
  was justified by the census magnitudes.
- One dev split (20% train subsample); λ/γ grids bounded; winners at grid
  edges are labeled rather than extrapolated.
