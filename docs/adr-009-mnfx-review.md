# ADR-009 review — MNFX Extended Image Wrapper (2026-10-29)

Reviewer verdict on the proposal (premises verified against the tree;
the decision itself = a product-scope decision, Jon's call).

## Premise check (all verified in v1.13.0)

- `to_rgb8()` conversion = real and at the image-I/O boundary
  (src/img.rs:21; four MNF8 call sites are tests).
- Decode routing = per-magic in src/main.rs::decode_cmd; the pub API
  = `decode_mnf8 -> (Dims, Vec<u8>, u32)` / `decode_mnf9 ->
  (Dims, Vec<u8>)` — new semantics genuinely need new entry points
  (the shape rule).
- The description on crates.io reads "Lossless RGB8 shared-patch
  atlas image codecs" — MNFX = a scope redefinition for the crate.

## Agreements (the load-bearing parts)

1. **Layering**: MNF8/MNF9 stay independently validated compression
   streams; the wrapper adds semantics, not compression. This
   protects every gate result through v1.13.0.
2. **Lossless alpha** (hidden RGB preserved) = correct — PNG's own
   rule; anything else breaks the contract.
3. **Metadata as preserved bytes with corrupt-critical rejection**
   = the security-audit recipe's shape; ICC/CICP precedence should
   REFERENCE the PNG spec's rules rather than restate them.
4. **Orientation**: store 1-8, never rotate samples, default = 1
   when absent (make the default normative).
5. **16-bit**: the byte-plane split is exactly the measured
   alphabet-relayout class — the proposal's own interleaved-zstd
   control = mandatory and correct; the pick prices both.
6. **Positioning**: "functionality first; not advertised as
   compression until measured" = the fair-evaluation contract,
   verbatim.
7. **W0+W1 first** = the staged ladder; the exit gate (embedded
   MNF8/MNF9 decodes identically) is the right proof.

## Corrections before W0 freezes the grammar

1. **The 80-B minimum is 2-3x too heavy for the small end.** The
   corpus's small files: solid 36 B (framing = +222%), gradient 254
   (+31%), document 776 (+10%). A 48-B header + 32-B chunk header
   can shrink to 18-B header + 16-B chunk header (type u16, codec
   u8, flags u8, RAW length u32 — required for the decode bound —
   stored length u32, CRC32C u32) with every decode-bound field
   kept: **+34 B minimum** vs +80. The wrapper must never be the
   reason a small file doubles; v1 should not freeze the heavy
   shape.
2. **Layer dispatch needs a normative rule**: exactly one pixel
   layer may be MNF9 (whole-frame; decode dispatches by magic);
   per-plane layers = MNF8/zstd/raw only. Without this the
   "pixel payloads" sentence is ambiguous on the composition.
3. **Chunk-count and total-chunk-bytes bounds must derive from the
   file length** (the mode-3 lesson: derive bounds from real byte
   accounting, never from a per-element multiplier).
4. **CRC32C validated BEFORE parse**, not after allocation.
5. **Where it lives**: inside `manifold-codec` (a new `mnfx`
   module, feature-gated `mnfx` on top of `image-io`) vs a sibling
   crate. In-crate = one consumer import; sibling = the lossless
   crate's surface and its dependency set stay exactly as
   published today (the packaged-tree audit rule favors this).
   Recommend the sibling crate (`manifold-mnfx`) unless the C ABI
   integration requires in-crate, in which case feature-gate.
6. The C ABI stays unchanged (agreed) — W5's new entry points are
   opt-in additions; semver-checks stay advisory per the freeze.

## W0+W1 exit criteria (concrete)

- Bounded parser: malformed header/chunks/lengths/CRC reject
  (Err-not-panic), fuzzed (a fuzz variant entry as with every new
  arm).
- Wrapper decode = EXACT equality with the standalone MNF8/MNF9
  decode of the same pixels (the differential pattern;
  `refdecode` style independent check).
- Plain RGB8 stays standalone (the 80/34-B rule); the CLI gets
  `--mnfx` (opt-in per the freeze policy).
- The 30-file corpus: MNFX-forced = ≥ 34 B overhead per file
  (measured, reported).

## Decision (2026-10-30)

JON DECISION: the wrapper = its own repo, `manifold-frame` (github:
bageera/manifold-frame; crates.io name probed free) — the sibling
crate option. W0 landed there (commit 458280e): the grammar frozen
per the corrections above (18-B header + 16-B chunk header = +34 B
minimum framing; CRC-before-parse with RFC 3720 vectors; bounds from
the file length; unknown-critical reject / unknown-optional skip;
exactly one PIXEL chunk; codec 5 = the sole MNF9 pixel layer; Err
never panic). 11 tests green, clippy -D warnings clean. The
manifold-codec crate's surface stays exactly as published (v1.13.0).
W1 = the MNF8/MNF9 embed layer + the embedded==standalone decode
gate on the 30-file corpus, against manifold-codec 1.13.0 as a
path dependency.
