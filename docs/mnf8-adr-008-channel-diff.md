# ADR-008: MNF11 channel-diff residual organization

Status: IMPLEMENTED (2026-10-28): flag bit 6 = 0x40, reserved mask
!0b1111111; shipped in the library's MNF8 pick (both-organizations arms
at complete-file cost; the decoder's inverse relabel runs before the
walk; gate numbers in docs/mnf8-adr-008-channel-diff.md § gate results
and the corpus = 8/8 test suite + fuzz). The ADR-008 pick composes with
every ADR-004 predictor × planar arm and the token arm.
Supersedes: nothing; competes inside the existing pick
Related: ADR-004 (branch predictors), TAG_NEAR (ADR-005), MNF11
diagnostic (docs/mnf10-g3-shared-model.md § MNF11)

## Context

The MNF11 diagnostic (Jon's Priority-1) measured the shipped
record-arm's residual streams. The dominant unused structure is
CROSS-CHANNEL: MI(R,G) ≈ MI(G,B) ≈ 1.6-2.5 bits — near half each
channel's entropy (H ≈ 4.2-5.5 b/B). The residual planes' own LZ
content is exhausted beyond 6-byte grams (Q3), so the container's
string layer finds what exists; the win must come from the SYMBOL
VALUES, not more matching.

Relabeling the residual symbols per pixel [r,g,b] → [g, r−g, b−g]
(reversible per pixel, zero added bytes) through the shipped files'
own backends: **−62,675 B (−6.6%) corpus total, 17/30 files win**,
losses confined to synthetic classes where the channels are already
near-uncorrelated (gradient +30, repeat +148, blob +1,732, plaid
+4,846) and two warm-grain photos (+2-4k). No new entropy coder —
the rANS denial's lesson (transport cost) does not apply: the
container is unchanged; only the symbol distribution entering it
changes.

## Decision (proposed)

Add MNF8 flag bit 6 (0x40) = FLAG_CHANNEL_DIFF:

1. ENCODER: after producing the record/token candidate body,
   ALSO produce the channel-diff relabeled variant (per-pixel
   [g, r−g, b−g] inside mode-0 bodies and token tag-0 literal
   triplets), compress both with every backend candidate, measure
   COMPLETE FILES, pick the min — the existing never-worse rule.
2. DECODER: flag 0x40 set → after token/record walking, apply the
   per-pixel inverse [g', r'+g', b'+g'] mod 256 inside every mode-0
   body and every token tag-0 literal triplet, BEFORE reconstruction.
   Flag unset → existing behavior (bit-exact old files).
3. Hardening: flag 0x40 files with ANY additional unknown bit reject;
   the per-mode/truncation/zero-distance reject classes carry over
   unchanged; MNF9 files are untouched (the relabeling lives inside
   the MNF8 record/token bodies only).
4. Conformance: 6 new goldens (gradient/plaid/slate/forest/dusk/
   screenshot, both flag states where the pick flips) + 5 reject
   vectors (truncated with the flag, flag on non-body arm, corrupt
   near-token under the flag, reserved-bit mix, empty-payload with
   the flag).

## Findings preserved (the three headline mechanisms)

1. **Zero-overhead residual relabeling.** The existing header flag word
   communicates the transform — no extra payload section, no per-body
   metadata; the wire shape (byte counts per mode/tag) is unchanged and
   the relabel is reversible per pixel by construction.
2. **Portfolio interaction.** CD composes PROFITABLY with the existing
   predictor × planar arms and the token arm (the diagnostic's −6.6%
   relabel-only estimate doubled to −12.3% once CD competed inside the
   full pick): the representation enables the existing arms rather than
   replacing them. This is the program's first measured positive
   interaction (previous composition attempts — plane, near, depth —
   each landed at ±0.1%).
3. **Decoder discipline.** The three walker bugs (post-mode-byte
   over-advance; MATCH's 6-byte form; NEAR's length-byte offset) were
   each caught by assertions and the golden-vector decode BEFORE any
   compression number was accepted — the round-trip-first rule kept the
   measurement honest.

## Consequences

- Wire-compatible growth: old decoders reject 0x40 files with the
  existing unknown-flag rule (Error, not panic); new decoders read
  old files unchanged.
- Portfolio: the corpus total improves −6.6% with zero regressions
  by pick construction; the classes that lose the relabel simply
  never take the flag.
- Cost: encode runs the relabel per candidate (≈ free vs the
  container's own cost); decode adds one pass per body (memory-band
  trivial).
- The Acoustics analogy (Jon's Priority 3): sparse-correction
  structures over shared frames may inherit the same
  cross-channel-coupling economics — measured first there too.

## Verification

- The committed instrument reproduces the table:
  crates/mnf-mans/scripts/mnf11_residual_diag.py (diagnostics) and
  the relabel-stream walker (this ADR's § experiment) — rerun both.
- make gate + fuzz (525 trials, 0 panics) + capi before any release.

## Gate results (2026-10-28, implementation gate)

- Corpus (30 files, the release CLI pick, all `verify` PASS): total
  **834,885 B** vs the pre-CD pick's 951,455 = **−116,570 (−12.3%)**;
  CD fires **23/30** (every photographic record file + document +
  poster + screenshot; the exact-generative MNF9-owned files and the
  gradient/record-pure classes keep their prior arms).
- The composition with ADR-004's predictors and PLANAR nearly doubled
  the diagnostic's estimate (−62,675 → −116,570): e.g. slate_224x160
  62,052 → 50,918; forest_224x160 55,804 → 45,522.
- Tests: in-lib pins 3/3 fire (roundtrip all predictor/planar shapes ×
  both backends; never-worse vs the pre-CD pick; malformed rejects);
  conformance 8/8 (the new CD|PLANAR|PLANE golden from the slate crop +
  reserved-bit-7 and truncated-payload rejects); lib suite 51 passed.
- The wire change: MNF8 flag bit 6; old decoders reject CD files via the
  reserved-mask rule (Error, not panic); new decoders read all pre-CD
  files unchanged; MNF9/MNF7/MNF6 untouched.

### Backward compatibility (the final gate's explicit check)

The three pre-ADR-008 conformance goldens carry flags 0x1 (PLANAR),
0xc (TOKENS), 0x20 (PRED_LEFT) — bit 6 UNSET — and decode exact through
the new decoder with the CD branch untaken (verified by
mnf8_conformance's 8/8 + the differential suite's 4/4, which re-agrees
the independent decoder on the same goldens + corpus arms). Old
decoders reject bit-6 files via their reserved-mask rule
(rej_mnf8_reserved_flag.mnf8 stays a rejection vector). New encoder
output remains bit-identical for files whose pick rejects the CD arm
(the CD arms are ADDITIONAL pick candidates: the plain arms still
compete byte-for-byte).

## v1.12 — the REF field (bits 6-7), the C2 gate's selector promoted

The C2 gate measured alternate reference channels through the shipped
streams' own walker: CD-G stands on aggregate (CD-R +19,278 / CD-B
+11,664 whole-corpus) but 9/27 MNF8 files prefer R or B per-file. The
selector generalizes bit 6 to a 2-bit field `(flags >> 6) & 0b11`
(0 none / 1 G = the exact v1.11 shape / 2 R / 3 B; FLAG_REF_R = 128;
reserved mask widens to bits 8..31). Old bit-6 CD files decode
unchanged (their shape = REF 01); a v1.11 decoder rejects REF=10/11
through its reserved-bit-7 rule — verified against the released 1.11.0
binary with the new goldens (ref2/ref3: reject on 1.11, exact on 1.12;
cdg + plain goldens exact on both).

Implementation: `channel_diff_relabel`/`channel_diff_unrelabel`
generalized per `ref_pick` (1→(g,r,b), 2→(r,g,b), 3→(b,r,g)); planar
bodies relabel per-plane in wire emission order; token tag-0 literals
and interleaved mode-0 bodies relabel per pixel. The encoder pick sweeps
reference 0..=3 across every record arm (predictor x planar x backend)
and the token arm — 4x the CD-relevant arms; measured encode cost
~1.5x on the CLI (photo 3.4→5.1 s, screenshot 12→17 s, document
3.2→5.0 s). Decode cost unchanged (one inverse pass before the walk).

Gate (this record's commit): corpus-gate test 832,765 (−2,120 vs
v1.11's 834,885, cd_fires 23); release-CLI e2e 30/30 verify = 832,739
(−2,146) — the 26-B instrument delta = exactly the one file where the
CLI's MNF9 whole-frame arm wins (solid 62→36; every other file
byte-identical between the two instruments). REF distribution
none 7 / G 13 / R 5 / B 5. The C2 census's ≈ −2.1 kB prediction
reproduced by the production pick within 2%.

Census caveat (recorded for the next instrument author): the C2
per-file table repacked with python-zlib level 9 while the shipped
"zlib-9" backend is flate2's Compression::default() (level 6, frozen
wire name); its per-file deltas are level-mismatched and document's
census row (−166) was a phantom the real pick correctly declined. The
aggregate direction survived (the real pick's −2.1 kB came from the
R/B files the census named).

Conformance: 10/10 — the two new REF goldens (mnf8_ref2_screenshot:
TOKENS|REF=2 flags 0x8c, zstd backend, 640x400;
mnf8_ref3_terrain160: PLANAR|PRED_MED|REF=3 flags 0xd1, zstd,
160x120) + the bit-8 reject replacing the bit-7 vector the mask
widening invalidated (the old vector's bytes are REF=11's shape and a
legal v1.12 file class).

Fuzz: scripts/fuzz.py full run on the v1.12 tree — see the commit's
gate log; conformance + lib + CLI + fuzz all green before tag.
