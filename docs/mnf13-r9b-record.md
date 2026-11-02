# MNF13-R9-B — Statistical attribution: record

Status: MEASURED on the saved R9-A per-symbol bits (no codec
re-extraction). Instruments: `crates/mnf-mans/scripts/mnf13_r9b_attrib.py`;
rows `/tmp/r9b_rows.json` (22 recompute-anchored corpus files); durable
copies + hashes in `.research/r9/` (gitignored, recipe in MANIFEST.md).

## The four quantities, file-aggregated

| ctx (key rule) | mean bit share | mean b/sym | measured regret |
|---|---:|---:|---:|
| 0 (prev=0, band any) | 24.3% | **2.06** | 692.5 B (9.8%) |
| 7 (prev≥97, band0) | 18.8% | 2.75 | 767.3 B (10.8%) |
| 15 (prev≥97, band1) | 16.0% | 3.25 | 781.6 B (11.0%) |
| 1 (prev 1–2) | 14.4% | 2.58 | 633.6 B (8.9%) |
| 9 (prev 49–96, band1) | 8.2% | 3.04 | 653.8 B (9.2%) |
| 8 (prev 49–96, band0) | 7.9% | 2.37 | 636.8 B (9.0%) |
| 11/10 (97-ish, band1) | 4.0/3.6% | 4.79/3.80 | ≈0.25 kB each |
| 2..6, 12..14 (mid/small) | ≤2.3% each | 3.6–7.0 | ≤0.2–100 B |

Structure answers (Jon's five):

1. **Ctx-0's 85% (document) is a POPULATION effect, not an
   inefficiency**: it has the LOWEST bits/symbol of any context
   (2.06 mean; document's 0.02 — the zero-runs) while carrying the
   most symbols. Cost share ∝ population, exactly the honest reading.
2. **Regret is nearly UNIFORM across contexts** (8.9–11.0% each for
   the six big ones): no context is "adaptively starved" relative to
   others. Per-context static-floor regret mirrors population.
3. **Regret is COLD-START-shaped, all 22 files**: the first 10% of
   the stream carries ~9× the per-symbol regret rate of the rest
   (e.g. document 0.00192 vs 0.00021); every file's thirds show
   front-loading (document: 196.0 / 181.2 / 136.5 B).
4. **Transitions are transition-RICH**: the 32 symbols after a key
   change cover a disproportionate cost share (document: 21.5% of
   symbols, 105% cost-share multiplier — windows overlap: the
   direction is what matters; every file shows >1.5× density at
   transitions vs steady state).
5. Mid-bucket contexts (3–6, 12–14) are tiny (≤2.3% bits, ≤0.3%
   regret each) — sparse-context sharing there can win almost
   nothing on this corpus.

## What this MEASURABLY targets

- **R9-C (hierarchical sharing)**: cold-start + transition regret
  dominate; uniform across contexts. The sharing form should target
  EARLY-STREAM statistics and post-transition warm-up (e.g. parent-
  initialized estimators, sharing across the 8×2 bucket hierarchy),
  NOT sparse small buckets (they cost nothing here). Ceiling: total
  regret 9,671 B corpus-wide (1.2% of L_model) is the maximum
  recoverable by ANY adaptation improvement; realistic wins are a
  fraction of it.
- **R9-D (higher-order contexts)**: the expensive-per-symbol contexts
  (15 at 3.25–5.06 b/sym on slate/dusk/sand) mix two bands of heavy
  pixels. A refined key inside ctx-15/7 (the ≥97 population) attacks
  the largest per-symbol costs; population is there to support it
  (16–24% of bits each).
- The hubble-style streams: regret 0.18% — near-nil adaptation
  headroom; its opportunity (if any) would also have to come from
  richer conditionality, measured identically.

## Boundaries (honest scope)

- The static floor is per-context empirical entropy with the WHOLE
  file visible — a diagnostic, not transport (as pre-registered).
- Transition windows (32 B) overlap at high transition density; the
  readout is the density ratio, not absolute attribution.
- The corpus is 20 photographs + document + screenshot; token-wire
  and backend-1 files stay outside (no model-cost surface).
- Attribution runs on RECOMPUTE-anchored files only (22); hubble's
  bits exist but its symbol stream is wire-decoded — same key rule,
  labeled provenance; excluded from cross-file means to keep the
  clean set homogeneous, checked separately (regret 2,576.8 B
  consistent with the distribution).

## R9-C/D pre-registration (from THIS attribution, not proxies)

Gate any candidate: complete-file wire through the SAME bridge
('e'-mode mode string), anchors byte-exact, three columns, against
the shipped constants — and against the shipped pick's FULL portfolio
per R9-E (never worse than 812,180).

Priority measured by this record: (a) parent-shared warm-up
(cold-start) — bounded upside ~5–9 kB corpus (the regret pool);
(b) ctx-15/7 refinement — bounded upside the b/sym gap between
ctx-15 (≈3.3–5) and per-band splits, worth one bridge experiment.
Anything beyond (sparse-mid-bucket sharing, symbol-representing
changes) is DENIED by the numbers before implementation: the
population is absent.