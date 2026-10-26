# Changelog

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

Validated: fmt clean, clippy 0 warnings, 45 tests / 7 suites green
(release), fuzz 540/540 rejected 0 panics (incl. --mnf8 arm), CLI
roundtrips exact on both profiles.

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