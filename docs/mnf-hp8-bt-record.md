# MNF-HP8-BT — balanced ternary and radix probability-factorization record

Deliverables: research/hp8/bt/representation.py (BT0), bt/trees.py
(BT1–BT3); CSV research/results/hp8-bt-comparison.csv.

## BT0 — representation correctness (GATE PASSED)

`representation.py`: exhaustive 256/256 round-trips for 11 representations
— unsigned/signed/zigzag binary, signed-magnitude, unsigned ternary,
balanced ternary (6 trits, {-1,0,+1}), radix-4/8/16, mixed-radix (4-8-8,
8-4-8) — with invalid-branch accounting (baltern digit space 729, 473
pruned; signmag 384 space, 128 pruned; radix-8 512 space, 256 pruned; the
complete powers-of-two arms have zero invalid branches). Signed-domain
arms close through the signed mirror r = x or x−256. No escape codes,
padding, or termination symbols introduced.

## BT1 — exact probability conservation (GATE PASSED)

Every tree arm's conditionals are INTEGER mass ratios built bottom-up from
production frequency vectors, so ∏ mass(child)/mass(node) = f(x)/Σf holds
exactly; verified symbol-by-symbol with Fraction arithmetic on uniform,
concentrated, floor-sparse, Dirichlet and native production vectors
(9/9 arms OK). Instrument corrections landed pre-verdict (recorded): the
balanced-ternary tree requires the {−1,0,+1} digit alphabet, canonical-form
leaves, and BYTE-domain leaf indexing — the naive signed-index mapping
drove all 224 small-value leaves to the wrong rows; ZSM's +128 is
unrepresentable and must map to None.

## BT2 — per-node precision ablation (MEASURED)

Per-node LR quantization at Q ∈ {16,12,8}; metric = mean distribution
entropy per row (400 rows × forest/terrain/screenshot prefixes + the
synthetic cohort):

| arm (forest rows) | Q=16 | Q=12 | Q=8 | decisions/sym | table cells |
|---|---|---|---|---|---|
| T0 direct categorical | 3.1493 | 3.2007 | **8.0000** | 1 | 256 |
| T1 binary tree | 3.1490 | 3.1490 | 3.1564 | 8 | 510 |
| T2 zigzag (=binary, permuted leaves) | 3.1490 | 3.1490 | 3.1564 | 8 | 510 |
| T3 unsigned ternary | 3.1490 | 3.1491 | 3.1581 | 3–6 | 387 |
| T4 balanced ternary (pruned) | 3.1490 | 3.1491 | 3.1676 | 2–6 | 390 |
| T5 radix-4 | 3.1490 | 3.1491 | 3.1649 | 4 | 340 |
| T6 radix-8 | 3.1490 | 3.1495 | 3.1844 | 3 | 292 |
| T7 radix-16 | 3.1490 | 3.1495 | 3.1844* | 2 | 272 |
| T8 zero/sign/magnitude | 3.1491 | 3.1494 | 3.1879* | 1/8 | 517 |
| T9 mixed 4-8-8 | 3.1490 | 3.1495 | 3.1844* | 3 | 292 |

(*values from the same-run columns of hp8-bt-comparison.csv; T7/T9 share
the 8-16 boundary family.)

The gauntlet's predicted effect is empirical: at Q=8 the 256-way
categorical is FORCED UNIFORM (8.000 bits/sym — every frequency floored to
1 at total 256, cf. HP8-P floor_cells = rows×256), while every factorized
arm keeps 3.15–3.19 bits/sym. Factorization makes small budgets possible
by distributing the floor across small per-node alphabets. At the budgets
production actually uses (Q≥16) all arms converge to the same entropy —
H(x) = H(T(x)) exactly as §3 states.

## BT3 — tree construction cost (MEASURED structurally)

- Bottom-up mass aggregation for a full binary tree = 255 integer adds
  (O(#nodes)); every arm's tables derive from the same production vector,
  so construction adds one full pass OVER the categorical work the model
  already does. A tree built AFTER the 256-probability vector (the only
  exact construction available from the production model) strictly ADDS
  work — the increase identified in the gauntlet's BT3 warning is real
  for Manifold's architecture.
- Per-symbol coder cost: a binary path costs 8 range-coder calls vs ONE
  categorical call. Per HP8-P3's measured coder economics (bytes flat
  across precision; the production coder consumes ONE model per symbol),
  the tree trades 1×256-cell CDF build for 8×small conditional builds —
  and the 256-cell build is already fused into the model step that
  trees cannot eliminate. No measured computational win.
- Structural table sizes (dirichlet vector): binary 255 nodes / 510 cells;
  radix-16 17 nodes / 272 cells; baltern 135 / 390 (2–6 decisions);
  ZSM 262 / 517 (1 or 8 decisions; zero symbols cost ONE decision — the
  concentrated-at-zero win the ZSM shape promises is visible in the
  decision-count column, not yet in a coder).

## BT4 / BT5 — CONDITIONAL, NOT EXECUTED

Gate condition ("plausible representational or computational advantage
under matched controls") NOT met: no state reduction (tree tables ≥ direct
cells), no wire win at production budgets, no computational win (added
construction pass; ≥2× coder calls). Shared-hierarchical state (BT4) and
digit-ordering (BT5) remain CLOSED-unless the HP8-E/C census changes the
per-symbol cost model.

## Gate disposition

GATE: "radix representation demonstrates measurable model-state, wire, or
computational benefit" — **NEGATIVE.** Verdicts: BT0 POSITIVE (correctness
artifacts, reusable), BT1 POSITIVE (exact-conservation proofs, reusable),
BT2 CONDITIONAL-positive (factorized trees are the ONLY precision-robust
shape below Q=12 — carried as a design fact for any future low-precision
experimental backend), BT3 NEGATIVE, BT4/BT5 CLOSED (gate unmet),
production verdict per §18: NO portfolio change.
