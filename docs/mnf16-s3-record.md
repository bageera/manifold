# MNF16-S3 record (2026-11-15) — coder-level cross-scale conditioning through the R9 bridge

S2 accepted G8's heavy-band refinement as the primary candidate; S3 ran
it through the ACTUAL adaptive backend-6 schedule via an ADDITIVE
extension of the r9_audit instrument (`g8child` token; S2-0/S3-0 modes
unchanged — the extension is inert without the token and the S3-0
anchor was re-verified byte-equal to the S2 audit values on three
fixtures after the edit). Production sources, backend identities and
wire untouched.

## Frozen spec (fixed before the holdout; recorded in the instrument header)

Feature = G8 on the same plane, mean(|west-8|, |north-8|), edges
[4, 16, 32, 64, 96, 128, 160] (the S1/S2 frozen edges), buckets 0..7;
mapping: refine-selected heavy children 16..19 -> 20 + (k-16)*8 + g,
context space 52; children = plain fresh models (no parent, no warm
mix — the S2-5 static replacement form); the shipped `share`
semantics retained for base contexts (children's parent = 0xFF);
update schedule = shipped lr 0.15 / inc 32, caps 32768 / 1048576;
quantization = 2^24 fixed point (the audit instrument's); control =
feature bytes permuted within file, seed 16.

## Feature-reconstruction contract (decoder availability)

Verified programmatically for every audited file: every feature byte is
a pure function of the DECODED stream's earlier bytes and the record
position math (no raster, no future bytes). The proof asserts every
read neighbor index <= the target's stream index, with equality only
for image-border clips resolving to the symbol's own position (the
value equals the intended zero-distance convention). The gate caught
one real defect during bring-up (strict-earlier was asserted before the
border-clip exemption was distinguished) — fixed before any arm ran.
Record coverage: every mode's residual block maps through the committed
META grammar (mode-0, law 6/7, flat 8 — mode 8 appears on the Hubble
wire), so features exist for every symbol the coder queries.

## Gates

| Gate | Status |
| --- | --- |
| Production anchor (S3-0) | PASS — extension byte-equal to the S2 audit on 3 anchors |
| Adaptive parity | PASS — the 'e' roundtrip inside the bridge validates encode/decode evolution for every audited mode (incl. g8child) |
| Feature causality | PASS — proof above |
| Incremental gain | PASS corpus + holdout (below) |
| Negative control | PASS — shuffled loses everywhere |
| Holdout | PASS — Hubble, frozen spec, single shot |
| Accounting | DONE — model cost / entropy-wire cost / complete-file cost distinguished; refine package vs children decomposed on the holdout |
| Performance | measured — python feature pass 74-213 ms/file (973 ms screenshot-class), 3.9 s on Hubble (research instrumentation; optimization explicitly deferred per program guardrails) |

## Corpus arms (22 mix-payload files, all backends audited as their own S3-0)

AGG: S3-0 583,366 B | S3-1 577,891 B | **gain +5,475 B** | S3-2 shuffled
589,531 B (**−6,165 B**). Per file: 21/22 gain (+64…+476 B); document
−0.3 B (flat, noise-scale); entropy-wire length tracks the model gain
closely (e.g. forest_3100008: 475.5 B model vs 472 B wire; slate_900008:
446.6/448). Children populations, cold-start costs (first-64-symbol
mean) and per-context accounts are recorded in the run log
(/tmp/mnf16_s3_full.log).

## Hubble holdout (S3-H, single shot, frozen spec)

The /tmp Hubble source fixtures were pruned; the holdout image was
regenerated losslessly from the surviving R9-F maximum wire
(/tmp/r9f_new_maximum.mnf8 -> decoded 1000x872 PNG, lossless by the
codec's contract). Hubble's pick is a BACKEND-5 stream, so S3-H
measures the frozen package (refineA + warm2p12 + share + g8 children)
against the stream's own shipped b5 schedule (S3-0 = its own):
S3-0 1,452,320.1 B | +g8children 1,437,964.6 B | gain +14,355.5 B
(~-1.0%). The accounting decomposition (the accounting gate's purpose):

| Package on Hubble's b5 stream | model cost | delta vs base |
| --- | --- | --- |
| base b5 (S3-0) | 1,452,320.1 B | — |
| refineA + warm2p12 + share alone | 1,453,752.7 B | **−1,432.6 B (regression)** |
| + G8 children (S3-1) | 1,437,964.6 B | **+15,788.0 B children-specific** |
| shuffled features (S3-2) | 1,455,488.1 B | −3,168 B |

The holdout's gain is cleanly attributable to the G8 conditioning
itself: on Hubble, plain prev3-band refinement regresses and the
feature-conditioned children carry the entire improvement (1.1x the
package net). Shuffled control negative.

## Disposition

G8's heavy-band child refinement survives the actual adaptive coder
(corpus + holdout, controls included). Per Jon's table: S3 closed;
S4 (experimental wire + independent decode) and S5 (full Maximum
portfolio gate) are the conditional next rungs; no new production
backend authorized; the v1.16.0 anchor 810,816 B stands; production
sources untouched. Research instruments: scripts/mnf16_s3_coder.py,
crates/mnf-mans/examples/r9_audit.rs (additive g8child token).
Run artifacts: /tmp/mnf16_s3_full.log, /tmp/mnf16_s3_hubble.log,
/tmp/mnf16_s3_hubble.json, /tmp/s3_decomp.log.
