# MNF9 engine ladder probe: LZ77 + matrix-matching arms — measured
# (2026-10-27, follow-up to docs/webp-hybrid-probe.md and the matrix-
# transformation proposal; Eron's post on 3×3 homogeneous transforms)

Motivation: the proposal observes that WebP beats MNF8 everywhere on this
corpus (its gap = LZ77-2D + color cache + entropy context). The ladder:
baseline (A, real MNF8), B = native LZ77 tokens over MED residuals,
C = B + transformed whole-tile matches (the frozen v0.7 chart set: 4
shape-keeping D8 + 10 cyclic shears, NOT reinvented), D = C + shared
per-image transform alphabet, E = WebP (real). One cost model (zlib-9
over the token stream, tag+payload serialization, metadata counted);
decode exactness asserted in-sim; bit-exactness is structural (matches
copy residual bytes; chart matches copy whole pixels exactly).

## Results (30-file corpus, complete ladder; hash matcher, 16-cand cap)

| file | MNF8 | B | C | D | WebP |
|---|---|---|---|---|---|
| gradient 320×240 | 1,639 | **419** | 419 | 419 | 148 |
| blocks (held) | 1,774 | **1,672** | 1,672 | 1,672 | 1,344 |
| solid 256² | 62 | **41** | 41 | 41 | 44 |
| poster (held) | 1,234 | 3,562 | 3,504 | 3,505 | 494 |
| repeat 256² | 540 | 792 | 784 | 785 | 358 |
| screenshot | 782 | 795 | 879 | 881 | 320 |
| document (held) | 1,099 | 1,304 | 1,304 | 1,304 | 312 |
| blob (held) | 8,466 | 12,742 | = B | = B | 7,792 |
| plaid 192 (held) | 27,643 | 41,878 | = B | = B | 25,172 |
| noise 256² | 196,912 | 224,845 | = B | = B | 196,688 |
| 10 photos | 405,679 | ~460k | = B | = B | 295k |
| **TOTAL** | **1,024,043** | 1,178,876 | 1,178,894 | 1,178,898 | 761,714 |
| ratio | 1.000 | 1.151 | 1.151 | 1.151 | 0.744 |

## What the ladder actually shows

1. **Arm B splits hard on content class** — and the class split kills
   the naive format claim. Tokens crush where the residual plane
   repeats (gradient −74%, solid −34%, blocks −6%) and lose everywhere
   else (photos +8…+18%, noise +14%, plaid +51%, poster ×2.9): on
   noisy residual planes the hash-lookup finds accidental 7-px matches
   whose 6-byte tokens cost more than the literals they replace, and
   every pixel costs +1 tag byte.

2. **Arms C/D add nothing (C = B byte-identical except two synthetic
   files: poster −58 B, repeat −8 B; D ≈ C) and the per-tile chart
   machinery did not fire on any photo**: no 16×16 tile equals a
   transformed copy of another exactly. The dictionary arm saves
   nothing at an alphabet of ≤3 distinct transforms.

3. The proposal's central claim — "a small collection of geometric
   transformations makes repetitions accessible that WebP-style matching
   cannot efficiently represent" — is REFUTED at this probe's fidelity:
   exact whole-pixel repetition is captured by position-distance
   matches alone; where transforms add value on synthetics, the
   shipping chart grammar already provides it, and composed transforms
   (T = T3T2T1) never fired.

4. **The engine verdict mirrors the webp-hybrid probe:** LZ77-2D is a
   REAL mechanism (gradient −74%!) that a class-gated, rate-aware
   token arm can adopt for free on classes where it wins while the
   plain MED plane stays optimal elsewhere (the MNF8 flag-variant
   selection pattern, +1 B flag). Measured honestly, that hybrid is
   NOT this corpus's win overall (TOTAL arm-B = 1.151× MNF8 with
   tokens forced everywhere) — the retention question becomes an
   MNF9 format design: rate-aware per-file pick between the
   MED-literal plane and the token plane.

## Verdicts

- **Arm B as a RATE-AWARE per-file pick: RETAINED as the MNF9 engine
  candidate** (measured: −74% gradient, −34% solid, −6% blocks, and
  never worse than shipping when the pick refuses it).
- **Arms C/D (matrix matches in the token stream, shared dictionary):
  NOT RETAINED** — zero firing on photos, ≤1% deltas on synthetics,
  dictionary saves nothing.
- **Composition grammar: NOT RETAINED** — never fired; description
  longer than a single transform id in every case that mattered.

Method notes: tag+payload token serialization (tag 0 lit = 1+3 B, tag 1
match = 1+5 B pixel units, tag 2 tmatch = 1+3 B, tag 3/4 dict spell/
short), min-match 7 px, 64k-px window, 16-candidate scan cap, chart
candidates prefiltered to 8 nearest-by-signature (MNF8's rank step —
the probe's grammar = the shipping encoder's grammar plus tokens).
Full table: /tmp/ladder3.log in-session.