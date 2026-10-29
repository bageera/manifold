# ADR-007 — MNF10 exact generative records (2026-10-28)

Status: PROMOTED (Track A — shipped decoder src/mnf9.rs + CLI pick arm;
prototype remains in the research crate; promotion record:
docs/mnf10-track-a-promotion.md).

## Context

Jon's MNF10 directive: the pick already selects arms by minimum
description; generative-function discovery adds a model class the pick
could never see before — functions that GENERATE the quantized pixels
exactly (integer div-form polynomials, finite-difference recurrences,
bilinear corner forms, per-segment piecewise ramps). The discovery probe
(docs/mnf10-genfunc-discovery-probe.md; committed script reproduces its
table) measured exact-fit wire 39–81 B against the v1.9.0 pick
(58–5,230 B) on 14/16 adversarial files while never losing a file the
pick should keep (no-rule files stay with the predictor arms; held-out
reproduces training classes). The measured quantization-floor boundary
(mnf9-windowed-geometry) is the same fact from the losing side: only
EXACT decoder-reproducible integer rules qualify.

## Decision

MNF10 = a new record grammar, not an MNF8 flag bit:

    File  := 32-B header (identical field layout to MNF8; magic "MNF9" —
             the ADR-003 reservation; "MNF10" is the research-track name)
             payload
    Body  := u8 model_id + model parameters (LE, canonical bounded forms)
             — whole-frame parametric records; no residual payload
             (residual-carrying generative forms are NOT in this grammar;
             the predictor record families already cover that case via
             MNF8)
    model_id 1 = constant               params: r,g,b (u8 each)
    model_id 2 = div-1D (per channel)   params per channel: N (s32 LE),
                                        T (s32 LE); D = shared u8 index
                                        into the convention table
                                        {1,2,4,w-1,w,2(w-1),256,65536}
    model_id 3 = rec-1                  params per channel: a0 (u8), d
                                        (s16 LE); orientation bit u8
                                        (row|col)
    model_id 4 = bilinear joint         corners (u8 x 4 x 3 channels);
                                        denominator ((w-1)*(h-1)) exact
    model_id 5 = bilinear nested round  corners + the nested rounding
                                        sequence (normative below)
    model_id 6 = piecewise div-ramps    nseg (u8), boundaries (u16 LE
                                        each), per segment per channel
                                        N (s32), T (s32), D (u8 table idx)
    0 and 7..=255 reserved — reject on decode (error, not panic)

Normative arithmetic (the decoder IS the equation):
- div form: floor division of signed 64-bit intermediates; N*T bounded
  by |num| <= (some bound) w*h*... (decode-time check: any computed
  value outside 0..=255 = file invalid — exactness is enforceable).
- rec-1: value_k = (a0 + d*k) mod 256 along the scan direction; scan
  lines must be IDENTICAL (decode re-checks cheaply via reconstruction).
- bilinear joint: (v00*(w-1-x)*(h-1-y) + v10*x*(h-1-y) + v01*(w-1-x)*y
  + v11*x*y) / ((w-1)*(h-1)) floored; w=h=1 rejected (denominator 0).
- bilinear nested: e00->e10 round-lerp per column across the top/bottom
  corner rows, then round-lerp top/bottom per row (half-up); the
  rounding ORDER and offset ((w-1)/2, (h-1)/2) are the convention.
- piecewise: boundaries at transmitted x positions; each segment per
  channel (N*x + T)//D with segment-local x (x index restarts at each
  boundary).

Validation = decode + compare: a record is valid only if evaluation
reconstructs SOME stream; exactness to the SOURCE is the ENCODER's
discipline (emit only on zero residual). The decoder validates:
model_id known, params in range, arithmetic intermediates bounded, and
the reconstruction is deterministic. No panics on any input.

## Pick integration (when shipped)

The MNF8 pick loop gains: for each candidate, attempt generative fits
(cost model: the exact-fit body above); a successful fit's full file
(32 + params) competes like any other arm. Files where no rule fits
keep their current arms — the never-worse invariant holds by
construction. MNF8/MNF5-7 untouched; MNF10 files are their own format.

## Alternatives rejected

- MNF8 flag bit (per-file mode): mixes two formats' validation surfaces
  and spends a global bit on classes that need no flag today.
- Residual-carrying generative records: the probe measured residual-fit
  dominating ONLY where exact fits exist (where the predictor arms
  already win otherwise) — no measured class needs it; excluded by YAGNI.

## Conformance plan (on implementation)

- Golden vectors: one per model_id.
- Rejections: reserved ids, oob params, denominator-0 dims, truncations.
- Exactness: corpus files (the probe's fixtures) decode-exact.
- Pick test: a floor-lerp gradient fixture must pick the generative arm
  in the library API and the CLI; a photo must NOT.

## Measured basis summary (from the probe, v1.9.0 pick comparator)

| family | wire form | beats pick on |
|---|---|---|
| constant | 36 B | solid |
| div-1D/2D | 42–50 B | floor/round/half-even ramps |
| rec-1 | 39–40 B | recurrence, pillow vertical ramp |
| bilinear joint/nested | 52 B | editor corner blends (−64..−99×) |
| piecewise ramps | 81 B | 2-segment ramps (pick 137) |
| (no fit) | — | noisy/resize-kernel stay pick (correct) 
