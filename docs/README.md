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
- [mnf13-r6.md](mnf13-r6.md) — adaptation & quantization ablation on the
  shipped backend-4 coder's own wires (13 arms, representation frozen;
  the byte-exact stream re-encode anchor passed BEFORE any arm number):
  the 16-key context + FAST/SLOW caps grid winner promoted as v1.14.0
  backend 5 (corpus 812,180 = −0.51%); surfaced the `--profile maximum`
  silently-ignored CLI wart (closed by the Benchmark-integrity rule in
  the same release). Instruments under `crates/mnf-mans/scripts/`,
  gate/caps logs in-repo.
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
- [mnf14-if4-corpus.md](mnf14-if4-corpus.md) — IF4 shipped: the
  compression-aware law arms (MNF8 record modes 6/7, planar + bilinear
  laws, mod-256 residuals, decoder-never-reruns discovery) landed in
  `src/laws.rs` + the encoder/decoder/relabel paths; ALL gates green
  and the 30-file release corpus lands BYTE-IDENTICAL to the v1.14
  anchor at 812,180 (law arms fire only when they win a tile; on this
  corpus they don't). Research retained as shipped arms, no backend
  change.
- [mnf14-pg1-disposition.md](mnf14-pg1-disposition.md) — projective
  RGB geometry discovery (MNF14-PG1): homography + color law in
  projected (s,t) coordinates, honest q12 transport, scale ladder
  256/512/1024 on the mechanism's own perspective fixture + the four
  photos. DENIED at every measured scale — the quantized-continuous
  model floor (fixed transport tax + quantization error) cannot beat
  causal prediction's zero-transport residual on content where MED is
  already near-free (the third measured statement of the floor, after
  windowed-geometry and CF1); scope section lists the open halves
  (content-estimated H, a projective-content corpus class).
- [mnf14-hf1-disposition.md](mnf14-hf1-disposition.md) — Hopf-fiber /
  relative-phase feature laws (MNF14-HF1): fiber identity asserted
  exact (the fiber phase is unrepresentable — vibratory ≡ stationary
  by construction), decoder-exact integer phase-ramp + cos/sin-LUT
  laws at 24 B transport. DENIED on photos (+28k..+180k) AND on the
  memo's own positive control (exact-law replay 81 B vs MED 60 B);
  the off-lattice fixture row (fractional Q10 refinement −32%) is a
  REAL precision-ladder result — transferred lesson from IF3's area
  3. The fibration adds no wire freedom: any future Hopf proposal
  reduces to a parameterized phase field under the existing pick.
- [adr-010-discovery-engine.md](adr-010-discovery-engine.md) — the
  pick IS the compression-aware discovery engine (post-HF1
  architectural decision): new structure families enter ONLY as
  candidate arms in the existing pick loops; the family admission
  contract (wire doc frozen first, encoder-side deterministic
  discovery with decoder-never-reruns, exact recon per candidate,
  honest quantized complete-record cost, wire-granularity freeze
  test, the five-site + pack-shape enumeration, corpus gate vs the
  shipped anchor with full revert on tie); the precision-ladder rule
  (fractional wire-lattice sweep before denying a grid-missed
  family); the firing-surface rule (name where the shipped residual
  is expensive BEFORE grammar work); losses recorded with instruments
  + scope lines, never carried as dead arms. consolidates ADR-003..008
  admission practice + IF4's shipped-arms precedent.
- [mnf14-id1-record.md](mnf14-id1-record.md) — ID1 inverse discovery
  (the memo's three stages measured with the engine built per
  ADR-010): ID1-A fixtures generated BY the wire law's own replay —
  the engine inverts plane + modular-recurrence members EXACTLY
  (76/74 B, −98/−97% vs MED) and the fp-LSQ/rounded-replay vocabulary
  plateaus at the rounding floor (22,209 on the same fixture):
  LOSSLESS inverse-search vocabularies must be exact-generative
  integer (ADR-007's rounding-convention rule, proven from the
  inverse side). ID1-B: whole-frame vocabularies lose on two-law
  composites; per-region rows invert exactly — compositions reduce
  to per-region laws under the EXISTING per-tile grammar (the
  concrete candidate = tile-scale div-form laws in modes 6/7
  competition). ID1-C: 3/4 photos decline (R0/GSE-02 floors hold);
  hubble const-arm −46,239 = an ISOLATION estimate (R1 rule), honest
  next rung = a Rust flat-field arm under the shipped framing + the
  corpus gate. Method-honesty notes: three accounting bugs found by
  the fixtures themselves (composition semantics, stride-based
  recurrence test, clipped-plane fixtures); final form = fixtures
  from the law's own replay.
- [mnf14-id2-record.md](mnf14-id2-record.md) — ID2 flat-field arm
  SHIPPED (MNF8 record mode 8, per-tile const field, 3-B params):
  the flat-field inverse-discovery line's first production rung —
  five-site slice per ADR-010 (e), wire-granularity freeze test,
  86 tests / 0 failed, fuzz 525/0-panic, corpus gate 812,180
  (byte-identical anchor), and the named-class photographic gate
  THROUGH THE SHIPPED PICK: hubble 1,452,400 vs 1,479,904 =
  **−27,504 (−1.86%), exact** — the first photographic win from the
  discovery-engine line (isolation estimate −3.0%/832-of-1024 tiles
  recorded separately per the R1 rule). Also the second confirmation
  of ID1-B's composition conclusion: dispatch + economics compose
  with a one-parameter law family.
- [mnf14-id3-disposition.md](mnf14-id3-disposition.md) — ID3 tile-scale
  div-form law family (free D + i16 rates + clamp): honest NEGATIVE —
  exact inversion 15/16 member fixtures (clips, negative slopes,
  sub-unit lattices; one pre-registered family-boundary decline), fp
  control names mode 6's decline as transport (i8 rate), but the
  firing surface is EMPTY: 0/300 tiles on gradient (the LEFT
  predictor owns axis-aligned ramps at ~0 cost, flags 0x20) and
  0/1024 on hubble-512 (const/literal/MED own the noise-dominant
  tiles). Composition stage (C) confirms the per-tile pick composes
  law arms a third time (96/256 div picks, −6.5% on the mixed frame).
  Instrument `crates/mnf-mans/scripts/mnf14_id3_probe.py`; wire
  byte-frozen, 812,180 anchor untouched.
- [mnf14-id4-census.md](mnf14-id4-census.md) — ID4 production
  residual-cost census, the entropy-turn baseline: 812,180 reconciled
  BYTE-EXACT through the instrument, 3,341 record tiles all mode 0,
  6,675 wire-exactness assertions (REF inverse + predictor semantics
  pinned), edge-class residuals 98.1% of proxy bytes. Qualifications
  recorded: cost concentration ≠ entropy limit (edge class conflates
  structures); backend-5 payloads not yet statistically attributed
  (23/30 walked at balanced, labeled). Consequences pre-register
  MNF13-R9 stages A–E (byte-equal backend-5 extraction → prequential
  census → R7/R8 backoff models → complete-profile gate); the
  production codec stays frozen.
- [mnf13-r9a-record.md](mnf13-r9a-record.md) — R9-A DONE: byte-equal
  backend-5 extraction (22 files anchored BOTH directions, 0 failures)
  + prequential audit — L_model ≈ L_entropy_wire to ≤ 0.006% coder
  overhead; adaptation regret 0.47% (9,671 B over 23 files); hubble
  audited at wire-decode provenance (its 277 non-mode-0 tiles make a
  recompute anchor a search re-run — out of instrument scope per
  ADR-010(b)). Attribution points the next rung at ctx-0 (the
  zero-run context carries 80–85% of model cost on document/
  screenshot). Instrument `r9_audit.rs` + `mnf13_r9a_gate.py`.
- [mnf13-r9b-record.md](mnf13-r9b-record.md) — R9-B attribution on the
  saved per-symbol bits: ctx-0's cost share is POPULATION (lowest
  b/sym of any context); regret uniform across contexts and
  COLD-START-shaped in all 22 files (first 10% ≈ 9× the rest's rate);
  transitions 1.5×+ cost density; mid-buckets cost ≤2.3% each.
  Targets bounded: R9-C parent-shared warm-up (≤9.7 kB pool), R9-D
  ctx-15/7 band refinement; durable artifacts in `.research/r9/`.
- [mnf13-r9c-record.md](mnf13-r9c-record.md) — R9-C measured: C2
  warm2p12/14 −288 B (20/22 wins) — mixing only while cold harvests
  the measured cold-start regret; C1 init −20 B (too little to seed
  from); C3 constant-λ −356 B but loses exactly on the ctx-0-heavy
  streams (hot pollution, as R9-B predicted; fix30 net-negative).
  All arms roundtrip byte-exact (one seeding order bug caught by the
  roundtrip assert). Promotion = new backend id, Jon's call after
  R9-D; R9-E gate vs 812,180 unchanged.
- [mnf13-r9d-record.md](mnf13-r9d-record.md) — R9-D measured: the heavy
  band's prev3 refinement is real (D1A −564 B, gain lives entirely
  inside ctx7/15, elsewhere asserted exactly 0) but per-file unstable
  (7/14); D4's composition works ONLY with live ancestors — D2
  (refineA + warm2p12 + share) −996 B, refine7+w2p12+share achieves the
  best refine-family shape (−640, 17/4). The mid-flight merged design
  (keys 7+15 collapsed) measured +1088 — merging destroys the
  conditioning; merged also loses statically (+8.6 kB). Per-file picked
  portfolio readout −1,346 B (+1 B flag); promotion = NEW backend id at
  R9-E, Jon's call.
