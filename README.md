Manifold Encoder (MNF) v0.5 — Rust Research Prototype
Status: experimental research software · RGB8 lossless image codec · not production-safe

Implemented in the packaged v0.5 source: literal tiles and backward patch references.
Not part of the packaged v0.5 format: geometric-reference mode, learned manifolds, or a stable bitstream specification.

Manifold Encoder (MNF) investigates whether repeated structure in images can reduce lossless encoding cost. Version 0.5 is a deterministic shared-patch predictive codec written in Rust. It uses previously reconstructed tiles as an implicit reference atlas and codes the byte-exact difference between a tile and a chosen reference.

The name manifold expresses a research direction. MNF5 does not learn a manifold, estimate intrinsic dimension, or encode latent coordinates. Its current contribution is an experimentally testable patch-reference baseline for future geometric modeling.
1. Research question and success criteria
Hypothesis. For some image classes, selecting previously encoded patches as predictors can reduce the final compressed bitstream size compared with emitting the same tiles literally.

This is an empirical hypothesis, not a guarantee of better compression. The reference index, mode byte, residual bytes, compressed-stream context, and all container overhead must count in the result.

The project has three separate evaluation criteria:
1. Correctness: For every accepted RGB8 image I, decode(encode(I)) == I in dimensions and every RGB channel byte.
2. Robustness: Invalid streams must be rejected without panicking or exhausting configured resources. This is a target, not yet a verified property of v0.5.
3. Compression value: Atlas-enabled files must be compared with atlas-disabled MNF and established lossless codecs on identical decoded RGB8 data. Savings must be demonstrated by measurements, not inferred from predictor similarity.
A larger MNF output is a valid research result. A decoding mismatch is not.
2. Scope and non-goals
Area	v0.5 contract
Input samples	RGB8: three unsigned 8-bit channels per pixel
Lossless target	Exact reconstructed RGB8 samples and image dimensions
Tile layout	16×16 maximum; partial boundary tiles supported
Traversal	Row-major order of tiles, with row-major pixels inside each tile
Reference atlas	Previously reconstructed tiles, no separately serialized dictionary
Candidate search	Last 128 earlier tiles, matching tile width and height
Candidate ranking	Sum of per-channel RGB-mean absolute differences
Candidate evaluation	Up to 8 ranked references; compressed local-cost heuristic
Residual	Componentwise subtraction modulo 256
Entropy coder	zlib over the complete tile-record stream
Container	MNF5 header + zlib payload
Geometric transforms	Not in the packaged v0.5 format
Learned manifolds / latent bases	Not implemented
Audio / video / arbitrary file-byte preservation	Out of scope


Important file distinction: Loading a JPEG and re-encoding its decoded RGB8 pixels does not preserve the original JPEG's compressed bytes, metadata, quantization tables, or color profile. PNG output is likewise a new file whose decoded pixels should match, not a byte-for-byte copy of the source PNG.
3. Algorithm
Let an RGB8 image be I of width W > 0 and height H > 0, with samples in {0, …, 255}. For T = 16, the tile grid is:
columns = ceil(W / T)
rows    = ceil(H / T)
count   = columns * rows
Each tile has a width tw <= 16 and height th <= 16. Edge tiles may be smaller, but their sample array always has exactly 3 * tw * th bytes.

For each tile P_i, the encoder considers:
- Literal, mode 0: store the RGB8 bytes of P_i.
- Reference, mode 1: choose an earlier, same-shaped tile P_j, with j < i, and store a residual R_i:
R_i[k] = P_i[k].wrapping_sub(P_j[k])
P_i[k] = P_j[k].wrapping_add(R_i[k])   // decoder
These operations are in the ring of integers modulo 256. Therefore, every byte is recovered exactly for a valid reference, irrespective of prediction quality. By induction over tile order, all tiles are recovered exactly when the decoder implements the stated operations and the stream conforms to the format.
Selection heuristic
The encoder ranks eligible tiles by RGB means, evaluates at most eight candidates, and uses a fresh local zlib compression to estimate each literal or residual's cost. The comparison incorporates a one-byte literal mode or a five-byte reference record prefix (one-byte mode plus four-byte index).

