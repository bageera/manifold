# MNF14-HF1 — Hopf vibration / Hopf-feature RGB laws (disposition: DENIED)

Work-order memo: vibrating along a Hopf fiber leaves the projection
UNCHANGED (h(e^{iφ}z1,e^{iφ}z2)=h(z1,z2), exact); the experiment therefore
uses a relative-phase field between two spatially varying complex
coordinates, whose Hopf-projected features predict RGB, in stationary and
vibratory forms, against conventional affine prediction and MED. Instrument:
`hf1_probe.py` (+ follow-up rows in `hf1_followup.py`), copied verbatim to
`crates/mnf-mans/scripts/mnf14_hf1_probe.py`. Standing honest protocol:
28-B matched frame, zstd-19, CD-G relabel, raster residual order, exact
recon asserted per record.

## The algebra that decides it first (assert-tested, not assumed)

For phasor fields z_k = A_k·e^{iφ_k(x,y)}, every Hopf-observable feature is
a function of the RELATIVE phase Δφ = φ1−φ2 and the amplitudes only:
   w1 = 2A1A2·cosΔφ, w2 = 2A1A2·sinΔφ, w3 = A1²−A2².
The fiber identity was asserted numerically (200 random samples, exact to
1e-9): the common phase cancels. Consequence for the wire: a "vibratory"
record has NO representable difference from the stationary one (the fiber
phase never enters the grammar) — asserted byte-identical per record in
the instrument (both re-costs equal on every image). The Hopf collapse
also means the feature vector's angular content is ONE relative-phase
field; a per-coordinate ramp model degenerates to an ordinary 2-D
sinusoid law. The fibration adds no descriptive freedom at wire cost.

## Decoder-exact form (zero fp in decode)

Relative-phase index idx = ((pQ10·x + qQ10·y) // (n−1)) mod 1024
(p,q = i16 Q10 turns/axis, 4 B); correction per channel =
(b_c·LUTc[idx] + s_c·LUTs[idx] + 2²⁰) >> 21 with LUTc/LUTs = the wire
doc's deterministic 1024-entry cos/sin at Q14 (ZERO transport,
MNF9-convention shape); pred = clip(a_c + corr, 0, 255), a_c i16 RGB
units, b_c/s_c i16 Q7 (18 B); + 2 tag bytes = 24 B fixed transport.

Discovery per record: integer-ramp grid stage-1 (L1 proxy from
normal-equation fits), honest wire re-cost of the top-6 through zstd-19,
then a LOCAL Q10 FRACTIONAL refinement sweep around the best ramp —
the memo's area-3 precision arm, measured explicitly.

## Measured (complete records)

### Fixtures (the mechanism's home turf + controls)

| image            | A_med | B_affine | H1dir | H1corr | H1q (pick) | exact-law replay |
|------------------|-------|----------|-------|--------|------------|------------------|
| HF-A exact law   | 60    | 651      | 90    | 84     | 84 (root)  | **81**           |
| HF-B off-lattice | 4,151 | 14,419   | 2,840 | 4,175  | 4,175 (root)| —               |
| NOISE            | 196,655| 196,730 | 196,679| 196,679| 196,679 (root)| —            |

- **The decisive negative**: even the KNOWN-PARAMS exact-law replay
  (81 B) LOSES to plain MED (60 B) on the memo's own positive control —
  the 24-B transport + container floor beat a zero-residual wire at
  this fixture scale. The memo's "strong exact-law synthetic result"
  does not survive honest complete-record accounting here.
- **Off-lattice (HF-B, p = 3.37 turns)**: the honest fraction-refined
  replay = **2,840 B vs MED 4,151 = −1,311 (−32%)** — a REAL per-class
  win, and it is the PRECISION arm (area 3), not the Hopf projection:
  the integer-ramp grid misses, the Q10 fractional ladder recovers.
  This row is the transferable lesson, already named by IF3.
- NOISE behaved (arms ≈ A + transport; no false wins after PG1's
  honest-cost lesson).

### Real photographs (512²)

| image     | A_med  | B_affine | H1dir  | H1corr | H1q (pick) |
|-----------|--------|----------|--------|--------|------------|
| astronaut | 405,842| 405,564  | 571,704| 456,224| 456,224 (root)|
| coffee    | 439,143| 443,637  | 619,645| 494,280| 494,280 (root)|
| chelsea   | 331,079| 331,751  | 585,499| 359,051| 357,047 (split)|
| hubble    | 486,037| 486,461  | 485,475| 489,502| 489,502 (root)|

Full-predictor phasor loses +41k..+180k (photos are not global
sinusoid fields); correction-form loses +28k..+55k. chelsea's quadrant
split FIRES (2,004 B better than its root) but still loses to MED by
~26k. hubble's H1dir −562 is inside framing noise (and its corr form
loses). No photographic arm beats MED anywhere.

## Verdict

**DENIED as a photographic arm.** No promotion, no wire change.

Two recorded findings survive the denial:
1. **The fiber identity IS the denial mechanism**: Hopf-projected
   phasor features collapse to a relative-phase sinusoid ramp — the
   geometry's extra freedom is not expressible at wire cost, so a
   "Hopf" arm is an IF-class sinusoid arm with 24 B more transport.
   Any future Hopf-named proposal reduces to a parameterized phase
   field under the existing pick (per-region ramps via the IF3
   compression-aware tree are the open form).
2. **Precision ladder confirmed on a new class**: the off-lattice
   row (−32%) is the fractional-ramp refinement earning its keep on
   sinusoid-phase content — the same rung IF3 opened for illumination
   laws. If a phase-field class ever arrives in a corpus, this is
   the discovery machinery to reuse.

Method-honesty note: the fitted-law arms on HF-A (90/84 B) measure
discovery-from-pixels; the follow-up's 81-B row measures the KNOWN
params — both lose to MED's 60-B complete record, so the fixture
verdict does not depend on discovery quality.

## The memo's six-areas accounting (measured evidence per area)

1. Structure discovery — the only proven lever: IF3's compression-aware
   tree (chelsea −5,146 / coffee −4,554) and now the phase-ramp
   precision row above. IF4 shipped the arms; they don't fire on
   today's corpus.
2. Spatial/frequency relationships — MED + the CD relabel own most of
   it; sinusoid-phase fields add structure only where MED's residual
   is already near-zero (HF-A's 60-B floor) — the model has nothing
   left to buy.
3. Parameter precision — REPRODUCED twice now (IF3's illumination
   refit, HF-B's off-lattice ramp): coefficient recovery at wire-lattice
   precision is often the whole win; quantize transport FIRST and
   sweep the fractional lattice, or the arm dies at the grid.
4. Residual entropy — R0 measured the honest floors (order-1 −0.33 B
   reproducible; beyond that, wire-declined); the remaining photographic
   entropy is not geometric structure (GSE-02's natural-data 0/5).
5. Model transport — PG1's and this probe's decisive rows: 24–160 B of
   fixed tax loses to zero-transport prediction on cheap-residual
   content; models pay only when they remove >> their transport.
6. Across resolutions — law classes are constant-cost per resolution
   (CS1/PG1 ladders) but real photographic detail is not inferable
   from coarse observations (IF2's precision-transfer control: ≈8.6×).

Net: the corpus-deciding lever remains (1)+(3) — compression-aware
discovery with wire-precision refinement — which is exactly the
IF3→IF4 line already shipped. The Hopf form adds no new freedom; the
phase-ramp precision arm is the reusable piece of this experiment.

Read before any Hopf/fibration/phase-field proposal.