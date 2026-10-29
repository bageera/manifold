# MNF13-R0 — conditional residual structure on the v1.12 streams (2026-10-29)

Diagnostics per the pasted matrix, all measured on the residual bodies
the RELEASED v1.12 pick actually coded (CLI `--mnf8=maximum` per file;
MNF9-magic files skipped; token wires excluded — the token engine's
file-level arms decide those files and their literal planes are
degenerate for the matrix; record files only). Instrument:
`crates/mnf-mans/scripts/mnf13_r0_diag.py` (committed). Per REF mode
r ∈ {0,1,2,3}: the shipped file's bodies are unrelabelled to the plain
residual plane (walk-verified: per-tile causality assert green
in-repo) then relabelled to the r-wire in coded order; contexts come
from TRUE pixels (the encode-side rule).

## Verdicts (matrix rows = the shipped ref's numbers)

| Diagnostic | Photos (n=10) | Heldout (n=9) | Verdict |
|---|---|---|---|
| H0 (CD-era residual entropy) | 3.169 | 3.049 | the shipping alphabet is already tight |
| H(R\|activity), 4-class q = floor(log2(1+\|L\|+\|T\|+\|TL\|)) | 3.168 | 3.048 | **−0.001 bits: CLOSE** |
| H(R\|activity,direction) | 3.166 | 3.047 | **−0.002 bits: CLOSE** |
| H(R\|prev coded residual) | 2.840 | 2.785 | **−0.33 bits, reproducible both splits** — the one real local signal |
| I(d1;d2\|c0) post-CD | 0.414 mean (sand 0.99, slate192 0.79) | 0.321 | reproducible — but the wire ALREADY answers it: files pick PLANAR over interleaved at pick level, so cross-channel ordering through the shipping backend does NOT convert this value |
| H(R\|causal offsets {left,up,up2,up4,upleft,upright,up8,up16,up32}) | mean best +0.232 | +0.225 | gains FLAT across all distances incl +32 rows — a generic order-1 effect, NO distance-specific dependency: **SI1-B's zero-index gate = NEGATIVE** |

Per-file detail (the log): the offset profile's near-identity across
left/up/up2/up4 and the slow decay to up32 (+0.2) = the signature of
broad residual memory, nothing spatially specific. The
activity/direction collapse matches the left-neighbor +0.3-bit
diagnosis surviving CD unchanged.

## What does NOT get built (measured)

- Activity/direction contexts: the conditional reduction is ~0.1% of
  H0 — the paste's own decision rule closes both branches without a
  codec change.
- SI1-B fixed-offset atlas arm: the flat offset profile = no
  distance-selectable dependency; a transmitted index has strictly
  less support than the zero-index control it would have to beat.
- DNA1 contact-map arms (fold / reverse-complement): the same gate —
  without a distance-specific dependency to expose, remapping bytes
  cannot add information the order-1 context does not carry; and both
  Morton/Hilbert controls already measured neutral-to-worse in
  earlier probes.
- A new context-conditioned entropy backend: H\|prev's −0.33 bits is
  an ENTROPY number, not a wire number; the shipping backends (deflate
  level-6-always, zstd-19) exploit order-1 structure only indirectly,
  but the rANS arm's honest-transport lesson stands — a specialized
  context coder must beat the container on FULL decodable transport.
  Recorded as the standing gate for any future coder arm.

## Standing numbers (for the next proposal)

- The v1.12 corpus = 832,739 (CLI) / 832,765 (lib gate); REF dist
  none 7 / G 13 / R 5 / B 5.
- Residual H0 by shipped ref: 3.169/3.049 (photos/heldout means) —
  CD moved the alphabet from ≈4.2-5.5 bits (pre-CD diagnostic) to
  ≈3.0-4.0 on the wire.
- The unconverted values measured here: order-1 −0.33 bits,
  I(d1;d2\|c0) 0.32-0.41 bits — both real, both
  representation-or-coder-bound, both behind the same honest-transport
  gate.
