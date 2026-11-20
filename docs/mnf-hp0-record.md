# MNF-HP0 — Baseline qualification and research preparation record

Program: MNF-HP (Hutter-style conditional-information research, v1.0 gauntlet).
Status: **GATE PASSED** — production baseline verified fresh, acquisition
reproducible, actual production outputs unchanged.

## 1. Repository and toolchain (MEASURED, fresh)

| item | value |
|---|---|
| revision | `e9df7f1615fda4a2120155cf53985ecc2027975c` (main, 2026-11-18) |
| crate | `manifold-codec` v1.17.1 (lib target `manifold`) |
| rustc/cargo | 1.99.1 (58cf8f9c3, stable-aarch64-apple-darwin) |
| host | macOS 26.6.2, aarch64 (Apple Silicon), single socket |
| deps | constriction 0.5.0, flate2 1.1.10, image 0.25.10, png 0.17.16, zstd 0.13.3 (+ zstd-safe 7.3.0) |
| MSRV | 1.91 (`rust-version` in Cargo.toml) |

Wire formats covered by the baseline: MNF8 (record arms: predictors
none/med/plane/left, planar on/off, REF reorganization 0–3; token arms),
MNF7/MNF5/MNF6 untouched, GSE containers out of scope here.

## 2. Corpus identities (MEASURED: sha256, fresh)

30 PNG fixtures, 4 partitions (path ⇒ sha256):

```
d64093f4cad10c9d93e9054d9ec6f9cfd23a3346b533ee3580554faebf3c7006  testdata/gradient_320x240.png
5b241851ce46cb5ee5a9ee1b49aad34461939e4a0e4e773ea76daff70d79ee4b  testdata/heldout-photos/autumn_160x120_s3100006.png
0b87983785c64b289acc8878923001a910ef76b0acac3829749c5190f7341a7f  testdata/heldout-photos/dusk_160x120_s3100003.png
249aff927a81a83e7c6284a3effd0baa5a223341fd7f9675708ff04506b0bd1f  testdata/heldout-photos/dusk_224x160_s3100002.png
c025bc63b48398c3a819a65b4d2df340f726669cc25a0ceb8b7fff5ef08bbecc  testdata/heldout-photos/forest_192x144_s3100001.png
58be687445ca2398c9917c725282c354c2c74dd5f652e53f8a79fb08d2e07cef  testdata/heldout-photos/forest_224x160_s3100005.png
6094bcff377dde05add41e8d65c4c1c5a1f5e497536eb6c12462e6e7b3437e10  testdata/heldout-photos/forest_224x160_s3100008.png
60ef70f8de4341ef8a7a9a3cb3beb0e61856a438d8a85fa3c8ee2450390644cd  testdata/heldout-photos/ocean_192x144_s3100007.png
0d318920264c19693ebaae62835cd5aa5c8bbf10741734285e641e9366d28155  testdata/heldout-photos/sand_192x144_s3100004.png
7f49fdfc3c690436e61bc5f07d9e95a5b0ac66b49829d789b35e697d62a7ebef  testdata/heldout-photos/slate_160x120_s3100009.png
61e05faad60bb745be7dcacd38aef3bc21b80cfd6a7874d64f1e4731657d7901  testdata/heldout-photos/terrain_160x120_s3100000.png
c2c88883a634f59bf13af0765ed8db85323f4274f2d22908ce7c73d7dffedf5f  testdata/heldout/blob_224x160.png
6895535c6db9bef0961a4dfe6eb45054dc5afc8fb5e4b4eeb5c6917c54890170  testdata/heldout/blocks_192x128.png
7d635922950aff9b57c2512841ab2830a0dd590f91935714d93e22f7121c4dd1  testdata/heldout/document_200x280.png
90a0ee3d5010ea9b3eae24cb3b6fc29bedbb13fa33476e777119a6d50607d51f  testdata/heldout/plaid_192.png
224209d8a12533fb1dbdfb2e88dd3fd10742835a6a5c5c0974cfa9f2bc313545  testdata/heldout/poster_208x144.png
97e6f1912dead66c0034a61005e033aafdb0dec2239b2590eafbd568b140a5ff  testdata/noise_256x256.png
a2eca23528efd0418974f89301697fd4fd5cf515977cea80de6596ed2846f339  testdata/photos/autumn_192x144_s900001.png
7382beab92d812924c895b67c5b5760a61c617c4579d3538a9e7b2fe1f6edc2b  testdata/photos/dusk_160x120_s900003.png
f68ddd6ddf4db359dacc433f9f15cf1ec39608fc3613bd3b0cc50572961219d0  testdata/photos/forest_224x160_s900002.png
400acc1b62cdbb592ce004e48223644b789dd3735b81d0cbe4357e4fbed47495  testdata/photos/ocean_192x144_s900007.png
d394956c0867cfddff15b753da8ad1807107912f5070747f3e71583c8025b7cc  testdata/photos/sand_160x120_s900000.png
dabebf6e8c8be6be2fe162688c3df9eb2e319b82dd62ad253347b5428208eadc  testdata/photos/slate_192x144_s900004.png
f87ee549ee20797f43bbb67a3e466921352e71a19c59b3d921ad4740c7a8e75a  testdata/photos/slate_224x160_s900008.png
a60908423b8914aa6dcacdad318e0af3f34d4875cf4091945a304ec9a74ff6c0  testdata/photos/stone_160x120_s900006.png
30e8c0bc959e236da28c242db4bd3cbf7388c877078c4772ae935f5b37615167  testdata/photos/terrain_160x120_s900009.png
eb6b4d7eee4eb78cdbe701d32e404105ba5874b2ac148715454f3f69e6a14d7f  testdata/photos/terrain_224x160_s900005.png
cf4035ad566de0d5582e0c01b0e0b751c975c6f2b3a2823b4c29550a0ef7116b  testdata/repeat_256x256.png
c53d50952ad3f9e61e4d9c52b7fe870dfe2e96ad286a23099462c7ad7f4abe24  testdata/screenshot_640x400.png
dedd391ced47c7cb0c3233726de7197438522b40227f3078566f6e2d5884e711  testdata/solid_256x256.png
```



