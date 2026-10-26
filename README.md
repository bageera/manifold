# Manifold Encoder (MNF)

A lossless RGB8 image codec suite in Rust, built as compression research:
shared-patch atlas prediction, geometric transforms, rate-aware coding.

Status: MNF5/MNF6/MNF7 frozen formats + MNF8 backend-field format
(v1.2.1, ADR-002) · hardened decoders (bounded, checksummed, canonical,
fuzzed) · independently validated · normative format spec at
[docs/format.md](docs/format.md)

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
literal bodies, causal inversion in the decoder) — it wins on photographic
content 20/20 (zlib-9 avg −21%).

## Results snapshot (v1.1.2, synthetic + held-out corpus)

Every number pixel-exact (`verify`); honest losses included.

| Image | MNF5 best | MNF7 | PNG | JXL | WebP |
|---|---|---|---|---|---|
| repeat-pattern 256² | 1,760 | **757** | 140,074 | 7,261 | 358 |
| solid 256² | 728 | **70** | 2,677 | 49 | 44 |
| gradient 320×240 | 3,887 | **1,886** | 69,512 | 1,603 | 148 |
| screenshot 640×400 | 3,947 | **1,823** | 11,181 | 498 | 320 |
| blob (held-out) | 16,490 | **10,820** | 32,759 | 7,786 | 7,792 |
| noise 256² | 196,929 | **196,908** | 196,947 | 216,172 | 196,688 |

WebP's palette mode wins the flat-color classes; MNF7 wins 10/10 against
the best MNF5; plain deflate beats MNF7 on solid tiles without the atlas
(documented proxy-cost caveat). Peak RSS ~3–7 MB — the lowest measured.
Full tables: [docs/benchmarks.md](docs/benchmarks.md).

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
- Fuzzed: mutation harness (scripts/fuzz.py) in CI, 1,300+ trials
  across arms, zero panics, corrupt streams rejected (checksummed:
  zlib Adler32, MNF6/7 zstd xxh64).

## Build and run

Requires Rust stable (crate pins MSRV 1.73+) and crates.io access.

    cargo build --release
    cargo test                    # debug; release runs the exhaustive proofs
    cargo test --release          # full gate incl. 2^24 reversibility
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings

CLI:

    manifold encode <image> -o out.mnf5 [--no-atlas] [--geom] [--mnf6]
    manifold decode <file> -o out.png        # routes MNF5/6/7 by magic
    manifold verify <image> <file>           # exactness check, exit 0/1/2
    manifold benchmark <image>               # 3 arms + PNG, bpp/ratio/ms
    manifold vis <image> [prefix]            # atlas mode/savings/origins maps

`--geom` enables D8 geometric references (MNF5 mode 2). `--mnf6` emits
MNF7 (latest format; the flag predates the MNF7 rename).

## Repository map

- [docs/format.md](docs/format.md) — normative specs (MNF5/MNF6/MNF7) + freeze stamp
- [docs/adr-001-color-transform.md](docs/adr-001-color-transform.md) — color-transform ADR
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
- No learned manifolds here — the name is a research direction.

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
