# BZ16-R2H — recovered-semantics reconciliation in-repo (Python ↔ Rust twin)

Companion to the R2H operator memo and the two repo records
([docs/bz16-r1-record.md](bz16-r1-record.md),
[docs/bz16-r2-record.md](bz16-r2-record.md)). R2's acceptance contract was
defined against the original research environment's reference; the R2H memo
authorized the handoff gate (R2H-A artifact verification, R2H-B semantic
reconciliation, R2H-C 36 golden vectors, R2H-D nine full-stream SHA-256
probability-trace digests) and, critically, delivered the recovered semantics
as a reconciliation table in the memo body itself.

The `BZ16-R2-research-transfer.zip` package (MODEL_SPEC.md, scalar/NumPy
reference engines, 36 golden vectors, `expected_stream_sha256.json`, LUT
constants export, three 128×128 fixture crops, replay harness — 145,088 B,
20-file SHA-256 manifest) is **not on this box** (verified: filesystem sweep,
git remotes, session history). Per the twin-prototype convention, R2H-B..D are
executed **in-repo**: the memo's own recovered-semantics table is implemented
twice (scalar Python reference + independent Rust twin), and the memo's
original acceptance arithmetic is reproduced in structure. R2H-A stays
**PENDING** that package's delivery; on arrival everything re-points as a
diff (the contract format below is already pinned).

- Reference: `crates/mnf-mans/scripts/bz16_r2h_reference.py` (stdlib-only)
- Twin: `crates/mnf-mans/examples/bz16_r2h_parity.rs` (stdlib-only)
- Golden export: `research/results/bz16-r2h-{camera,brick,noise}-Q{8,12,16}.txt`
  (gitignored local logs: `bz16-r2h-python.log`, `bz16-r2h-rust.log`)

## 1. R2H-B — recovered semantics (provenance split)

