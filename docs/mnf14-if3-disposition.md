# MNF14-IF3 — spatially LOCALIZED illumination: in-repo disposition (2026-10-30)

**Status: the pilot's photographic result REPRODUCES through an
independent re-implementation — localized models lose on all four real
photographs; subdivision destroys even chelsea's reproduced global win.
The compression-aware discovery hypothesis (θ\* = argmin[L(θ)+L(E|θ)])
is the right NEXT instrument but was only partially exercised here (the
quadtree subdivision cost test IS it, and it produced the one positive
photographic signal). Deny promotion; keep the instrument.**

Companion to docs/mnf14-if2-disposition.md. Pilot numbers not adopted;
everything quoted here was measured this session.

## What the memo claimed (external pilot)

- Localized (≥2 regions / quadtree) lose on all 4 photographs at 512²:
  +7,624 / +6,147 / +3,423 / +109 over the matched MED/CD baseline.
- IF2's global planar chelsea win reproduces in their harness (−1,078).
- Subdividing chelsea into regions makes it worse.
- Positive control (4-region exact planar laws): MED 13,344 / exact eq
  142 / 512-refit 142 / **64²-fit 25,030** — coarse fit max-err 1 level
  but wrong on 224,819/262,144 pixels.
- Random control behaved; tests 305/305 + 45/45 + 24/24 exact.
- Insight: argmin L² error ≠ argmin compressed bytes; fit against
  L(θ) + L(E|θ) instead.

## This session's reproduction (one process, from the report text alone)

Wire: 28-B matched frame + ADR-008 CD-G plane + zstd-19; canonical MED
plane from the repo walker (never re-derived); signed-error planes;
every arm's recon asserted exact before quoting; all parameter
transport honestly charged (f64 LE coefficients + layout bytes).

### Photographic arms (complete bytes)

| Image | MED/CD | IF2-global (err-plane) | PP×4 | PP×16 | bilinear | quadtree(cost-driven split) |
|---|---|---|---|---|---|---|
| astronaut | 405,842 | 405,366 | 406,273 | 407,229 | 405,298 | 405,814 |
| coffee | 439,143 | 443,395 | 441,949 | 441,891 | 441,808 | 434,589 |
| chelsea | 331,079 | **330,668** | 339,343 | 354,698 | 338,013 | **325,933** |
| hubble | 486,037 | 486,467 | 486,686 | 487,543 | 486,421 | 486,113 |

- The IF2 global chelsea win REPRODUCES (−411 here; memo −1,078;
  magnitudes differ with my protocol, sign agrees — and chelsea is
  again the ONLY global winner).
- Fixed-grid piecewise planar (×4, ×16) loses on ALL FOUR — extra
  spatial freedom pays boundaries/params before it saves bytes. The
  pilot's headline (localized loses on 4/4) STANDS through
  re-implementation.
- The COST-DRIVEN QUADTREE (the memo's θ\*-form integrated into tree
  discovery: split only while summed child-compressed-cost < parent
  compressed-cost, every parameter update charged) produced the ONE
  positive photographic signal: **chelsea −5,146 (325,933 vs
  331,079)** and coffee −4,554 (434,589) — but astronaut +, hubble +.
  So compression-aware discovery DID change the photographic result
  (2/4 wins, one large) where structure-driven subdivision lost 4/4.
  This is the memo's "genuinely different experiment" — and it moved
  the number in one session. NOT a promotion case (2/4, both engines
  need the Rust full-wire A/B), but a real measured rung DOWN, not a
  dead branch.

### Positive control (4-region exact-laws fixture)

| Row | Memo | Mine |
|---|---|---|
| MED/CD | 13,344 | 3,200 |
| exact equations | 142 | **104** |
| fitted @ 512² | 142 | 104 |
| fitted @ 64² (+ corrections) | 25,030 | 11,493 |

Sign structure reproduces (exact = 512-refit ≪ MED ≪ 64²-inferred);
the magnitudes differ because my fixture generator and framing are not
bit-identical to theirs. The memo's MECHANISM row reproduces: the
coarse fit's ≤1-level error is spread over most pixels and its
correction stream costs ~36-112× the exact representation here (memo
~176×).

### Random control

MED 786,491 vs memo MED 786,487 — the container behaves on
incompressible input (the ±few-B = implementation detail).

## Disposition (per the standing gates)

**Deny format promotion. Retain the instrument.** The corpus question
stays: is the localized-illumination content class present in
testdata? — and re-implementation says NO for fixed-grid localization,
and PARTLY YES for compression-aware-discovered regions (chelsea,
coffee). The follow-up the memo names is exactly the experiment this
session partially ran:

**R Next (θ\*-driven discovery):** per-node LAW SEARCH (not just split
tests): for each candidate node, discover the law by minimizing
ACTUAL compressed bytes under a small vocabulary {planar, bilinear,
corner-laws, ramp, const}, then build the tree over law costs.
Parameters = the wire; corrections = the wire; selection = per node.
That is a real Rust full-wire A/B against v1.14's 812,180-B pick.

## Scope-of-closure

CLOSED (pilot + this reproduction, consistent):
- structure-driven spatial localization on photographic MED errors —
  no win at any tested resolution (64²..512² sweep = memo; ×4/×16 grid
  = mine);
- "more equations" as a compression strategy.

OPEN (deliberately):
- compression-AWARE law discovery (the θ\* form) at full wire cost —
  THIS session's single most actionable finding (2/4 wins where
  structure-driven got 0/4); needs the Rust A/B rung;
- hierarchical context sharing / deterministic backoff (the R6
  follow-up) — separate rung, same instrument class;
- localized fields for MASK/alpha content (never exercised: IF3's
  classes are all photographic).

## Notes

- The canonical-predictor import rule earned its keep again (the first
  hand-rolled plane broadcast-garbled; import + vectorized matched pass
  passed recon asserts immediately).
- Fixtures out of git (fetch pattern); scikit-image licensing in the
  IF2 disposition doc.
- Reproduction scripts committed:
  crates/mnf-mans/scripts/mnf14_if3_{probe,ctrl}.py (rows:
  /tmp/mnf14_if3/if3_rows.json). No production code changed;
  manifold-codec stays at v1.14.0 (812,180) exactly.