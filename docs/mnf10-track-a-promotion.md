# MNF9 Track-A promotion (ADR-007 → shipped encoder arm, 2026-10-28)

## What shipped

- `src/mnf9.rs` — the shipped-decoder port of the research prototype:
  magic "MNF9" (4 B, the ADR-003 reservation), 32-B header with the SAME
  field offsets as MNF8 (payload_len at 20, flags at 24, backend at 28;
  12..20 reserved-zero-on-write, reject-on-read), the six decoder-defined
  model families, and interval-intersection discovery solvers for all six
  (the probe's algorithms with the bounded growth rule).
- Encoder pick arm (ADR-007's plan): `mnf9::encode_exact(px, dims)`
  competes INSIDE the CLI's `--mnf8` flow at FINAL FILE SIZE; a winning
  zero-residual rule ships an MNF9-magic file (`[mnf9] exact rule
  discovered: N B (mnf8 arm M B)`), losing files keep MNF8 unchanged.
- `decode`/`verify` dispatch by magic (MNF9 → mnf9::decode_mnf9).
- API: `pub fn encode_exact`, `pub fn decode_mnf9` (decoder = error-not-
  panic on every malformed class).

## Verify chain (real)

- Library: 3/3 pins (floor-lerp discovers at 58 B file < 128; recurrence
  discovers < 64 B; noise discovers NOTHING — the never-worse polarity);
  malformed classes (bad magic / short / flags / backend / plen mismatch /
  reserved id / truncated / oob boundary) all Err, 0 panics.
- CLI e2e: the div-1D golden fixture PNG encodes to a 58-B MNF9 file
  (MNF8's own best was 97 B) and `verify` PASSes through the dispatch;
  solid_256x256 picks MNF9-const 36 B (was 62); noise/photos/repeat/
  document/poster keep their MNF8 arms (the arm stays silent — no-rule
  files unchanged).
- Fuzz: 525 trials, 0 panics, 1 accepted mutation DIFFED per the
  audit rule: a dims-word flip on the size-free MNF9-const payload yields
  a VALID different-size file — the recorded benign semantic class
  (annotated in scripts/fuzz.py).

## Track-A promotion status vs the milestone list

1. Golden vectors for all six ids: DONE (tests/mnf10_conformance.rs in
   the research crate reads the root fixtures; 3/3).
2. Malformed-input handling: DONE (decoder + fuzz).
3. Full portfolio selection without losing to the existing candidate:
   DONE at the CLI (`--mnf8` flow) — the two-format pick.
4. Photographic opportunity ranking (the Autopsy readout): recorded in
   docs/mnf10-autopsy.md; Track B (shared-model discovery, MNF10-G3
   arms C/D) is the next research lane — NOT started here.
5. crates.io publication of the promoted surface: NOT in this commit
   (Track A's promotion gate = this record; publish waits for a
   version decision alongside the next release).

## What is deliberately NOT claimed

- Photos: no generative win is claimed or implied (the corpus sweep
  confirms none fit — that is the finding that motivates Track B).
- MNF9 as a public RELEASE format: the grammar is promoted in the
  shipped LIBRARY + CLI behind the same --mnf8 flow, but the crates.io
  release naming/tagging decision is Jon's; nothing here renames v1.9.0
  artifacts.