Every recovered row **overrides the corresponding R2 pin** ("do not preserve a
pinned assumption if it conflicts with the recovered research implementation").

FROZEN by the memo's reconciliation table (verified numerics):

| Component | Recovered reference |
| --- | --- |
| Feature order | circular distance to zero, upper residual, left residual (`cdist(v,0)`, `cdist(v,up)`, `cdist(v,left)`) |
| Initial coefficients | two rows of three zeros |
| Expert inverse temperatures | 1 and 2 (`k_β = clamp(β·E >> (n−4))`, β = 2 folded as `2·e` in-room) |
| Probability total | 32,768 (fixed across Q8/Q12/Q16) |
| Mixture weight total | 65,536 |
| Fixed-share | normalize posterior at o, `×999`, `+16384`, normalize again |
| Gradient rounding | signed nearest, ties away from zero (step 1/32768) |
| Normalization remainder | first maximum of original input order (zero repair pulls from the first max) |
| Counter initialization | all ones |
| Counter rescaling | none in the reference (+1 per observation, unbounded growth) |

Recovered component set (per the memo's reconciliation work): {hi expert, lo
expert, **counter unigram**, uniform} — the counter unigram replaces R2's
freeze component. Mixture `m(v) = (Σ w_j·x_j) >> 16` with components at the
2^15 probability domain (`x_e(v) = L[k_e(v)] >> 9` from the frozen 2^24 LUT;
counter by largest remainder to Σ 32,768; uniform = 128).

PINNED IN-REPO (underdetermined; each affects the golden contract):

| Choice | Pinned value |
| --- | --- |
| Weight init | equal quarters `16384 × 4` (Σ 65,536) |
| Probability-domain fold | `x = L[k] >> 9` (2^24 → 2^15) |
| Pre-stream padding | `up = left = 0` for t < SIZE / column 0 |
| Audit window | first 512 positions per (fixture, precision) |
| Golden positions | t ∈ {0, 13, 255, 511} (the memo's 36 = 3 × 3 × 4) |
| Fixtures | synthetic stand-ins (archived crops pending): camera `97r+31c+(rc>>2)`, brick `37(r/8)+c+3(r%8)`, noise LCG (seed 12345); stream SHA-256 `94db39be…`, `7622b64c…`, `231b37ef…` |
| Zero-mixture fallback | if `Σ m(v) = 0` (LUT saturation flooring every numerator): uniform `f = 128 × 256`, no repairs — mirrored exactly in both languages |
| Coder instrument window | 18-bit CACM87 (exit floor ≥ 2·F; the 16-bit R2 instrument is insufficient at Σ f = 32,768) |
| Export convention | `p/n` lines carry the carries AFTER the update at t and counters AFTER the t-th increment; `E/k` at o are pre-update; one `d` line carries the R2H-D digest |
| State tail | `z` line = final `(c_hi, c_lo, w)` after the complete 16,384-update run |

## 2. Results — R2H-B/C/D PASS in-repo across all nine (fixture, precision) rigs

The golden exports are byte-stable across independent reruns (`cmp`: all nine
identical), and the Rust twin matches every exported field bit-for-bit; a
single frequency unit or digest byte aborts the run.

| Rig | Parity 512-window | 36-golden subset | R2H-D digest | z carries | Decode instrument | Repairs |
| --- | --- | --- | --- | --- | --- | --- |
| camera-Q8 | PASS | PASS | match | match | roundtrip, 4098 bits (Py = Rs) | 0 = 0 |
| camera-Q12 | PASS | PASS | match | match | roundtrip, 4135 bits (Py = Rs) | 0 = 0 |
| camera-Q16 | PASS | PASS | match | match | roundtrip, 4172 bits (Py = Rs) | 1,802 = 1,802 |
| brick-Q8 | PASS | PASS | match | match | roundtrip, 3962 bits (Py = Rs) | 14,055 = 14,055 |
| brick-Q12 | PASS | PASS | match | match | roundtrip, 3356 bits (Py = Rs) | 888,366 = 888,366 |
| brick-Q16 | PASS | PASS | match | match | roundtrip, 3816 bits (Py = Rs) | 2,686,642 = 2,686,642 |
| noise-Q8 | PASS | PASS | match | match | roundtrip, 4099 bits (Py = Rs) | 0 = 0 |
| noise-Q12 | PASS | PASS | match | match | roundtrip, 4098 bits (Py = Rs) | 0 = 0 |
| noise-Q16 | PASS | PASS | match | match | roundtrip, 4127 bits (Py = Rs) | 1,410 = 1,410 |

- Compared per position: observed symbol, `E_hi`/`E_lo` at o, `k_hi`/`k_lo`,
  post-update coefficient carries (both rows) and weights, post-increment
  counter FNV-1a, full 256-entry `f` and 257-entry cumulative `F`.
- The full 512-window per rig (512 × 9 = 4,608 vector comparisons) reproduces
  the memo's acceptance arithmetic ("4,608/4,608 independent probability-vector
  matches") in structure.
- R2H-D discipline exactly as memoed: the incremental digest consumes the 256
  pre-observation frequencies as **big-endian u16 at every position**, no
  retained CDFs, maintained across the whole 16,384-position stream
  (147,456 predictions and 37,748,736 frequency entries per the memo's
  accounting; 16,384 × 512 B consumed per rig).
- CACM87 carry-less decode instrument (the R2-G instrument, mirrored): the
  decoder recomputes every F_t from the decoded prefix alone; window bit
  counts agree across languages. The instrument's window is **18-bit** for
  the recovered total: the classic 16-bit CACM87 exit zone leaves a minimum
  interval of window/4 + 1 (≈16,385), which at `FREQ_SUM = 32,768` inverts
  `f = 1` sub-intervals and never terminates the renorm loop (observed live
  on noise-Q16 before the fix: Python grew `bits` unboundedly, the Rust
  i64 twin wrapped and spun). The 18-bit window's exit floor is 2^16 + 1 =
  65,537 ≥ 2·F, making every `f ≥ 1` sub-interval non-empty. R2's 16-bit
  pin stays valid for R2's own `F = 4096` (4× margin there).

## 3. The nine incremental probability-trace digests (twin-internal)

| Fixture | Q8 | Q12 | Q16 |
| --- | --- | --- | --- |
| camera | `d6520a0f5784f248…` | `79868fb35c6a3bcd…` | `24cd931961c5b9cb…` |
| brick | `1eae64499d7a28a0…` | `8075bc25e3d36d64…` | `599b85ef38cebe3d…` |
| noise | `7d15f8469c9a6470…` | `8140643a4d8d9496…` | `eae77a934e52cd46…` |

These are the **in-repo pair's** digests (Python export = Rust computation,
byte-equal). The memo's nine *archived* targets
(`expected_stream_sha256.json`) remain PENDING with the transfer package —
they encode the archived fixture crops and are not reproducible without them.

## 4. Q16 bounds extension (memo §5) — asserted as identities on the typed path

| Bound | Value | Type | Status |
| --- | --- | --- | --- |
| Energy accumulator Q8 | 12,582,912 | i32 | PASS |
| Energy accumulator Q12 | 201,326,592 | i32 | PASS |
| Energy accumulator Q16 | 3,221,225,472 | i64 (> `i32::MAX`) | PASS |
| Gradient numerator ×16 | 26,388,279,066,624 | i64 | PASS (< 2^62) |

Widened arithmetic is used consistently (i64 state, u64-scale products);
debug-mode overflow panics stay live on the typed path for every streamed
position. Every per-precision shift is boundary-tested (`k` table incl.
negative clamp and the 4096 cap). I7 (normalization products, accumulator,
counters, stream-length bounds) remains the memo's next bounds rung — HOLD.

## 5. Saturation census and economy note (honest, synthetic-only)

The live counters quantify the recovered engine's LUT saturation per rig
(`k = K_MAX` fraction over the 2 · 16,384 · 256 lookups):
camera-Q8 98.5%, camera-Q12 64.8%, camera-Q16 0.07%. The Q8 rig is
effectively saturated (probability vectors collapse toward the cap), which is
exactly why a higher precision is the working reference.

Ideal-model cost (15 − log₂ f(o), reporting only; synthetic fixtures are NOT
the archived crops, so these numbers do not reproduce the memo's 4.03/3.04/8.00
b/sym economy table — that comparison stays fixture-locked):

| Rig | Q8 | Q12 | Q16 |
| --- | --- | --- | --- |
| camera | 7.97 b/sym | 8.03 | 8.11 |
| brick | 7.93 | 6.88 | 4.87 |
| noise | 8.000 | 8.000 | 8.03 |

The incompressible fixture sits at (just above) the uniform baseline in every
precision — sanity signal that the recovered engine behaves like the original
on the comparable regime; the synthetic brick shows the counter component
buying real economy as precision rises; the synthetic camera ramp does not
reward the cdist features. Zero-repair pressure is the visible cost of
concentration (brick-Q16 ≈ 164 repairs/position; noise-Q16 1,410 in total).
Any economy decision stays with BZ16-N's native replay (HOLD per memo).

## 6. What this does and does not confirm

CONFIRMED: the **recovered model** (memo reconciliation rows, each overriding
the R2 pin) is deterministic and portable across two independent
implementations — scalar stdlib Python and safe Rust — including the full
digest-consumption discipline, the post-update export convention, and the
memo's §5 Q16 bound widening, over the complete 16,384-position streams and
their incremental digests.

NOT YET: R2H-A. The archived package (36 original golden vectors, nine
original stream targets, scalar/NumPy engine sources, LUT binary export, three
fixture crops) has not been delivered to this box; the memo's
"direct equality of all 4,097 LUT entries" is exercised in-repo as the frozen
table's FNV-1a digest (`50abef206188b374`, byte-equal to R1's regeneration;
archived-binary diff PENDING). Never call in-repo twin parity "parity with
the accepted reference". "Do not preserve a pinned assumption if it conflicts
with the recovered research implementation": the R2 pins listed in §1 are
superseded in-repo by the recovered rows.

## 7. Disposition

| Gate | Disposition |
| --- | --- |
| R2H-A original artifact verification | PENDING (transfer package; artifact list = §intro, contract format pinned) |
| R2H-B recovered-semantics reconciliation | PASS (in-repo twin, provenance split recorded) |
| R2H-C 36 golden vectors | PASS (in-repo twin; archived vectors re-point PENDING) |
| R2H-D nine trace digests | PASS (in-repo twin; archived targets re-point PENDING) |
| Q16 bounds widening (memo §5) | PASS (asserted identities) |
| I4B matched float/fixed comparison | CONDITIONAL per memo |
| I5 change-point evaluation | HOLD per memo |
| I7 exhaustive bounds + streaming | HOLD per memo |
| Native replay / production integration | HOLD per memo |
| R2 portability milestone (853187f / 087ace0) | PRESERVED |
