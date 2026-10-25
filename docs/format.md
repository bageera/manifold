# MNF5 Bitstream Format — Normative Specification

Version: 1 (v0.5-layout + opt-in mode 2, as shipped in v0.6.0)
Status: normative for the `manifold` crate. Golden vectors live in
`tests/conformance/` and are enforced by `tests/conformance.rs`.

## 1. Container

A well-formed MNF5 file is:

    file := header(24) payload(payload_len)

All integers are **little-endian**. No padding, no alignment, no extension
fields. Files with fewer than 24 bytes are invalid. Files whose total length
differs from `24 + payload_len` are invalid — trailing bytes are rejected,
not ignored.

### Header (offset, width, field)

    0   4   magic            ASCII "MNF5" exactly
    4   4   width            u32, image width in pixels, > 0
    8   4   height           u32, image height in pixels, > 0
    12  4   tile_size        u32, MUST be 16 in this version
    16  4   tile_count       u32, MUST equal ceil(width/16) * ceil(height/16)
    20  4   payload_len      u32, zlib byte length, MUST be <= 2^31 - 1

Decoder MUST validate magic, dimensions, tile_size, tile_count, and
payload_len **before** decompressing. Additionally:

- `width * height` MUST NOT exceed 2^26 pixels (allocation bound).
- Decompressed payload MUST NOT exceed
  `tile_count * (1 + 5 + 16*16*3)` = `tile_count * 774` bytes
  (zip-bomb bound; worst-case record = mode + idx + transform + full data).

## 2. Payload

payload_len bytes of zlib stream (RFC 1950). Decompressed content is
exactly `tile_count` consecutive tile records, concatenated with no framing:

    record := literal | exact_ref | geom_ref
    literal    := u8 mode=0                       tile_bytes(n)
    exact_ref  := u8 mode=1 u32_le prior_index    tile_bytes(n)
    geom_ref   := u8 mode=2 u32_le prior_index u8 transform    tile_bytes(n)

    n := 3 * tw * th   (tw, th = tile dimensions at this grid position,
                        each <= 16, smaller at right/bottom edges)

`tile_bytes` is row-major RGB8, row-major within the tile.

Tile positions: tile `i` sits at grid row `floor(i / cols)`, column
`i mod cols`, where `cols = ceil(width / 16)`.

## 3. Reference semantics

- `prior_index` MUST be `< i` (the index of the record being decoded) and
  MUST point at an already-decoded tile. Self- and forward references are
  invalid.
- The referenced tile MUST have shape exactly `(tw, th)` — equal shape, not
  merely equal byte length.
- For `geom_ref`, `transform` is a D8 identifier (Section 4) and MUST be
  member of the shape-keeping subgroup for this tile:
  all 8 transforms are valid iff `tw == th`; otherwise only 0..=3.
- Decoding:
      base = transform == 0 ? ref_pixels : apply_transform(transform, ref_pixels)
      pixels = (base + residual) mod 256        (bytewise)
  Literal decoding is the identity: `pixels = tile_bytes`.
- All additions/subtractions on samples are in the ring Z/256 — this is
  what makes the residual lossless regardless of prediction quality.

## 4. Transform identifiers (D8, opt-in as of v0.6.0)

Pull maps — output pixel `(x, y)` samples source `(sx, sy)`:

    0  identity        sx=x                  sy=y
    1  flip horizontal sx = w-1-x            sy=y
    2  flip vertical   sx=x                  sy=h-1-y
    3  rotate 180      sx=w-1-x              sy=h-1-y
    4  transpose+      sx=y                  sy=h-1-x     (rot90 ccw family)
    5  T1 after T4     composite
    6  T3 after T4     composite (= inverse of 4 in this labeling)
    7  T1 after T3 after T4   composite

