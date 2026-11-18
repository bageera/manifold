> **R2H-E REVISED (2026-11-18, operator scale clarification):** the
> coefficient scale is `S = 2^Q` per precision (equivalently `c_lim/4`
> with `c_lim = 4<<n_q`), not the first-read `S = 3·2^Q`. The first-read
> tables were a mis-parameterization — superseded in place by the corrected
> values below. The §5 identities 5·e_lim/4 are the **within-expert** E
> range `5·S·12288`.

# BZ16 addendum — R2H-E clamp-polarity reconciliation (2026-11-18)

Operative memo directive: the recovered Python reference implements the
**asymmetric** coefficient clamp `min(4·scale, max(−scale, b + step))`
explicitly in source; the handoff-era twin used symmetric `±c_lim`. The
operator's observed Q8 energy-spread maximum (24,717,184 > the memo §5
value 15,728,640) matched the symmetric hypothesis, and the disposition is:
**recovered asymmetric clamp is the primary model; the symmetric variant is
retained only as a labeled ablation.**

## The scale convention (operator clarification)

The recovered scale is `S = 2^Q` per precision: clamped interval
`b ∈ [−2^Q, +4·2^Q]`, i.e. `S_Q8 = 256`, `S_Q12 = 4,096`, `S_Q16 = 65,536`.
This is the recovered parameterization itself (not a widened internal
representation with compensating normalization — the clamp is applied
directly to the gradient-updated coefficients in the same units the §5
derivation uses), and it makes the §5 identities read directly as the
within-expert E range:

```
5·S·12288 = 15,728,640 / 251,658,240 / 4,026,531,840
```

with `E = Σ₃ c_k·d_k`, `d_k = cdist(·)<<5 ≤ 4096` ⇒ `Σd ≤ 12288`. The
full live ceilings (the lo expert's energy enters the mixture doubled via
`clamp_k(2·e_lo, shift)`) are also asserted as pinned identities:

| Quantity | Formula | Q8 | Q12 | Q16 |
| --- | --- | --- | --- | --- |
| \|E\| ceiling (lo doubled) | 8·S·12288 | 25,165,824 (i32) | 402,653,184 (i32) | 6,442,450,944 (i64) |
| Cross-expert spread ceiling | 9·S·12288 | 28,311,552 | 452,984,832 | 7,247,757,312 |
| memo §5 within-expert range | 5·S·12288 | 15,728,640 | 251,658,240 | 4,026,531,840 |

Capacity utilization under the conservative |E| bounds: Q8 25.2M/2³¹
≈ 1.2%, Q12 402.7M/2³¹ ≈ 18.75% (56.25% under the memo's single-feature
frame), Q16 6.44G/2⁶³ ≈ 7×10⁻⁷% — comfortably inside the typed paths.
i32/i64 classification unchanged (Q8/Q12 i32, Q16 i64).

## R2H-D digests (twin-internal, corrected asymmetric polarity)

| rig | Q8 | Q12 | Q16 |
| --- | --- | --- | --- |
| camera | `627c2db122f48e78d370bafc466e36133c23cb031bd9b8dc42f900d863a068b6` | `79868fb35c6a3bcd…` (unchanged) | `24cd931961c5b9cb…` (unchanged) |
| brick | `e9ba4650e20c25edbb11b59ab3a532204856c98401a4caaa808bcf2508245bac` | `8075bc25e3d36d64…` (unchanged) | `599b85ef38cebe3d…` (unchanged) |
| noise | `98de1694acd1da50f800758197ce81a2f03cf5c594f3dc031642f5fa7e8366a5` | `8140643a4d8d9496…` (unchanged) | `eae77a934e52cd46…` (unchanged) |

Q12/Q16 remain byte-identical to the R2H-B..D set — the clamp never binds
at those precisions on these fixtures (verified directly: twin parity, 36
goldens and 512-window f-vectors all match the symmetric-run states). Q8
crosses the new −S floor, the digests change, and the observed Q8 spreads
settle at 23,204,928 / 24,757,632 / **24,717,184** — the last being exactly
the operator's reported observed maximum, confirming the scale reading
end-to-end.

Window ac bits (R2-G-equivalent instrument): camera 4099/4135/4172,
brick 3962/3356/3816, noise 4099/4098/4127. All nine parity runs PASS at
512/512 with the 36-golden subsets matching; Python and Rust agree to the
byte on the corrected states.

## Provenance

Same rule as R2H: these are **twin-internal** digests (twin == twin); the
original-reference archive comparisons remain OPTIONAL. The §4a open
question is CLOSED (resolved against the recovered source + the operator's
scale clarification). The symmetric-pole measurements recorded earlier in
the I-Gate addenda stand as the labeled symmetric-clamp ablation (no
BZ1–BZ15 rerun, no Python fixture reproduction attempted).