- [mnf13-r9e-record.md](mnf13-r9e-record.md) — R9-E measured AND
  integrated as backend id 6 (byte parity with the instrument proven
  22/22 first): corpus gate 812,180 → 810,816 (−1,364, −0.168%), 14
  wins / 0 losses / 16 ties, mix6 fires 15/30; the fixed-stream gap
  (−288) reconciles exactly to the joint backend×arm sweep; encode-time
  + memory columns reported separately. Backend 4/5 byte-frozen (knobs
  inert); new mix6 golden + generator; mix5 golden's pick claim
  recomputed. Version release pending review.
- [mnf13-r9f-record.md](mnf13-r9f-record.md) — R9-F: the released pick's
  external surface — hubble (1000×872) swept old (pre-backend-6 binary)
  vs v1.15.0 across both profiles: backend 6 silent off-corpus
  (byte-identical wires, +0 both profiles); the ID4 hubble anchor
  reproduced byte-for-byte through the fresh old binary.
- [mnf15-p1a-record.md](mnf15-p1a-record.md) — P1-A encoder search-cost
  attribution (recorded inputs, anchors asserted): 62.0% of the added
  Maximum-profile time (+275.1 s) lands on mix6-lost files for 0 B;
  added time pixel-scaled (~219 s/Mpx, corr 0.97); RSS not a bottleneck
  (Δmax 1.9 MB); decision table + gate log committed
  (mnf15-p1a-decision-table.csv, mnf15-p1a-gate.log); P1-B/C/D rungs
  assigned (record-stream identity proof first).
