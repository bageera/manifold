# Ternary residuals — first-cost analysis (2026-10-27)

Question: do balanced-ternary digit splits of prediction residuals beat
the byte stream? Measured on the same corpora as the fold/chart probes
(bottle/smooth, plaid, random) plus the MED-residual distribution.

## 1. Independent-digit split (the naive ternary transform)

Residual bytes → centered [-127,128] → 6 balanced-ternary digits
(t_k ∈ {-1,0,1}); per-digit empirical entropy summed:

| class | raw H (b/byte) | per-digit entropies | ideal BT sum | ratio |
| --- | --- | --- | --- | --- |
| bottle/smooth | 4.26 | 1.58, 1.56, 1.56, 0.12, 0.09, 0.00 | 7,550 B | 0.61 |
| plaid | 2.81 | 1.32, 1.05, 1.02, 1.23, 1.21, 0.04 | 9,031 B | 0.73 |
| random | 7.98 | 1.58 ×5, 0.32 | 12,652 B | 1.03 |

Verdict: the INDEPENDENT-digit transform is worse than the raw byte's own
order-0 entropy on every class (bottle: BT ideal 0.61× vs raw ideal
0.53×) — because digits of the same byte are correlated and splitting
them destroys that. No win.

## 2. Joint/conditional structure — already captured

The raw byte stream's z9 packing (1,212 B on the MED-residual stack)
already exploits the cross-byte/digit structure that a conditional
ternary model would reconstruct. The measured chain: MED residuals
centered → p(0)=0.44, p(|v|≤2)=0.94 → order-0 entropy 2.23 bits/sym
→ **z9 packs to 1,212 B, below even the ideal ternary-Golomb-style
estimate (1.19 bits/sym ≈ 1,822 B)**.

## 3. The honest mechanism comparison

| scheme | bits/sym (measured/ideal) | bytes (64×64×3 MED) |
| --- | --- | --- |
| centered byte + z9 (**shipping path**) | 1.98 (packed) | **1,212** |
| ternary digit split (independent) | ideal ≥ 6·1.58 | ≥ 7,550 |
| ternary Golomb-style (log-k + marker) | ~1.19 ideal | ~1,822 |

## Verdict

Ternary residuals lose to the shipping byte+zlib path on every measured
point — the same lesson the entropy-backend study recorded (coders rank
behind models at equal inputs):
- the alphabet transform (ternary digits) redistributes but does not
  reduce information, and the digit decorrelation penalty exceeds the
  high-order-digit saving;
- the concentration in |v|≤2 the ternary idea targets is ALREADY exploited
  better by LZ's string layer + Huffman on centered bytes;
- a ternary Golomb front only reaches the ideal-entropy scale, still
  1.5× worse than the packed path.

NOT RETAINED. No format item. The measured record now covers: scan-order
transforms (dead), digit-alphabet transforms (dead), backend family
(measured per image), predictor family (shipped), structural repeats
(candidate: shears-in-reference-path, from the v0.7 replication).