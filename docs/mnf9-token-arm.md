# MNF9 token arm — Rust port of the retained ladder result (2026-10-28)

The [engine ladder](mnf9-engine-ladder-probe.md) retained exactly one
engine candidate: **arm B, LZ77-2D tokens over the MED residual plane,
as a rate-aware per-file pick** (never worse than the picked literal
arm). This record covers the Rust implementation of that arm in
`crates/mnf-mans` (`lz77_2d.rs`) — the prototype stage between probe
verdict and any future format flag. No format change: MNF5–8 untouched.

## What was built

- `crates/mnf-mans/src/lz77_2d.rs` — the token engine with a **real
  decoder**, which the python probe lacked (it asserted exactness
  in-sim only):
  - wire: tag 0 literal = 1+3 B; tag 1 match = 1+2+2+1 B
    (dx, dy u16 le + len px, len ≥ 7); byte-forward overlapping copies;
    matcher = the probe's (3-px-key hash chains, newest-first,
    16-candidate cap, 64k-px window, ≤255-byte run).
  - `decode_plane` is the exact inverse and *errors* (never panics) on
    unknown tags, truncation, below-minimum matches, out-of-range
    sources, and plane-size mismatch.
  - `pick_plane` = the retained rate-aware rule: cost both arms through
    the same coder, take the min (returns the chosen cost + a won flag).
  - The probe's tags 2/3/4 (transformed chart matches / shared
    dictionary) are deliberately absent — measured dead in the ladder.
- `crates/mnf-mans/examples/bridge9.rs` + `scripts/bridge9_gate.py` —
  the probe→crate bridge (stdin w/h/plane → tok/lit sizes; exactness
  asserted per call in Rust) and the 30-file corpus gate.
- 7 crate tests: roundtrips, planted-repeat firing, the pick rule on
  both regimes, overlapping-run semantics, malformed-wire rejection
  (`catch`-free — errors are values, not panics).

## Corpus gate (30 files, 2026-10-28)

Cost coder flate2 zlib-9 on BOTH arms (in-crate parity; python zlib-9
column logged beside it, same-shape numbers — miniz/zlib deltas ±1.5%
per file as known). Full log preserved at `scripts` run time; summary:

| class | pick | notes |
|---|---|---|
| gradient 320×240 | TOK −77% (1966→446) | matches the ladder's B = 446 exactly |
| solid 256² | TOK −81% (218→41) | ladder B = 41 exactly |
| screenshot | TOK −42% (1364→795) | ladder B = 795 exactly |
| repeat 256² | TOK −49% (1628→793) | ladder B = 792 (1 B coder delta) |
| blocks (held) | TOK −15% (1962→1669) | ladder B = 1672 (3 B coder delta) |
| photos ×20, blob, plaid, poster, document, noise | lit | tokens lose +2.7k…+28k; zero matches on 20 photo planes (matches=0) — MED residuals are noise-dominated there |

Rate-aware pick TOTAL: **1,074,514 vs literal 1,077,908 (0.997×)**
over the 30-file corpus — never worse by construction, winning
everywhere the residual plane repeats. The win-classes are exactly the
ladder's: gradient/solid/screenshot/repeat/blocks.

## Honesty notes

- The 5 TOK wins reproduce the ladder arm-B numbers byte-exactly
  (gradient 446 = 446, solid 41 = 41, screenshot 795 = 795; repeat/blocks
  within 1–3 B = flate2-vs-zlib coder delta, same-coder-parity rule kept:
  each gate row's two arms run through ONE library).
- At 32×32 scale the token arm ties (29 vs 29 B on a run plane): the
  measured win is a large-stream effect; the crate test pins the pick
  contract (pick == min of both arms, matches fired), not a synthetic
  win number.
- Zero matches on all 20 photo residual planes is itself the honest
  evidence for the rate-aware pick: the matcher finds nothing real there,
  so the +1 flag bit is the only cost of the retained form on photos.
- The `tokens_win_…` test earlier in development asserted a small-scale
  token WIN that the engine does not produce at 32×32 (tok 29 = lit 29).
  Caught by running the test, not by reasoning; the assertion was
  corrected to pin the actually-retained contract (pick = min, matches
  fire). No number in this doc cites the wrong-scale claim.

## Format outlook (unchanged verdict, now implementable)

The retained form needs: 1 pick bit per FILE (the flags field already
exists in MNF6/7/8's 32-byte headers), the token wire above, and a
decoder pass after `decode_record_walk`-style causal MED inversion. The
cost model (zlib/zstd backend over the token stream vs the plane) is
the same one measured here. Nothing ships until a format decision
reviews this record; MNF5–8 stay byte-identical.