The actual file is then compressed as one combined zlib stream. Thus the local heuristic is not the true marginal cost of a tile in the final compressed stream, and the atlas can increase final file size. No optimality claim is made.
Complexity and memory
For N tiles, the current search examines at most 128 predecessor signatures and scores at most eight candidate references per tile. Each tile contains at most 768 RGB bytes. The prototype retains all reconstructed tiles, not only the last 128, because reference indexes identify absolute earlier tile positions. Consequently, memory usage grows with image size; the search window is not a decoder-memory bound.
4. MNF5 container: observed v0.5 layout
All integer header and reference fields are unsigned 32-bit, little-endian.
Offset	Width	Field	Meaning
0	4	Magic	ASCII MNF5
4	4	Width	RGB image width in pixels
8	4	Height	RGB image height in pixels
12	4	Tile size	Must be 16
16	4	Tile count	Must match derived tile grid
20	4	Payload length	Number of zlib-compressed bytes
24	Variable	Payload	zlib-compressed tile records


The decompressed payload consists of exactly tile_count consecutive records:
mode 0 (literal):
    u8       mode = 0
    u8[n]    literal RGB bytes

mode 1 (reference):
    u8       mode = 1
    u32_le   prior_tile_index
    u8[n]    modulo-256 residual bytes

n = 3 * tile_width * tile_height
Tiles are decoded in row-major tile order. Reference indexes must be strictly less than the current tile index; tile dimensions must match for reference prediction. Every record's sample count is implied by the image dimensions and tile position. There is no separate atlas dictionary.

Compatibility policy: MNF5 is a research format, not a frozen standard. Do not interpret mode 2 as valid for the packaged v0.5 decoder. A separate Rust source excerpt under development introduces geometric mode 2, gradient/luma/variance signatures, and eight transform identifiers; it is not the same implementation as the packaged v0.5 archive. Adding mode 2 must be accompanied by an explicit compatibility decision, conformance vectors, and a documented decoder update; prefer a new format version if decoding semantics change.

A normative bitstream specification and golden test vectors are planned in docs/format.md and tests/conformance/ (not yet provided).
5. Build and run
Prerequisites
- Rust stable and Cargo, at versions compatible with the dependency versions Cargo resolves.
- Access to crates.io for the initial build, unless dependencies are vendored or cached.
- The example photograph (1000033041.jpg) from the prototype bundle, or your own image.
Do not rely on a claimed minimum Rust version without checking dependency MSRVs. This prototype currently declares image = "0.25"; use a contemporary stable Rust toolchain, and record rustc --version in benchmark reports. Commit Cargo.lock for repeatable application builds.

From the directory containing Cargo.toml:
cargo build --release
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# Encode, check exact RGB8 reconstruction, and decode:
cargo run --release -- encode 1000033041.jpg -o sample.mnf5
cargo run --release -- verify 1000033041.jpg sample.mnf5
cargo run --release -- decode sample.mnf5 -o reconstructed.png

# Baseline ablation and built-in comparison:
cargo run --release -- encode 1000033041.jpg -o literal.mnf5 --no-atlas
cargo run --release -- benchmark 1000033041.jpg
If your image resides in the parent folder, use ../1000033041.jpg instead. The CLI currently exposes encode, decode, verify, benchmark, and the --no-atlas encoding option.

Status of verification: These commands describe the source interface. They are not evidence that the code has been compiled, the test suite passes, or that a benchmark has been run in the current environment.
6. Testing and correctness evidence
Tests present in packaged v0.5
The library includes small round-trip tests across several image sizes, a repeated-patch test expecting a reference, and a basic invalid-stream rejection test. These are valuable smoke tests, not exhaustive proof.
Required before an MNF5.1 hardening milestone
- Property-based testing: Random dimensions, edge tiles, image contents, atlas on/off, and round-trip equality; deterministic failing-case reproduction.
- Format conformance: Fixed binary golden vectors with expected RGB output and explicit rejection vectors for invalid metadata, references, modes, and lengths.
- Mutation and fuzz testing: Exercise the decoder on truncated, corrupted, adversarially compressed, and structurally inconsistent streams with enforced memory/time budgets.
- Independent implementation: At least one decoder implementation driven by the format specification rather than reused encoder helper functions.
- Release/regression tests: Preserve decoding of every previously published golden vector for each supported bitstream version.
Current baseline guardrails: The packaged v0.5 decoder checks the MNF5 magic, tile size, tile count, compressed-length equality, backward reference ordering, and a decompressed-output bound derived from the image dimensions. It also limits image pixels. Nevertheless, it has not undergone a full adversarial-security review. In particular, reference validation should compare tile width and height, not only equal byte lengths; hostile-input resource behavior and zlib stream consumption still need testing.