- [mnf15-p1b-record.md](mnf15-p1b-record.md) — P1-B construction reuse
  SHIPPED: builder/finisher split + representation-first pick with the
  original-rank tie-break; B1 contract measured (stream = f(coder
  proxy) only — 4 backends share one stream, zlib its own); Maximum
  constructions 200→72 (−64%); gates green (committed candidate
  manifest reproduced byte-exactly; 30/30 max + 30/30 balanced digests
  identical, total 810,816 = the anchor; corpus wall 710.2 s = −37.0% vs
  the 1,127.9 s same-machine baseline, above the 30% target; RSS
  +4.7%; fuzz 525/0); B5 instrument retained (construction ≈ 64% of
  Maximum search); acceptance closed: hubble external wires byte-identical
  both profiles (lossless wire round-trip check) + matched balanced
  timing 165.92→164.17 s (−1.1%, RSS +0.5 MB) — (mnf15-p1b-acceptance.log).
- [mnf15-p1c-record.md](mnf15-p1c-record.md) — P1-C exact construction
  pruning SHIPPED: the per-tile sweep + token matcher are reference-blind
  (ADR-008 REF = terminal relabel applied by the caller); constructions
  per Maximum encode 72 → 18 (−75%); every candidate byte identical
  (committed manifest, 30/30 max + 30/30 bal, hubble); Maximum wall
  481.1 s = −57.3% cumulative vs the pre-P1 baseline (2.34×), Balanced
  56.38 s (−65.7%), RSS 96.2 MB; construction share 31% → P1-D's probe
  surface quantified (mnf15-p1c-gate.log).
