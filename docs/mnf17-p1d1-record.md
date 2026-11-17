# MNF17-P1-D1 — counterfactual termination census

**Status**: all arms ran to completion (no candidate was aborted); the
certified lower bound (docs/mnf17-p1d0-record.md) was evaluated at
every symbol boundary of every mix arm, and the four incumbent
strategies were computed counterfactually from the same series. Runs:
screenshot pilot + full 8-file cohort; 1,600 arms (1,024 mix), **239.26
million mix symbols**, 192.2 s of contemporaneous C4-A mix encode time.

## 1. Bound validity (the proof obligations)

| Clause | Result |
| --- | --- |
| `num_words` monotone non-decreasing per arm | **ALL arms** |
| Exact final accounting `4·series[N]+4 == payload.len()` | **ALL arms** |
| Container prefix `HEADER8 == 32` | asserted per arm |
| Probe wire byte-identity vs production encode | asserted per arm |

## 2. Headline: certified avoidable mix-symbol work (cohort)

| Incumbent strategy | Symbols avoided | Time-weighted | Bound-check overhead |
| --- | --- | --- | --- |
| D1-A — production order | **61.4%** | 59.8% | 0.098% @ every symbol |
| D1-B — zstd/zlib complete-first | **68.1%** | 66.8% | 0.098% |
| D1-C — progressive cost-asc | **63.1%** | 61.6% | 0.098% |
| D1-Oracle — final winner known | 69.0% | 67.6% | (reference only) |

The oracle is 0.85 pp above D1-B: **an ordered evaluation with cheap
incumbents first captures nearly the entire bound opportunity.**
Production order (D1-A) already recovers 61.4% because the first mix
arm of each stream sets an immediate incumbent for its siblings.

Screenshot (the priority fixture): A **69.8%** / C 71.4% / O 77.7%.

## 3. Interval ladder (strategy A)

| Check interval | Symbols avoided | Overhead |
| --- | --- | --- |
| every symbol | 61.446% | 0.0979% |
| every 16 | 61.443% | 0.0061% |
| every 64 | 61.433% | 0.0015% |
| every 256 | **61.393%** | **0.0004%** |

Crossings are early and monotone: **checking every 256 symbols loses
only 0.05 pp while the overhead drops to 0.0004%** — the bound check is
effectively free at coarse intervals. (`num_words()` per-symbol cost
measured: 0.19 s over 192.2 s = 0.10%.)

## 4. Per-file structure (strategy A / oracle)

| File | mix symbols | A | C | Oracle | enc s |
| --- | --- | --- | --- | --- | --- |
| screenshot_640x400 | 98.5 M | 69.8% | 71.4% | 77.7% | 73.5 |
| gradient_320x240 | 29.6 M | **93.2%** | 95.6% | 99.3% | 25.6 |
| repeat_256x256 | 25.3 M | **93.7%** | 96.3% | 98.5% | 20.1 |
| solid_256x256 | 25.2 M | **84.8%** | 88.9% | 89.1% | 19.0 |
| noise_256x256 | 25.2 M | 1.2% | 1.2% | 1.2% | 23.5 |
| plaid_192 (held-out) | 14.2 M | 24.5% | 25.0% | 48.8% | 12.2 |
| forest_224x160 | 13.8 M | 8.5% | 7.9% | 20.0% | 12.0 |
| dusk_160x120 | 7.4 M | 8.5% | 7.7% | 23.7% | 6.4 |

Structured/degenerate inputs approach the ceiling (gradient 99.3%
under oracle); noise is honest — losing candidates do not lose early
(nothing to certify); photographic entropy is the residual cost.

## 5. Where crossings sit

- Strategy A: median `τ/N = 0.46`, p25 = 0.06 — most arms are certified
  shortly after starting; the tail runs long (never-crossed 40/1,024:
  the eventual winners + competitive arms, correct behavior).
- Oracle: median `τ/N = 0.34`, p25 = **0.01**, never-crossed 4/1,024 —
  with a perfect incumbent almost every losing arm is certified
  immediately.

## 6. D1 evidence checklist (per the authorization)

- Candidate identity: file/backend/kidx/planar/reference/arm columns.
- Original candidate priority: `rank` = bidx·40+arm (the tie order).
- Structural attribution: backend + (kidx: pred kind, planar, reference).
- Complete symbol count: `N` (raw stream length).
- Complete wire size: `size` (assembled container bytes).
- Incumbent at evaluation: computed per strategy (A orders exact;
  B/C/Oracle offline from the recorded sizes).
- First certified crossing: `tau_*` per strategy/interval.
- Symbols avoided: columns + per-file aggregate above.
- Final bound validity: monotone/exact flags, ALL PASS.
- Bound-checking cost: `probe_us` per arm; §3 ladders.
- Winner preservation: no arm was actually eliminated (census-only);
  the exhaustive winner is the production pick already proven
  byte-identical in C5 (1,216/1,216 replay) — zero-regret holds by
  construction, and D2 must re-prove it after termination is wired in.

## 7. Dispositions

- D0: record + theorems landed (`mnf17-p1d0-record.md`), all clauses
  [D1-verified] over 239 M symbols.
- D1: CLOSED·PASS — certified opportunity measured, bound overhead
  ≈ 0.1%, interval-256 = 61.4% recovery at 0.0004% overhead.
- D2 (implementation): HOLD pending the user's disposition against this
  census. The natural deployable shape: D1-B ordering + interval-256
  bound checks ⇒ 68% fewer mix symbols at essentially zero cost;
  production promotion still subject to D3–D6 validation.
