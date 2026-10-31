# ADR-010 — The pick IS the compression-aware discovery engine

## Status

Decided 2026-10-31 (Jon's direction, post-HF1). Supersedes nothing; consolidates
ADR-003/004/005/006/007/008 admission practice + IF4's shipped-arms precedent.

## Context

The architectural thesis: a compression-aware mathematical discovery engine
that selects among multiple kinds of structure instead of committing to a
single geometric model — every candidate law evaluated against the COMPLETE
encoded cost, retained only when it wins, with the existing predictors and
entropy backends carrying everything else. The bottleneck is no longer
representing a mathematical function; it is discovering one that explains
enough real photographic information to cost fewer bytes than the shipped
codec.

The repo ALREADY implements exactly this contract — it is spread across the
pick loops, not centralized, and that is correct (ponytail: the grammar is
the framework; a registry abstraction would add indirection, not power):

- Per-tile records: literal+predictor (0), atlas refs exact/geom, color
  domain (4/5), YCoCg (3), and the IF4 law arms (6/7) — per-tile cost
  competition at complete-record cost.
- Per-file: predictor branch (ADR-004), planar/interleaved, CD relabel
  (ADR-008), REF field (v1.12), token arms (ADR-003/005/006) with matcher
  depth (ADR-006), backend competition (0/1/4/5), MNF9 whole-frame
  generative arms (ADR-007) by magic at final file size.
- Every MNF9 family is a decoder-reproducible discovery (interval
  intersection, bounded D scans) — discovery without decoder rerun.

Measured precedent that motivated the formalization:
- IF4 (first slice of this direction, 6d1c7d9): law arms shipped INSIDE the
  pick; corpus byte-identical to the v1.14 anchor — zero regression while
  the grammar gained a new class. The pick absorbs losing families by
  construction; adding a family is bounded-downside.
- IF3: compression-aware (cost-driven) selection beat structure-fitting on
  2/4 photographs in the research codec; the honest Rust port showed the
  shipped pipeline's framing already covers what those laws found (the
  pilot-quoting rule, R1/IF2/IF4 lineage).
- The denial record (CF1, CS1, PG1, HF1, X1/X2, IF2) — each closed by the
  honest-transport, strongest-control, and corpus rules, each leaving its
  instrument in-repo, none leaving dead code.

## Decision

1. **The pick is the engine.** New structure families enter ONLY as
   candidate arms inside the existing pick loops (per-tile record modes,
   per-file flag arms, MNF9 whole-frame families) — never as parallel
   systems or a new abstraction layer.
2. **Family admission contract** (every candidate family, one slice):
   (a) wire doc block frozen BEFORE any harness runs — field widths,
    quantization/granularity, per-record byte counts;
   (b) discovery is encoder-side, deterministic, bounded-budget;
    the decoder parses shipped params and NEVER reruns the search;
   (c) exact integer reconstruction asserted per candidate before any
    size is printed (mod-256 residual rule);
   (d) complete-record honest cost accounting — params + residual +
    framing, transport quantized to the wire lattice first (never fp64
    tax, never sign-folded or magnitude-folded cost planes);
   (e) wire-granularity freeze test: a known-plane eval asserted EXACT;
   (f) a new record mode touches the FIVE enumeration sites (encoder
    cost + stats dispatch, decode walk, both CD-relabel record branches)
    + pack-shape consistency when joining a planar stream;
   (g) the corpus gate vs the CURRENT shipped anchor (812,180 at v1.14)
    decides: revert entirely on tie.
3. **Precision-ladder rule** (IF3 area-3, measured twice — HF1's
   off-lattice row −32%): a family whose integer grid misses must sweep
   the fractional wire lattice locally before being denied; parameter
   refinement at wire precision is part of discovery, not a tuning after-
   thought.
4. **Cost-floor expectation is normative**, not a surprise: on content
   where causal prediction's residual is already near-zero
   (smooth/planar/projected-rasterized classes), zero-transport
   prediction beats ANY quantized-continuous model regardless of fit
   (PG1/HF1 fixtures). A family must name WHERE the shipped residual is
   expensive BEFORE its grammar work starts (the firing-surface rule).
5. **Losses are recorded**, not reverted into silence: every denied
   family lands its instrument + disposition (with scope-of-closure),
   and the pick never carries a dead arm.

## Consequences

- No registry, no plugin layer, no "engine" module: the engine is the
  grammar + the pick + the gate. Families are small, testable arm slices.
- The grammar space grows only with measured corpus wins; the negative
  space is documented and cheap to re-litigate from one scope line each.
- Candidate generators (discoverers) can be improved independently per
  family (fractional lattices, better seeds) as long as (b) holds.
- The standing comparator is ALWAYS the shipped pick's own files —
  pilot numbers are isolation estimates, never promotion results.

## References

- IF3/IF4: docs/mnf14-if3-disposition.md, docs/mnf14-if4-corpus.md
- Floor denials: docs/mnf13-cf1-disposition.md, docs/mnf14-cs1-probe.md,
  docs/mnf14-pg1-disposition.md, docs/mnf14-hf1-disposition.md,
  docs/mnf14-if2-disposition.md
- Wire admission rules: references/research-probe.md (manifold-codec-dev),
  docs/format.md