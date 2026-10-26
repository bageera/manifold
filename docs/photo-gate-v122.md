# Photographic Gate + Research-Queue Verdicts (2026-10-25)

Status: research; frozen formats untouched. Artifacts:
- scripts/gen_photocorpus.py — deterministic photo-class generators
  (octave value-noise structure, natural palette ramps, sensor grain,
  low-pass chroma noise, vignette, optional defocus;
  10 dev + 10 held-out, seed-disjoint)
- testdata/photos/ + testdata/heldout-photos/ (the new corpora)

## Item 1 — corpora built
20 images (160x120 / 192x144 / 224x160); vision-checked as plausible
photo-class content (smooth cloudy structure, natural palettes, grain).
Generators disjoint from all previous classes; held-out stays gate-only.

## Item 2 — MED on photos: 20/20 WINS, the format case is real
zlib-9: med-residual beats raw by -14%..-32% (avg ~-21%).
zstd-19: beats raw by -10%..-33% (avg ~-19%). Every single file.
The MNF8 FLAG_PRED_MED (bit2, image-wide LOCO-I in front of literal
bodies, causal in-walker inversion) is now implemented: rate-aware
measured selection over {none, planar} x {med, no-med}; decoder carries
a pixel-causal MED reconstruction in decode_record_walk (MNF6/7 call
sites pass med=false — frozen semantics unchanged); MNF8 flags mask
widened to !0b111.
Regression pinned: mnf8_med_flag_roundtrip_photo_like (measured pick +
exact reconstruction).

## Item 3 — mixed run/rANS coder: TIES zstd-19 even ideally
Ideal contextual accounting (order-1 class context, per-context empirical
entropy, table transport included — no real-transport costs at all)
averages ~-0.5% vs zstd-19 on the 20 photo streams, wins 11/20 but all
within noise; real transports would flip it negative. VERDICT: the
custom coder does not clear the bar on photographic content even with
the context work; zstd-19 remains the shipping backend; mnf-mans
records the measurements as the track's honest end (for now).

## Item 4 — coordinate-quantization rerun: still FAIL
E6 with span-48/step-8 search + per-coordinate refinement over learned
PCA dirs (k=8): 10,419-10,609 B vs literal 3,099 (and E4's 8,863-11,153).
The fp-PCA -> integer-synthesis gap dominates any quantization gains;
the synth-residual inflation persists. The subspace retention verdict
stands with a rerun measurement instead of speculation.

## Item 5 — toroidal atlas on photos: LOSES 20/20
Random-crop consensus textures (k=1..8) + 6-bit phases: +14%..-74% worse
than MED everywhere on the photo corpora; partial edge tiles fall back
to MED-residual literals. CY01's own boundary holds: phases pay only on
genuinely periodic content — natural-image format item: denied, honestly.

## What would change any of these verdicts
- Real photos (not procedural) with genuine repetition: only item 2's
  verdict is load-bearing and independent of that.
- A learned-synthesis path that does not go through integer rescaling
  of fp principal directions (the failure class we keep measuring).