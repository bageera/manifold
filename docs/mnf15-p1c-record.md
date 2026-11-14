# MNF15-P1-C — exact construction pruning: the REF terminal pass hoisted out of the sweep

**Status**: IMPLEMENTED + all gates green (retained; the research
instrument IS the production code — the pruning is an identity proof, not
a probability). The per-tile sweep is REFERENCE-BLIND: the ADR-008 REF
field (v1.12's reference-channel variants 0..3) is a terminal
`channel_diff_relabel` pass, and both builders now return the ref-0
stream with the CALLER applying the relabel per variant (0 = untouched).
Record sweep constructions per Maximum encode drop **64 → 16 (−75%)**;
token wires 8 → 2; corpus outputs byte-identical (30/30 + hubble); the
wire grammar, decoder, backend ids and candidate ranks are untouched.

## Why this answers P1-C's requirement

P1-C asked for an inexpensive, provable lower bound L̄(a) usable BEFORE
construction (or safe termination DURING it) — a bound cheap enough to be
worth its own cost. The source-level audit found something stronger than
a bound: for the REF dimension the "bound" is EXACT and costs ~zero —
`build_record_stream`'s sweep never reads `reference` (it applies
`channel_diff_relabel(&raw, planar, false, …, reference)` once at the
end), and the token matcher is likewise reference-blind. A ref-variant's
"construction" is therefore a terminal relabel, not a re-derivation of the
transformation. The L̄ > L_best discard machinery was never needed: the
candidates are not skipped — they are computed at exact-equality cost
(one O(len) pass), which beats *any* valid bound whose bound-computation
cost exceeds the relabel's. The remaining non-shared sweeps are the 2
proxy classes × 4 predictor kinds × 2 layouts — genuinely different
searches (their bodies differ), not redundant work.

## Measured

| Gate | Result |
|------|--------|
| C1 candidate identity | committed manifest reproduced byte-exactly (200 candidates × 3 files + pick digests) |
| C2 corpus | 30/30 Maximum digests identical, **total 810,816 = the anchor**; 30/30 balanced identical |
| C3 wall | Maximum **481.1 s** vs 1,127.9 s baseline = **−57.3% cumulative** (P1-C alone 710.2 → 481.1 = −32.3%); hubble 585.9 → 376.9 s (−35.7%) |
| C4 RSS | corpus peak **96.2 MB** — 4.4 MB BELOW the P1-B vintage's 100.6 MB (the hoist removed the extra in-flight stream), +0.1 MB vs the historical 96.1 MB anchor |
| C5 balanced (matched, idle) | 164.17 → **56.38 s** (−65.7%); peak 98.1 → 95.9 MB; 30/30 digests identical |
| C6 fuzz | 525 trials, 0 panics |
| C7 split | 18 sweeps + 54 relabels = 9.33 s of a 29.94 s pick = **31% construction share** (was 64%) |

Cumulative P1 line: **1,127.9 → 710.2 (P1-B) → 481.1 s (P1-C)** = −57.3%,
2.34×, with byte-exact output everywhere. The original 30% target
(801.7 s) was passed at P1-B; P1-C extends the margin by another 320.6 s.
Balanced cumulative: 165.92 → 56.38 s (−66.0%, 2.94×).

## Where the remaining cost is (P1-D's surface, honestly bounded)

The B5 split now puts construction at ~31% of Maximum search
(9.33/29.94 on the representative fixture), and the corpus-level −57.3%
is consistent with that share plus the balanced result. What remains:

- the 16 record sweeps themselves (proxy-driven mode choice — genuinely
  content-dependent, not reusable);
- entropy coding (~69%), whose biggest measured piece inside
  `proxy_cost` is the per-body compression PROBE the sweep itself runs
  (source, src/mnf6.rs: the zlib class probes an actual zlib compression
  of the body; the zstd class probes zstd at level 3, not the shipped
  19) — P1-D's exact territory: make the probe cheaper or eliminate it
  without changing any chosen body (the mode choice depends on probe
  ORDERING, not its absolute scale — a strictly monotone cheaper probe
  is an identity if rankings are provably preserved; that is exactly the
  discipline ADR-010 demands and it must be proven, not assumed).

P1-D remains CLOSED pending a work order; this record's scope is the
P1-C landing.

## Gates

docs/mnf15-p1c-gate.log (script-backed, digit-checked): C1 identity
manifest (269.1 s), C2 corpus B4/B6 via mnf15_p1b_verify.py, C3/C4
verify rows, C5 bal sweep CSVs (.research/p1b/bal_p1c.csv), C6 fuzz,
C7 hubble round trip. The B5 instrument (p1b_build_code_split) is
retained and updated to count the new construction shape (its history:
pre-refactor 64+8 → P1-B 72 → P1-C 18 sweeps + 54 relabels).
