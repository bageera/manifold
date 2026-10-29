# GSE-02: hierarchical MDL mathematical byte-stream codec — gate record

Status: research instrument committed (`crates/gse02/`, workspace-excluded,
publish = false). 2026-10-28. Rust implementation of Jon's GSE-02 design;
the acceptance-matrix gate + three-source separation executed and
recorded here. This is an independent byte-stream codec sharing the MDL
methodology, not the image format.

## Architecture (per the GSE-02 spec)

- **Models** (decoder-reproducible, deterministic integer arithmetic):
  Const(x); Arith{a,b} = `(a + b·i) mod 256` — the GSE-01 class covering
  constant-step recurrences; Periodic{phase, residues} = `p[(i+phase) mod k]`
  for exact periods, with a container-coded residual stream = the
  near-mathematical arm; Raw = literal fallback (never-worse arm).
- **Hierarchical MDL**: DP over 64-aligned cuts with Jon's block sizes
  {64, 256, 1024, 4096} plus the whole-stream fit; shared model table =
  segments reference an equation by id (one equation serves many blocks);
  every cost = complete serialized bytes.
- **Wire**: 12-B header (magic `GSE2`, orig len, container len) +
  segments (len u16 + model id | RAW marker; raw inline) + model table +
  residual stream, through one zstd-19 container pass. Segment ids carry
  a has-residual high bit.
- **Decoder**: independent, exact, error-not-panic (magic, truncations,
  zero-len segments, out-of-range/unflagged model ids, raw-length
  mismatch, residual-length accounting, unreferenced payloads, length
  mismatch, trailing bytes (the header field = the CONTAINER length),
  expansion bomb).

## The three-source separation (Jon's §4 requirement)

| source | measured |
|---|---|
| Generator advantage | arithmetic 288 → **42** (−246); near-arith: LOSES (+62 to zstd-19 — its range coder models the noise better than generator+residual) |
| Partition advantage | arith+random 33,088 → **32,866** (−222); alternating 22,246 → **21,945** (−301) — segmentation + per-region models win |
| Shared-model advantage | shared-rec 304 → **176** (−128): three arith segments, one shared equation, one params entry |

## Acceptance matrix (complete serialized, framed both sides)

| dataset | A: framed zstd-19 | GSE-02 | delta |
|---|---|---|---|
| arithmetic | 288 | **42** | −246 |
| constant | **30** | 41 | +11 |
| ACGT | **33** | 46 | +13 |
| random | **65,557** | 65,631 | +74 |
| arith+random | 33,088 | **32,866** | −222 |
| alternating | 22,246 | **21,945** | −301 |
| shared-rec | 304 | **176** | −128 |
| near-arith | **48,198** | 48,260 | +62 |

**GSE beats complete-framed zstd on 4/8**; decode verified per dataset
(gate example asserts round-trip on every row). Tests: 4/4 (round-trip
all fixtures, GSE-01 class < 200 B, malformed-reject set, honest random
overhead bound). clippy `-D warnings` clean.

## Honest readings

1. The **generator** and **partition** sources produce real wins on
   structured synthetic data; the **shared-model** source produced a
   −128 B measured saving (the biggest per-parameter-set win).
2. Fixed framing loses on tiny-structure fixtures (constant: zstd-19's
   own machinery reaches 30 B; period-4 ACGT: 46 vs 33) and on
   incompressible data (+74 framing). The wire is not production; the
   DP's shared-table accounting still overestimates multi-block params
   (fit_block charges params per block; serialize dedupes) — recorded as
   a known limitation: the DP may miss segmentations whose advantage
   exists only under shared-cost accounting.
3. No natural/held-out data measured yet — the acceptance matrix is
   synthetic by design (Jon's GSE-03 lesson: natural-generalization
   needs its own gate with real binary/database/image/audio data).
4. GSE-03's Python results (masks-vs-positions, the 15-B
   shared-affine saving) are consistent with this instrument's shared-model
   column.

## Relation to the Manifold program

- The X1 lesson is honored structurally: models compose inside one
  grammar; a model wins only by complete-file cost; the decomposition
  (generator vs partition vs shared) is measured via separate fixture
  classes, not read off one number.
- MNF9's framing-economy lesson shows up again: exact models win where
  the whole structure fits one record (−246), and lose wherever fixed
  framing exceeds the structure's savings.
- The Riemann-chart arm (`x_i = F_θ(φ_b(i)) + r_i`, coordinate-mapped
  shared generators) remains a separate future arm per Jon's
  recommendation — gated behind the periodic/recurrence/shared evidence
  that now exists in measured form.