Heldout discipline: `testdata/heldout/` and `testdata/heldout-photos/` are
eval-only partitions (disjoint generators/scenes from `testdata/photos/` and
the synthetic corpus). Per the standing rule, neither HP2 tuning nor model
selection may touch them; they are the held-out qualification pool.

## 3. Production candidate matrix and exact serialized sizes (MEASURED, fresh `make corpus` run at §1 revision)

Backend set (all arms evaluated per file by the Maximum pick):
zlib-9 (backend 0), zstd-19 (1), mix4 (4), mix5 (5), mix6 (6), mix7 (7),
plus token-wire arms (token16/tokenmax) on backends 0/1. MNF8 header:
32 B (magic MNF8, w/h/tile/tile_count/plen/flags/backend) + payload.

Fresh totals over the 30-file corpus (identical decoded RGB8 input, verified
lossless per row by `compare_lossless.py`):

| codec | total bytes | wins/30 |
|---|---|---|
| PNG | 1,039,956 | 0 |
| MNF8 balanced | 832,791 | 3 |
| **MNF8 maximum (production pick)** | **768,303** | 3 |
| WebP lossless | 761,714 | 5 |
| JPEG XL lossless (e7) | 736,342 | 22 |

MNF8-maximum = exactly the frozen v1.17.0/v1.17.1 production total
(768,303 B): the fresh run reproduced the published byte-frozen baseline with
zero drift. Per-file table: `research/results/hp0-corpus-fresh.csv`
(copy of `/tmp/cmp.csv`; regenerated by `make corpus`).

Reference points (HISTORICAL, published records — not re-measured here):
Hubble photographic stress test 1,438,848 B; backend-7 selections 23/30 at
promotion (MNF16-X1-L record), decode 1.141x (P1-C/D records).

## 4. Encoding/decoding CPU time and peak memory (MEASURED, `/usr/bin/time -l`, fresh release binary)

| arm | wall | peak RSS |
|---|---|---|
| screenshot 640x400 encode (--mnf8=maximum) | 20.4 s | 109 MB |
| forest 224x160 photo encode (--mnf8=maximum) | 14.8 s | 103 MB |
| screenshot 640x400 encode (--mnf8=balanced) | 2.26 s | 108.9 MB |
| screenshot decode | 0.54 s | 5.5 MB |
| forest decode | 0.09 s | 3.0 MB |

Peak RSS is dominated by the zstd-19 arms (level-19 match tables) during
candidate evaluation; decode RSS stays single-digit MB on these fixtures.

## 5. Production probability model: precision and state allocation (MEASURED from source at §1 revision)

- Predictor arms: PredKind::None / Med / Plane / Left image-wide residual
  predictors; per-tile records (atlas exact/geometric references, MED mode 2,
  planar-law 6, bilinear-law 7, YCoCg packed mode 3); REF channel-diff
  reorganization (flags bits 6-7) applied decoder-side before the walk.
