# MNF16-X1-L R6.2 record — wire reconciliation (CLOSED — PASS)

The production backend-7 wire for forest measured 40,660 payload bytes
(10,164 rANS words) while the audit's research wire on the same file's
archived stream measured 40,732 bytes (10,182 words) — a 72-byte gap
with the model trails reportedly identical. R6.2 reconciled this.

## Root cause: stale artifact pairing of sibling candidates (R6.2-A)

The Maximum pick evaluates the complete candidate matrix. Two left-
planar-ref1 constructions of the arm family — call 21 and call 29 —
share the SAME flag triple (0x61: left + planar + REF 1) but DIFFER in
their record-stream content at byte 2. The frozen research instruments
ran on archived v1.16.0-arm streams: the trace = call 21's form. The
pick's winner = call 29's form, which is SMALLER.

| Artifact | sha256 (first 16) |
| --- | --- |
| call 21 stream (archived form) | fcb8dc7906ab8eb8 |
| call 29 stream (winning form) | 24d7a55da6ab995f |
| call 21/29 class arrays (identical) | 60f96fd7927d8b9f |
| production payload (winner, 40,660 B) | 682e23165304b050 |
| audit wire on call 29's dumped stream | BYTE-EQUAL to the payload |
| audit wire on call 21's stream | 40,732 B (2c1030568a999493) |

Running the audit on each arm's OWN dumped stream + classes:
call 29 reproduces the production payload byte-for-byte. The 72-byte
gap = comparing different candidates, not different implementations.

## Model-layer parity (R6.2-B)

Both instruments were extended to emit a per-symbol hash of the FULL
2^24 probability vector (its prefix sums ARE the cumulative
frequencies, so hash parity covers cumulative/coding-interval parity).
On the matched pair (call 21 vs the audit on the archived stream), all
107,660 symbols agree on index, context key, class, symbol byte, coded
frequency, nlocal, full-vector hash, and parent mapping.

## Independent implementation comparison (R6.2-C)

A synthetic 16×4 planar mode-0 record (16×4×3 body + per-symbol class
array + geometry) through the production lane coder vs the audit
reference: **byte-exact wire match**, with a truncation bisect harness
prepared for any future divergence. The rANS state transitions,
renormalization, and terminal serialization agree end to end.

## Dispositions

- No production algorithm adjustment was necessary.
- The production implementation faithfully reproduces the validated L1
  model; production outperformed the research reference by legitimately
  finding a better candidate representation inside the same arm family.
- The research reference 769,108 B remains the competition target
  (expected minimum under equivalent candidate availability).
- - Savings attribution for the native portfolio: the 22 archived
  streams contributed 42,276 B (+568 sibling-candidate improvement);
  the remaining 8 files contributed 237 B — **plaid alone flipped to
  backend 7 among the non-archived files** (20,677 -> 20,440; plaid,
  not document, is the sole non-archived backend-7 selection: document
  and screenshot were archived (backend-1) picks inside the 22).
- The archived-vs-winning candidate distinction is recorded
  permanently: a flag triple identifies an arm FAMILY, not a unique
  input stream. Future parity comparisons bind the identity tuple
  I = (H_body, H_classes, geometry, config, model) — the wire hash then
  verifies the resulting representation. This is the research
  equivalent of P1-D's exact cache-identity contract: configuration
  equality alone is insufficient to establish computational identity.

## Evidence retained

research/results/r6_2_evidence/ (gitignored artifacts + SHA256SUMS):
both streams, the class arrays, geometry records, both per-symbol
trail CSVs (gzipped), the archived-stream audit wire. The synthetic
parity test remains in src/mix_coder.rs's test module as a permanent
regression guard, independent of the temporary MIX7_DEBUG machinery
(which is stripped at the production freeze step).
