# GSE region wire format — frozen v1.1 (2026-10-30)

Shared storage format for the GSE-09/GSE-10 chain (GSE-10 retains this
format as its starting point). This document freezes the format BEFORE
GSE-10 implementation; any change requires a version bump and a new
gate record.

## Container

Wire stream (below) passes through one zstd-19 container pass. All
size claims in measured tables = container output size (complete
serialized cost). The zstd-19 comparator applies `zstd --ultra -19` to
the raw fixture bytes with identical framing policy on both sides.

## Stream structure (self-describing, pre-order quadtree)

Internal node:  type byte 0x03        (partition: 4 children follow
in order TL, TR, BL, BR; no offsets — stream order is the structure).

Leaf tiles:
- 0x00 CONST:  1 B value; represents s*s cells.
- 0x01 PLANAR: 3 B params (A, B, C), varint exception count k,
               then k entries of (varint row-major position within
               tile, 1 B replacement value).
- 0x02 RAW:    s**2 literal bytes, row-major.

Type byte 0x03 on a leaf-sized tile is legal (partition to min size).
Positions inside one tile are unique and < s*s (decoder rejects
duplicates and out-of-tile positions).

## Model semantics (normative)

- PLANAR: value(r, c) = (A + B*r_local + C*c_local) mod 256, with
  r_local/c_local counted from the TILE ORIGIN. Params are canonical
  (mod-256). Fitting uses 2-point differences per the committed
  procedure; verification must mask with & 0xFF before compare (a
  raw-integer compare silently rejects wrapped generators).
- CONST: value = V everywhere.
- RAW: no model.

## Exception contract (v1.1 normative amendments)

- The wire stores the REPLACEMENT BYTE v_p — never a delta of any
  modulus.
- Evaluation derives corrections at query time:
  S(Q) = S_M(Q) + sum over p in (E ∩ Q) of (v_p − M_theta(p)),
  computed in SIGNED integer arithmetic AFTER the generator's
  mod-256 reduction. (M=250, v=0 gives correction −250; storing +6
  is a spec violation.)
- Exception entries inside one tile may be buffered by the decoder
  before applying (ordered fill: generator plane first, replacements
  second). Overlap across tiles follows traversal order (later tiles
  do not overlap earlier ones by construction of the quadtree).

## Analytical evaluation contract (GSE-10)

- Linear carry accounting: S_M for a contiguous window uses the
  closed form nA + B(ns + n(n−1)/2) − 256·(wrap count), wrap count
  computed by the harmonic floor-sum algorithm; mathematical floor
  division (never truncation toward zero) for negative coefficients.
- Periodic: prefix table over one period; arbitrary start offset via
  phase = start mod k.
- All intermediate arithmetic in checked signed integers (i64:
  worst-case planar term at 256² side ~8.4e6, no overflow risk;
  bounds validated before evaluation, not assumed from fixtures).
- Query cost target: O(rows + exceptions-intersecting-Q), never
  O(pixels). Exception lookup must be spatially indexed (sorted
  positions with row offsets, or row buckets with cumulative
  corrections); a generic spatial tree is explicitly out of scope
  until measurements justify one.

## Decoder requirements

- Independent path: the decoder does NOT reuse encoder working state.
- Malformed input returns an error, never panics: truncation, unknown
  type byte, duplicate or out-of-tile exception positions, varint
  overflow (>4 continuation bytes), trailing bytes after the final
  tile, raw body overrunning the stream.
- The decoder's structural walk IS the independent oracle entry for
  GQ3: brute-force region sums are computed over GRID FILLED BY
  DECODE, not over encoder-side working data.

## Reproduction fixtures (committed)

Seed 9, both --side 128 and 256; six families: constant,
mathematical, sparse edits (80/frame at 128, 80 at 256 committed
family density), mixed (checkerboard structured/random 16x16
super-tiles), smooth noisy (planar ±4), uniform random. Numbers in
results-*-reproduction.json are NEW measurements by
prototype_reproduction.py, not regenerations of the original GSE-09
session (see gate-record.md provenance section).

## Known measured limitation (carried into GSE-10 design)

Per-tile exception overhead dominates when edits scatter across tile
boundaries (sparse_edits row LOSES at both sizes in the
reproduction): exception payloads must be charged at the granularity
the query layer indexes them at. A whole-frame exception table
competes as an alternative arm in GSE-10's storage matrix — not a
format change, an arm addition at the MDL selection layer.