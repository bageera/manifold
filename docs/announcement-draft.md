# Manifold: a lossless RGB8 codec research program — announcement draft (2026-10-27)

STATUS: DRAFT for review. Every number below is measured and reproducible
from the repo (see "Reproduce" per claim). Losses are stated beside wins —
the honest-results contract applies to the announcement itself.

---

## The technical announcement

### Manifold is a research program on making prediction, geometry, and structure do real compression work — bit-exactly

**Repo:** https://github.com/bageera/manifold
**Crate:** https://crates.io/crates/manifold-codec (v1.4.0,
`cargo add manifold-codec`)
**Docs:** https://docs.rs/manifold-codec · API docs complete, 0 missing-doc

**What it is.** A Rust lossless codec family for RGB8 images (MNF5→MNF8),
built strictly from the README spec outward: format → validation gates →
optimization research → retention gate. Formats are frozen. Every fallible
call returns a matchable error; malformed input never panics. Decode paths
are deterministic integer arithmetic.

**The architecture.** Model × coder, measured separately:

- **Predictors do the heavy lifting where they're earned.** JPEG-LS MED
  (FLAG_PRED_MED, MNF8) wins 20/20 on photographic content (avg −21% vs
  raw+zlib-9) and ships rate-aware: the encoder measures the flag variants
  and keeps the smallest — the decoder inverts causally.
- **Entropy coding is ranked behind models at equal inputs.** The backend
  study (10 coders: zlib/zstd/brotli/xz) found NO general backend
  dominates: xz wins smooth literals (−75%), brotli wins structured/
  tiny streams, zstd-19 wins mid-rate residuals — and zstd can LOSE to
  zlib-9 on gradient bytes. MNF8 therefore SHIPS a measured backend id in
  its container and picks per image.
- **Structure is a compression mechanism only when it's measured.**
  The atlas/tile-reference design loses to plain zlib on residual streams
  (v12); learned quantized subspaces lose 131–191% (the fp→integer
  synthesis gap); a toroidal shared-latency texture beats every control on
  periodic content (−61.8%) but loses on photographs 20/20 — recorded as
  denials, not absorbed into the format.

**Measured results** (reproduce: `make corpus`, `make sizes`):

| class            | MNF8 vs png     | WebP lossless | JXL lossless |
| ---------------- | --------------- | ------------- | ------------ |
| synthetic (15)   | 0.803× (10/15 wins) | 0.277×    | 0.909×       |
| heldout (5)      | **0.711× (5/5)** | 0.480×       | 0.760×       |
| photographic (20)| 1.005× (parity, 8/20) | 0.690×  | 0.622×       |

The honest headline: Manifold BEATS PNG on structured/synthetic and
held-out content and sits at parity on photographs; WebP/JXL still win the
photo-class. A new codec that publishes its own losses is the point.

**Engineering guarantees** (reproduce: `make gate`, plus CI):

- 53 tests / 8 suites green; every format bound pinned by crafted-input
  regression; mutation fuzzer exit-0 with interior-diff triage records
- `cargo-semver-checks` advisory gate vs the published baseline (blocking
  at 2.0)
- criterion decode benches: 106 Melem/s on a 640×400 screenshot
- one-command reproducibility: `make corpus` runs the fair lossless
  comparison (same decoded RGB8 to every codec, external codec outputs
  verified pixel-exact, missing tools skipped with explicit notes)

**Where it's going.** Three live fronts, each with a recorded contract:

1. **Multi-view shared-latent coding (arm P).** The one mechanism that
   wins when photographs fail: many projections of ONE structure. The
   slice-phase arm on synthetic depth already shows 0.26× base with exact
   phase recovery; real cryo-EM event stacks are the deciding corpus.
2. **Low-dose event coding (arm S).** EMPIAR's Falcon 4i movies ARE event
   streams: the vendor's wire at high dose ≈ dense-z9 (measured), but the
   low-dose regime — where the EER paper places the wire far below dense
   coding — is being probed now.
3. **Regional-class selection (arm F).** Score spatial/edge/periodic/
   projection predictors per region instead of per image.

**Engineering honesty section (the part most codecs skip):**

- Reindexing 3D→2D "folds" are NOT compression: measured 1.0× best-case,
  up to 7× worse, on every class probed.
- MNF8 ≈ png parity on photographs — WebP/JXL dominate that class today.
- All three CI reds in one stretch were distinct root causes (stale
  `-p` flags after a rename; new-toolchain clippy lint; a `*` gitignore
  hiding conformance fixtures) — each diagnosed in docs, each pinned.

---

## The validation pass (research-paper claims, checked against our record)

| source claim (EER paper, Tegunov 2021 / PMC7467176) | our measured status |
| --- | --- |
| EER wire ≪ dense formats | CONFIRMED at high dose: ship wire 3.93 MiB/frame vs 16 MiB dense render |
| RLE at 7 bits/subpixel "straightforward, suboptimal" | our dense-z9 ≈ wire at 7 e/px/s render — confirms "suboptimal" headroom at high density, but not for us until LOW dose |
| sparse pays at low dose | PENDING — that's exactly the 11213 low-dose corpus (553-frame fractionated movies, ~0.37 MB/frame wire) being fetched |
| EER preserves full spatial+temporal resolution | consistent with our probe (lossless render of events; bit-exact on the render) |

| source claim (med_image/LOCO-I, Weinberger 2000) | our measured status |
| --- | --- |
| JPEG-LS-style prediction strong on photographic | CONFIRMED 20/20, avg −21% (the shipped FLAG_PRED_MED) |
| weak on noise-dominated content | CONFIRMED on real cryo-EM planes: row-LOCO 0.6495× LOSES to plain zlib (detector noise) |
| edge-sense beats plane-prediction on photos | CONFIRMED (v12: edge-sense wins photos, plane wins flat-UI — neither dominates) |

| source claim (RFC 8878 / zstd) | our measured status |
| --- | --- |
| zstd ≫ zlib generally | PARTIAL — true on residual streams; REVERSED on smooth gradient literals (measured, every level 3–22) |

| source claim (Eckart–Young / PCA) | our measured status |
| --- | --- |
| PCA gives variance-optimal low-rank | CONFIRMED as math; FAILED as lossless codec: 131–191% inflation (integer-synthesis gap) — the fidelity claim doesn't transfer to a bit-exact coder |

Each claim row will move into docs/research-paper-validation.md with
links, then the announcement cites it ("we tested what the papers claim;
here's where our measurements agree").

---

## Open notes for Jon before publishing

- Photo-class parity is the headline risk: reviewers WILL fixate on WebP.
  The framing answer is already in the draft: the program's contribution is
  the measured-gate method (predictor/coder/structure ranked separately),
  not one bitmap record.
- The 'engineering honesty' section is the differentiation. Keep it.
- Draft tone: factual numbers, no adjectives; the losses stay in the table.