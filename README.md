# Manifold Encoder v0.5 — Rust Research Prototype

A **lossless RGB8 shared-patch atlas** experiment, not yet a learned manifold codec.

## What is implemented

- 16×16 tiles (partial edge tiles supported), scanned in row-major tile order.
- Searches the last 128 already-decoded, identically shaped patches; ranks by RGB means; evaluates the best 8 references.
- Per-tile selection between literal RGB bytes and modulo-256 residual against a previous patch, using a **local zlib proxy cost** including reference metadata.
- Entire tile stream compressed with zlib, with a versioned `MNF5` header.
- Standalone decoder uses only the encoded file and deterministic integer arithmetic.
- Lossless verification tests and an atlas-disabled ablation comparison.

**Limitations:** The local compressed-cost proxy does not guarantee the globally smallest stream. It is possible for the atlas to make a file larger. References are limited to previous tiles; no learned geometry, translation search, reversible rotations, or shared latent manifold yet. Decoding matches the RGB8 pixels of the loaded image, **not the original JPEG bytes**. The container is experimental and not hardened for untrusted files.

## Build and run

Requires Rust stable 1.73+ (the `div_ceil` integer API) and Cargo; crates.io access for initial dependency installation.

```
cargo build --release
cargo test
cargo run --release -- benchmark ../1000033041.jpg
cargo run --release -- encode ../1000033041.jpg -o test.mnf5
cargo run --release -- verify ../1000033041.jpg test.mnf5
cargo run --release -- decode test.mnf5 -o reconstructed.png
cargo run --release -- encode ../1000033041.jpg -o ablation.mnf5 --no-atlas
```

## Fair evaluation

Compare both MNF5 modes to PNG, MNF4, JPEG XL lossless, and WebP lossless using the **same decoded RGB8 input**. Measure file size including all metadata, exact pixels, time and peak RSS. MNF4 and MNF5 are different algorithms; improvement cannot be assumed.

## MNF5 format

Header (24 bytes): `MNF5`, little-endian `u32 width`, `u32 height`, `u32 tile_size=16`, `u32 tile_count`, `u32 zlib_payload_len`.

Decompressed payload: each tile has a `u8 mode` (0=literal, 1=reference), optional `u32 prior_tile_index`, then `tile_width*tile_height*3` bytes (RGB or wrapping residual).

No independent patch dictionary: earlier reconstructed patches form the atlas.

## Development priorities

1. Benchmark on multiple image classes, verify encoder/decoder, fuzz malformed files.
2. Add translation/rotation/reflection candidates and cost-aware pruning.
3. Replace mean-only nearest-neighbor ranking with edge/texture signatures.
4. Compare global bitstream costs, add reversible color transform and better residual entropy coding.
5. Test whether geometric models improve compression over exact-reference and conventional predictors.

Project status: v0.5 is an experimental implementation. The next engineering priority is to compile and validate the Rust code, establish reliable compression benchmarks, and then expand the atlas to support geometric transformations.