# G6 — Quaternary (Nibble-Split) Representation Probe — DENIED

**Status**: DENIED — the quaternary/nibble-split transformation loses the
corpus aggregate decisively against the standing in-plane control.

## Hypothesis

From the ACOUSTICS byte-representation discussion: mapping 8-bit pixels into
a 4-symbol (quaternary, DNA-alphabet-shaped) representation as two separated
nibble planes might expose a smaller total description. The theory's
restricted-alphabet arithmetic (log₂ of the distinct-value count) is correct
but concerns FIXED-WIDTH per-value cost; the code-shape question in a
corpus-structured coder is whether the SPLIT creates exploitable structure.

## Arms (same coder: zlib-9 everywhere; 25-file corpus = photos 10 +
heldout 5 + heldout-photos 10)

| Arm | Description | TOTAL (B) |
| --- | ----------- | --------- |
| raw | zlib over the gray raster | 379,466 |
| plain_med | MED/NIPS spatial predictor on the ORIGINAL raster — the shipped pick's strong arm | **311,244** |
| split_raw | nibble-split, planes packed raw | 396,311 |
| split_med | nibble-split + MED on each plane, consecutive | 434,710 |
| split_med_int | nibble-split + MED, row-interleaved | 448,186 |

## Verdict

- **Aggregate**: the best split variant (396,311) loses to plain_med
  (311,244) by **+27.3%**. The split is not competitive.
- **Per-file**: 2/25 files beat plain_med through the split —
  blob_224x160 (4,229 vs 4,684) and poster_208x144 (1,107 vs 1,755). Both
  are the mostly-CONSTANT-plane classes (poster flat regions; blob sparse):
  when one nibble plane collapses to a single value, the split's second
  plane is nearly free. Everywhere else the low-nibble plane carries the
  detail and is near-random ⇒ the pair pays ~2× residual cost and destroys
  the correlation MED exploits.

## Mechanism attribution

This is the fold-probe law from the third axis again: **the redundancy is
the mechanism, the re-layout is incidental** — and here the re-layout is
worse than incidental, it actively degrades the strongest predictor's input.
The restricted-alphabet theory's own boundary clause says it: "If all 256
values remain possible and equally likely, no lossless fixed-width
representation below eight bits per value exists" — the low nibble of
natural images is close to exactly that case.

## Honest scope

- The corpus is the repo's synthetic/probabilistic photo corpus, not real
  recordings; a palette-art corpus could reverse the verdict (poster class
  shows the split CAN win where planes are constant). That class is already
  served by the shipped pick's own arms — no new capability.
- No pick-arm promotion is proposed. The probe closes the quaternary
  hypothesis in this repo without opening follow-up work.

Reproduce: `scripts/g6_quaternary_probe.py` (venv: mnfpca), committed log:
`docs/g6-quaternary-gate.log`.