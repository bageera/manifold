# MNF13-R9-D — Higher-order contexts 15/7 (ctx-15/7 refinement): measured record

Status: MEASURED · all arms roundtrip byte-exact through the R9-A bridge
(C0 anchored byte-identical to the shipped payload on all 22 files;
attribution closes exactly — prequential totals reconcile with entropy-wire
bytes; non-heavy contexts provably untouched on no-warm refine arms).
Instruments: `r9_audit.rs` (`refine*`/`share` modes),
`crates/mnf-mans/scripts/mnf13_r9d_gate.py`; rows
`.research/r9/r9d_rows.json` + `r9d_attrib.json` (+ `_comp`, `_static`,
`gate.log`). Production codec untouched; NO backend identifier changed.

## Design (the work order's registration, arms D0–D4 + C2 comparator)

The refinement adds prev3's coarse band bit INSIDE the heavy band only
(base keys 7 and 15 = prev ≥ 97 buckets, 35 % of model bits, R9-B's
largest per-symbol costs); everywhere else the base 16-key rule stands.

| arm | shape |
|---|---|
| D0 / C0_shipped | frozen backend 5 (anchor) |
| D1M_merged | keys 7+15 share ONE child pair on prev3's band (the mid-flight design, re-run verbatim for provenance) |
| D1_refineA | distinct children per heavy key: 7→16+band3, 15→18+band3 |
| D3a_refine15 | key 15 refined only |
| D3b_refine7 | key 7 refined only |
| D2_refA_w12s | refineA + C2's warm2p12 schedule + `share` (the base heavy key keeps LIVE statistics for its children to mix into while cold; `share` = the ancestor model also takes the symbol's update) |
| C2_w2p12 | R9-C's winner re-run standalone (comparator) |
| compose refine{7,15,M,A} | D4-family compositions measured in the second gate run |

## Measured (22 production streams, Δ entropy-wire vs C0 = 584,804 B)

| arm | Δ B | wins/losses | attribution ctx7 / ctx15 / elsewhere (prequential B) |
|---|---:|---|---:|
| D1M_merged | **+1088** | 2 / 15 | +553.2 / +524.6 / **0.0** |
| C2_w2p12 | −288 | 20 / 2 | 0.0 / −71.5 / −224.2 |
| D1_refineA | −564 | 7 / 14 | −210.8 / −359.7 / **0.0** |
| D3b_refine7 | −208 | 10 / 9 | −210.8 / 0.0 / **0.0** |
| D3a_refine15 | −340 | 6 / 14 | 0.0 / −359.7 / **0.0** |
| D2_refA_w12s | **−996** | 13 / 8 | −333.9 / −489.9 / −174.1 |
| compose refine7+share | −640 | **17 / 4** | |
| compose refine15+share | −656 | 10 / 9 | |
| compose refineM+share | +784 | 10 / 12 | |
| compose refineA (no share) | −456 | 6 / 15 | |

Static pre-registration (omniscient per-context floor, diagnostic only —
`.research/r9/r9d_static.json`, positive = refinement loses bits):
distinct-children refineA −14,172 B aggregate (refine7 −6,444 /
refine15 −7,728; ocean −412, dusk224 −2,513, sand −2,298, slate224
−3,289); **merged refineM +8,631 B** (slate224 +2,601, sand +2,017 —
the merge destroys the gain on exactly the biggest heavy-band files).
Band3 populations support the split (heavy bands split ~40/60, no
starved children — e.g. document: heavy 0 %, 142/0 + 141/141).

## Method honesty (all caught mid-experiment, all fixed before the verdict)

1. The mid-flight `refine` arm's comment claimed per-key children while
   the key rule actually MERGED keys 7 and 15 into one child pair and
   discarded prev2's band bit. Re-read against the code caught it; the
   merged design is now registered verbatim as `refineM` (D1M) and
   measured: it loses statically (+8.6 kB) AND causally (+1088 B) —
   merging the heavy keys destroys the conditioning that carried the
   structure. Its provenance reproduces the mid-flight run byte-exactly
   (+1088, per-file equality vs `/tmp` rows).
2. The mid-flight compose arm's parents were WRONG: `build_tree` gave
   refined slots 16/17 parent = k & 7 = contexts 0/1 (the measured
   +812 was warm-mixing against arbitrary contexts). Fixed to the base
   heavy keys (16,17←7; 18,19←15). The corrected comparison is in the
   table: refineA+warm2p12 WITHOUT `share` (frozen parents — the base
   key never receives symbols under refinement) −456, WITH `share` −996:
   the composition needs LIVE parents, not just any parents.

## What the arms say

- **The heavy band's extra conditionality is real but causal recovery is
  a fraction of the static pool** — the omniscient refined floor says
  −14.2 kB is available; the honest per-file causal arms realize
  −0.2..−1.0 kB (7 %), the same harvest-fraction lesson R9-C taught on
  the warm-up half. Per-symbol structure exists (D1A's gain lives
  ENTIRELY inside ctx7/15 with exact-zero elsewhere — the assert is the
  proof), but per-file outcomes are unstable (7/14): a refined child
  re-learning from scratch beats its parent's model only on the
  photo-heavy streams (slate224 −332, dusk224 +276).
- **The composition (D4's question) answers cleanly: refine + C2 cold
  mixing + LIVE ancestors compose** — D2 −996 B, and refine7+w2p12+share
  reaches 17/4 wins with −640. Without `share` the composition half
  works (−456, frozen parents mix uniform-ish); the live-parent chain
  is what doubles the heavy-side gain (−823.8 inside heavy for D2 vs
  −570.5 for D1A alone).
- **Distribution vs aggregate**: D2 wins the aggregate but 13/8; C2
  keeps the best pure shape (20/2). The refined arms' 8 losses are the
  same class R9-C saw (ctx-0-dominated streams prefer zero intervention;
  slate/dusk/sand photos prefer the refinement) — a per-file picked arm
  covers both classes in the portfolio.
- **Per-file picked portfolio readout** (measured rows, +1 B pick flag
  per file, NOT the R9-E gate itself): best-of-all-D-arms + C2 per file
  = **−1,346 B vs C0** (picker distribution: C2×5, refine7+w2p12s×4,
  D2×3, one-sided/mixed ×10).

## Verdict within the registered contract

- D1–D3 close: prev3's band bit carries genuine conditional information
  in the heavy band; the DISTINCT-children key (not the merged one) is
  the form that exposes it; the causal, complete-file fraction realized
  through the production coder is −0.2..−1.0 kB (refine arms) / −996
  (D2's composition) — modest, unstable per-file.
- D4 closes: the composition works and needs live ancestors (share);
  its best distribution shape is refine7+warm2p12+share (17/4, −640).
- No backend identifier changed (research instrument only). All
  candidates that alter the decoder's model evolution would enter
  production under the standing rule: NEW backend id — **R9-E (full
  production pick vs the 812,180 / 1,452,400 anchors) is the remaining
  gate and the promotion decision stays Jon's call**, now with both
  halves of the regret pool measured (warm-up −288, refinement −996
  aggregate / −1,346 picked).

## Boundaries

- The 22 recompute-anchored production streams only; hubble-class
  wire-decoded and 8 skipped files stay outside (R9-A convention).
- The static floor is whole-file omniscient — a diagnostic bound, never
  transport; the verdict rests on the causal bridge rows.
- `share` changes child+ancestor model EVOLUTION (both sides recompute
  it from decoded bytes; zero transport) — it is a candidate-mechanism
  measurement, not a shipped path.