- Entropy backends 4–7 = the R9 mix coder family
  (`src/mix_coder.rs::MixCtx`): per-context two-rate estimators
  (FAST + SLOW, adaptive halving caps), exponential-log-weight mixing
  (EWA, `EWA_LR`, clamped to ±20), fixed-point probability vector per byte
  at SCALE = 2^24 (24-bit precision), LR floor (every symbol ≥ 1, sum
  exactly 2^24), then constriction
  `ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>` + range coder.
  Backend 5 = 16 byte-trigram contexts (prev,prev2,prev3 bucketed); backend
  6 = 20 contexts + warm/cold parent mixing (`warm_n0` = 12) + share/live
  ancestors + heavy-band children; backend 7 = 40 contexts = backend 6
  topology + lane-class children (keys 20+) keyed from the decoder-accessible
  grammar walk (`LaneWalk`), zero transmitted classification.
- State allocation per encoding: MixCtx ≈ 4,104 B (two 256-entry u64 counter
  planes + weights + maintained sums) ⇒ backend 4 ≈ 32.9 KB, backend 5 ≈
  65.9 KB, backend 6 ≈ 82.4 KB, backend 7 ≈ 164.8 KB transient, freed at
  seal. zlib/zstd arms carry no statistical model state.
- Backends 0/1 (zlib/zstd) expose NO probabilities; per the gauntlet's
  no-invention rule they contribute sizes only.

## 6. N1 native-stream acquisition reused and re-verified (MEASURED)

The MNF17-N1 instrument (`src/mnf8.rs::mnf17_n1_acquisition`, env-gated
`#[ignore]`) remains the production candidate enumerator: it rebuilds every
arm's record stream via `build_record_stream`, matches the production pick
byte-exactly against assembled files (winner reconciliation — exactly one
distinct stream), and emits per-byte (byte, class, lane, causal left/upper,
pixel) traces. Its historical stream identities (fnv) and per-arm payloads
for the 7-fixture cohort (solid/gradient/repeat/screenshot/noise/forest/
terrain) match the §3 fresh run's pick behavior.

**Acquisition reproducibility gate**: the new HP1 instrument
(`src/mix_coder.rs::mnf_hp1_census_dump`, env-gated `#[ignore]`) independently
rebuilds each file's production pick arm from the pick's own header flags and
asserts byte-exact reconstruction of the assembled production file, then
replays the production probability model over the stream. Validation:

- rebuilt stream FNVs equal the N1-ledger identities on the overlapping
  fixtures (e.g. terrain `bc3e42c1ec950cd7`, forest `f87a3e74824317f0`);
- the replay's ideal-bit totals equal the independently recorded MNF17-N2
  A1 (mix7-family production-model replay) totals to the decimal on all
  shared fixtures: gradient 51,940.38 bits, screenshot 3,418.71 bits, forest
  325,207.47 bits (ledger `research/results/mnf17-n2-summary.csv`).

Statistical-family replay controls: for files whose production winner is
zstd/zlib (solid, gradient, repeat, noise and any corpus file), the HP1
instrument replays the backend-7 model over the SAME winner-arm stream and
labels them `control=1` in the ledger. These are replay controls, NOT
selected wire costs; production wire sizes stay §3's.

## 7. Deliverables produced

- `research/results/hp0-corpus-fresh.csv` + `research/results/hp0-corpus-fresh.log`
  (fresh `make corpus` run, regeneration: `make corpus`)
- `research/results/hp1-dump.log` + `research/results/hp1-native-traces/`
  (per-image gzipped per-byte traces + probability-vector prefixes + ledger;
  regenerated by MNF_HP1_OUT=… cargo test --release mnf_hp1_census -- --ignored)
- docs/mnf-hp1-record.md (census; next stage)
- docs/mnf-hp9-benchmark-spec.md (HP9 benchmark specification, prepared in parallel)

## 8. Limits and disclosures

- Timing/RSS rows §4 are single-run /usr/bin/time captures (no variance
  reported); they are baseline anchors, not benchmark claims.
- The 30-file corpus carries ~810 KB aggregate payload at the frozen
  baseline; zstd wins 4 of the 7 N1 screening fixtures' picks —
  the statistical census population (mix-model streams) overlaps those
  fixtures only through labeled replay controls (§6).
- JXL/WebP rows exist because cwebp/cjxl were on PATH; if absent on a future
  host the rows are SKIPPED with a logged note per the fairness contract.

## 9. Gate disposition

GATE HP0: **PASSED** — production baseline verified fresh (byte-identical
corpus pick 768,303 B), acquisition reproducible (stream + ideal-bit parity
to independent instruments), production outputs unchanged (no source changes
to encoder/decoder semantics; test-instrument and pub(crate) visibility only).
Proceed to HP1.
