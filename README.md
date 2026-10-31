# Manifold Encoder (MNF)

A lossless RGB8 image codec suite in Rust, built as compression research:
shared-patch atlas prediction, geometric transforms, rate-aware coding.

Status: MNF5/MNF6/MNF7 frozen formats + MNF8 backend-field format (tokens,
branch predictors, near matches, matcher depth; ADR-003..006) + the MNF9
exact-generative whole-frame format (ADR-007: six decoder-defined integer
families; the `--mnf8` flow emits an MNF9 file when the generative arm wins
the pick; ADR-008's FLAG_CHANNEL_DIFF selects the per-pixel
[g, r−g, b−g] residual organization when it wins the pick; the v1.12
REF field generalizes bit 6 to a 2-bit reference selector (G/R/B
per-file); the v1.13 backend 4 adds the dual-timescale mix coder to
the pick)) — release v1.13.0 · hardened decoders
(bounded, checksummed, canonical, fuzzed) · independently validated ·
normative format spec at
[docs/format.md](docs/format.md)

Research frontier (2026-10-30): the moonshot lab's verdicts stand (X1
program discovery closed honest-negative; X369's (1,1) anchor = the
64-combo optimum; the G4/G5 sparse-template records); ADR-008 shipped
(−12.3%) and generalized to the v1.12 REF selector (−2,146 B); v1.13.0's
backend 4 mix coder took the released corpus to 816,341 B (−1.97% vs
v1.12, 20/30 mix picks, no regressions). v1.13.0 is the shipping baseline;
the MNF14-CS1 coastline-scaling probe closed the shared-rule and
boundary-refinement arms as measured records
([docs/mnf14-cs1-probe.md](docs/mnf14-cs1-probe.md)); MNFX = the wrapper
format in its own repo ([bageera/manifold-frame](https://github.com/bageera/manifold-frame),
ADR-009 review in docs/), not a change to this crate. v1.10.0's MNF9 arm
is unchanged inside the pick. Separately, the GSE byte-stream research
line continues: GSE-02 gate re-verified 8/8 on this machine (2026-10-30);
GSE-09 (recursive Aleph regions) artifact package committed under
[docs/gse09/](docs/gse09/) — honest provenance (original package not
recoverable; report tables verbatim + seeded reproduction labeled as new
measurements) and frozen wire-spec v1.1; GSE-10 (Rust analytical query
engine, gates GQ1–GQ4) is specified and pending.

Identifier map: **Manifold v1.13.0** = the library release; **MNF9** = the
generative file format's magic; **MNF10** = the research program's milestone
name (never a wire magic); **MNFX** = the wrapper format, own repo.

## Library surface (v1.2+)

Manifold publishes to crates.io as **`manifold-codec`** (the codec library;
the in-repo crate and lib name stay `manifold`) and **`manifold-codec-capi`**
(C ABI, `crates/manifold-capi`), plus a CLI binary.

| API | Status | Since |
| --- | ------ | ----- |
| `manifold-codec` / `codec::*` (MNF5 encode/decode, `DecodeLimits`) | stable (v1.x semver) | v1.0 |
| MNF8 (backend field + FLAG_PRED_MED + FLAG_TOKENS, ADR-002/003) | stable (v1.x semver; TOKENS since v1.6) | v1.2.1 |
| `mnf8::*` MNF8 encode/decode, profiles, predictor flags (ADR-002/003/004) | stable | v1.2.0 |
| `error::Error` / `error::Result` (matchable categories) | stable | v1.2.1 |
| `refdecode::ref_decode` (independent MNF5 decoder) | stable | v1.2.1 |
| `img` I/O shims (JPEG/PNG) | stable under the default `image-io` feature | v1.0 |
| `crates/manifold-codec-capi` (`manifold_decode`, `manifold_encode`, `manifold_encode_mnf8`) — published as `manifold-codec-capi` | stable status codes | v1.2.1 |
| `crates/mnf-subspace`, `crates/mnf-mans` | research, workspace-only (`publish = false`), no semver | — |

Formats are frozen (MNF6/7 flag masks frozen; MNF8 owns flag bit2 only). All
fallible library operations return `error::Result<T>`; malformed input is
`Error::Format`, never a panic. MSRV: current stable Rust (`rust-version = 1.91`).

## Formats

| Format | Magic | Header | Coder | Modes | Introduced |
|---|---|---|---|---|---|
| MNF5 | `MNF5` | 24 B | zlib | 0 literal, 1 exact ref, 2 geometric (D8, opt-in `--geom`) | v0.5 |
| MNF6 | `MNF6` | 28 B + flags | zlib or zstd-19 (bit 1), planar RGB bodies (bit 0) | 0-3 (3 = YCoCg-R literal, 9-bit packed chroma) | v0.9 |
| MNF7 | `MNF7` | 28 B + flags | as MNF6 | 0-5 (4/5 = color-domain residuals, [ADR-001](docs/adr-001-color-transform.md)) | v1.1 |
| MNF8 | `MNF8` | 32 B + flags + backend | zlib-9 or zstd-19 per-image ([ADR-002](docs/adr-002-entropy-backends.md); balanced/maximum profiles; backends 2/3 reserved) | as MNF7 | v1.2 |

All tile the image at 16×16 (row-major, partial edge tiles), rank up to 8
candidates from the last-128-window atlas by structural signature
(RGB mean + gradient + luma + variance), and pick each tile's mode by
cost. MNF6/MNF7 encoders are rate-aware: they build the full tile stream
under every flag variant and emit the smallest file. MNF8 additionally
measures FLAG_PRED_MED (image-wide JPEG-LS MED pre-predictor in front of
literal bodies, causal inversion in the decoder) and the v1.13 backend-4
dual-timescale mix coder — both picked per file at complete-file cost;
the shipped pick is never worse than the prior arms by construction.

## Results snapshot (v1.13.0 pick, fair corpus comparison)

Complete serialized bytes from one `make corpus` run (scripts/
compare_lossless.py: identical decoded RGB8, pixel-exact checks, current
corpus files, webp/jxl if installed); honest losses included.

| Image | pick (mnf8-max) | PNG | WebP | JXL |
|---|---|---|---|---|
| solid 256² | **36** | 761 | 44 | 49 |
| repeat-pattern 256² | **301** | 2,799 | 358 | 7,261 |
| screenshot 640×400 | **667** | 2,118 | 320 | 498 |
| gradient 320×240 | **254** | 2,902 | 148 | 1,603 |
| blob (held-out) | **5,986** | 9,778 | 7,792 | 7,786 |
| noise 256² | 196,912 | 197,106 | **196,688** | 216,172 |

WebP's palette mode keeps the flat-color classes; the pick's MNF9/record
arms keep the synthetic corpus; photographic files still lose to WebP/JXL
(the engine-scale LZ77-2D-per-region gap, documented honest). Full tables:
[docs/benchmarks.md](docs/benchmarks.md); regenerate with `make corpus`.

## Correctness and robustness

- Exactness: decode(encode(I)) == I byte-for-byte, all corpora, all arms.
- Reversibility proofs: modular color transform exhaustive over all
  2²⁴ RGB8 triples (release builds, counter-asserted); YCoCg-R sampled
  sweep + corners.
- Independent decoder: `src/refdecode.rs` (spec-driven, push-map D8),
  agreement-tested with the primary decoder across golden vectors, 14
  rejection vectors, and the full corpus × 3 arms
  ([tests/differential.rs](tests/differential.rs)).
- Resource bounds: checked decompression limit `3WH + 6N` enforced
  during expansion, per-tile minimum-stream gate before image
  allocation, configurable [limits](`src/codec.rs` `DecodeLimits`).
- Fuzzed: mutation harness (scripts/fuzz.py) in CI — 300 trials per CI
  run across the 5 encoder arms, 1,500+ in full local gates, zero
  panics, corrupt streams rejected (checksummed: zlib Adler32,
  MNF6/7 zstd xxh64).

## Build and run

Developer gate (run before pushing; CI runs the same via `make gate`):

```
make gate        # fmt --check, clippy -D warnings, tests, docs
make bench       # criterion decode-throughput benches
make sizes       # deterministic bitstream-size report (30 corpus roundtrips)
make corpus      # one-command fair lossless comparison (png/webp/jxl)
make fuzz        # bounded mutation fuzzer (must exit 0)
```

Requires Rust stable (crate pins MSRV 1.91) and crates.io access.

    cargo build --release
    cargo test                    # debug; release runs the exhaustive proofs
    cargo test --release          # full gate incl. 2^24 reversibility
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings

CLI:

    manifold encode <image> -o out.mnf5 [--no-atlas] [--geom] [--mnf6]
    manifold decode <file> -o out.png        # routes MNF5/6/7/8 by magic
    manifold verify <image> <file>           # exactness check, exit 0/1/2
    manifold benchmark <image>               # 3 arms + PNG, bpp/ratio/ms
    manifold vis <image> [prefix]            # atlas mode/savings/origins maps

`--geom` enables D8 geometric references (MNF5 mode 2). `--mnf6` emits
MNF7 (latest format; the flag predates the MNF7 rename). `--mnf8` emits
MNF8 with ADR-002 backend profiles and ADR-003's rate-aware token arm
(FLAG_TOKENS: LZ77-2D tokens over the MED residual plane, picked per
file only when it measures smaller; decode routes token payloads
transparently by magic).

## Repository map

- [docs/README.md](docs/README.md) — docs index (specs, measurements, research
  records, audits)
- [docs/format.md](docs/format.md) — normative specs (MNF5/MNF6/MNF7) + freeze stamp
- [docs/science.md](docs/science.md) — the principles behind the mechanisms
  (information theory, coding theory, MDL, measured research principles §11)
- [docs/adr-001-color-transform.md](docs/adr-001-color-transform.md) — color-transform ADR
- [docs/mnf14-cs1-probe.md](docs/mnf14-cs1-probe.md) — scale-axis growth curves
  (coastline scaling): law-codings are constant-cost, boundary refinement grows
  slowest of the honest arms — measured records, no format change
- [docs/benchmarks.md](docs/benchmarks.md) — measured tables + held-out corpus
- [CHANGELOG.md](CHANGELOG.md) — every milestone
- [SECURITY.md](SECURITY.md) — vulnerability reporting (security@nocturnalinc.com)
- testdata/ + testdata/heldout/ — development + evaluation corpora
- .github/workflows/ci.yml + .gitlab-ci.yml — CI (fmt, clippy, tests, fuzz, roundtrips)

## Integrity notes

- Encoding a decoded JPEG's RGB8 reproduces pixels, not the original
  JPEG bytes (decoder-identical outputs only; cross-decoder IDCT varies).
- The encoder's per-tile cost proxy is local, not globally optimal: the
  atlas can lose to deflate on some content (measured, kept honest).
- No learned manifolds here — the name is a research direction. The
  research program's measured denials are part of the record:
  scan-order/metric traversals, ternary residual alphabets, a
  Rust-implemented D8+shears format candidate, conformal square→disk
  guidance, and matrix-composed chart matching were all gated and
  non-retained (docs/riemannian-traversal-probe.md,
  docs/ternary-residuals-probe.md, docs/shear-refs-format-probe.md,
  docs/mnf8-riemann-conformal-probe.md,
  docs/mnf9-engine-ladder-probe.md; verdicts in CHANGELOG).
- The ladder's token arm shipped as MNF8's FLAG_TOKENS (ADR-003); the
  `MNF9` name now denotes the exact-generative whole-frame format
  (ADR-007, v1.10.0), not the token engine's research name.

## License

MIT — see [LICENSE](LICENSE).

## Release history

| Tag | Content |
|---|---|
| v0.5.1-rc1 | format spec, conformance vectors, CI, lib/bin split |
| v0.5.1 | gate closed: JXL/WebP benchmark, held-out corpus, RSS |
| v0.6.0 | geometric atlas (D8 mode 2), vis maps |
| v0.7-research | entropy/planar study (+9-bit finding; tag on request) |
| v0.9.0 | MNF6: flags, zstd, planar, YCoCg-R mode 3, rate-aware encoder |
| v1.0.0 | format freeze |
| v1.1.0 | MNF7: modular color-domain modes 4/5 (ADR-001) |
| v1.1.1 | SECURITY.md, doc sync, truly-exhaustive proof |
| v1.1.2 | decoder hardening (checked bounds, limits, alloc gates), D8 conformance |
| v1.3.1 | CI green on the published surface |
| v1.4.0 | packaging + API convenience (consumer pipeline) |
| v1.5.1 | research records: engine ladder + conformal probe |
| v1.6.0 | MNF8 FLAG_TOKENS: LZ77-2D token arm on the wire (ADR-003) |
| v1.7.0 | MNF8 branch-prediction flags PRED_PLANE/PRED_LEFT (ADR-004) |
| v1.8.0 | MNF8 token-wire TAG_NEAR narrow-coordinate matches (ADR-005) |
| v1.9.0 | MNF8 TAG_NEAR + matcher candidate depth two-arm pick (ADR-006) |
| v1.10.0 | MNF9 exact-generative whole-frame format ships (ADR-007 Track A) |
| v1.11.0 | MNF8 FLAG_CHANNEL_DIFF: channel-difference residual organization (ADR-008; −12.3% corpus, 23/30 files) |
| v1.12.0 | MNF8 REF field: the channel-diff reference selector G/R/B (ADR-008 generalized; −2,146 B corpus, REF dist 7/13/5/5) |
| v1.13.0 | MNF8 backend 4: the dual-timescale mix coder (MNF13-R5; −16,398 B corpus = −1.97%, 20/30 mix picks, zero regressions) |
| — (research) | MNF14-CS1 coastline scaling: rule-codings constant-cost but fixture-only; boundary refinement = slowest honest growth (docs/mnf14-cs1-probe.md) |
| — (research) | MNF11: channeldiff relabel −62,675 B/17-of-30 on shipped streams (ADR-008 PROPOSED); X369 grid: (1,1) = optimum; X1 program discovery honest-negative; G4/G5 shared-template + mask verdicts (docs/mnf10-g3-shared-model.md) |