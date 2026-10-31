# MNF14-IF4 — Compression-aware law arms in Rust (MNF8 record modes 6/7)

Status: LANDED IN THE WORKING TREE, corpus verdict = **byte-identical to v1.14**.
Research retention: mechanisms shipped as encoder arms; no wire-format change beyond
the two new per-tile record modes already described here. Instruments in
`crates/mnf-mans/scripts/` per that directory's README.

## The question

IF3's pivot (cost-driven, compression-aware discovery beats structure-fitted
illumination on real photographs) asks whether the mechanism survives actual
MNf8 encoding: per-tile mathematical laws competing WITHIN the shipped
backend-5 pick at complete-record cost.

## Wire (MNF8 records 6/7 — additive, byte-frozen backends untouched)

- Mode 6 = LAW-PLANAR: `[1 B mode][12 B params][n B residual]`
  Pred per channel: `a + (b·x + c·y + t) >> 5`, in-tile coords; b/c carry
  rate×32 in i8, t is 3 B bias (5-bit field, <<3-aligned).
- Mode 7 = LAW-BILINEAR: `[1 B mode][12 B params][n B residual]`
  Pred per channel = 2-D lerp of the tile's 4 corner values (u8).
- Residual = (X − pred) mod 256, exact regardless of law quality.
- Laws are channel-independent → `channel_diff_relabel`/`unrelabel` copies
  modes 6/7 verbatim. Decoder parses params from the wire and NEVER reruns
  the search (IF4's separation rule).
- In the planar record layout, law bodies carry `planar_pack`-ed residuals
  like every other arm (this exact asymmetry was the cd-relabel roundtrip
  bug; fixed at the cost model too — the sweep charges the packed form).

## Encoder

`encode_stream_coder`'s per-tile competition grows two arms behind
`!pure_pred`: law-planar (seed = end-point plane fit; bounded sweep
db/dc ∈ ±2, dt ∈ 0..3, every candidate at complete-record cost: mode byte
+ params + proxy_cost(coder, packed residual)), law-bilinear (seed =
corner pixels; shift sweep d ∈ {0, ±4}). Complete-file pick unchanged
backends still compete. Decode side: `mnf6.rs` modes 6/7 branch.

## Measured (30-file release-pick corpus, `scripts/report_sizes.py --out`)

Working-tree total = **812,180 B = byte-identical to the v1.14 anchor**
(7b4cb96). All 30 rows identical. 0 regression, 0 gain at corpus level:
the law arms never win a tile on this corpus (they are designed to only
cost the pick when a law beats literal/refs/mix3 at complete-record cost,
and nothing did).

Fixture-level (bring-up, all exact): document golden byte-identical;
gradient 254 B / flags 0x20 / backend 1; noise 196,912 B / flags 0x0 /
backend 1; solid → mnf9 36 B; repeat → 301 B atlas.

## Gates (all green, release profile)

- `cargo fmt --all -- --check`, `cargo clippy -p manifold-codec --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`
- `cargo test --release` 7 suites: 58+0+3+4+5+14+1 passed, 0 failed
- Fuzz 300 trials: rejected 524/525(=524+1 re-run lines), accepted=1
  (the known valid-different-file class on solid), panics=0
- Corpus A/B vs 812,180 → byte-identical (above)

## Instruments

`src/laws.rs` unit tests (4): wire roundtrips both families, law-recon
exactness, seed-fits-an-actual-plane. `tests/mnf8_conformance.rs` mix5
golden still byte-exact (law arms must not disturb the pick).

## Verdict

Retained as shipped arms (modes 6/7 in the format surface), exactly like
the IF3 prediction: the arms exist and fire only when they win. Nothing
to promote beyond the arms themselves; no backend id change (the pick's
backend selection is untouched; modes 6/7 ride inside the record grammar
the existing backends already decode).