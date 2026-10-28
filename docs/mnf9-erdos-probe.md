# Erdős-offset candidate probe — measured verdict (2026-10-28)

The Erdős-grid proposal, sharpened to what the wire can see: the token
wire already encodes ANY 2D offset (dx:u16 dy:u16), so structured
candidate geometry cannot change the per-reference wire cost — its only
possible win is finding matches the shipping matcher misses within the
same candidate budget, or making addresses cheaper to re-encode. Arms
(equal search budget = 16 candidates, same wire grammar, same zlib-9
cost, exactness structural): A = shipping content-keyed chains; B =
candidates restricted to Erdős offsets (a²+b² = m, densest-m table, 1568
signed offsets, content-key verified); A+B-union (shared budget); D =
uncapped A (upper bound). Corpus subset: the classes every prior probe
split on + one photo smoke. Harness committed:
`crates/mnf-mans/scripts/erdos_probe.py` (+ scratch v2 subset runner).

## Results (wire, zlib-9)

| file | A ship | B erdos | A+B union | matches A/B/union |
|---|---|---|---|---|
| gradient | 446 | 2,614 | 533 | 1366/1212/1405 |
| solid | 41 | 183 | — | 771/768 |
| screenshot | 795 | 986 | 820 | 3144/3151/3133 |
| repeat 256² | 792 | 4,515 | — | 1554/3368 |
| noise 256² | 224,845 | 224,845 (tie, 0 matches both) | | |
| blocks | 1,672 | 2,256 | 1,674 | 639/469/624 |
| document | 1,304 | 2,148 | 1,382 | 955/985/968 |
| plaid | 41,878 | 41,879 (tie) | | 1/0 |
| poster | 3,562 | 6,504 | — | 1147/473 |
| **blob** | **12,742** | **11,160** | 12,854 | 1938/445 |
| stone (photo) | 18,917 | 18,917 (tie, 0 matches) | | |
| **TOTAL (11)** | 306,994 | 316,007 (+9013) | | |

## Findings

1. **B loses the aggregate (+9013) and every class except blob.** On
   synthetics the restricted geometry is strictly worse than
   content-keyed search at equal budget (poster ×1.83, repeat ×5.7 —
   geometric priors waste probes where the useful repeats aren't at
   algebraically-structured distances). Ties on the matchless classes
   (noise, photo) are the correct invariant behavior.
2. **The blob result is real and interesting but dies at the format
   question.** B beats A by −1582 B (−12.4%) on blob — cross-checked
   under zstd (9718 vs 11232, −1514; not a zlib-luck artifact). The
   mechanism: B fires only 445 matches (vs 1938) with 29% at far (>64
   row) offsets the row-keyed chain cannot see, trading match count for
   geometrically-spread longer reaches. BUT the supplement arm (A+B
   union, shared budget) measures WORSE than A alone on blob (12,854 vs
   12,742): Erdős probes displace content-keyed probes that would have
   fired, and the two candidate families overlap too little to share a
   budget profitably. A per-FILE switch A↔B would win blob only — and
   the honest per-file-pick arithmetic then charges the switch to the
   wire (+1 bit + the ordering must be decoder-reproducible: the ENTIRE
   geometric candidate ordering becomes encoder-side search policy, fine,
   but B's win survives only on one file of thirty).
3. **Address re-encoding: the proposal's (m, dir) compact address is NOT
   wire-viable.** The dense-m table tops out at 8 representations per m
   in range 65..40000 (3 bits/dir max) but requires the m-ID first: even
   a 12-bit m-ID + 3 bits dir = 15 bits only beats 32-bit (dx,dy) at
   distances where BOTH coordinates fit — and the m-ID must then be
   transmitted per stream or referenced. Against the actual histogram
   (blob B: dx,dy both < 224 for a 224-wide image — dy fits 8 bits for
   ALL matches, meaning (dy:u8, dx:u8) = 16 bits beats any Erdős address
   universally without any table). The structured-offset idea loses to
   plain variable-width coordinate coding — a finding for the coordinate
   coder, not for Erdős geometry.

## Verdict

- **Erdős-offset candidate generation: DENIED as a replacement or
  supplement** (A+B-union worse than A everywhere; B-only worse on 10/11
  files). Equal-budget content-keyed chains dominate: the useful
  candidate is defined by CONTENT similarity, and the hash chain already
  enumerates exactly that set — geometry only dilutes the budget.
- **The blob-class far-offset signal is recorded, not retained:** one
  file in thirty, −12.4%, real under a second coder, unreachable by
  union-with-A at the same budget. If a far-offset variant is ever
  revisited, the contract is a per-file A↔B pick with B's ordering
  specified decoder-side — pending a corpus class where far repeats are
  common (none in the current 30).
- **(m, dir) address coding: DENIED** — plain variable-width (dx,dy)
  coding beats it universally (dy fits u8 on every measured match);
  the Erdős address's table transport exceeds its savings.
- The proposal's own scientific caveat is confirmed by measurement:
  equal-distance structure ≠ compression gain; the gain must come from
  data redundancy, and where that redundancy exists the content-keyed
  chain already exploits it at lower search cost.

## Scope of this closure (explicit)

This record closes THREE hypotheses and NO more: (1) Erdős offsets as
reference-search candidates, (2) Erdős+hash union at a shared budget,
(3) structured (m, dir) address encoding. It does NOT close alternative
grid TOPOLOGY as a compression representation: a differently-shaped
prediction lattice changes the causal-neighborhood structure (who MED's
neighbors are), which changes the residual plane itself — a
prediction-context question, not a candidate-search question. That
hypothesis is Open and is measured separately (mnf9-lattice-probe.md),
per §11's rule that ordering/traversal claims refine into
prediction-context claims.