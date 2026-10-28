# Changelog

## Research — MMF9 engine ladder + conformal probe (2026-10-27)

Two more measured records; MNF5–8 bitstream unchanged.

- **MNF9 engine ladder** ([mnf9-engine-ladder-probe.md](docs/mnf9-engine-ladder-probe.md)):
  four arms, one token cost model, in-sim decode asserted. Arm B
  (LZ77-2D tokens over MED residuals) is the first measured strict
  win of the geometry program: gradient −74% (1,639→419 B), solid
  −34% (62→41, beating WebP's 44), blocks −6% — and loses +8…18% on
  photos when forced everywhere (TOTAL 1.151× MNF8), so the retained
  form is a rate-aware per-file pick. Arms C/D (matrix-composed
  chart matches in the token stream, shared transform dictionary,
  factorization T = T₃T₂T₁): measured dead — zero chart-match firing
  on photos, ≤1% on two synthetics, dictionary saves nothing at this
  alphabet size. The matrix-representation idea reduces to the
  shipping MNF8 chart grammar at this corpus scale.
- **MNF8-Riemann conformal probe** ([mnf8-riemann-conformal-probe.md](docs/mnf8-riemann-conformal-probe.md)):
  square→disk conformal guidance (Riemann 1851, Schwarz–Christoffel
  verified) is an analytic no-op for causal prediction — angle
  preservation makes the neighbor ranking position-invariant — and
  the atlas window spans ≤0.2% of the conformal scale variation, so
  `d_f` degenerates to a monotone warp of the Euclidean distance
  (measured: control differs by weight-tuning noise on every file).
  The anisotropic orientation tiebreak shows a real but single-class
  effect (document −5.5%, everything else ≤0.1%): recorded, not
  retained; the one live MNF8-compatible candidate for text-heavy
  corpora.

## v1.5.0 — research soak: benches + measured denials, format surface unchanged (2026-10-27)

Library release pairing the research block with the developer benching
surface. No encoder/decoder behavior change; MNF5–8 streams byte-identical
to v1.4.0 output.

- `benches/decode.rs` (criterion): decode-throughput benches for MNF7/MNF8
  over corpus samples — `cargo bench -p manifold-codec` replaces the
  `/usr/bin/time` scripts. `[[bench]]` requires the `image-io` feature;
  benches are a dev surface only.
- `make bench` / `make sizes` / `make corpus` / `make fuzz` targets wired;
  CI builds benches (`cargo bench --no-run`) and runs sizes.
- Research program records (all denials measured and reproducible, none
  retained): metric/scan-order traversals
  ([riemannian-traversal-probe.md](docs/riemannian-traversal-probe.md)),
  ternary residual alphabets
  ([ternary-residuals-probe.md](docs/ternary-residuals-probe.md)), the
  D8+shears format candidate implemented end-to-end and reverted after the
  30/30 corpus gate ([shear-refs-format-probe.md](docs/shear-refs-format-probe.md)),
  cryo-EM technique probes ([cryoem-plan.md](docs/cryoem-plan.md),
  [dimensional-fold-probe.md](docs/dimensional-fold-probe.md)).
- Announcement draft approved, in-repo
  ([announcement-draft.md](docs/announcement-draft.md)).
- README refreshed: integrity note links the denial records; release
  history current.

## Research — geometry probes, all denied by the retention gate (2026-10-27)

Four measured probes, none retained; the format surface (MNF5–8) is
unchanged. Each has a standalone record in docs/ with repro numbers:

- Traversal scan orders (docs/riemannian-traversal-probe.md): metric-
  guided greedy traversal (w(p,q)=√((q−p)ᵀG(p)(q−p))) loses to raster on
  smooth (3.79×) and plaid (17.5×), ties random, and its single sharp-edge
  win (0.67×) dies against ~2 bits/px of explicit path metadata under the
  derivability contract. Hilbert/morton verified bijective then measured —
  never better than raster+MED on this corpus.
- Metric-warp predictor context (same doc postscript): measured per-tile,
  plaid-only −9.2%, +2–4% metadata tax elsewhere → not retained.
- Ternary residuals (docs/ternary-residuals-probe.md): independent-digit
  split sits below raw byte entropy on every class (0.61/0.73/1.03
  ideal-ratios); ternary Golomb ideal 1,822 B > shipping byte+LOCO+z9 at
  1,212 B.
- D8+shears format candidate (docs/shear-refs-format-probe.md): the last
  surviving candidate was implemented end-to-end in Rust (MNF8 flag bit3,
  10 cyclic shears, MNF6/7 frozen, regression-tested) and reverted after
  the retention gate measured 30/30 byte-identical corpus sizes — MED + D8
  already cover the classes where the atlas-only python arm won.
- Synthetics-only structural rules (slice-repeat 0.48×, torus-phase 0.26×)
  remain non-format: measured on synthetic classes, no real-corpus benefit
  (docs/dimensional-fold-probe.md).

Announcement draft approved and left in-repo to soak
(docs/announcement-draft.md).

## v1.4.0 — adoption readiness: packaging + API convenience (2026-10-26)

From docs/audit-crate-131.md (published-crate audit):

- `[package] exclude` for testdata/, scripts/, .github/, .gitlab-ci.yml —
  the published tarball drops from 1.4 MiB to 307 KiB; repo tests keep
  their corpora via CARGO_MANIFEST_DIR (dep tests don't compile for
  consumers, so no surface loss).
- One-call MNF5 encode: `codec::encode_mnf5(pixels, dims, use_atlas,
  use_geom)` — parity with `mnf6::encode_mnf7` / `mnf8::encode_mnf8`
  (regression: encode_mnf5_one_call_roundtrip).
- ONE DecodeLimits: `mnf6::DecodeLimits` is now a re-export of
  `codec::DecodeLimits` (non-breaking alias; consumers stopped needing two
  imports; MNF5/6/7/8 all share the same ceilings type).
- C-API: added `manifold_encode_mnf5` (append-only ABI addition;
  include/manifold.h updated; rust test + C smoke cover the export).

## v1.3.1 — adoption readiness: CI green on the published surface (2026-10-26)

Three CI fixes, each a distinct root cause found by the gate itself:

- Crate-name flags: `-p manifold` / `-p manifold-capi` resolved outside the
  workspace after the publish rename — all CI `-p` flags now use the real
  package names (`manifold-codec`, `manifold-codec-capi`).
- Newer-stable clippy: the CI toolchain denies const-size
  `chunks_exact(_mut)`; 6 sites migrated to `as_chunks::<3>()` (1.88+-stable,
  MSRV-safe).
- `*.mnf5` ignored the conformance fixtures themselves — CI never received
  the golden/rejection vectors (tests passed only on machines with stale
  files). Fixed the ignore (`!tests/conformance/*.mnf5`) and committed all
  16 fixtures.
- Plus: consumer stumbling block documented in lib.rs (package
  `manifold-codec` vs lib target `manifold`), and an advisory
  cargo-semver-checks job against the published baseline.

External verification: fresh consumer project at /tmp installs
manifold-codec v1.3.0 from the registry and roundtrips MNF7/MNF8 exactly;
docs.rs serves all module pages.
## v1.3.0 — library readiness + C API (2026-10-26)

Eight-item library-readiness pass (approved scope: 1–8 with C-API, MSRV =
current stable = 1.91):

- Error taxonomy: shared `error::Error` / `error::Result` for every fallible
  call (all String-typed errors removed from codec/mnf6/mnf8/refdecode/img);
  malformed input = `Error::Format{detail, position}`, matchable categories
  `Limits` / `Decompress` / `Unsupported` / `Io` / `ImageIo`.
- `decode_mnf7_with_limits` / `decode_mnf8_with_limits`: configurable
  ceilings (max_pixels/max_tiles) at header validation — MNF6/7/8 now honor
  the same limit contract MNF5 already had.
- Debug probes (`MNF6_DEBUG`/`MNF7_DEBUG`/`MNF8_DEBUG`) removed from library
  paths (non-hermetic; findings were test-pinned before removal).
- `vis` demoted to CLI-only module (`src/vis_cli.rs`, `#[path]` include);
  the library root no longer exports it. `refdecode` stays public — it is
  the independent differential decoder the format spec cites.
- `image`/`png` moved behind the `image-io` feature (default on; codec-only
  consumers build with `--no-default-features`); `image` also added as a
  dev-dependency for tests. The `brotli` dependency is gone (backend never
  shipped).
- New crate `crates/manifold-capi` (published as `manifold-codec-capi`): C ABI (`manifold_decode`,
  `manifold_encode`, `manifold_encode_mnf8`) — never panics, status codes
  (stable), caller-owned buffers with the size-query idiom. Ships
  `include/manifold.h`; C smoke test compiles + roundtrips against the
  staticlib. `Error::msg` is public (FFI constructor).
- Crate metadata: `rust-version = "1.91"`, description/repository/keywords/
  categories; publishes as `manifold-codec` (plain "manifold" is taken on
  crates.io); `mnf-subspace` + `mnf-mans` marked `publish = false`
  (workspace-only research crates). Crate-level rustdoc on `lib.rs` +
  missing_docs discipline (113 items documented; `cargo doc` warnings = 0,
  CI now denies them).
- README: "Library surface" table (stable vs research vs C-API since-versions).
- CI: doc warnings denied, codec-only build, `cargo package -p manifold`,
  C-API tests.

- MNF8 MED feed indexing fix (regression by the library pass, caught by
  the test gates): the encoder sliced the flat per-pixel `med_pred` with
  tile-grid row strides instead of pixel width — for any image wider than
  one tile, `FLAG_PRED_MED` streams decoded wrong (the synthetic 64×64
  gates missed it: the measured pick chose references there; the failing
  case was `gradient_320x240.png` under the maximum profile, wrong from
  pixel (1,0)). Stride fixed; all MNF8 pins + the never-loses gate green.

## v1.2.1 — MNF8 review: decoder panic + encoder arm fixes (2026-10-25)

External review of mnf8.rs (9 findings, all dispositioned):

- Backend 2 (brotli prototype) reached `unreachable!()` after the header
  checks — a crafted 32-byte header panicked the decoder. Every
  non-shipped backend is now an explicit error; `mnf8_rejects` covers
  backend 2 with a catch_unwind not-a-panic pin. Backend 2 is marked
  RESERVED like 3 (brotli unshipped: dictionary cost accounting measured
  only in the study; dead code deleted).
- `use_geom = false` still tried hflip/vflip/rot180 — restored the
  three-way split matching codec.rs v0.5 semantics (no-geom = exact
  references only). Regression: mnf8_no_geom_no_flips.
- Added the missing **no-atlas arm** (`use_atlas` parameter; shared
  YCoCg mode 3 + planar layout stay on in both arms) — the ablation's
  pure-prediction control exists again. Regression: mnf8_no_atlas_arm.
- Color modes 4/5 carried a phantom +1 in the cost model (the wire has
  no extra byte) — the encoder under-selected color modes; fixed and
  pinned with forced mode-4/mode-5 end-to-end records through
  decode_mnf8 (structured chroma delta on a noisy base).
- zstd decoder window was the 128 MiB library default — now
  window_log_max(23) + single_frame() in BOTH MNF8 and MNF6 (hostile
  frames can't claim long windows; concatenated frames reject).
- Decompression bound was N x 833 (full-tile assumption) — now
  3WH + 6N + mode-3 overhead with checked arithmetic (the 1x67108864
  degenerate case drops from ~3.5 GB to its true maximum).
- Canonical decode: zlib payloads now reject trailing bytes
  (total_in == payload.len()); zstd tolerated (honest limitation noted
  in code — the stream API can't cheaply report the exact consumed
  offset).
- chosen_flags was never written to the winner's stats in encode_mnf8
  (the old balanced test read a hardcoded tuple and could not fail);
  wired, and the test now verifies a MEASURED pick (planar 231 B vs
  interleaved 7,640 B under zstd-19 on gradient-like bodies).
- Tile loop factored into ONE encode_stream_coder parameterized by
  coder/backend/flags/planar/use_atlas/use_geom (review fix 8);
  maximum_never_loses_to_mnf7 documented as an empirical gate,
  never_loses is not a structural identity.
- modcolor T(tile) hoisted out of the candidate x transform loop.
- payload_len / tile_count: u32::try_from (no lossy casts).
- Encode-cost premise (#6) measured, did not reproduce: the candidate
  screen already runs at zstd level 3 (proxy_cost); zstd-19 runs once
  per candidate on the final whole stream — no cut made.

- Implemented the flagship review item: FLAG_PRED_MED (bit2,
  image-wide LOCO-I MED pre-predictor in front of literal bodies) with
  causal in-walker inversion in decode_record_walk (MNF6/7 pass
  med=false — frozen semantics untouched); rate-aware measured
  selection over {none, planar} x {med, no-med}. Measured on the new
  photo corpora: MED wins 20/20 (zlib-9 avg -21%, zstd-19 avg -19%).
  Regression: mnf8_med_flag_roundtrip_photo_like.
- Context-fetch discipline fix (audit bug found during the gate):
  in-tile MED neighbors live in `recon` until the tile blits;
  out-of-tile neighbors in `pixels`. The first draft read everything
  from `pixels` and decoded garbage at pixel (0,1)+.
- Photo-corpus CLI sweep: 40/40 roundtrips exact (both profiles).

---

## v1.2.0 — MNF8: entropy backend field + profiles (ADR-002)

Implements ADR-002 (docs/adr-002-entropy-backends.md), evidence in
docs/entropy-backends-v13.md (stages 1-2):

- MNF8 (new magic, 32-byte header = MNF7 + 4-byte backend id): backends
  0 zlib-9 (control), 1 zstd-19 (checksummed), 2 brotli-9 (dictionary
  cost documented at the decoder-artifact level), 3 RESERVED (custom
  mixed run/rANS coder — prototype in crates/mnf-mans beats zlib-9 8/10
  but loses to zstd-19 8/10; not shipped until its gate passes),
  unknown values reject.
- Profiles: balanced (default; zstd-19 pinned, flags measured as MNF7)
  and maximum (backend x flags candidates, per-tile mode selection
  re-costed through each candidate's own coder — stage-2 evidence shows
  selection is backend-dependent; emits the smallest file). Measured:
  MNF8-maximum <= MNF7 + 4 on all corpus images (+4 = the backend
  field's own wire cost; measurement parity).
- Record walk shared with MNF7 (decode_record_walk extraction — one
  grammar, two containers); MNF8 rejection vectors (unknown/reserved
  backend, reserved flag, bad magic) + golden vector (balanced profile,
  zstd-19) in tests/conformance/; tests/mnf8_conformance.rs.
- CLI: --mnf8[=balanced|maximum]; fuzz.py gains the --mnf8 arm
  (540/540 trials: all malformed rejected, 0 panics).
- Stage-1/2 tooling (scripts/entropy_backends*.py) + findings doc +
  docs/science.md (informative theory doc) included in this line.
- No MNF5/6/7 changes; their decoders/files untouched.

## v1.1.0 — MNF7: modular RGB8 color-domain residuals (ADR-001)

Implements the color-transform ADR (docs/adr-001-color-transform.md):

- MNF7 (new magic; superset of MNF6): modes 4/5 code exact/geometric
  reference residuals in a MODULAR 8-bit color domain (u8 wrapping
  YCoCg-like lifting — a bijection over RGB8 triples, NOT standard
  signed YCoCg-R, whose 9-bit chroma requirement is why it stays in
  mode 3 with bit-packed planes). Decoder: recon_T = T(base) + resid;
  pixels = T^-1(recon_T), per-tile, cost-selected.
- Reversibility proven exhaustively (stride-17 sweep of all RGB8
  triples + corner blocks; tests/mnf7_modcolor_is_bijective_exhaustive).
- ADR shipping-gate measurement: 0 color selections across the
  10-image corpus (RGB residual already captures that correlation);
  mechanism proven on structured-chroma-delta workloads
  (tests/mnf7_color_domain_used_when_it_wins). Cost model pays nothing
  when the color domain does not win.
- Atlas patches stay canonical RGB8 (ADR integration rule).
- MNF7 fuzz: 400/400 malformations rejected, 0 panics; CLI stats now
  report color utilization; format.md §MNF7 documents layout/proof/
  selection semantics; version 1.1.0.

Validated: fmt/clippy clean, all suites green (mnf6_conformance 4 +
differential 3 + unit 23 + integration), full-corpus MNF7 == exact
10/10; every MNF7 file <= best MNF5 (+4-byte worst case) and < on all 10.

---


## v1.0.0 — Stable lossless image codec (2026-10-25)

Format freeze per the README §9 v1.0 exit criteria:

- MNF5 and MNF6 frozen: no semantic change without a new magic (MNF7).
  docs/format.md gains the freeze stamp, naming tests/conformance/ as
  the conformance set and src/refdecode.rs + tests/differential.rs as
  the independent-validation pair.
- README status lines updated to the frozen/stable profile.
- Both decoders: bounded (take(limit+1)), checksummed (zlib Adler32
  inherent; MNF6 zstd xxh64 mandatory), panic-free across 1,300+fuzz
  trials on the full corpus in three encoding arms each.
- Known limits carried into stability honestly: encoder proxy cost is
  local (not globally optimal); atlas remains backward-only and
  shape-identical; MNF6 encoder runs 4 flag variants (slow but
  rate-optimal); no WebP/AVIF container integration.

v1.0 = same code quality gate: fmt/clippy clean, 31 test suites green.

---


## v0.9.0 — MNF6 format v2: rate-aware selection + YCoCg-R (v0.7/v0.8 gates)

Implements the v0.7/v0.8 research conclusions as MNF6 (magic `MNF6`,
28-byte header + `flags` word): MNF5 tile semantics untouched, plus

- **flags bit0 PLANAR**: literal bodies + reference residual payloads in
  planar RGB (per-channel planes).
- **flags bit1 ZSTD**: payload compressed with zstd-19 (xxh64 frame
  checksum mandatory — see hardening below) instead of zlib.
- **mode 3 YCoCg-R**: bit-packed literal, 1-byte Y plane + two 9-bit
  biased planes (LSB-first). Reversibility verified by exhaustive sweep
  (2^24 / 41-sample stride ≈ 411k colors) plus bit-pack roundtrip.
- **Rate-aware encoder (v0.8)**: builds the full tile stream under each
  of the 4 flag variants with per-variant proxy costs and emits the
  smallest; decode dispatches on magic + flags bits.

**Measured (dev + held-out corpora, all pixel-exact via `verify`):**
MNF6 wins 10/10 vs the best MNF5 arm — gradient 3,887→1,882,
repeat 1,760→753, screenshot 3,897→1,819, solid 875→66, held-out blob
16,490→10,816, plaid 100,402→58,794.

**Hardening found by MNF6 fuzzing:**
- Checksumless zstd silently ACCEPTED interior corruption (3 confirmed
  bit-flip acceptances) — fixed by enabling the xxh64 frame checksum;
  with it, 400/400 mutations reject, 0 panics.
- Spec-vs-implementation divergence on mode 3: the Y plane is n/3 bytes
  (1/pixel), not n; an exact-size accounting fuzz walk found the
  decoder reading 2n+2n9 and desyncing at the first YCoCg tile.
  Fixed; format.md corrected with the same finding.

**New tests** (tests/mnf6_conformance.rs): golden vector (flags
planar+zstd), reserved-flag rejection, exhaustive-arms corpus roundtrip,
`MNF6 ≤ MNF5` rate-aware guarantee. fuzz.py gains a --mnf6 arm.
CLI: `encode --mnf6`; decode/verify route MNF6 by magic.

Dependencies unchanged for MNF5; zstd returns as an MNF6-only dep.

---


## v0.7-research — entropy coder & channel-plan study (2026-10-25)

P2 experiments (README dev priorities 3-5), measured, none shipped as
format changes (all would require a v2 format decision):

- Measured zlib-6 vs zstd-19 × interleaved vs planar RGB on the literal
  tile streams of the dev + held-out corpora (docs/benchmarks.md).
- zstd-19 dominates zlib-6 everywhere except noise (ties): screenshot
  interleaved 3,884 → 1,429 (63%), poster 3,634 → 981 (73%). Entropy
  coding is the largest untapped lever.
- Planar layout beats interleaved on smooth content (gradient zlib:
  5,018 vs 219,782; plaid 64,886 vs 108,184) but loses on palette/flat
  classes (poster 6,620 vs 3,634). Best combined variant on held-out
  blob: planar+zstd 10,692 vs shipped 24,306 (2.3×).
- RESEARCH FINDING: reversible YCoCg-R cannot be byte-packed in this
  format — co = r−b and cg = g−tmp each span 511 values; one byte holds
  256, and mod-256 wrapping breaks the shift-based reconstruction terms.
  Adoption requires a 9-bit planar layout (version bump; not adopted).
- An optimal v2 would spend 2 header bits selecting {interl, planar} ×
  {zlib, zstd} per image by proxy cost; deferred with the v0.8
  rate-aware optimization milestone.
- Transient `research.rs` experiment module and the zstd dependency were
  removed after the study (numbers live in benchmarks.md); the shipped
  0.6.0 codec and its dependency set are unchanged.

---


## v0.5.1 — Validation & hardening gate CLOSED (2026-10-25)

Tag promoted from v0.5.1-rc1 after closing the remaining exit items.

**External codec comparison (README §7):**
- `scripts/bench.py` + `docs/benchmarks.md`: JPEG XL lossless (`cjxl -d 0`)
  and WebP lossless (`cwebp -lossless -m 6 -z 9`) against the three MNF5
  arms and PNG, on identical RGB8 pixels, all verified pixel-exact
  (MNF5 via `verify`; JXL/WebP via decode-to-PNG + `tobytes()` compare).
- Peak RSS per codec via `/usr/bin/time -l`. MNF5 ~3-7 MB — the lowest of
  the measured set (WebP 4-26 MB, JXL 7-49 MB).
- Honest results recorded: WebP's palette mode dominates the synthetic
  classes; MNF5 leads JXL/PNG on repeat-pattern; MNF5 loses to plain
  deflate on solid tiles (documented proxy-cost caveat).

**Held-out evaluation corpus (README §7):**
- `testdata/heldout/`: 5 images from disjoint generators (plaid sines,
  poster blocks, photographic blob, 8x8 color blocks, text document),
  never used for tuning. All arms verify exact. Geom atlas shows its
  first held-out wins on natural content (blob 16,490 vs 18,878 exact-
  atlas vs 24,408 no-atlas; plaid 100,402 vs 107,554); tile-grid content
  honestly favors deflate (blocks 2,034 no-atlas vs 2,195 atlas).

**CI status (README §9 "clean CI"):**
- Local gate fully green: fmt, clippy -D warnings, 21 test suites
  (15 unit + 3 conformance + 3 differential), 220-trial fuzz, release
  build, CLI roundtrips incl. --geom.
- GitLab CI config pushed (.gitlab-ci.yml), but the instance has NO
  runner for this project: the only live runner on Gamma is the
  project-locked `bellybot-gitlab-runner`. Registering one needs a
  GitLab registration token/PAT not present in the environment — the
  pipeline stays `pending` until a runner is registered (project task
  for Jon, not solvable from this session).

---


## v0.5.1-rc1 — MNF5.1 gate: formalization & hardening (2026-10-25)

Closes the v0.5.1 exit conditions (README §9): format spec, fixes, bounded
decoding, property/fuzz tests, golden vectors, clean CI.

**Normative spec:**
- `docs/format.md`: container layout and validation ordering, record
  grammar (literal / exact-ref / geom-ref), ring-Z/256 reference
  semantics, D8 transform table with pull maps, encoder cost model
  (informative), versioning policy.

**Decoder fix:**
- Explicit magic check added — the old test passed only because a
  payload-length mismatch fired first, so any non-MNF5 magic within an
  otherwise-consistent header would have decoded. Now rejected upfront.

**Conformance:**
- `tests/conformance/` + `tests/conformance.rs`: 2 byte-exact golden
  vectors (exact reference; geometric rot180 with mode 2) and 14
  rejection vectors (magic, zero/oversized dims, tile size/count,
  dangling reference, shape mismatch, transform range, transform-4 on
  non-square, mode 3, truncation, trailing data, short header, corrupt
  zlib). Property-based roundtrip loop (deterministic xorshift).
- Crate split into lib + bin (`src/lib.rs`) so integration tests drive
  the real library.

**Independent implementation (README §6):**
- `src/refdecode.rs`: reference decoder written from the spec, no reuse
  of codec parsing/blitting/transforms — D8 implemented as push maps
  instead of pull maps. `tests/differential.rs` cross-checks both
  decoders: agreement on golden vectors, rejection parity on all 14
  rejection vectors, agreement + source equality on the full corpus
  across all three encoding arms.

**CI:**
- `.github/workflows/ci.yml` and `.gitlab-ci.yml` (GitLab is the actual
  remote host): fmt --check, clippy -D warnings, full test suite,
  release build, CLI roundtrips including `--geom`, bounded fuzz via
  `scripts/fuzz.py`.

**Benchmark (README §7 metrics):**
- `benchmark` now reports bits-per-pixel, compression ratio, and split
  encode/decode timing next to file size.

Still open on the path to v1.0: peak-RSS instrumentation, external codec
comparison runs (WebP lossless / JPEG XL lossless), held-out evaluation
corpus.

---

## v0.6.0 — Geometric Atlas (2026-10-25)

Geometric patch references behind an opt-in `--geom` flag; default `encode`
output remains v0.5-format (modes 0/1 only). Added `manifold vis` for atlas
inspection. Validated: 15/15 tests, 1,200 fuzz mutations across five corpus
images with zero panics, lossless roundtrips verified in the CLI.

**Format (mode 2, opt-in `--geom`):**
- New `GeomReference(idx, transform)` mode 2: `[2][u32 idx][u8 transform][residual]`.
- D8 group (identity, hflip, vflip, rot180, and 4 composites) for square
  tiles; shape-keeping subgroup {identity, hflip, vflip, rot180} only for
  non-square partial tiles (transforms that swap w/h are rejected).
- Exact references (t=0) emit as cheaper mode 1 (`[1][u32 idx][residual]`).

**Matching quality:**
- Candidate ranking upgraded from mean-RGB-only to a structural signature:
  RGB means + horizontal/vertical gradient energy + luma mean + variance.
  D8-aware distance takes the cheaper gradient-axis alignment.
- The 8 signature terms are computed once per patch at atlas insertion.

**Cost model (review fixes):**
- Literal baseline no longer double-charged (was 2 zlib streams: mode byte
  + data); references charge their exact metadata (5 bytes, or 6 with a
  nonzero transform). Honest accounting flips constant-tile references to
  literals — deflate compresses a constant literal as cheaply as a zero
  residual, so the atlas only wins where it truly wins.
- Candidate index storage widened u16→u32 (u16 silently collapsed indices
  >65535; wrong references on >65k-tile images).

**Decoder hardening (review fixes):**
- Decompression zipped-bomb capped: `.take(limit+1)` at
  `tile_count * (1 + 5 + 768)` bytes, reject on exceed.
- Structural checks ordered before decompression: declared payload length
  equality, trailing-byte rejection, then zlib.
- Mode-2 arm validates transform range/shape compatibility, reference
  bounds, reference shape, residual presence.

**Tooling:**
- `manifold vis <image> [prefix]`: three per-tile PNG maps — `modes`
  (literal red / exact green / geom blue), `savings` (grayscale, normalized
  by max), `origins` (each reference colored by its source patch;
  deterministic index palette). `benchmark` now reports per-arm atlas
  utilization (literal / exact-ref / geom-ref counts).
- `benchmark` runs the three-way ablation: no-atlas / exact-atlas /
  geometric-atlas on identical pixels.
- `encode` gains `--geom` (opt-in); `--no-geom` removed.

**Measured (synthetic corpus, benchmark subcommand):** geometric matching
finds real wins the exact-atlas misses (screenshot: 27 transformed
references out of 1000 tiles, 3,947→3,897 bytes) and never regresses vs the
exact-atlas superset; repeat-pattern stays at 1,760 bytes (94% under PNG's
140,074); noise correctly stays literal. Geometric search costs
~3–7× atlas encode time (657 ms vs 74 ms on 300 tiles) — noted as the next
optimization target, not yet optimized.

**Known limitations (unchanged from v0.5):** local zlib proxy cost is not
the true marginal cost in the final stream — the atlas can still lose to
deflate's own LZ77 matching (solid: 875 vs 728). References remain
backward-only, shape-identical, translation-free.

---

## v0.5.1 — Validation (2026-10-25)

P0 validation of the packaged v0.5 codec, all criteria met.

- Fixed atlas eviction bug: evicting patches shifted every stored reference
  index, corrupting decodes past tile 128 (gradient image failed lossless
  check). Fix: retain all patches (index space must match the decoder);
  "last 128" is the ranking window, not storage.
- Fixed honest-cost bug found post-fix: per-tile literal cost double-charged.
- 15 unit tests: grid math, odd/edge tiles (1×1…47×48), solid, gradient,
  random pixels, atlas on/off roundtrips, decoder rejections (bad magic,
  wrong tile size/count, dangling reference, shape mismatch, oversized
  dims, truncated payload), proxy-cost monotonicity.
- Decoder hardening: magic, tile size, tile count, declared payload length
  (trailing bytes rejected), reference bounds/shape, mode validation,
  pixel-count cap (2^26 px), zlib-bomb decompression cap.
- Fuzz: 1,260 malformed mutations (bit flips, truncations, appends) across
  the corpus — zero panics, all structurally invalid inputs rejected; the
  one surviving acceptance was a verified byte-identical no-op flip.
- CLI roundtrips exact on all five corpus images (solid, gradient,
  screenshot, repeat-pattern, noise) plus JPEG-sourced pixels.
- Benchmark harness: MNF5-atlas / MNF5-no-atlas / PNG on identical RGB8;
  atlas ablation reproduces the README caveat about local proxy cost
  (repeat 1,760 vs 3,918 atlas/no-atlas, 140,074 PNG; solid 875 vs 728 —
  atlas loses where deflate wins).
- Committed as 58b2e67 (root); tagged v0.5.1.

## v0.5.0 — Initial packaged codec

Lossless RGB8 16×16 tile codec per the v0.5 spec: MNF5 container (24-byte
header + zlib tile stream), literal/reference modes, backward patch atlas
(mean-RGB ranking, last-128 window, 8 candidates), standalone integer-only
decoder, CLI (encode/decode/verify/benchmark), atlas-disabled ablation.