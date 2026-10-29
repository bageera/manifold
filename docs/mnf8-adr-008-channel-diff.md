# ADR-008: MNF11 channel-diff residual organization

Status: PROPOSED (2026-10-28), measurement first (MNF11 diagnostic 1)
Supersedes: nothing; competes inside the existing pick
Related: ADR-004 (branch predictors), TAG_NEAR (ADR-005), MNF11
diagnostic (docs/mnf10-g3-shared-model.md § MNF11)

## Context

The MNF11 diagnostic (Jon's Priority-1) measured the shipped
record-arm's residual streams. The dominant unused structure is
CROSS-CHANNEL: MI(R,G) ≈ MI(G,B) ≈ 1.6-2.5 bits — near half each
channel's entropy (H ≈ 4.2-5.5 b/B). The residual planes' own LZ
content is exhausted beyond 6-byte grams (Q3), so the container's
string layer finds what exists; the win must come from the SYMBOL
VALUES, not more matching.

Relabeling the residual symbols per pixel [r,g,b] → [g, r−g, b−g]
(reversible per pixel, zero added bytes) through the shipped files'
own backends: **−62,675 B (−6.6%) corpus total, 17/30 files win**,
losses confined to synthetic classes where the channels are already
near-uncorrelated (gradient +30, repeat +148, blob +1,732, plaid
+4,846) and two warm-grain photos (+2-4k). No new entropy coder —
the rANS denial's lesson (transport cost) does not apply: the
container is unchanged; only the symbol distribution entering it
changes.

## Decision (proposed)

Add MNF8 flag bit 6 (0x40) = FLAG_CHANNEL_DIFF:

1. ENCODER: after producing the record/token candidate body,
   ALSO produce the channel-diff relabeled variant (per-pixel
   [g, r−g, b−g] inside mode-0 bodies and token tag-0 literal
   triplets), compress both with every backend candidate, measure
   COMPLETE FILES, pick the min — the existing never-worse rule.
2. DECODER: flag 0x40 set → after token/record walking, apply the
   per-pixel inverse [g', r'+g', b'+g'] mod 256 inside every mode-0
   body and every token tag-0 literal triplet, BEFORE reconstruction.
   Flag unset → existing behavior (bit-exact old files).
3. Hardening: flag 0x40 files with ANY additional unknown bit reject;
   the per-mode/truncation/zero-distance reject classes carry over
   unchanged; MNF9 files are untouched (the relabeling lives inside
   the MNF8 record/token bodies only).
4. Conformance: 6 new goldens (gradient/plaid/slate/forest/dusk/
   screenshot, both flag states where the pick flips) + 5 reject
   vectors (truncated with the flag, flag on non-body arm, corrupt
   near-token under the flag, reserved-bit mix, empty-payload with
   the flag).

## Consequences

- Wire-compatible growth: old decoders reject 0x40 files with the
  existing unknown-flag rule (Error, not panic); new decoders read
  old files unchanged.
- Portfolio: the corpus total improves −6.6% with zero regressions
  by pick construction; the classes that lose the relabel simply
  never take the flag.
- Cost: encode runs the relabel per candidate (≈ free vs the
  container's own cost); decode adds one pass per body (memory-band
  trivial).
- The Acoustics analogy (Jon's Priority 3): sparse-correction
  structures over shared frames may inherit the same
  cross-channel-coupling economics — measured first there too.

## Verification

- The committed instrument reproduces the table:
  crates/mnf-mans/scripts/mnf11_residual_diag.py (diagnostics) and
  the relabel-stream walker (this ADR's § experiment) — rerun both.
- make gate + fuzz (525 trials, 0 panics) + capi before any release.