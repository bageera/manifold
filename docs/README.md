# docs/ — study and specification index

Normative and study documents for the MNF formats. Every measured claim
points at its producing record; `docs/science.md` maps the underlying
principles (information theory, coding theory, MDL) to the mechanisms.

## Specifications (normative)

- [format.md](format.md) — MNF5/MNF6/MNF7 bitstream specs + freeze stamp;
  MNF8 extension described in the crate docs (`src/mnf8.rs`) and
  [ADR-002](adr-002-entropy-backends.md).
- [adr-001-color-transform.md](adr-001-color-transform.md) — MNF7 color-domain
  residuals decision record.
- [adr-002-entropy-backends.md](adr-002-entropy-backends.md) — MNF8 backend
  field + profiles decision record.
- [mnf8-adr-003-tokens.md](mnf8-adr-003-tokens.md) — MNF8 FLAG_TOKENS
  (LZ77-2D token payload over the MED plane) decision record.
- [mnf8-adr-004-branch-pred.md](mnf8-adr-004-branch-pred.md) — MNF8
  PRED_PLANE / PRED_LEFT branch-predictor flags decision record.
- [mnf8-adr-005-token-near.md](mnf8-adr-005-token-near.md) — token-wire
  TAG_NEAR narrow-coordinate matches decision record.
- [mnf8-adr-006-matcher-depth.md](mnf8-adr-006-matcher-depth.md) — token
  matcher candidate depth (two-arm pick) decision record.
- [mnf10-genfunc-discovery-probe.md](mnf10-genfunc-discovery-probe.md) —
  exact generative-function discovery probe (MNDF10 track opener).
- [mnf8-adr-007-mnf10-genfunc.md](mnf8-adr-007-mnf10-genfunc.md) — MNF10
  exact generative records decision record (PROMOTED Track A: shipped
  decoder + pick arm; status header in the ADR).
- [mnf10-track-a-promotion.md](mnf10-track-a-promotion.md) — the Track A
  promotion record: CLI pick arm, dispatch, verify chain, fuzz diff.
- [mnf10-g3-shared-model.md](mnf10-g3-shared-model.md) — Track B G3:
  shared-model competition verdicts (Arm C denied at zstd; wins under a
  window-constrained container; fixture generator committed) + the G4
  revised gate (identity+sparse vs D8+sparse vs the ACTUAL 1.10.0
  portfolio; sparse+template is the mechanism), G5
  sparse-representation economics (masks/channel-split DENIED), and
  the MNF11 diagnostic + relabel-shipped-stream experiment
  (−62,675 B, 17/30) + X369 grid + X1 program-discovery closure.
- [ADR-008](mnf8-adr-008-channel-diff.md) — MNF8 channel-difference
  residual organization — IMPLEMENTED v1.11.0 (bit 6; −12.3% corpus /
  −116,570 B, 23/30 files; pick-measured both organizations) and
  GENERALIZED v1.12.0 to the 2-bit REF field (G/R/B selector; −2,146 B
  corpus = the C2 census's ≈ −2.1 kB prediction reproduced; REF2/REF3
  goldens + the bit-8 reject boundary).
- [mnf13-r0.md](mnf13-r0.md) — conditional residual structure on the v1.12
  streams: activity/direction contexts ≈ 0 (CLOSE); order-1 −0.33 bits
  reproducible; offset profile flat (SI/DNA contact-map arms gated
  negative).
- [mnf13-rt4-disposition.md](mnf13-rt4-disposition.md) — rectangle
  references: the representation existed all along (record mode-1 refs);
  rerun ablation +150 on the best real case = CLOSE.
- [mnf13-r1.md](mnf13-r1.md) — context-aware coding vs the v1.12 wires:
  adaptive single-estimator loses +2..+4.4% (6/6); the FIRST honest
  floors (shipped −5..−6% below the order-0 entropy floor; the
  conditional floor = −7..−8% per file).
- [mnf13-r5.md](mnf13-r5.md) — the dual-timescale mix coder promoted
  (v1.13.0 backend 4): −1.80% corpus at the gate (−1.97% shipped),
  19/19 wins, zero transport; the R4 mechanism shipped.
- [mnf13-cf1-disposition.md](mnf13-cf1-disposition.md) — circular-field
  records: 95-99% wins on radial-structure fixtures but the corpus
  control rejects photos (CLOSED as an instrument).