For 4..=7 the intermediate shape is `(h, w)`. `apply_transform` is used on
both encode and decode sides — maps are exact permutations, no
interpolation, so any consistent map is lossless by construction. Golden
vectors pin the exact composition semantics (Section 7); do not relabel
transform ids without bumping the format version.

## 5. Cost model (encoder-side only; informative)

Not normative for decoding, but the reference encoder decides literal vs
reference by a per-tile local zlib proxy: literal pays `1 + zlib(data)`;
a reference pays `(5, or 6 if transform != 0) + zlib(residual)`. The proxy
is not the true marginal cost in the final combined zlib stream, and the
atlas can legitimately lose to deflate's own LZ77 matching (e.g. constant
tiles). No global optimality is claimed.

## 6. Conformance vectors

`tests/conformance/` contains byte-exact vectors with expected pixels;
`tests/conformance.rs` asserts decode-exactness and rejection lists:

- `solid_32x16.mnf5` — two solid tiles, second is an exact reference.
- `rej_*.mnf5` — rejection vectors (bad magic, dangling reference, shape
  mismatch, invalid transform on non-square tile, truncated stream,
  trailing garbage, oversized dimensions, zero dimensions).

Deterministic rejection: each vector must be rejected, and must not panic,
regardless of its zlib payload.

## 7. Versioning

Any semantic change — new mode, new transform labeling, new header field,
relaxed/tightened validation — requires a format-version decision and a
fresh set of golden vectors. MNF5 is currently a research format; v0.5-
layout files and v0.6 `--geom` files share this document with mode 2
gated behind encoder options. The packaged v0.5 decoder subset accepts
modes 0/1 only.

---

# MNF6 Bitstream Format — Normative Specification (format v2)

Status: implemented in v0.7+ of the `manifold` crate (`src/mnf6.rs`),
rate-aware encoder, magic-routed decoder. MNF5 remains current; MNF6 files
are produced only via `encode --mnf6`.

## 1. Container

    file := header(28) payload(payload_len)

    0   4   magic            ASCII "MNF6" exactly
    4   4   width            u32 LE, > 0
    8   4   height           u32 LE, > 0
    12  4   tile_size        u32 LE, MUST be 16
    16  4   tile_count       u32 LE, MUST equal ceil(width/16)*ceil(height/16)
    20  4   payload_len      u32 LE, MUST be <= 2^31 - 1
    24  4   flags            u32 LE bitmask (Section 2)

Validation order and bounds are those of MNF5 (docs/format.md §1) plus:
total file length MUST be exactly `28 + payload_len`. Decompressed payload
MUST NOT exceed `tile_count * (1 + 5 + 768 + 2)` bytes.

## 2. Flags

    bit 0 = PLANAR   mode 0 literal bodies and mode 1/2 residual payloads
                     carry planar [R][G][B] planes (n bytes each) instead
                     of interleaved RGB
    bit 1 = ZSTD     payload is a zstd frame (with xxh64 checksum) instead
                     of a zlib stream
    bits 2..31       reserved; MUST be 0 on write, rejected on read

The encoder MEASURES all four flag variants (v0.8 rate-aware selection)
and emits the smallest final stream; the decoder dispatches on bits only.

## 3. Records

    record := literal | exact_ref | geom_ref | ycocg_ref
    literal    := u8 mode=0  body(n, layout)
    exact_ref  := u8 mode=1  u32_le prior  body(n, layout)
    geom_ref   := u8 mode=2  u32_le prior  u8 transform  body(n, layout)
    ycocg_ref  := u8 mode=3  Y plane (n/3 bytes) | co9 plane | cg9 plane
                 (co9/cg9 are ceil(9*(n/3)/8) bytes, LSB-first packed,
                  9 bits per biased sample, co/cg stored +256)

    body(n, layout) := interleaved RGB or planar per flags.bit0

Reference semantics (order, shape, D8 transforms) are exactly MNF5 §3.
Mode 3 decodes via YCoCg-R:
    tmp = y - (cg >> 1); g = tmp + cg; b = tmp - (co >> 1);
    r = tmp + co - (co >> 1)         (co, cg in biased signed form)