The separate geometric draft has different security and correctness problems (including missing magic validation, unbounded zlib output, index saturation above u16::MAX, and non-distinct transforms). Those findings must be fixed before its format is merged or released.
7. Fair compression benchmarking
Always benchmark the same decoded RGB8 pixel matrix. For a JPEG input, decode once and use that canonical RGB8 matrix for all lossless codecs. Do not compare byte-exact JPEG storage with pixel-exact RGB8 reconstruction as though they are equivalent tasks.

At minimum, measure:
Metric	Definition
Compressed size	Entire output file length, header and metadata included
Bits per pixel	8 * file_size_bytes / (W * H)
Compression ratio	3 * W * H / file_size_bytes for RGB8
Encode time	Encode operation only, measured over repeated runs
Decode time	Decode operation only; do not accidentally include pixel verification
Peak memory	Peak resident memory or a consistently defined allocator metric
Exactness	Dimensions and every RGB8 sample compare equal
Atlas utilization	Number/fraction of reference records, and their measured global net effect


Compare literal-only MNF5, atlas-enabled MNF5, PNG, WebP lossless, JPEG XL lossless, and previous MNF experimental baselines wherever reproducible implementations are available. Include diverse images: photographs, screenshots, illustrations, synthetic repeated patterns, gradients, and incompressible/random data. Separate development data from held-out evaluation data.

Record CPU, OS, Rust version, dependency lockfile, codec versions, CLI settings, corpus hashes, and whether compression is single- or multithreaded. Report losses as well as wins; avoid selecting only favorable images. The built-in v0.5 benchmark command is a preliminary two-mode measurement and includes verification in its decode-related timing, so it is not a replacement for an instrumented benchmark harness.
8. Security and trust model
Do not expose this prototype as a network-facing decoder or process untrusted files without additional hardening. The desired hardened decoder must:
1. Validate header fields before dimension, grid, or allocation arithmetic.
2. Use checked arithmetic for counts, sizes, offsets, and memory budgets.
3. Cap both compressed input size and decompressed output during expansion.
4. Reject illegal mode bytes, forward/self-references, wrong reference shapes, truncated records, trailing data, and malformed compressed streams.
5. Avoid panics and unchecked indexing on malformed inputs.
6. Preserve deterministic error classifications and stable rejection behavior.
7. Run fuzz testing with CPU/time and memory limits.
These are acceptance conditions, not claims that all conditions are already met.
9. Project status and release gates
Milestone	Purpose	Exit conditions
v0.5	Shared-patch baseline	Experimental literal/reference codec and CLI; source and initial smoke tests
v0.5.1	Formalization and hardening	Format spec, fixes, bounded decoding, property/fuzz tests, golden vectors, clean CI
v0.6	Geometric atlas research	Distinct reversible transforms, safe indexing, exact reconstruction, transform ablation
v0.7	Richer residual models	Reversible color transforms, improved entropy models, measured gains
v0.8	Rate-aware optimization	Measure end-to-end costs and avoid unprofitable atlas selections where feasible
v0.9	Interoperability and performance	Independent decoder, version compatibility, profiling, fuzzing
v1.0	Stable lossless image codec	Documented stable format and demonstrated robustness on a representative corpus


Immediate next task: MNF5.1. Write docs/format.md as a normative grammar, add strict tile-shape checks and malformed-stream vectors, verify the Rust build/test suite, then run a documented atlas-on/off benchmark. Only after the baseline is reliable should the geometric source draft advance toward v0.6.
10. Research integrity
MNF5 is a geometry-inspired predictive compressor, not evidence that the image lies on a learned low-dimensional manifold. Claims about compression advantage require independent reproducible measurements; claims about losslessness require a documented contract and conformance evidence. A mathematically exact residual construction is necessary but does not replace parser and implementation verification.
License
Cargo.toml declares MIT. The source archive should include a matching LICENSE file before release or redistribution; the package manifest alone does not provide the full license text.