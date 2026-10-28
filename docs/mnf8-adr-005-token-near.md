# ADR-005 — token-wire TAG_NEAR: narrow-coordinate matches (2026-10-28)

## Context

The Erdős-offset probe's address study (docs/mnf9-erdos-probe.md)
measured that `dy` fits a u8 on every match in the corpus — the finding
that killed structured (m, dir) addressing simultaneously measured that
plain narrow coordinates would have been cheaper than the u16 pairs the
token wire carries. TAG_NEAR implements exactly that finding.

## Decision

**Token grammar gains tag 2** — a self-describing per-token choice, no
per-file flag, no new magic:

    tag 2 near: 0x02, dx:u8, dy:u8, len:u8   (4 B vs tag 1's 6 B)

legal iff dx < 256 AND dy < 256 (the decoder re-checks source range and
plane bounds as it does for tag 1). The matcher additionally prefers
the NEARER candidate on equal match length (near-encodable = strictly
better wire at zero decode cost); longer matches always win regardless
of distance. Backward compatibility: decoders from 1.6/1.7.0 reject
tag 2 as an unknown tag — the same property that makes every tag-1/0
decoder deterministic; files remain self-identifying.

## Measured basis (real wire, 30-file CLI sweep, all decode-verified)

The pick extended with near-form token arms:

    TOTAL 1,007,891 → 951,501 (−56,390, −5.6%), fires 28/30 files

- repeat 256²: 540 → 301 (−44%)
- gradient: 417 → 254 (−39%)
- screenshot: 807 → 700 (−13%)
- plaid: 27,643 → 20,677 (−25%)
- blob: 8,617 → 5,986 (−31%)
- sand_224x160 photo: 53,894 → 48,599 (−9.8%)
- noise/solid: +0 (the pick keeps their shipping arms)

This is larger than ADR-004's −49,341 at a fraction of the surface: no
new predictor walk, no new flag validation — one extra tag arm in a
self-describing wire.

## Conformance

Tokens unit tests: near fires + roundtrips exactly, near preferred on
equal length, near rejects malformed (truncation, below-minimum,
overrun). Full corpus decode-verified per file (CLI verify).

## Consequences

- Wire grammar documentation updates (format.md §3.1 gains the tag-2
  row); stats gain `near_matches` (diagnostic only).
- Encoder-policy note: near preference on equal length is legal because
  the wire cost is strictly lower; no decode-side change beyond the
  tag dispatch.
- MNF5–7 untouched; the MNF8 record path untouched (this is token-wire
  internal).