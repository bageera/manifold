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


---

# MNF8 Bitstream Format — Normative Specification (format v4, ADR-2026-10-25)

Status: implemented (`--mnf8[=balanced|maximum]` emits MNF8; superset of
MNF7). MNF5/MNF6/MNF7 files remain readable by their own decoders; the
MNF8 decoder accepts only the `MNF8` magic. ADRs: docs/adr-002-entropy-backends.md
(backend field), docs/mnf8-adr-003-tokens.md (FLAG_TOKENS).

## 1. Container

    file := header(32) payload(payload_len)

    0   4   magic            ASCII "MNF8" exactly
    4   4   width            u32 LE, > 0
    8   4   height           u32 LE, > 0
    12  4   tile_size        u32 LE, MUST be 16
    16  4   tile_count       u32 LE, MUST equal ceil(width/16)*ceil(height/16)
    20  4   payload_len      u32 LE, MUST be <= 2^31 - 1
    24  4   flags            u32 LE bitmask — §3
    28  4   backend          u32 LE identifier (§2)

Validation order and bounds are those of MNF5/MNF6 (docs/format.md §1), plus
the mode-3-aware decompression bound; when FLAG_TOKENS is set the bound is
the token path's own worst case (§3.1). Total file length MUST be exactly
`32 + payload_len`.

## 2. Backend identifier

    0 = zlib-9     payload is a zlib stream (control arm; MNF7 flag-free parity)
    1 = zstd-19    payload is a zstd frame (xxh64 checksum MANDATORY — encoder
                   side; the decoder rejects checksum failures)
    2 = RESERVED   brotli prototype — unshipped (the static dictionary's
                   cost was accounted only in the study harness; the corpus
                   gate moved zstd/brotli within measurement error).
                   Decoders MUST reject.
    3 = RESERVED   custom mixed run/rANS coder — NOT shipped; decoders MUST
                   reject. Reserved for the mnf-mans prototype pending the
                   retention gate (docs/entropy-backends-v13.md stage 2).
    4 = mix        the dual-timescale adaptive range coder (MANF13-R5,
                   v1.13.0; docs/mnf13-r5.md): payload = [u32 stream_len]
                   + LE u32 words; per-context fast/slow fixed-point
                   (2^24-scale) estimators with a log-space EWA weight
                   updated from the decoded symbol (zero model transport;
                   contexts = the previous coded byte's 8-bucket
                   magnitude); decode rejects unaligned payloads, an
                   over-limit stream length, and truncations with errors.
    other          decoders MUST reject (unknown backend)

## 3. Records and flags

Flags bitmask (MNF8 superset — MNF6/7 keep bits 2..31 frozen, MNF8 owns
bits 2-5 by ADR-002/ADR-003/ADR-004):

    bit 0  FLAG_PLANAR    channel-planar record bodies (MNF6/7 semantics)
    bit 1  FLAG_ZSTD      zstd container (MNF7 semantics; MNF8 decodes by
                          the backend field instead — bit 1 is accepted
                          for MNF7-record parity and MUST be 0 in MNF8
                          files written by this encoder)
    bit 2  FLAG_PRED_MED  mode-0 literal bodies carry image-wide MED
                          residuals (decode inverts causally)
    bit 3  FLAG_TOKENS    payload is NOT a record stream — §3.1
    bit 4  FLAG_PRED_PLANE mode-0 literal bodies carry image-wide
                          LOCO-I-plane residuals (a+b−cc, clamped) — §3.2
    bit 5  FLAG_PRED_LEFT  mode-0 literal bodies carry image-wide LEFT
                          residuals — §3.2
    bits 6..31             RESERVED — decoders MUST reject

At most ONE predictor bit {PRED_MED, PRED_PLANE, PRED_LEFT} may be set;
multiple are rejected. TOKENS implies MED (its payload IS the MED
residual wire): TOKENS+{PLANE,LEFT} rejected. Without FLAG_TOKENS,
records are EXACTLY MNF7's; the record walker is shared with the MNF7
decoder. PLANAR and TOKENS are mutually exclusive.

### 3.1 FLAG_TOKENS payload (ADR-003, 2026-10-28)

Payload := container-codec(token_wire). The wire encodes the image-wide
MED residual plane, `R[i] = (pixels[i] - MED[i]) mod 256`, interleaved
RGB8 row-major, MED per docs/format.md §MNF7 predictor definition
(first pixel 127, top row = left, left col = above, interior =
edge-sense a+b−cc branch).

    tag 0 literal:  0x00, b, g, r          (1 + 3 B; one residual pixel)
    tag 1 match:    0x01, dx:u16, dy:u16, len:u8
                                          (1 + 5 B; len >= 7 pixels)
    tag 2 near:     0x02, dx:u8, dy:u8, len:u8
                                          (1 + 3 B; len >= 7 pixels;
                                          legal iff dx < 256 AND dy < 256)
    dist = dy*width + dx pixels; the match source starts dist pixels
    earlier in the plane; copies are byte-forward (overlapping copies
    are exact LZ77 self-referential runs). len counts PIXELS. Tag 2 is
    the narrow-coordinate form of tag 1 (ADR-005) — the encoder emits
    it for any match whose dx and dy both fit a byte, preferring nearer
    candidates on equal length; the decoder dispatches by tag and
    applies the same range/bounds checks as tag 1.

