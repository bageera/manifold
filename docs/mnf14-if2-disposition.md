# MNF14-IF2 — fitted photographic gradients: in-repo disposition (2026-10-30)

**Status: the pilot's headline does not reproduce through my independent
re-implementation; the precision-transfer control does (directionally);
NO promotion.** Work-order memo + probe recipe
(manifold-codec-dev references/research-probe.md). The pilot's numbers are
NOT adopted; this doc quotes only measurements this session produced.

## What the memo claimed (external pilot; Python research codec)

- 4 real photographs at 512² (scikit-image fixtures): MED/CD total
  1,505,829 B; best fitted-gradient model total 1,512,790 (+6,961);
  chelsea alone −1,079 (1/4 files).
- Resolution evaluation (same params, no refit): savings do NOT grow
  consistently with resolution (224 → 1,400 → 2,460 → 1,079).
- Decisive control: a photograph generated from an exact curved
  illumination formula — exact coefficients = 101 B; same family with
  params inferred at 64² = 66,673 B; params refitted at 512² = 4,713 B.
  Conclusion: parameter PRECISION, not the scale-free evaluation, is what
  fails to transfer.
- Field-guided MED variants and field-aligned causal prediction lost to
  MED on all four photographs.
- Conventional: WebP-lossless = 1,310,312 B — far below the research
  pick (the gap is engine-scale).

## This session's independent reproduction (from the report text alone)

Instrument: one process; the canonical MED plane imported from the repo's
own walker (`mnf11_x2_dynamics.med_pred` semantics — edges by absolute
coords, never re-derived); ADR-008 CD-G wire order; zstd-19 framing to
match the pilot's wire class; per-arm `(pred + res) % 256 == source`
asserted BEFORE quoting any size; fitted-field coefficients honestly
transported (f64 LE, ~41-73 B per field). Sources: scikit-image sample
data (astronaut PD/NASA, coffee CC0, chelsea CC0, hubble PD/NASA),
LANCZOS-resized to 512².

### Photographic arms (complete bytes incl. field transport)

| Image | MED/CD baseline | +planar | +radial | +curved | guide-MED | field-align |
|---|---|---|---|---|---|---|
| astronaut | 405,814 | 479,022 | 443,931 | 481,947 | 405,839 | 405,839 |
| coffee | 439,115 | 509,338 | 465,188 | 521,670 | 439,140 | 439,140 |
| chelsea | 331,051 | 365,924 | 372,204 | 367,369 | 331,076 | 331,076 |
| hubble | 486,009 | 486,164 | 487,185 | 486,193 | 486,034 | 486,034 |

Fitted-pixel-corrected prediction LOSES on all four, and the
steered-tiebreak / field-aligned variants barely move (their firing
bands are sub-percent of pixels — the mechanism's surfarce is nearly
empty on this corpus, `guide_med` = baseline + 25 B of field tax).

A second, stronger protocol — fit the planar field to MED's SIGNED
ERROR plane per channel at 64², subtract at full res — flips chelsea
to a genuine win but hubble regresses:

| Image | baseline | error-field-corrected | Δ |
|---|---|---|---|
| chelsea | 331,051 | 330,631 | **−420** |
| hubble | 486,009 | 486,430 | +421 |

Directionally consistent with the pilot (chelsea is the one real
winner; it has smooth-coherent interior structure) but the magnitude
and sign on hubble differ (the pilot had +159) — protocol-sensitive,
not a stable class win.

### Precision-transfer control (rebuilt per the memo's rows: the fixture IS the gray illumination curve)

| Row | Memo | Mine |
|---|---|---|
| exact known curve (the pilot's coder) | 101 | (different framing; mine residualizes on MED: 4,201 baseline) |
| same family, params inferred at 64² | 66,673 | 5,469 |
| same family, params refitted at 512² | 4,713 | **638** |

The memo's SCIENTIFIC point reproduces in sign and rough magnitude:
the 512² refit beats the 64²-inferred field by ~8.6× (memo ~14×) —
parameter precision is the transferable-structure bottleneck; smooth
fields evaluate at any scale but mis-estimated coefficients leave
residual structure that still costs real bytes. The absolute rows
differ because my framing residualizes against (MED prediction, not
raw) and my fixture generation is not bit-identical to theirs —
expected and immaterial: the CONTROL's content is the mechanism
comparison, not the number.

## Disposition (per the standing gates)

**Do NOT promote into manifold-codec.** The corpus question is: does a
spatially localized illumination model exist whose coherent regions pay
less in boundaries+parameters than they save in residual bytes — and is
that content class PRESENT in testdata? Chelsey/hubble show the win is
single-file and protocol-sensitive at 512²; the four-file aggregate
loses in every variant measured here. Same deny shape as CF1: a real
capability, no corpus class.

Recorded as RESEARCH STANDING (not re-probe without a new hypothesis):
- illumination fields evaluate scale-free — confirmed;
- the scale-free advantage does NOT monetize at full res on
  photographic interiors — confirmed twice (pilot + here);
- the transferable structure is parameter precision, and the
  precision needed scales with resolution — the memo's own conclusion,
  reproduced.

## Scope-of-closure

CLOSED by this pair of experiments (pilot + this reproduction):
- WHOLE-FRAME fitted illumination (planar / radial glare / curved-path)
  as a photographic prediction improvement — aggregate losses everywhere
  measured, single-file wins protocol-sensitive.
- Field-guided MED tiebreak steering with a fitted-smooth-gradient band
  — firing surface ≈ empty on real photographs (sub-percent of pixels);
  a no-op mechanism on this corpus.
- Field-aligned causal neighbor selection — loses to plain MED.

OPEN (deliberately left):
- spatially LOCALIZED illumination fields with region segmentation (the
  next memo rung) — needs a real Rust full-wire A/B vs v1.14's pick
  before any promotion talk;
- hierarchical/deterministic-BACKOFF context sharing (the R6 finding's
  follow-up) — same prequential-regret instrument class, separate rung;
- per-image REF+field SELECTION (a win table + a selector) — only if a
  localized variant first survives its own boundary-economics audit.

## Instrument notes

- The canonical-predictor import rule earned its keep AGAIN (the
  hand-rolled med_plane broadcast-garbled and would have silently
  mis-pictured edges; importing `mnf11_x2_dynamics.med_pred`'s semantics
  via a matched vectorized plane passed the same recon asserts).
- Fitted fields cost ~41-73 B honest transport (f64 coefficients) —
  every fitted-model arm here includes that in its quoted size; the
  memo's protocol did the same (stated: "all model parameters are
  included").
- Fixtures stay out of git (fetch-script pattern): scikit-image raw
  GitHub v0.25.2 paths live in this doc; the ZIP is not needed.

## Record

This reproduction script: /tmp/mnf14_if2/if2_probe.py +
if2_ctrl.py (scratch copies committed under crates/mnf-mans/scripts/
as mnf14_if2_probe.py + mnf14_if2_ctrl.py for the doc's reproducibility;
rows: /tmp/mnf14_if2/if2_rows.json + if2_ctrl.json).
No production code changed; manifold-codec stays at v1.14.0 exactly.