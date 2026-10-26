# ADR-002 — MNF8 Entropy Backend and Residual Probability Modeling

Status: ACCEPTED (2026-10-25) — implemented in v1.2.0. Evidence: docs/entropy-backends-v13.md
stages 1+2; docs/atlas-eval-v12.md; crates/mnf-mans (mixed coder gate).

## Decision

MNF8 = MNF7 + a backend-identifier field + per-image encoder selection
(`balanced` / `maximum` profiles). New magic "MNF8"; MNF5/6/7 decoders and
files remain untouched (per README §9 versioning rule: new semantic field =>
new magic).

### Header

    file := header(32) payload(payload_len)
    header := "MNF8", u32 LE width, height, tile=16, tile_count,
              payload_len, u32 flags(existing MNF7 semantics),
              u32 backend

    backend := 0 = zlib-9 (MNF7-compatible control)
               1 = zstd-19 (checksummed frame, as MNF7's FLAG_ZSTD stream but
                   level pinned and window bounded ≤ 27)
               2 = brotli-9 (dictionary OFF for size-accounting honesty;
                   cost = the 122 kB dictionary ships with the decoder, not
                   the file)
               3 = reserved (custom mixed run/rANS coder — lands only if the
                   constriction prototype passes its gate after the context
                   work; see "Custom coder status" below)

    Decoder MUST validate backend ∈ {0,1,2} (3 reserved → reject), enforce
    per-backend output limits (zlib: MNF7 bound; zstd: take(limit) +
    include_checksum; brotli: bounded output stream), and reject unknown
    values exactly like reserved flags.

### Profiles (encoder policy, not wire format)

- `balanced` (default): measure NOTHING extra; emit zstd-19. Stage-1/2
  evidence: never lost to zlib-9 in the matrix (10/10).
- `maximum`: build the tile stream under each backend candidate
  (mode selection re-costed per backend — stage-2 evidence shows selection
  is backend-dependent), emit the smallest, selector included. Encoder-side
  only; the wire format carries just the winning backend id.

### planar flag

The stage-2 planar measurements are content-dependent (−28% gradient / +5%
blob) and re-use MNF7's existing FLAG_PLANAR under the new selection loop —
no new bit: the `maximum` profile measures planar vs interleaved per image,
exactly like backend per backend.

## Evidence summary (retention gate inputs)

| arm | corpus verdict (10 imgs) |
|---|---|
| zstd-19 frozen streams | ≤ zlib-9 10/10; −68% best (screenshot-med) |
| integrated selection | zstd/brotli beat zlib 9/10; xz wins plaid only (stays archival) |
| planar | wins gradient −28%, loses blob +5% |
| mixed custom coder v0.1 | beats zlib-9 8/10; loses to zstd-19 8/10 |

## Custom coder status (honest)

The mnf-mans v0.1 prototype (structural LZ + order-0 class/context + rANS
machinery proven via constriction roundtrip) beats zlib-9 on 8/10 corpus
streams but loses to zstd-19 on 8/10: the class/context models are too
shallow (no prev-class chaining, no per-tile context carry, static vs
adaptive). Backend id 3 stays RESERVED — the prototype does not pass the
"should replace a generic backend" bar yet. Next steps if pursued: adaptive
context mixing (the chain-rule lever from docs/science.md §5), per-block
model transport amortization, then the same 10-image + held-out gate.

## Consequences

- New magic MNF8; v1.2.0 per the minor-version-per-semantic-addition policy.
- Frozen formats keep decoding via their own magics — no compatibility break.
- Every backend change re-runs the corpus + held-out gates (bench.py +
  entropy_backends.py) before a tag.
- brotli cost accounting includes its static dictionary at the DECODER
  artifact level (documented; not in per-file sizes).
