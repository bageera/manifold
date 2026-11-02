# MNF13-R9-C — Parent-shared warm-up: measured record

Status: MEASURED · all arms roundtrip byte-exact through the R9-A
bridge (decode replicates seeding/mixing; zero transport — pure model
evolution). Instruments: `r9_audit.rs` (`warm*` modes),
`crates/mnf-mans/scripts/mnf13_r9c_gate.py`; rows
`/tmp/r9c_rows.json`. Production codec untouched; C0 anchored
byte-identical to the shipped payload on every file.

## Design (per the work order)

Parent topology = the grammar's own tree: parent of band-split key
k = k & 7 (the R5 bucket the 16-key rule split). Arms:
- C0 shipped: frozen backend-5 (anchor)
- C1 warminit: seed child tables from parent at first use, then
  ordinary updates
- C2 warm2pN: λ = 2^N/(2^N+n) child↔parent mixing while cold
  (nlocal < 256), pure child afterwards
- C3 warmfixΛ: constant-λ mixing — the "broader sharing" comparator
  (hot-context pollution is its designed-in risk)

## Measured (22 production streams, C0 anchored everywhere)

| arm | total B | Δ vs C0 | wins/losses |
|---|---:|---:|---|
| C1_warminit | 584,784 | −20 | 5 / 0 |
| C2_2p8 | 584,536 | −268 | 20 / 2 |
| C2_2p10 | 584,520 | −284 | 20 / 2 |
| C2_2p12 | 584,516 | −288 | 20 / 2 |
| C2_2p14 | 584,516 | −288 | 20 / 2 |
| C3_fix15 | 584,448 | −356 | 20 / 2 |
| C3_fix30 | 584,868 | **+64** | 15 / 6 |

## The mechanism, confirmed live

- **C1 ≈ no-op** (−20 B, 5 files): first-use parents carry nearly no
  useful statistics yet on this corpus; seeding alone barely moves.
- **C2 delivers R9-B's prediction**: −288 B (2p12/2p14) with 20/22
  wins and only 2 small losses. Mixing ONLY while cold captures the
  cold-start regret R9-B measured, then steps out of the way.
- **C3's extra savings are exactly its predicted pollution**:
  fix15 nets −356 B (+68 over C2) but its two losses are precisely
  the two ctx-0-dominated streams (document +12, screenshot +8) —
  the hot-context contamination R9-B flagged. fix30 flips negative
  (6 losses). Constant sharing is the wrong shape for non-stationary
  photo streams.
- Every arm's two losses, where present, are document/screenshot —
  the extreme-population ctx-0 streams; their per-symbol cost is
  already 0.02 b/sym and any mixing during their long hot run only
  hurts.

## Verdict within the pre-registered contract

- Real, honest entropy-wire win: **−288 B (−0.05% of the streams)**
  at warm2p12/14 — 20/22 files, 2 tiny losses, zero transport cost.
- vs the 9,671 B regret pool: captures ~3%. The static floor was
  recorded as optimistic; the causal, economical fraction of cold
  regret is what C2 harvests.
- Standing rule (tuned model ⇒ NEW backend id): promotion needs
  backend id + full corpus gate (R9-E) — Jon's call after R9-D
  measures the other half of the pool.

## Open (R9-D)

ctx-15/7 refinement (P(s | c, h) only for the selected contexts),
report conditional-information gain vs added adaptation cost through
the same bridge. R9-E portfolio gate after both halves are measured.