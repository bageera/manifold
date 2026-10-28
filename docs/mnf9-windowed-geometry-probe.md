# MNF9 windowed-geometry probe (2026-10-28)

## Hypothesis (unbounded shape / bounded observation)

A continuous geometric model exists beyond the image boundary (Jon's
formulation): the model's periods need not close within the tile —
ω_x, ω_y are free parameters and the image observes only a window of
the shape. This corrects the CY01 torus-tied family (periods tied to
tile dimensions, DENIED earlier). The question: does a freed-frequency
continuous model, quantized to decoder-representable parameters, beat
causal prediction?

## Arms

- med+z9 — the control predictor stream (zlib-9 container)
- left — pure left predictor control
- windowed — A·sin(ωx x + φx) + C per channel, free frequency/phase/
  amplitude/offset, parameters q-quantized for the wire, residual
  stream coded
- torus-tied — the earlier phase-locked form, same quantization

Script: crates/mnf-mans/scripts/windowed_probe.py; committed gate log:
docs/mnf9-windowed-geometry-gate.log (the numbers below are quoted from
it verbatim).

## Gate log (bytes)

```
== SYNTH (constructed classes, exactness structural) ==
sin-0.25cycle-x                    med+z9=     229 left=     801 windowed=     991 torus-tied=     975
sin-0.5cycle-x                     med+z9=     263 left=     835 windowed=    1124 torus-tied=    1025
sin-1.7cycle-y                     med+z9=     476 left=     962 windowed=     623 torus-tied=     542
sin-2d                             med+z9=    4064 left=    4139 windowed=   51143 torus-tied=   44925
== REAL CORPUS (no-false-win invariant) ==
gradient_320x240.png               med+z9=    1696 left=    1635 windowed=  148032 torus-tied=  209576
screenshot_640x400.png             med+z9=    1354 left=    2352 windowed=    5044 torus-tied=   35698
stone_160x120_s900006.png          med+z9=    17078 left=    17552 windowed=   28309 torus-tied=   27799
slate_192x144_s900004.png          med+z9=    49343 left=    46153 windowed=   58546 torus-tied=   54615
```

## Verdict: DENIED — the model floor, not fit quality

1. Freeing the frequency fixed model QUALITY (the shapes fit well) and
   changed nothing about the verdict: quantized continuous parameters
   (frequency grid error + q-quantization + parameter transport) cost
   more than the residual-coder floor of causal prediction on every
   smooth class. Even on the constructed sinusoids the windowed arm
   wins only where the shape is exactly 1-D along y (sin-1.7-y) — and
   the win there is an artifact of the shape's degeneracy, not of the
   freed frequencies: torus-tied ties it.
2. On 2-D structure (sin-2d) the continuous family is 12× WORSE than
   MED+z9 (51,143 vs 4,064): parameter quantization error explodes per
   sample in two dimensions.
3. Real-corpus no-false-win invariant held: every real file loses
   (gradient +87×, screenshot +3.7×, photos +65–19%).

## The boundary condition that survives

A model family wins only when its decoder-representable parameter
lattice reproduces samples with ZERO model error (exact discrete
structure — integer phases, integer div-forms). Quantized-continuous
forms pay a floor (parameter quantization + fit error + transport)
that smooth-class causal prediction already beats; measure the floor
on one constructed home-turf class BEFORE sweeping a family.

(2026-10-28 later record: the MNF10 generative-function probe
docs/mnf10-genfunc-discovery-probe.md measured exactly this boundary
from the winning side — the EXACT integer families win their classes
at 40–52 B against the full shipping pick, confirming the rule.)

## Scope

This probe closes the freed-frequency continuous-sinusoid family and
its torus-tied variant. It does not close exact-discrete generative
families (that is the MNF10 track), piecewise/exact forms, or the
CY01 phase-lattice mechanism (proven separately on periodic texture,
docs/cy01-torus.md).