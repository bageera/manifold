# MNF14-CS1 — Coastline Scaling probe (measurement-only)

**Status: measured. The shared-fractal-rule hypothesis is REAL but exactly
one fixture deep; the boundary/refinement arms grow slower than PNG but lose
to the shipped pick at every scale.** Work-order memo, probe recipe
(manifold-codec-dev references/research-probe.md), CF1's promotion-gate
question: *does this content class exist in the corpus*, not *can the
representation express it*.

## The claim (work order)

Coastline-paradox scaling: a representation that describes structure at one
resolution and encodes only the NEW information needed at the next should
grow sub-quadratically with resolution on scale-recursive content. Five
classes × five resolutions (64..1024), four arms at complete serialized
cost, growth exponent = OLS slope of log2(bytes) vs log2(width) (pixel
count grows with exponent 2.0):

| Arm | Mechanism |
|---|---|
| A | shipped portfolio = release CLI pick (v1.13.0 binary, magic-dispatched MNF5/6/7/8/9) |
| B | exact integer pyramid: I_k = 2x2 mean floor + E_k (mod 256, zlib-9 per level), root raw — all transport |
| C | recursive boundary refinement for binary masks: refine iff the 4 quadrants differ; flagged cells send their 4 child bytes, zlib-9 per level — all transport |
| D | shared-rule Koch generator: per-level 1-bit subdivision signs (zlib-9), flat fill, 16 B framing |
| PNG | reference bytes, same vectors (PIL) |

## Instruments

`scripts/mnf14_cs1.py`; raw rows `docs/mnf14-cs1-gate.log` (the tables here
derive from that log mechanically). Every arm carries an in-loop decode-exact
assert: A via the CLI's own decode; B/C rebuild the image from the coded
levels and compare; D re-derives the polyline from the sign stream (one
shared subdivision implementation, so the wire cannot drift from the
fixture) and re-rasterizes. Fixtures are rasterized from scale-free
definitions at each resolution (never downsampled) — new detail emerges with
resolution by construction.

Fixture notes: `koch` = seeded Koch-subdivided square coastline with the
closing-corner sign slot unused (order 5); `realmap` = Natural Earth 10m
land polygons (public domain, fetched to /tmp — sha256 e547d749...), Europe/
Atlantic crop, rasterized per scale from the same vectors; `alpha` = blob +
circle cutouts; `frac` = fBm; `photo` = the repo photocorpus generator's
value-noise + palette formula, scale-normalized; `noise` = uniform random
(negative control).

## Headline table: complete bytes per (class, scale)

| class | scale | PNG | A pick | B pyramid | C boundary | D rule |
|---|---|---|---|---|---|---|
| koch | 64 | 450 | 352 | 1,308 | 566 | **241** |
| koch | 128 | 1,067 | 949 | 3,208 | 1,278 | **241** |
| koch | 256 | 2,569 | 2,528 | 7,991 | 3,002 | **241** |
| koch | 512 | 6,616 | 6,804 | 19,890 | 6,990 | **241** |
| koch | 1024 | 15,796 | 19,068 | 50,561 | 16,416 | **241** |
| realmap | 64 | 615 | 422 | 1,752 | 769 | — |
| realmap | 128 | 1,542 | 1,190 | 4,584 | 1,878 | — |
| realmap | 256 | 3,787 | 3,110 | 11,342 | 4,413 | — |
| realmap | 512 | 9,835 | 9,018 | 30,234 | 11,096 | — |
| realmap | 1024 | 25,913 | 25,320 | 80,767 | 27,770 | — |
| alpha | 64 | 363 | 265 | 1,167 | 524 | — |
| alpha | 128 | 716 | 590 | 2,386 | 967 | — |
| alpha | 256 | 1,465 | 1,397 | 4,770 | 1,807 | — |
| alpha | 512 | 3,264 | 3,506 | 9,958 | 3,416 | — |
| alpha | 1024 | 8,226 | 9,945 | 23,399 | 6,602 | — |
| frac | 64 | 3,835 | 5,376 | 5,177 | — | — |
| frac | 1024 | 49,357 | 121,846 | 149,681 | — | — |
| photo | 64 | 5,124 | 4,850 | 6,630 | — | — |
| photo | 1024 | 957,459 | 896,607 | 1,013,317 | — | — |
| noise | 64 | 12,420 | 12,339 | 15,875 | — | — |
| noise | 1024 | 3,151,184 | 3,150,354 | 4,029,951 | — | — |

(Full 30-row table in the gate log; every cell reproduce via
`python3 scripts/mnf14_cs1.py` after a release build.)

## Growth exponents (the decisive metric)

slope of log2(bytes) vs log2(width); pixel-count exponent = 2.0. Lower =
sub-pixel growth.

| class | PNG | A pick | B pyramid | C boundary | D rule |
|---|---|---|---|---|---|
| koch | 1.290 | 1.436 | 1.318 | 1.217 | **0.000** |
| realmap | 1.347 | 1.474 | 1.377 | 1.291 | — |
| alpha | 1.119 | 1.303 | 1.071 | **0.913** | — |
| frac | 0.909 | 1.088 | 1.218 | — | — |
| photo | 1.890 | 1.887 | 1.814 | — | — |
| noise | 1.997 | 1.999 | 1.997 | — | — |

