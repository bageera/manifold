# ADR-003 — MNF8 FLAG_TOKENS: LZ77-2D token payload (status: ACCEPTED, 2026-10-28)

## Context

The engine ladder probe (docs/mnf9-engine-ladder-probe.md, 2026-10-27)
retained exactly one engine candidate: LZ77-2D tokens over the image-wide
MED residual plane, as a rate-aware per-file pick. The Rust port +
measurement record (docs/mnf9-token-arm.md) confirmed the ladder numbers
with a real decoder. This ADR decides the format step: where the arm
ships, what the wire is, and what stays out.

## Decision

**FLAG_TOKENS = bit 3 in the MNF8 flags field** (MNF8 owns bits 2-3;
MNF6/7 masks stay frozen). Not a new MNF9 magic:

- The token payload needs no new record grammar — the container, backend
  field, limits struct, and header are MNF8's, so a new magic would buy
  nothing but churn.
- MNF9 is RESERVED for the step that actually needs new record grammar
  (the shared-chart/geometry family); burning the new magic on the first
  engine would leave the chart step with no clean format slot.
- The rANS engine (reserved backend id 3) composes freely with
  FLAG_TOKENS later: payload = token wire, backend = 3. Both are
  orthogonal axes already on the wire.

## Wire grammar (bit-exact with the measured prototype)

Payload = `encode_plane` output of the MED residual plane
(`pixels - med_predict(pixels)` wrapping, interleaved RGB8), then the
container coder (backend field applies as usual):

- tag 0 literal: 1 + 3 B (one residual pixel)
- tag 1 match: 1 + 2 + 2 + 1 B (dx u16 le, dy u16 le, len px, len >= 7)
- dist = dy*width + dx pixels; sources are strictly earlier; copies are
  byte-forward (overlapping self-referential runs exact)
- matcher (encoder-side only): 3-byte pixel-key hash chains, newest-first
  scan, 16-candidate cap, 64k-px window, <= 85-px matches

Decode = container-decompress → structural wire validation (`tokens::
decode_plane`: unknown tags / truncation / below-minimum / out-of-range
sources / plane-size mismatch are Error, never panic) → causal MED
invert (raster order, `med_predict_pixel` — identical progression to the
encode-side predictor, exactness by the mod-256 induction, same as
FLAG_PRED_MED's in-walker inversion).

**PLANAR ∥ TOKENS are exclusive** (the token plane is interleaved by
definition); decode rejects the combination. TOKENS implies MED on this
payload (the encoder emits PRED_MED|TOKENS together); a decode of
TOKENS without PRED_MED is legal (the payload shape is what matters).
The token flag has its OWN decompression bound: 4*width*height (worst
case is an all-literal wire at 4 B/px, which exceeds the record bound on
large images — the mode-3 lesson).

## Selection rule (the retained contract)

Every backend candidate (Balanced: zstd-19; Maximum: zstd-19 + zlib)
builds the token arm once (PRED_MED|TOKENS) and competes by final file
size against that backend's four record arms. Token wins only when it
measures smaller — the pick is never worse than the record pick by
construction.

## Measured basis (docs/mnf9-token-arm.md, 30-file corpus, flate2-z9 parity)

Token arm wins 5/30 (gradient −77%, solid −81%, screenshot −42%,
repeat −49%, blocks −15%), zero matches on all 20 photo residual planes,
rate-aware pick TOTAL 0.997× literal. CLI-gated in-repo post-integration:
gradient 1639→417 (−74.6%, flags 0x0c), solid/screenshot/noise keep
record arms (62/782/196912 — the ladder's class split exactly).

## Consequences / rejected alternatives

- No MNF5–8 stream format change other than MNF8's new flag bit; MNF9
  stays reserved for new-record-grammar work (charts).
- Chart tags (transformed whole-tile matches, shared transform
  dictionary) measured dead — are NOT in the wire.
- Conformance: mnf8_tokens golden vector (byte-exact), PLANAR|TOKENS and
  reserved-bit rejection vectors; fuzz gained the --mnf8=maximum arm.
- The `st.literal`/`st.ref_exact` stats fields carry token
  literals/matches counts on token-pick files (per-arm accounting
  documented here, not a wire concern).
- Future: backend 3 (mixed run/rANS) re-costs the pick through the rANS
  coder (stage-2 rule: selection is backend-dependent) — the token wire
  is coder-agnostic by construction.