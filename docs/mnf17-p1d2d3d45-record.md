# MNF17-P1-D2/D3/D4/D5 — certified termination, ordering, combined validation

**Status**: D2 CLOSED·PASS (prototype + differential proofs), D3
CLOSED·PASS (ordering arms), D4 CLOSED·PASS (combined measurement),
D5 CLOSED·PASS (full validation). All research instruments cfg(test);
production encoder untouched. Version bump/publication remain on HOLD.

## 1. D2 — the termination prototype (`mod p1d2`)

`mix_encode_with_term(params, data, geom, incumbent, stride)` mirrors
the production mix encode byte-exactly; every 256 symbols it evaluates
the D0 bound `B(t) = 32 + 4 + 4·num_words(t)` with checked integer
arithmetic against the live incumbent and returns
`Dominated { at }` — no fabricated payload, no shared state (the
differential proves a dominated rerun yields byte-identical full
wires). Original model and writer untouched.

Differential proof set (all green, release test build):
- completed candidates byte-identical to the exhaustive oracle (4
  fixture classes × 4 backends);
- incumbent exactly equal to the final size ⇒ completes (equality is
  never eliminated — required by the rank-tie rule);
- dominated candidates verified against a full exhaustive re-run:
  final size > incumbent strictly (chain L_final ≥ B > incumbent);
- post-dominance state leakage: none (byte-identical reruns).

## 2. D3 — candidate ordering (`p1d23_census`)

Live strategies over the 8-file cohort (pass-1 exhaustive for ground
truth; strategy passes with real incumbents and stride-256 checks).
**Certificate verification: zero false eliminations; winner (size,
rank) identity + winner byte equality asserted per strategy per file.**

| Strategy | Mix wall (rate-weighted) | vs same-run ORDCTL 177.1 s | vs exhaustive 192.2 s |
| --- | --- | --- | --- |
| D2 — production order + termination | 76.7 s | 2.31× | 2.51× |
| D3 — cheap-first + termination | **68.2 s** | **2.60×** | 2.82× |
| Ordering control (no termination) | 177.1 s | 1.00× | 1.09× |

The ordering-only control isolates the effects: reordering per se
changes ~ nothing (1.09× = run-to-run noise); **termination is the
entire 61.5% elimination**; cheap-first adds the projected B-order
incumbent quality. Dominated arms: D2 975, D3 996 (of 1,216 mix
decisions × per-file); median termination point 56.6k / 47.9k
symbols (D2/D3).

Per-strategy mix wall by file (D3): gradient 0.0 s (vs 23.5
exhaustive), repeat 0.0 (18.3), solid 0.0 (17.7), noise 22.2
(22.2 — the certificate honestly declines to save work where
candidates remain competitive), screenshot 62.0 (67.6), photos
proportional.

## 3. D4 — combined measurement

- Census accounting: all 1,600 arms accounted (1,216 mix complete +
  384 cheap); every dominated arm's certificate re-verified against
  the exhaustive oracle before discharge (`false_elim == 0` asserted).
- Net performance: D3 = **2.82×** on mix-family wall time (release
  build, contemporaneous controls; bound-check overhead stays in the
  sub-0.1% band measured in D1 at stride 256).
- Memory: peak RSS 31.3–31.9 MB across D3/D5 runs — matched with the
  ~30 MB D1 baseline (no termination-era growth).
- Adversarial classes exercised within the cohort: winner-appears-late
  (order-A tail, p75 tau/N ≥ 0.95), equal-size ties (solid/repeat —
  rank order preserved by the (size, rank) lexicographic identity),
  weak incumbent (first mix arm runs unbounded → becomes incumbent),
  no-termination (noise arms run to completion, certificate silent).
- No equivalence memoization was added (attribution clean).

## 4. D5 — full validation (`p1d5_portfolio`)

Virtual production selector = D3 order + certified termination, over
the full 30-file corpus (corpus.lock + 8 non-mix) in BOTH phases and
Hubble, with the two CLI-composed arms the release binary applies
(model + enumeration semantics identical to
`encode_mnf8(Maximum)`; the MNF9 whole-frame exact arm competes
strictly-smaller; records built atlas=true/geom=false = the shipped
CLI defaults):

| Gate | Requirement | Result |
| --- | --- | --- |
| D5 — disabled portfolio | 810,816 B | **810,816 exact** |
| D5 — enabled portfolio | 768,303 B | **768,303 exact** |
| D5 — Hubble | 1,438,848 B | **1,438,848 exact** |
| D5 — wire parity | winner bytes == exhaustive | asserted per file, per phase |
| D5 — reconstruction | exact | 60/60 + Hubble |
| D5 — held-out | zero diffs | plaid/poster/blob/blocks in-corpus, byte-exact |
| D5 — profile behavior | preserved | byte-identity ⇒ unchanged |
| D5 — release gate | green | fmt/clippy/gate clean |
| D5 — memory | matched baseline | peak 31.3 MB |
| D5 — certificate verification | zero invalid | asserted per arm |

Termination prevalence: **3,648/3,840 enabled mix arms dominated
(95.0%)**; Hubble 122/128 — the certificate generalizes to ~1.3M-pixel
photographic streams (its winner arm mix7, rank 11 = the shipped
backend 7, flags 0x45).

Methodology notes retained:
- the CLI composes TWO extra arms beyond the raw portfolio: the MNF9
  whole-frame exact rule ships when strictly smaller (solid_256x256:
  36 B vs the MNF8 pick 62 B; repeat_256x256: 301 B), and the record
  arms are built atlas=true/geom=false (the binary defaults). The
  first two D5 runs missed these and hit the frozen asserts with
  +26 B (missing MNF9) and −124 B/+4 B (geom mapping) — fixed to
  exact CLI semantics and re-verified; totals then landed exactly.
- D2/D3 cohort measurements (and the D1 census) were instrumented in
  the geom=true arm family; D5 reproduces the SHIPPED semantics
  (geom=false). The bound and the termination rule are
  geometry-independent; only arm sizes/tau positions shift.

## 5. Dispositions

- D2 CLOSED·PASS; D3 CLOSED·PASS (B-order adopted: cheap incumbents
  first); D4 CLOSED·PASS; D5 CLOSED·PASS.
- Projected mix-family speedup realized as measured 2.82× (D3),
  within the D1 counterfactual envelope (68.1% avoidance → 3.0×
  counterfactual vs 2.82× measured incl. bounds + interval losses).
- P1-D6 (production promotion) and version bump/publication remain
  HOLD pending the user's disposition.