Decoders MUST verify: unknown tags → reject; truncation → reject;
len < 7 → reject; dist == 0 or source beyond the decode cursor → reject;
any overrun of the w*h*3 plane or a final size mismatch → reject; the
wire MUST consume exactly the decompressed payload (trailing bytes are
structurally impossible — a literal overruns the plane — and therefore
reject). Reconstruction: rebuild R, then invert causally in raster
order: pixels[i] = (R[i] + MED(causal reconstructed neighbors)) mod 256 —
exact by the mod-256 induction (reconstructed earlier pixels are the
true ones), identical to the FLAG_PRED_MED in-walker inversion.

Decompression bound for token payloads: 4 * width * height bytes (worst
case = an all-literal wire at 4 B/px; this EXCEEDS the record bound on
large images, so token files do not inherit the record bound — the
mode-3 lesson).

Conformance: tests/conformance/mnf8_tokens_gradient_64x64.mnf8 (golden,
byte-exact) + rejections rej_mnf8_tokens_planar.mnf8 (PLANAR|TOKENS) and
rej_mnf8_tokens_flag.mnf8 (reserved bit 4).

### 3.2 FLAG_PRED_PLANE / FLAG_PRED_LEFT payloads (ADR-004, 2026-10-28)

Record layout is MNF7's; mode-0 literal bodies carry the image-wide
residual of the SELECTED predictor instead of MED's:

    PRED_PLANE: pred(x,y) = clamp(a + b − cc, 0, 255)
    PRED_LEFT:  pred(x,y) = a        (the left neighbor)

with (a, b, cc) = (left, above, above-left) causal neighbors, edges
identical to the PRED_MED walk: pixel (0,0) predicts 127; the top row
predicts from the left; the left column predicts from above. These
files are PURE prediction streams — every record is mode 0 (the
measured winning form; the ADR-004 decoder rejects any other mode byte
under these flags). Inversion is the same causal walk as PRED_MED's,
exact by the mod-256 identity.

Encode-side neighbor reads are from TRUE pixels; decode-side from
reconstructed pixels — the mod-256 induction makes them identical
(reconstructed-earlier == true). Reading the PREDICTION stream on the
encode side would chain predictions and break exactness (the bug the
gradient gate caught; the rule is normative here).

Conformance: tests/conformance/mnf8_branch_left_poster.mnf8 (golden) +
rejections rej_mnf8_two_predictors.mnf8 (MED|PLANE) and
rej_mnf8_tokens_left.mnf8 (TOKENS|LEFT).

### 3.3 REF field — channel-diff reference (ADR-008, v1.11 bit 6 generalized v1.12)

Bits 6-7 form a field REF = (flags >> 6) & 0b11:

    0 = none     no channel-diff organization (bodies are plain)
    1 = G        per-pixel [G, R−G, B−G]  (v1.11's FLAG_CHANNEL_DIFF
                 bit-6 semantic; every old CD file is this shape)
    2 = R        per-pixel [R, G−R, B−R]
    3 = B        per-pixel [B, R−B, G−B]

All differences are mod 256. REF != 0 relabels mode-0 record bodies and
token tag-0 literals (planar bodies relabel per-plane in wire emission
order: [ref plane][first-diff plane][second-diff plane]); every other
record mode is absent from CD files. Decode applies the per-pixel
inverse BEFORE the predictor/token walk; exactness is the mod-256
induction. REF-composed arms compete inside the encoder pick at complete
file size (never-worse by construction; the wire adds no metadata).

Compatibility contract (verified against both released binaries):

    REF      v1.11 decoder      v1.12 decoder
    00       decode             decode
    01       decode             decode
    10       reject (reserved)  decode
    11       reject (reserved)  decode