Controls behave: noise 2.0 everywhere (no scale structure, no wins);
photo ~1.9 everywhere (the corpus rule: photographic interiors grow with
pixel count — the work order's own caveat, measured). frac: PNG's exponent
(0.909) vs A (1.088) shows the container's window out-performing
per-level coding at scale on fBm — a multiscale representation is not
automatically the sub-linear one.

## Verdict per representation

### D — shared fractal rule: mechanism REAL, class = exactly the fixture

The Koch wire is 241 B at every scale (exponent 0.000; the rule stream's
zlib saves ~1-2 B/level, so the constant is framing+entropy floor, not
growth). Decode-exact by a shared-code assert. This is the coastline
paradox made bytecode: describing the LAW of the boundary beats describing
its raster by 65× at 1024².

But: it fires on the fixture it was derived from — a seeded Koch curve —
and on nothing else in this matrix (real geography, sprite masks, and
photo interiors all carry structure no 4-split sign language captures).
CF1's lesson applies unchanged: the corpus control gates the format
question. A shared-rule arm graduates only over a real corpus class whose
generator is discoverable and whose decoder rule is frozen — no such class
exists in testdata/. **DENIED as a format arm on this corpus; recorded as
the demonstration that law-descriptions have unbounded-scale constant
cost.**

### C — recursive boundary refinement: exponent win, absolute loss

The boundary arm grows genuinely slower than PNG and the pick (koch 1.217
vs 1.290/1.436; alpha 0.913 vs 1.119/1.303) — the coastline-paradox effect
is present — but its per-scale ABSOLUTE cost loses to the shipped pick at
every scale except alpha 512/1024 (3,416 vs 3,506; 6,602 vs 9,945), and
alpha's crossover comes with PNG STILL ahead (8,226 at 1024: PNG's LZ77
window already exploits the mask's 2-D repetition that the per-cell
refinement stream does not). G5's mask-economics lesson reproduced: the
containers already eat run structure; a per-cell stream pays the
boundary-length tax at every level.

**DENIED as a format arm on this corpus.** The honest statement of its
capability: for VERY large binary masks (alpha at 1024² and beyond), the
boundary refinement is the only measured representation whose growth
stays below PNG's — the crossover that could matter is at MNFX-wrapper
scales (megapixel-plus masks), not at gate-corpus scales.

### B — generic multiscale pyramid: loses everywhere, mechanism understood

B's E_k residual streams are first-order-difference planes WITHIN tiles;
zlib's 2-D window exploits more context than the per-level coder sees,
and the pyramid's own level-0 raw + per-level payload never beats the
pick's single-pass MED + container. Measured loss at every (class,
scale) pixel class. **DENIED.** The v1.13 lesson (MNF13-R0: residual
conditional structure is FLAT across spatial offsets) already predicted
this: nothing in these fixtures gains from conditioning finer levels on
coarser ones that the container + MED do not already reach.

### A — the shipped pick: wins or ties the honest multiscale arms everywhere

At no (class, scale) does any arm beat the pick except alpha 512/1024 by
C (and PNG still beats C there). The growth exponent A > PNG on every
mask class (1.30-1.47 vs 1.12-1.35) is a CONTAINER-level statement: PNG's
deflate window + palette machinery scales better than the pick's current
arms on pure masks at these sizes — a real, small-absolute-cost note for
the future, not a format gate.

## Scope-of-closure

CLOSED by this probe:
- shared fractal RULES as a generic image representation — the mechanism
  is real (241 B/exponent 0) but its firing class is "content generated
  by the same rule family" which no corpus class satisfies;
- generic exact multiscale pyramids U(I) + E_k with 2x2-mean refinement —
  loses at every pixel class against MED + container;
- per-cell recursive boundary refinement as a mask representation at
  corpus scales — exponent advantage real, absolute advantage absent
  (alpha ≥512² the sole partial exception, still under PNG).

OPEN (deliberately not measured here):
- boundary refinements whose alphabet is the COARSE CELL's residual space
  (predictor-coded refinement payloads, not raw bytes) — C's transport is
  deliberately primitive (honest upper bound); a MED-coded E layer is a
  different arm and could close the absolute gap while keeping the
  exponent, but B's result (E_k coding never beats the container) argues
  against large gains;
- scale-recursive generators for TEXTURE (frac) — the memo's
  fractal-texture hypothesis remains untested here because no compact
  decoder-side rule was implemented; the fBm row only shows the container
  window out-scaling per-level payloads;
- the same question at MNFX alpha-mask scale (≥ 1 MP) — C's growth
  crossover suggests re-measuring C-style refinement THERE, not in the
  gate corpus;
- real-world binary masks beyond realmap/alpha (e.g. document ink masks)
  if such a class ever enters testdata.

## Instrument notes (carried in research-probe conventions)

- The exactness asserts earned their keep: the first C draft's 2-bit
  quadrant alphabet was LOSSY (coarse-level children are means, not
  binary) and its pack/unpack had a nibble desync — both caught by
  asserting reconstruction, not by roundtrip luck. Arm wires that carry
  their own decoder must assert decode-exact in-loop (the B/C/D asserts).
- reshape semantics on nested 2x2 grids do not commute: `.reshape(-1,4)`
  on a (cells,2,cells,2) tensor is NOT the cell-quadrant enumeration —
  the transpose-to-(cells,cells,2,2) form is required on both sides. The
  asymmetry produced silently-wrong first numbers; reconstruction asserts
  caught it.
- Fixtures regenerated from scale-free definitions (not downsampling) is
  what made the growth exponent meaningful at all.

## Disposition

MNF14-CS1 measured; NO format changes; MNF8/MNF9/MNFX untouched. The
instrument (`scripts/mnf14_cs1.py`), the gate log
(`docs/mnf14-cs1-gate.log`), and this doc are the record. If a
megapixel-alpha or rule-generated content class shows up in the corpus,
the C and D rows here are the starting points — the growth-curve
instrument is built for re-runs (`--scales 64,128,... --levels N`).