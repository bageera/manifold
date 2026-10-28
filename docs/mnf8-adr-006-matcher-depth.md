# ADR-006 — token matcher candidate depth (two-arm pick, 2026-10-28)

## Context

The Erdős closure recorded a blob-class far-offset signal (−12.4%)
pending "a far-repeat corpus class" to prove breadth. The far-offset
rung tested the cheapest form of that breadth: deepening the existing
content-keyed chains. No geometry, no wire change — candidate cap 16 →
1024 as a SECOND measured arm beside the v1.7 shallow arm.

## Decision

- `encode_plane_cap(res, w, h, cap)` — the matcher with an explicit
  candidate cap; `encode_plane` keeps the default (deep).
- The MNF8 pick loop measures the token arm at caps 16 AND 1024 per
  backend (`encode_tokens_coder_cap`); the pick absorbs the choice.
  Decode side is UNCHANGED — the wire is bit-identical in shape; only
  which matches the encoder found changes.
- `WINDOW_PX` 1<<16 → 1<<24 (the cap, not the window, was binding; u16
  dx/dy fields still bound what is wire-expressible).

## Measured (30-file CLI sweep, all decode-verified, v1.8.0 baseline)

    deep-only arms:   951,501 → 951,460 (−41), blocks +5 regression
    two-arm pick:     951,501 → 951,455 (−46), regression absorbed
    document −24, screenshot −22; noise/solid/record-class files +0

The blob class does NOT move: its file-level pick is a record arm
(5,986 B) that beats the token arm at every cap — the Erdős B-arm's
−12.4% was a tokens-arm-internal comparison that never survives the
rate-aware pick. Far-offset breadth measured: real but tiny; recorded
and closed.

## Consequences

- Encoder tokens-arm cost doubles (two depths measured); decode
  untouched, wire unchanged, no new flags.
- The Erdős far-offset revisit trigger is CONSUMED: the far-repeat
  corpus question is answered by measurement, not pending.