- [mnf15-p1d-record.md](mnf15-p1d-record.md) — P1-D D1 exact probe memo
  SHIPPED: memcmp-decided (config, body) identity + the tile-granularity
  laws-lattice memo (pure in tile bytes/planar/proxy — ties verbatim);
  corpus + hubble byte-identical; Maximum wall 428.3 s = −62.0%
  cumulative (2.63×), Balanced 39.87 s (−76.0% cumulative, 4.16×);
  executions −64.2% on screenshot; RSS 107.9 MB = the 16 MiB budget's
  price with the 40 MB D0-ceiling knob documented (mnf15-p1d-gate.log).
  D2 workspace reuse SHIPPED (zstd-3 context recycled via the library's
  own SessionOnly reset; zlib measured reuse-neutral and stays fresh):
  Maximum 414.5 s (−63.2% cumulative, 2.72×), Balanced 28.92 s (−82.6%
  cumulative, 5.74×), hubble 363.3 s byte-identical (-5.2%, reversing
  D1's open +1.7%), zstd probe 2.95×/miss, RSS 108.6 MB
  (mnf15-p1d2-gate.log).
- [mnf15-p1d3-gate.log](mnf15-p1d3-gate.log) — P1-D D3 cost-weighted
  census (bounded measurement): zlib atlas = 87.4% of screenshot's
  remaining miss wall, ~38.6× repeats over 6,609 distinct bodies, 100%
  budget-exhausted/0 stored; value frontier ~434 ms/MiB (screenshot);
  D3-B admission arms measured under the frozen budget (matched
  instrument, per-arm digest assertions): strongest arm ≈ 0.1% wall —
  NEGATIVE result, P1 CLOSED at −63.2%/−82.6% cumulative (2.72×/5.74×)
  (mnf15-p1d3b-gate.log).
- [mnf16-s1a-record.md](mnf16-s1a-record.md) — MNF16-S1 stage 1
  conditional-information census on the shipped balanced streams
  (measurement only): predictor disagreement D = +0.30 bits/sym and
  8-px cross-scale gradient G8 = +0.92 bits/sym prequentially beyond a
  16-bin prev-symbol context (26 files / 2.535M symbols); stage 2 =
  exact production-context measurement through the Rust bridge.
- [mnf16-s2-record.md](mnf16-s2-record.md) — MNF16-S2 stage 2
  production conditional attribution through the R9 bridge (measurement
  only): three gates measured (production identity PASS; decoder
  causality PASS; information: G8 +18.5 kB prequential over the
  production C6, heavy-band split positive on every backend-6 file,
  disagreement prequential-NEGATIVE = already captured, shuffled control
  negative); G8 advances to the coder-level arm; no wire changes.
- [mnf16-s3-record.md](mnf16-s3-record.md) — MNF16-S3 coder-level
  adaptive experiment (additive r9_audit `g8child`; frozen spec before
  the holdout): corpus +5,475 B prequential across 22 mix files (21/22
  gain, shuffled control negative everywhere), Hubble holdout single
  shot +14,355 B with the accounting decomposition separating the G8
  children (+15,788 B children-specific) from the refine package
  (−1,433 B alone); feature-causality proof, adaptive parity, per-child
  accounting recorded; S4/S5 conditional, no backend, anchors untouched.
- [mnf16-s4-w2-finding.md](mnf16-s4-w2-finding.md) — S4 W2 self-
  containment finding: the G8 stencil reads its own target
  (unrealizable); the strictly-past re-measurement collapses the S3
  gains (corpus −2,083 B, 21/22 negative) — G8 line CLOSES; negatives
  stand; anchors untouched.
- [mnf10-autopsy.md](mnf10-autopsy.md) — two-track opener: Bit-Cost
  Autopsy instrumentation + hand-off to generative discovery
  (docs/mnf10-autopsy-gate.csv carries the per-file walk).
- [mnf16-x1-record.md](mnf16-x1-record.md) — MNF16-X1 cross-channel
  discovery (first extractor built under T0–T5; 22 mix files, 2.54M
  stream bytes): X1-A/B/X3 content noise-scale and net-negative against
  the lane-availability partition (+3,989 B estimator-level, structural,
  target-independent by construction); partition advances deferred as a
  pending coder-arm decision; A1 queued; anchors untouched.
- [mnf16-x1l-record.md](mnf16-x1l-record.md) — MNF16-X1-L lane-class
  coder experiment (additive `lanechild`/`laneparent` instrument
  tokens; class = the exact X1 availability mapping, decoder-derivable,
  zero transmission): corpus +41,720.9 B model / +41,708 B wire (7.15%,
  22/22 files, L0 = the exact S3-0 anchor), Hubble holdout +12,653 B
  (0.87%), shuffled control negative everywhere; X1 closes positive;
  independent self-contained decode + portfolio accounting required
  before any promotion.
- [mnf16-t1-record.md](mnf16-t1-record.md) — MNF16-T1 research
  throughput, first deliverable: versioned production traces (22 mix
  streams, 2.54M bytes, v1.16.0 digest-bound) + the executable T0–T4
  prefix contract + the multi-arm runner; X1-L reproduced from the
  trace with max diff 0.0 on every cost and wire byte at 8x lower
  marginal cost; screening population pre-registered.

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
- [g6-quaternary-probe.md](g6-quaternary-probe.md) — quaternary
  nibble-split representation (DENIED: best split arm +27.3% vs the
  plain_med aggregate, 2/25 class-local wins only on constant-plane
  fixtures; the low nibble of natural content is near-random, so the
  split destroys the correlation the strongest predictor feeds on).
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
  change). GSE-10 step 2: `crates/gse10/` skeleton (independent v1.1
  decoder, floor-sum carry math, analytical region sums
  O(rows+exceptions), two exception-index strategies; 7/7 tests,
  clippy `-D warnings` clean) + the steps 5–7 gate matrix
  ([gse10/gate-report.md](gse10/gate-report.md)): GQ1 PASS
  (full-frame 41 ns vs 6,084 ns native scan; aligned 8×8 83 vs
  459 ns; unaligned windows lose honestly), GQ3 PASS (300
  randomized windows vs independent decode), GQ4 PASS (decomposition
  asserted, container charged), GQ2 NEGATIVE RESULT (12/14 vs
  zstd-19; +1 B on uniform random). Exception-tolerant sparse fit
  added — planar-with-exceptions reachable (sparse_edits wire
  4,913→245 B); wire byte-frozen at v1.1; the combined
  storage-and-query gate GSE-09 failed now PASSES on the
  structured synthetic family.

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
