# Changelog

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