# MNF14-ID3 — Tile-scale div-form law family: honest negative (instrument-only)

Status: **CLOSED as a production candidate** (Jon's disposition, 2026-11-01):
the inverse solver is retained as a research instrument and a measured
member of the composition-aware candidate bank; no arm, no wire record.
The decisive number is not 15/16 exactness — it is **0 winning tiles
across 1,324 opportunities** (300 gradient + 1,024 hubble). The sixteenth
fixture is preserved as an explicit negative control (the pre-registered
family boundary), NOT folded into the solver to score a perfect board.
Instruments in `crates/mnf-mans/scripts/mnf14_id3_probe.py`; rows in
`/private/tmp/mnf14_id3/id3_rows.json` (regenerable).

## The proposal (wire block frozen before the harness, ADR-010(a))

Per-tile law, per channel, all-integer:

    pred(x, y) = clamp(v00 + (tau + vx·x + vy·y) // D, 0, 255)

Wire (mode 9 LAW-DIV `--mnf8` record — proposed, NOT shipped):
`[1 B mode][21 B params][n B residual]`, params per channel
`[v00 u8][tau u8][vx i16 LE][vy i16 LE][D u8]`; floor division incl.
negatives (div_euclid); clamp part of the wire law; canonicalized to
primitive triples (tau in [0, D) resolves the v00/tau redundancy: 
pred(0,0) = v00 exactly; per-pixel offsets reprice in the residual).

Mode 6 already contains the D=32 unclamped slice of this family with i8
rates; the proposed new reach = (a) free D, (b) i16 rates, (c) clamp.

## Measured (probe stages A / B / F / C)

**A — exact inversion 15/16 member fixtures** (fixtures from the wire
law's own replay; membership by construction): steep integer rates
(±9..48/px — mode 6's i8 ceiling is ±3.97/px), fractional lattices
(1/2, 1/3, 3/5, 5/6, 25/24, 130/64, −70/64), clip saturation (plateaus at
0 and 255, including a 2-D case clipping at both corners), negative
slopes, flat degenerate, and an off-lattice tau folding into v00. The one
decline is pre-registered as the family boundary: a ramp whose generated
origin sits AT the 255 clamp while the row drops below it — `pred(0,0) =
v00` cannot continue that row (no wire-law member can generate it; the
fixture was generated OUTSIDE the wire lattice). Inversion verification is
independent decode-form replay; solver = interior-pair interval
intersection (clip-aware: free pixels are 1..254) × bounded 14-entry D
table; textured/noise tiles reject in O(n) via a spread>1 span check.
Budget: 0 candidate-cap skips fired on every measured class.

**B — fp control, mechanism named**: fp-LSQ on the steep tile lands its
fit EXACTLY on the true plane (abs-err 0 in float), but the mode-6 wire
(i8 rate at 1/32) cannot transport rate 9.0/px — its clamped replay costs
77 B vs the div-form's exact 37 B. The mode-6 decline is TRANSPORT, not
fit quality: this is the inverse-side form of the transport floors, not a
new rounding-floor instance.

**F — firing census (the decisive negative, named-class files)**:

| file | tiles | div wins | tile argmin | div-exact tiles |
|------|------:|---------:|-------------|----------------:|
| gradient_320x240 (testdata) | 300 | **0** | MED 300 | 0 |
| hubble_512 (CLI-decoded, ID2's own fixture) | 1024 | **0** | literal 515 / const 282 / MED 227 | 0 |

The div form never wins a tile on either named class, and is not even
exact on any tile (0 div-exact). Mechanism, measured two ways:

- Axis-aligned ramps (mode-6/9's own best case — gradient) ride the
  shipped LEFT predictor at ~0 marginal cost: IF4's record shows this file
  encodes as `flags 0x20` = FLAG_PRED_LEFT. A ramp law that decomposes to
  left-neighbour + small residuals cannot beat that arm's framing.
- Hubble's expensive-to-predict tiles (the class that elected const/MED)
  are black-dither noise + stars, not ramps — their residual under any
  div law is full-amplitude and the arm declines exactly where the ID2
  arm was built to fire. The div form and the flat-field arm cover
  DIFFERENT named classes, and the div form's class is already owned.

**C — composition (the machinery works; the demand is absent)**: on a
256×256 frame of 85% law-generated tiles (six mixed div families incl.
steep/frac/clip) + 15% noise, the per-tile pick selects div on 96 tiles,
literal on the 82 noise tiles, MED/planar on the remainder; totals
42771 vs 45725 without the arm = **−2,954 B (−6.5%)** per-tile-proxy sum.
The existing per-tile record grammar composes per-region laws exactly as
ID1-B concluded; where real generative tiles exist, mode 9 would win.
CLI anchor (current-binary encoding of the composite) not run: release
binary not built this session — recorded as absent, not as a number.

## Scope of closure

CLOSED (this scope): the tile-scale div-form law family has NO firing
surface on the named classes reachable by its vocabulary: axis-aligned
ramps are owned by the LEFT predictor arm (a per-file flag, ~0 transport),
diagonal-content ramps decompose to per-axis ramps per 16×16 tile, and
photographic prediction-expensive content is noise-dominant, which no
exact-generative law can express. Mode 6's D=32, i8 slice already serves
the in-family tiles this corpus produces (IF4: never fires there either).

The composition result is a positive instrument: mode-6/7/8/9-style arms
compose inside the shipped pick and their cost model prices them
correctly against literal/refs — the ADR-010 engine shape is confirmed a
third time (ID1-B, ID2, ID3-C).

NOT closed (open, priority-ordered next rungs): 
1. The firing-surface rule now points back the OTHER way: run the
   Bit-Cost Autopsy on the current pick's hubble/photographic tiles to
   find where the pick's residual is expensive BEFORE proposing the next
   vocabulary (ADR-010.4's order: name the surface first, grammar second).
2. Composition-aware discovery (ID1 open rung 3): propose region
   boundaries (quadtree over the residual-cost map) + per-region laws —
   ID3-C measured that per-tile dispatch is the mechanism; a boundary
   proposer is the missing piece, and div-form is now a MEASURED member
   of the candidate bank for it.
3. A div-form REFINEMENT would need a class producing per-tile diagonal
   or cross-axis ramps at tile granularity (none in this repo's corpora —
   recorded, re-litigate only with a new corpus).

## Provenance

- Instrument: `crates/mnf-mans/scripts/mnf14_id3_probe.py` (stages
  argv-selectable: `ab`, `abf`, `c`; run under the mnfpca2 scratch venv).
- One process-accounting bug caught by stage A itself: a first diff-span
  membership precondition rejected clip-saturated members (saturation
  injects boundary diffs); replaced by interior-pair interval derivation
  (1..254 pixels are formula-exact by construction). The committed
  instrument is the post-fix form; `id3_rows.json` rows are the record.
- Stage B's first control was mis-specified (fp fit quantized WITHOUT the
  mode-6 wire's i8 clamp and beat the law spuriously); corrected to the
  wire-limited form above — the corrected table is the only honest one.
- The 812,180 corpus anchor is untouched: no wire change shipped, so the
  corpus gate that guards ADR-010(g) does not trigger; the ADR-010(h)
  full-revert-on-tie rule never engages for an instrument-only rung.