(v1.11's reserved-mask rule rejected any bit-7 set; v1.12 owns bit 7 and
reserves bits 8..31 — rej_mnf8_reserved_bit8.mnf8 is the new boundary
vector; the old bit-7 reject file was REF=11's exact shape and would
decode wrongly under v1.11's replacement.)

Conformance: mnf8_cdiff_slate64.mnf8 (REF=1, v1.11 golden, unchanged) +
mnf8_ref2_screenshot.mnf8 (REF=2, token wire) +
mnf8_ref3_terrain160.mnf8 (REF=3, planar records) + the rejects above.

## 4. Encoder profiles (informative; not a wire property)

- balanced (default): zstd-19 pinned; flags measured as MNF7 measures
  (planar vs interleaved under zstd) PLUS the ADR-003 token arm
  (PRED_MED|TOKENS) and ADR-004's Plane/Left pure predictor arms, all
  competing by final file size. The winner's flags id is recorded in
  the stats (`chosen_flags`).
- Arm controls for ablations: `use_atlas` (references at all) and
  `use_geom` (D8 transforms; false = exact references only, t=0,
  codec.rs v0.5 semantics). Shared YCoCg mode 3 and the planar layout
  flag stay available in all arms.
- maximum: builds the tile stream under each backend x flags candidate,
  re-costing per-tile mode selection through that candidate's own coder
  (stage-2 evidence: selection is backend-dependent), and emits the
  smallest file. Measured never worse than MNF7 + 4 bytes (the backend
  field's own wire cost) — an EMPIRICAL gate on stable corpus files
  (the two encoders' candidate sets differ; never-loses is measured,
  not structural).

## 5. Versioning

MNF8 = MNF7 semantics + 4-byte backend field (new magic per the freeze
rule), extended by FLAG_TOKENS (§3.1; ADR-003) and the branch-predictor
flags (§3.2; ADR-004) without a magic change — the token payload needs
no new record grammar, and the plane/left predictors need only flag
bits on unchanged records. Conformance vectors under tests/conformance/
are extended with MNF8 golden + rejection vectors (unknown backend,
reserved backend, reserved flag bit, bad magic) plus the token set
(golden mnf8_tokens_gradient, PLANAR|TOKENS and reserved-bit
rejections) and the branch set (golden mnf8_branch_left_poster,
two-predictors and tokens+left rejections).

---

# MNF9 — exact generative whole-frame format (ADR-007, v1.10.0)

## 1. Header (32 B; field offsets mirror MNF8)

    0   4   magic           ASCII "MNF9" exactly
    4   4   width           u32 LE
    8   4   height          u32 LE
    12  4   reserved        MUST be 0 on write, rejected on read
    16  4   reserved        MUST be 0 on write, rejected on read
    20  4   payload_len     u32 LE; file total = 32 + payload_len
    24  4   flags           MUST be 0 (no flag grammar in v1.10.0)
    28  4   backend         MUST be 0 (no container in this format)

## 2. Payload: one parametric whole-frame record

    u8 model_id; then the model's parameters (LE):

    1 constant        r,g,b                                             (4 B total)
    2 div-1D          per channel: i32 N, i32 T; u8 D-index last        (26 B)
    3 rec-1           per channel: u8 a0, i16 d; u8 orientation          (11 B)
    4 bilinear joint  per channel: corners c00,c10,c01,c11 (u8 x4)      (13 B)
    5 bilinear nested corners as id 5 (nested round-lerp, half-up)      (13 B)
    6 piecewise       u8 nseg (1..=32), (nseg-1) x u16 boundary,        (var)
                      then per segmentxchannel: i32 N, i32 T, u8 D-idx
    0 and 7..=255     reserved - reject (Error, never panic)

## 3. Normative arithmetic (the decoder IS the equation)

- div form: N*x + T evaluated in i64 per channel x = pixel column
  (piecewise: segment-local x restarting at each boundary); divided by
  D (from the convention table, or the segment-relative form: index 3 =
  segment length - 1, index 4 = segment length) using FLOOR division
  (div_euclid with D > 0 strictly - the negative-direction lerp rule).
  Any computed component outside 0..=255 -> file invalid.
- rec-1: value_k = (a0 + d*k) mod 256 along the scan direction
  (orientation 0 = rows, 1 = columns); all scan lines are identical by
  construction.
- bilinear joint: (c00*(W-1-x)*(H-1-y) + c10*x*(H-1-y) + c01*(W-1-x)*y
  + c11*x*y) / ((W-1)*(H-1)) floored; W=1 or H=1 rejected.
- bilinear nested: per channel, top/bottom round-lerps
  (c00->c10, c01->c11 with the +floor((W-1)/2) half-up offset), then the
  y-round-lerp between those two rows (+floor((H-1)/2)); W=1 or H=1
  rejected.

## 4. Denominator convention table

    idx 0 = 1, 1 = 2, 2 = 4, 3 = W-1, 4 = W, 5 = 2(W-1), 6 = 256, 7 = 65536

(Inside piecewise records, indices 3/4 are segment-relative.)

## 5. Encoder contract

`mnf9::encode_exact` emits a record ONLY when the rule reproduces every
pixel exactly (zero residual, verified by real decode inside the
encoder). The CLI --mnf8 flow measures the MNF9 file at final size
against every MNF8 arm and writes whichever candidate won - the output's
magic names the format (never assume). Malformed MNF9 input: Error,
never panic (reserved ids, truncated params, out-of-table D index,
piecewise boundary >= W, degenerate dims, bad magic, reserved words set,
payload-length mismatch).
