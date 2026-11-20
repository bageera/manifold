# MNF-HP9 — Hutter-style visual information benchmark specification (v0.1 draft)

Status: SPECIFICATION (no execution in HP9 v0.1; this document defines the
fixed benchmark the program will report against). Prepared in parallel with
HP0/HP1 per the initial execution authorization.

## 1. Purpose

A fixed, reproducible, license-clean benchmark for lossless visual
information compression, following the Hutter Prize's accounting discipline
(actual shipped bytes; every transmitted byte charged; independent decoding)
adapted to images. It exists so that Manifold research claims have a
stable measurement target and so that fixed-corpus specialization is
distinguishable from general-purpose compression.

## 2. Corpus tiers

| tier | decoded information volume | status |
|---|---|---|
| HP9-0 (screening) | the existing 30-file testdata corpus (~1.0 M decoded RGB8 bytes ... aggregate byte stream under MNF8 framing 768,303 B shipped) | EXISTS (testdata/, sha256 in docs/mnf-hp0-record.md) |
| HP9-1 (qualification) | ≈ 10 MB decoded image-information bytes | TO BUILD (license-clean sources below) |
| HP9-2 (stretch) | ≈ 100 MB | TO BUILD |
| HP9-3 (Hutter-style target) | ≈ 1,000,000,000 decoded image-information bytes | LONG-TERM; requires dedicated storage + license review |

**Decoded-information definition.** One byte = one 8-bit sample of the
decoded RGB8 (or explicitly defined deeper) representation at exact
reconstruction. The channel depth, color transform, and sample packing are
fixed per tier and recorded in the tier's manifest. Bits per pixel and per
sample are reported against that definition. No tier may redefine "byte"
between runs.

## 3. Image-class coverage (each tier)

1. Natural photography (camera-origin, calibrated scenes)
2. Scientific imagery (microscopy/satellite-class data)
3. Astronomy (long-exposure / instrument frames)
4. Documents (scanned or rendered text)
5. Screenshots (UI, flat regions, text antialiasing)
6. Procedural graphics (gradients, repeats, synthetic generators)
7. Textures
8. High-bit-depth (>= 10 bit/channel sources, exact down-conversion
   target defined)
9. Negative random controls (seeded uniform noise; incompressible anchor;
   any claimed gain below entropy on these voids the run)

## 4. Provenance, licenses, identities

Every source image records: origin URL/dataset id, license identifier,
author/date if stated, and sha256 of BOTH the source artifact and the exact
decoded RGB8 bytes shipped. Transform derivations (crops/rescales) record
their parameters. No image without a deterministic reconstruction path
enters the corpus. Sources with attribution requirements that a
~1 GB redistribution cannot satisfy are excluded at intake.

## 5. Partition rules

Three partitions — training / development / held-out qualification — split
**by original source**: same scene, camera session, document, or generator
seed never spans partitions. Crops/transformed variants of one source stay
in one partition (contamination rule: related variants must not bridge
train→held-out). Partition assignment is frozen in the tier manifest
(`research/manifests/hp9-<tier>.lock`), regenerated only by appending new
images, never by re-splitting.

## 6. Baselines

Reported per image and in aggregate, always on the identical decoded RGB8:

- Manifold production (`manifold encode --mnf8=maximum`, byte-frozen baseline)
- PNG (image crate reference decoder, level-matched flags recorded)
- WebP Lossless (`cwebp -lossless -m 6` + `dwebp` verify)
- JPEG XL Lossless (`cjxl -d 0 -e 7` + `djxl` verify)
- FLIF (if a maintained decoder is available; otherwise record as SKIPPED)
- other credible lossless compressors by availability (zopfli-PNG etc.)

Missing tools are SKIPPED with an explicit note — never silent rows.

## 7. Accounting

Per submission:

- total compressed bytes (actual shipped artifact sizes, checksums recorded)
- bits/pixel and bits/sample against §2's definition
- compressor program size AND decoder program size (stripped release
  binaries or the declared model/dictionary artifacts, in bytes)
- model/dictionary/training-artifact sizes (anything reconstructed at
  decode time is part of the archive and charged)
- standalone MDL vs installed-codec MDL reported side by side; the run
  must state WHICH regime each row uses and never switch mid-column
- encoding CPU time, decoding CPU time, peak RSS, hardware configuration
- exact reconstruction verification (decoder output == input bit-for-bit)

Hutter-style submission totals count the shipped (program + archive) bytes
once — no double counting of shared libs — and uncharged dependencies are
forbidden: the archive must state its dependency closure or the baseline
charges a fixed allowance.

## 8. Protocol

1. Freeze tool versions + flags in the tier manifest.
2. Encode all partitions with all codecs on identical inputs.
3. Verify decode-exactness for every row (MNF8 `verify`; external decoders
   decode + byte-compare).
4. Aggregate; report wins AND losses per class; no selection across
   codecs is implied.
5. Publish the run manifest (inputs, shas, tool versions, sizes, times)
   beside the results.

## 9. Negative controls and validity

Random-noise fixtures bound the coder: reported bytes must be >= the
incompressible anchor (with framing) and ~equal across codecs. Any coder
"winning" on noise beyond framing noise is an instrumentation defect, not a
result. A passing implementation test never implies a positive scientific
result; the §16-verdict standard of the gauntlet applies to HP9 runs.

## 10. Open items (explicit)

- HP9-1 source list and license audit (pending; proposed: CC0/PD scientific
  sets + generated documents/screenshots with committed generator scripts).
- A 1 GB tier needs dedicated storage, a hash-chained manifest, and a
  license re-review; not scheduled until HP1–HP8 dispositions justify it.
- FLIF decoder availability on this host (record as SKIPPED if absent).
