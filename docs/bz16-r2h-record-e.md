# BZ16 addendum — R2H-E clamp-polarity reconciliation (2026-11-18)

Operative memo directive: the recovered Python reference implements the
**asymmetric** coefficient clamp `min(4·scale, max(−scale, b + step))`
explicitly in source; the handoff-era twin used symmetric `±c_lim`. The
operator's observed Q8 energy-spread maximum (24,717,184 > the memo §5
value 15,728,640) matched the symmetric hypothesis, and the disposition is:
**recovered asymmetric clamp is the primary model; the symmetric variant is
retained only as a labeled ablation.**

## What changed

Both the scalar Python reference
(`crates/mnf-mans/scripts/bz16_r2h_reference.py`) and every Rust twin
(`bz16_r2h_parity.rs`, `bz16_i4b_float_fixed.rs`, `bz16_i5_changepoint.rs`,
`bz16_i7_bounds_streaming.rs`) now clamp the gradient-updated coefficients
to `b ∈ [−S, +4S]` with `S = 3<<n_q` (i.e. the same interval up to the
scale constant: `3·c_lim/4` to `3·c_lim`; the recovered form uses `scale
= S`). Per-precision: `S_Q8 = 768`, `S_Q12 = 12,288`, `S_Q16 = 196,608`.

Energy-domain live bounds asserted in all mirrors (exact identities):

| Quantity | Formula | Q8 | Q12 | Q16 |
| --- | --- | --- | --- | --- |
| \|E\| ceiling (lo expert doubled) | 8·S·12288 | 75,497,472 | 1,207,959,552 | 19,327,352,832 |
| Cross-expert spread ceiling | 9·S·12288 | 84,934,656 | 1,358,954,496 | 21,743,271,936 |
| memo §5 single-feature reading | 5·e_lim/4 | 15,728,640 | 251,658,240 | 4,026,531,840 |

`E = Σ₃ c_k·d_k` with `d_k = cdist(·)<<5 ≤ 4096`, so `Σd ≤ 12288`; the lo
expert's energy enters the mixture doubled (`clamp_k(2·e_lo, shift)`),
which is why the engine's exact ceiling is 9·S·12288 — the §5 value 5·e_lim/4
models a single feature magnitude and understates the three-feature sum.
The §5 identities remain asserted; the wider exact value is the live bound.
i32/i64 classification unchanged (Q8/Q12 i32, Q16 i64).

## R2H-D digests (twin-internal, asymmetric polarity)

| rig | Q8 | Q12 | Q16 |
| --- | --- | --- | --- |
| camera | `1ee7c1b7b97150a78ed7ab038dfbc32ecb6da500335035131b42e39fe40f0922` | `79868fb35c6a3bcd…` (unchanged) | `24cd931961c5b9cb…` (unchanged) |
| brick | `ae30a2e2541c381b27574e8981a260299caa77043da7f84cccfc5e7d8069562f` | `8075bc25e3d36d64…` (unchanged) | `599b85ef38cebe3d…` (unchanged) |
| noise | `6fff5f00d8c29005f56ead8dd8495198125d4564ba4f545131c5c8c8f052c3f6` | `8140643a4d8d9496…` (unchanged) | `eae77a934e52cd46…` (unchanged) |

(Q12/Q16 columns unchanged from the R2H-B..D set — see §3 there. The clamp
never binds at Q12/Q16 on these fixtures, which is itself verified: twin
parity, 36 goldens, 512-window f-vectors, and the digests are byte-identical
to the symmetric implementation at those precisions. Only Q8 crosses −S.)

Window ac bits (the R2-G-equivalent instrument): camera 4099/4135/4172,
brick 3962/3356/3816, noise 4098/4098/4127. All nine parity runs PASS at
512/512 with the 36-golden subsets matching; the Python reference and the
Rust twin agree to the byte on the new Q8 states; full-stream cost
float-domain values moved within noise (camera-Q8 130,672.81; brick-Q12
112,714.49; noise-Q16 131,533.50).

## Provenance

Same rule as R2H: these are **twin-internal** digests (twin == twin). The
original-reference archive comparisons remain OPTIONAL. The §4a open
question from this record (clamp polarity) is now **CLOSED — resolved
against the recovered source**, per the operator memo; the symmetric-pole
measurements recorded earlier in the I-Gate addenda become the
symmetric-clamp ablation record (no rerun of BZ1–BZ15; no Python fixture
reproduction attempted).