Y plane carries 1 byte per pixel — MNF5's spec draft wrongly said n bytes;
n counts RGB bytes throughout both specs (MNF6 decoder bug found and fixed
by differential fuzzing: a 2497-byte mode-3 record was being read as
2*n + 2*n9).

Mode 3 is layout-independent (always bit-packed, whatever flags say).

## 4. Versioning

MNF6 = MNF5 semantics + flags word + mode 3. Any further semantic change
requires a new magic. The zstd requirement includes framed checksums:
checksum-less zstd accepted bit-flip corruptions in fuzzing (3 found,
all now reject on checksum).


---

# Format freeze (v1.0, 2026-10-25)

MNF5 (24-byte header, modes 0-2 with mode 2 opt-in) and MNF6 (28-byte
header, modes 0-3, flags PLANAR|ZSTD) are frozen as of tag v1.0.0:
- No semantic change without a new magic ("MNF7").
- Golden + rejection vectors under tests/conformance/ are the
  conformance set; every future decoder must reproduce them exactly.
- Independent decoder: src/refdecode.rs (MNF5, push-map D8, spec-order
  validation) validated against the primary decoder by
  tests/differential.rs (verdict + pixel agreement on goldens, 14
  rejection vectors, and the full corpus across encoding arms).
- Both decoders bounded (zip-bomb take(limit+1)), checksummed
  (zlib Adler32 inherent; MNF6 zstd xxh64 mandatory), panic-free under
  scripts/fuzz.py.


---

# MNF7 Bitstream Format — Normative Specification (format v3, ADR 2026-10-25)

Status: implemented (`--mnf6` emits MNF7; superset of MNF6). MNF5/MNF6
files remain readable by their own decoders; MNF7 decoder accepts only
`MNF7` magic.

## Decision (ADR): modular RGB8 lifting, standard YCoCg-R deferred

- Canonical pixel representation stays RGB8 everywhere outside mode 3.
- Added: reversible MODULAR 8-bit color transform (u8 wrapping lifting,
  algebraically NOT standard signed YCoCg-R; a distinct bijection over
  RGB8 triples). Reference residuals may be coded in this domain:
  mode 4 = exact reference, color-domain residual
  (record `[4][u32 prior][body]`)
  mode 5 = geometric reference, color-domain residual
  (record `[5][u32 prior][u8 transform][body]`)
- Decode: base_T = T(G_t(P_hat)); pixels = T^-1(base_T + body mod 256),
  where body layout follows flags bit0 (planar or interleaved).
- Atlas patches remain canonical RGB8 (ADR integration rule).
- Standard signed YCoCg-R remains available as MNF6 mode 3 (9-bit
  bit-packed chroma) — NOT truncated to 8 bits (that would break
  reversibility; rejected per the ADR alternatives table).

## Reversibility proof obligation

modcolor forward/inverse are exact inverses: verified exhaustively over
a stride-17 sweep of all RGB8 triples plus corner blocks (tests:
mnf7_modcolor_is_bijective_exhaustive). Any change to the lifting MUST
re-run this proof before a new tag.

## Selection semantics

Encoder cost model evaluates, per candidate reference, BOTH domains:
RGB residual cost vs color-domain residual cost + 1 mode overhead; the
cheaper wins. Color selection is per-tile; stats counters
(ref_exact_color / ref_geom_color) expose utilization.

## Benchmarked (ADR shipping gate)

Measured across dev + held-out corpora (10 images): color-domain
references were selected 0 times on the synthetic/natural corpus — the
RGB-domain residual already captures the correlation structure there.
The mechanism is exercised by a dedicated structured-color-delta test
and proven exact; adoption remains cost-based selection, so MNF7 costs
at most 4 extra mode bytes vs MNF6 on streams that never choose it.
