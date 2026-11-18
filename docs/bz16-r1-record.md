# BZ16-R1 — Rust confirmation of the frozen exponential LUT and preliminary Q12 bounds

Companion to the BZ16 memo ("Long-stream precision and independent probability
verification"). The BZ16 Python experiments ran in a separate research
environment; per the work-order convention this repo record captures what the
Rust side can confirm **today, from the memo's frozen specification alone**.

Instrument: `crates/mnf-mans/examples/bz16_lut_confirm.rs` (stdlib-only, no new
dependencies). Log: `research/results/bz16-r1-confirmation.log`.

## What R1 confirms

### R1.1 Frozen exponential LUT (memo §3)

Regenerated in Rust from `L_k = max(1, round(2^24 · e^{−k/256}))`, k ∈ [0, 4096],
using half-to-even rounding (Python `round()` parity for the positive domain):

| Property | Memo claim | Rust result |
| --- | --- | --- |
| Entries | 4,097 | 4,097 ✓ |
| Binary size (u32) | 16,388 B | 16,388 B ✓ |
| Maximum | 16,777,216 | `L_0 = 16,777,216` ✓ |
| Minimum | 2 | `L_4096 = 2` ✓ (max represented energy 4096/256 = 16) |
| All entries u32 | yes | ✓ (no clamped 1-entries) |

Full-stream checksum of the little-endian u32 binary table (FNV-1a 64,
stdlib-only — the digest is only ever diffed against an export of the same
stream): **`50abef206188b374`**.
Head/tail entries: `[16777216 16711808 16646655 6196149 …  2 2 2]`.

Honest caveat: entry-level byte parity against the *exported* constants is
settled in R2; f64 `exp()` is not required bit-identical across libm
implementations, so the frozen table itself remains the authority (which is
exactly why the memo freezes it).

### R1.2 Preliminary Q12 integer bounds (memo §5)

Evaluated in exact integer arithmetic (no float):

| Bound | Value | Fits |
| --- | --- | --- |
| Energy accumulator `3·(4·4096)·(2·2048)` | 201,326,592 | i32 (memo: `i64` proposed anyway) ✓ |
| Gradient numerator `32768·4096·4096·3` | 1,649,267,441,664 | i64 (comfortably < 2^63) ✓ |

The example computes these on the typed path (`i64`/`u64`) so debug overflow
panics would catch any regression.

### R1.3 Proposed Rust type mapping (memo §5)

Representability asserted: Q12 coefficients (`|c| ≤ 4096`, `i32`), circular
distance (`≤ 4096`, `u16`), energy products (`i64`), exponential constants
(`u32`), frequencies/cumulative (`u32`). Scope preserved as the memo's: these
are the **tested correction engine's** bounds, not an exhaustive overflow proof
for the complete probability model.

## Blocked: BZ16-R2 (golden-vector parity)

Requires artifacts from the Python research environment (not present on this
box — verified: no BZ16/Boltzmann commits, docs, or python artifacts anywhere
in `~/projects/blacktech`):

1. 36 golden probability vectors (§4) — contract `F_Rust,i(s) = F_Python,i(s)`
2. Nine full-stream SHA-256 digests (§4)
3. The scalar reference engine source + exact model spec (features, Q12
   coefficient values, expert inverse-temperatures, fixed-share weights,
   rounding/normalization/tie-break semantics)

On arrival, R2 implements the scalar engine in Rust against the frozen LUT
constants and replays every audited position; any one-frequency-unit mismatch
is a parity failure.

## Memo arithmetic re-verified independently (2026-11-18)

Q8 excess 97.81 bits ≈ 12.23 ideal bytes over the two image fixtures ✓;
Q16 excess 0.24 bits ✓; Q8/Q12/Q16 totals consistent with the memo table.
