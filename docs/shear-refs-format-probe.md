# Shear-refs format probe (D8 += 10 integer shears) — measured, REVERTED (2026-10-27)

The one surviving format candidate from the v0.7 charts replication was
"extend the MNF8 D8 reference family with the 10 integer shears
{3, 5, 7, 11, 13} × {row, column}", measured per tile, rate-aware
(same shape as FLAG_PRED_MED). This probe implemented it end-to-end in
Rust (MNF8 flag bit3 = FLAG_SHEAR_REFS, t ids 8..17, cyclic shears,
square-tile-only, decode walker accepts shear ids only with the flag,
MNF6/7 untouched), with regression tests, then ran the retention gate —
and reverted the change when the gate failed.

## What was implemented (then reverted; full diff in git history of this session)

- `codec.rs`: `NUM_TRANSFORMS` 8 → 18, `SHEAR_STEPS`, cyclic shear pull
  maps `out(x,y) = src((x + s·y) mod w, y)` / `out(x,y) = src(x, (y + s·x)
  mod h)`. Cyclic shears are bijections on any tile size, shape-keeping,
  and never need an explicit inverse (residual = tile − T(ref), decoded
  as base + resid).
- `mnf8.rs`: flag bit3; encode candidates extended when the flag wins;
  decode passes the flag through.
- `mnf6.rs`: `decode_record_walk` gained a `shear_ok` gate — frozen
  grammars keep rejecting tr ≥ 8; a hand-crafted record with a shear id
  is rejected in MNF6/7 and accepted in MNF8-only (regression test).
- Tests: shear bijectivity (16×16, per-id source-coordinate distinctness),
  shear-refs MNF8 roundtrip + flag-legality, MNF7-shear-reject gate,
  D8 inverse-scoping fix (the exact-inverse search test is a D8-group
  property; shears are covered by bijectivity instead).

All gates passed at that state: lib suite green, clippy -D warnings clean,
fmt clean.

## Retention-gate measurement (the one that decides)

The repo's 30-file corpus (5 held-out, 10 photographic, 15 synthetic) via
the release CLI, `report_sizes.py`, MNF8 balanced, all roundtrips exact:

    A/B mnf8-balanced shears-on vs shears-off: wins 0 | losses 0 | ties 30

30/30 byte-identical sizes. The rate-aware pick never chose a shear
reference on any tile of any corpus image: MED-first prediction plus D8
references already cover what cyclic shears add, because the classes the
python atlas-only matrix won on (synthetic curved repeats, −6% to −19%)
are exactly the classes where MNF8's MED path already reduces residuals
to near-zero on this corpus. The flag's cost is real (8 extra
variant-encodes per image in the worst case, 1 more flag bit of grammar)
for zero measured benefit.

## Verdict

NOT RETAINED — implemented, gated, reverted. The D8+shears candidate is
removed from the open queue (it was the last one); the v1.5 format
surface is unchanged from v1.4.0. Evidence retained: this doc + the
committed probe script (`scripts/riemannian_traversal_probe.py` is the
Riemannian arm; the shear implementation is reproducible from the session
history diff if ever revisited — the mechanism record in
docs/v07-charts-replication.md stands, but the Rust-atlas arm was the
winning path there, not the MNF8 rate-aware arm).

Riemannian side note recorded in the same session: the metric-warp
predictor context (R2) measured plaid-only (−9.2%) with +2–4% metadata
tax elsewhere → also not retained.