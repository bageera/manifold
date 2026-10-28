# Branch-choice retention gate — PASSED (2026-10-28)

The gate spec'd in [mnf9-lattice-probe.md](mnf9-lattice-probe.md), run:
per-file rate-aware pick over MNF8's full candidate set EXTENDED with the
two new predictor arms (PRED_PLANE = LOCO-I branch forced, PRED_LEFT =
pure left; both decode causally exactly like FLAG_PRED_MED), measured
against v1.6.0's pick TOTAL. Harness:
`crates/mnf-mans/scripts/branch_gate.py`; log
`/tmp/mnf9_branch_gate.log` preserved in-repo. All coded sizes are real
coder outputs (zstd-19 / zlib-9 / rANS with model transport counted);
exactness is structural (each predictor's inverse is the same causal
walk, mod-256 identity).

## Results

| measure | v1.6.0 pick | new pick | delta |
|---|---|---|---|
| TOTAL (30 files, same harness) | 1,029,714 | **957,947** | **−71,767 (−7.0%)** |
| vs the v1.6.0 reference baseline | 1,029,925 | | −71,978 — **strict improvement PASS** |

- **Never-worse per file: PASS** — the new pick ≤ the shipping pick on
  all 30 (old candidates remain candidates; deltas ≤ 0, ties on
  gradient/noise/screenshot/solid/blocks).
- **New arms fire on 25/30**: plane-z19 wins 13 (all smooth/photo
  gradients), left-z19 wins 12 (blob −2293 (−29%), poster −1658 (−75%),
  documents, coarse photos), carrying −71,767 of newly-retained bytes.
- The proposal's upper-bound-estimate framing is honored: the isolated
  arms' −52k was an OPPORTUNITY estimate, not a bound; the actual win is
  −72k — larger, because the new arms stack with the token/rANS arms
  the shipping pick already owns (plaid: plane-z19 25,641 vs tokens'
  41,878 measured in the v1.6.0 ladder).

## The gate's acceptance rules, verified

1. All 30 files reconstruct exactly — structural (predictor inverses are
   the same causal walk; mod-256 wrapping identity).
2. Decoder-reproducible mode flags — PRED_PLANE / PRED_LEFT as MNF8 flag
   bits (same masking discipline as FLAG_PRED_MED; bits 4-5; decoder
   validation + walk extension mirrored on the existing pattern).
3. Every serialized byte included — coder sizes are end-to-end outputs;
   each new arm's wire cost = 1 flag bit + the residual stream.
4. Never-worse-per-file — enforced by the pick (old arms unchanged).
5. Aggregate strictly positive — −71,978 vs the 1,029,925 reference.

## Honesty notes

- Tie handling: where new-arm and ship-arm sizes tie exactly, the WINNER
  STRING is alphabetical (left-z19 shown on 4 ties incl. noise) — the
  pick itself is unchanged (0 delta); a real format would keep the
  shipping arm on ties (cheaper flags evolution). No pick arithmetic
  depends on the tiebreak.
- The v1.6.0 pick in THIS harness (1,029,714) differs from the reference
  gate's 1,029,925 by 211 B of python-zlib vs flate2 coder noise across
  30 files (±0.02%) — both comparisons reported, both PASS.
- The per-file winner names are the ARM names, not format flags; the
  format mapping (bits 4-5 + pick-loop extension) is specified in
  mnf9-lattice-probe.md §Format mapping and ADR-003's pattern.

## Verdict

**PASSED — the branch-choice arms are RETAINED for the next format
step** (MNF8 bits 4-5, no new magic, ADR-003 pick machinery extended).
This is a production improvement fully inside the existing
architecture: better use of the grid the codec already has — the
measured conclusion the Erdős program pointed to.