- [mnf14-cs1-probe.md](mnf14-cs1-probe.md) — coastline scaling
  (scale-axis growth exponents, 5 classes × 5 scales): shared Koch
  rule = 241 B at every scale (exponent 0.000) but fixture-only (no
  corpus class); recursive boundary refinement grows slowest of the
  honest arms (koch 1.217, alpha 0.913 vs PNG 1.290/1.119) yet loses
  absolutely at gate scales — DENIED as format arms; instrument
  committed.
- [mnf14-if2-disposition.md](mnf14-if2-disposition.md) — fitted
  photographic illumination fields (external pilot IF2 + an independent
  in-repo reproduction): whole-frame field correction loses 4/4
  aggregate (chelsea's win is single-file and protocol-sensitive);
  precision-transfer control REPRODUCED (512² refit ≈ 8.6× under the
  64²-inferred field); no promotion — CF1-shaped deny.
- [mnf14-if3-disposition.md](mnf14-if3-disposition.md) — spatially
  LOCALIZED illumination (external pilot IF3 + independent reproduction):
  fixed-grid localization loses 4/4 at every resolution; subdivision
  destroys chelsea's global win; the memo's θ*-form (fit against actual
  compressed cost) exercised as a cost-driven quadtree produced the ONE
  new photographic signal (chelsea −5,146, coffee −4,554; 2/4) where
  structure-driven got 0/4 — the actionable rung for a Rust full-wire
  A/B vs v1.14's 812,180 pick. Deny promotion; retain the instrument.
- [mnf10-autopsy.md](mnf10-autopsy.md) — two-track opener: Bit-Cost
  Autopsy instrumentation + hand-off to generative discovery
  (docs/mnf10-autopsy-gate.csv carries the per-file walk).

## Measurements (canonical numbers)

- [benchmarks.md](benchmarks.md) — lossless comparison tables (PNG/JXL/WebP
  columns), held-out corpus.
- [atlas-eval-v12.md](atlas-eval-v12.md) — encoder arm ablation (MED, atlas,
  D8, planar at coder parity).
- [entropy-backends-v13.md](entropy-backends-v13.md) — backend matrix at
  frozen-stream parity (zlib/zstd/brotli/xz).
- [subspace-v01.md](subspace-v01.md) — PCA/MDL subspace study (FAIL gate).
- [photo-gate-v122.md](photo-gate-v122.md) — photographic corpus gate.

## Research records (2026-10-27 probe program)

Six geometry-family denials + one engine retention, all measured with
metadata counted and exact reconstruction asserted:

- [riemannian-traversal-probe.md](riemannian-traversal-probe.md) — metric-guided
  scan order (DENIED; also the metric-warp predictor postscript).
- [shear-refs-format-probe.md](shear-refs-format-probe.md) — D8+10-shear MNF8
  flag candidate implemented and reverted at the 30/30 gate.
- [ternary-residuals-probe.md](ternary-residuals-probe.md) — balanced-ternary
  digit splits on prediction residuals (first-cost analysis).
- [research-paper-validation.md](research-paper-validation.md) — published
  compression claims vs manifold measurements (per-claim verdicts).
- [mnf8-riemann-conformal-probe.md](mnf8-riemann-conformal-probe.md) — conformal
  square→disk guidance (analytic no-op) + anisotropic orientation tiebreak
  (single-class candidate).
- [riemann-array-v05-probe.md](riemann-array-v05-probe.md) — shear-wrap torus
  prediction (1/W lever; 0/30 corpus wins).
- [periodic-prediction-v06-probe.md](periodic-prediction-v06-probe.md) — periodic
  structure prediction (25/30 corpus wins, −8.92%; mechanism split T=1 vs true T;
  coefficient hybrid denied).
- [webp-hybrid-probe.md](webp-hybrid-probe.md) — WebP-style predictors on the
  MNF8 coder (DENIED) + the fair corpora comparison table.
- [mnf9-engine-ladder-probe.md](mnf9-engine-ladder-probe.md) — the ladder:
  LZ77-2D token engine RETAINED rate-aware; matrix-matching arms DENIED.
- [mnf9-token-arm.md](mnf9-token-arm.md) — the retained arm ported to Rust
  (`crates/mnf-mans`): real decoder, 30-file corpus gate, 5/30 token wins
  reproducing the ladder numbers.
- [mnf9-rans-arm.md](mnf9-rans-arm.md) — mixed run/LZ + contextual rANS
  (backend-3 candidate): DENIED as universal, RETAINED behind the
  rate-aware pick (screenshot −34%, document −23%); wire-through-rANS DEAD.
- [mnf9-charts-probe.md](mnf9-charts-probe.md) — step-5 candidates:
  gradient-orientation tiebreak DENIED (the tie surface does not exist
  post-residual); 2D-token column-layout RETAINED as a pick candidate
  (−576 B/30-file, text/screen classes); arm F recorded as the pick's
  own per-file readout.
- [mnf9-erdos-probe.md](mnf9-erdos-probe.md) — Erdős-offset candidate
  generation DENIED (equal-budget content-key chains dominate; A+B union
  worse everywhere); (m,dir) address coding DENIED (u8 coordinate pair
  beats any structured address); blob far-offset signal recorded, not
  retained.
- [mnf9-lattice-probe.md](mnf9-lattice-probe.md) — grid-topology
  ablation: hex/tri causal lattices DENIED (+8.1%/+13.3%); algebraic
  coordinates = analytic no-op tie; the measured discovery is the
  branch-choice family (LOCO-I-plane −4.6%, left-only per-file arm),
  MNF8-compatible and cheap.
- [mnf9-branch-gate.md](mnf9-branch-gate.md) — the branch-choice arms'
  RETENTION GATE: PASSED (−71,767/−7.0% vs the v1.6.0 pick, never-worse
  per file, 25/30 files on the new arms). Format step cleared:
  MNF8 bits 4-5 + ADR-003 pick extension, no new magic.
- [mnf9-windowed-geometry-probe.md](mnf9-windowed-geometry-probe.md) —
  unbounded shape / bounded observation (frequencies freed from the
  window): DENIED at quantized-parameter cost; loses even on constructed
  sinusoids (the model's quantization floor exceeds the residual-coder
  floor); the tied-period negative result STANDS with its binding
  constraint corrected. Discrete/integer geometric structure (CY01
  phases) remains the family's measured-win territory.

Earlier research notes: [v07-charts-replication.md](v07-charts-replication.md),
[cy01-torus.md](cy01-torus.md), [dimensional-fold-probe.md](dimensional-fold-probe.md),
[cryoem-plan.md](cryoem-plan.md), [eer-probe-10061-10943.md](eer-probe-10061-10943.md),
[riemannian-review.md](riemannian-review.md).

- [vid03-r2-topology-gate.md](vid03-r2-topology-gate.md) — VID03-R2 topology-dependent reconstruction gate (DENIED: multi-sheet topology representable + renderable but not economical; sparse-event control covers the sheet record)

- [gse02-gate.md](gse02-gate.md) — GSE-02 hierarchical MDL byte-stream
  codec gate (Rust research crate: generator/partition/shared-model
  separation measured; 4/8 vs framed zstd-19 on the acceptance
  matrix; decode-verified; natural-data gate pending)

- [gse09/gate-record.md](gse09/gate-record.md) — GSE-09 recursive
  Aleph-regions artifact preservation: original prototype package not
  recoverable → report tables preserved verbatim + seeded reproduction
  (wire roundtrip ×72, malformed rejection, v1.1 signed-correction
  contract) + wire-spec v1.1 frozen
  ([gse09/wire-spec.md](gse09/wire-spec.md); replacement bytes on
  wire, corrections signed post-mod). Carried finding:
  exception-charge granularity must equal query-index granularity
  (per-tile exceptions lose the sparse fixture; whole-frame
  exception table = a competing MDL arm in GSE-10, not a format
  change). GSE-10 v1.1 = Rust analytical query engine,
  gates GQ1–GQ4 pending.

## Communication

- [science.md](science.md) — the principles document: information theory,
  coding theory, MDL, and the measured research principles (§11).
- [announcement-draft.md](announcement-draft.md) — approved announcement
  numbers draft (in-repo, soaking).

## Audits

- [audit-crate-131.md](audit-crate-131.md) — published-crate audit (v1.3.1).
- [audit-2026-10-25.md](audit-2026-10-25.md) — full-repo technical audit.
## Container decisions

- [adr-009-mnfx-review.md](adr-009-mnfx-review.md) — ADR-009 MNFX
  wrapper proposal: premises verified in-tree; layering/alpha/
  metadata/orientation/16-bit-control agreements; W0 grammar
  corrections (34-B minimum framing, one-MNF9-layer rule, bounds from
  file length, CRC-before-parse); in-crate vs sibling-crate = the
  open product-scope decision.
