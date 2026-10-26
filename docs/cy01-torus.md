# MNF-CY01 — Toroidal Atlas: Independent Reproduction + Proposed Next Experiment (2026-10-25)

Status: research; frozen formats untouched. Script:
`scripts/cy01_torus.py` (self-contained, deterministic; zlib-9 parity
throughout; per-tile exactness asserted in the torus arm).

## The math (per the CY01 specification)

- E = C/(Z+iZ): elliptic curve; a Calabi-Yau one-fold under the broad
  definition. E[8] = (Z/8)^2 — 64 periodic phase states = 6 bits/tile.
- Predictor: Xhat_{a,b}(x,y,c) = T[(x+a) mod 8, (y+b) mod 8, c], T the
  shared 8x8 texture (192 B).
- Exactness: R = (X - Xhat) mod 256; X = (Xhat + R) mod 256 — the Z/256
  ring identity the whole atlas family rests on; holds for EVERY input,
  good predictor or bad. Asserted per tile in the run.

## Reproduction (128x128 RGB8, zlib-9, all metadata counted)

| image | raw+z | med+z | blk-exact | blk-OPT(nonzero) | torus-d | torus-o |
|---|---|---|---|---|---|---|
| periodic | 765 | 1,320 | 1,318 | 1,318 | **292** | 292 |
| periodic+2% noise | 2,713 | 6,403 | 6,404 | 6,404 | **2,161** | 2,161 |
| gradient | 45,132 | 215 | 212 | 212 | 49,317 (LOSS) | — |
| random | 49,173 | 49,242 | 49,242 | 49,242 | 49,422 (+0.5%) | — |

Reproduction vs the CY01 report within construction differences
(texture entropy, whether the controls included the byte-copy arm):
- periodic: **-61.8%** vs best conventional (report: -64.96%) ✓ SMALLER
- periodic+2%: **-20.3%** (report: -16.17%) ✓ SMALLER — honest discovery
  (consensus median across the 64 phase-aligned crops) matches the
  oracle texture exactly here (torus-d == torus-o)
- gradient: loss (report +229%; here +23,163% — my controls are stronger:
  the %256 ramps make exact byte copies available to the block arms; the
  torus model cannot encode non-periodic structure — honest loss kept)
- random: +0.5% (report +0.2%) ✓ par

## The proposed next experiment — ANSWERED

CY01 asked: does the toroidal model still win against FULLY OPTIMIZED
unaligned block matching WITH NONZERO RESIDUALS (not exact refs only)?
The `blk-opt` arm above is exactly that (stride prefilter -> stride-1
refine -> residual zlib-scored, metadata counted). Verdict: **the
toroidal model still wins on periodic content** (-61.8% clean, -20.3%
corrupted) — the phase representation beats even a strong residual-capable
block matcher because blocks must pay a position per tile (u32), while
phases pay 6 bits, and shared-model discovery amortizes perfectly here.

What this experiment does NOT establish (honest boundaries):
- No Calabi-Yau geometry beyond the translation group: the implemented
  phase operations are cyclic translations of an 8x8 torus; E[8]'s group
  structure enters only as the (Z/8)^2 phase index. Any gain must come
  from (shared model + tiny phase coords), which is the measured claim.
- Natural-image relevance untested: the corpus synthetic classes show
  the torus loses (gradient) or ties at overhead (random). A real-image
  gate (photo tiles with near-periodic structure) is the follow-up that
  would decide whether a periodic-atlas mode is worth a format item
  (texture id + 6-bit phase + mod-256 residual) in MNF8+.

## Cost-accounting notes (for any future format proposal)

The 292-byte periodic file decomposes as: texture 192 B (zlib-compresses
little — random texel bytes) + phases 48 B (64 x 6 bits, LSB-packed) +
residuals ~70 B (near-zero after phase alignment) + container 4 B.
Discovery transport: the decoder needs EXACTLY the texture the encoder
used (192 B carries it; consensus median discovery is encoder-side only).
For images with MULTIPLE periodic families, per-family textures +
per-tile family ids would be the extension — measure before proposing. 