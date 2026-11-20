# MNF-HP8-I / HP8-R — incremental representation and redundancy dispositions

Authorization basis: the operator ledger marks HP8-I and HP8-R CONDITIONAL
("entry conditions satisfied by measured findings"). This record applies the
HP8-E measurements (docs/mnf-hp8-e-record.md — committed 200b46d) as the
entry evidence and dispositions both gates. No new instruments; no
production change; integration HOLD throughout.

## HP8-I — incremental representation (CLOSED · NOT MET)

Scope: exact incremental probability/model representation in place of
per-symbol dense construction (counter deltas, maintained aggregates,
decoder-reproducible roll-forward).

Entry analysis against the measured record:
- The counters ARE already incremental: FAST/SLOW step by COUNT_INC with
  maintained Σfast/Σslow (C4-A, promoted in MNF17-P1-C4-A) and halving
  checks per step. There is no remaining un-incremented state class in the
  mix model; `step()` benches at ≈ 202–205 ns isolated, a minor share of
  the walk, against a model layer that is ≈ 100% of per-symbol cost
  (HP8-0 twin attribution).
- The dominant constructible work is the per-symbol dense probability
  VECTOR (256 outcomes) plus its CDF — exactly the structure HP8-E's five
  wire-exact candidates attacked. All reproduce production outputs
  bit-exactly; all measure slower in situ (+8.9% to +13.4% aggregate,
  worst on high-entropy streams). An incremental representation must
  ultimately materialize the same dense vector for the constriction call
  (E3: no public from-CDF constructor), so its saving is bounded by the
  same dense-materialization floor the E2 family failed to undercut.
- The one incremental-representation hypothesis the record has NOT
  measured (skip re-derivation across candidate evaluations within a
  single encode — reusing the vector when the context state is unchanged
  between two candidate arms) belongs to the candidate-evaluation layer
  of the production encoder (mnf8 backend search), which is
  production-path code and remains outside HP8's research-only boundary
  absent promotion authorization.

Verdict: the entry finding required by the ledger ("measured findings"
supporting an incremental win) is present only in the NEGATIVE direction —
exactness-preserving alternatives to dense materialization measured slower
in situ; already-incremental state covers the step path. **HP8-I CLOSED
(NOT MET), with the backend-search vector-reuse idea recorded as a
future promotion-authorized candidate.**

## HP8-R — redundant mathematical operations (CLOSED · NOT MET)

Scope: expert correlation, repeated normalization, equivalent probability
computations (the gauntlet's redundancy classes beyond run structure).

Measured dispositions from HP8-E/HP8-0:
- Repeated normalization: the rebalance/argmax/zero passes are exact and
  were fused (E2c/E2e) — in-situ slower; the separate-pass form is what
  the optimizer executes well (SIMD-friendly). No admissible reduction
  found within the exactness constraint.
- Equivalent probability computations across symbols: exploited exactly by
  E2a–E2e memoization forms — slower in situ (same vectorization floor).
- Expert correlation: the two estimators (FAST/SLOW) feed one mixture
  expression per symbol; their correlation is a MODELING property (EWA
  weight), not a computed redundancy — no measurable duplicate arithmetic
  exists between the two estimator maps (each symbol touches each map
  once; both divisions are distinct computations except where values are
  identical, which is the E1/E2 case already dispositioned).
- Redundant coder work: the constriction pass is single-pass and required
  under its public API (E3).

Verdict: every redundancy class the record can name within the exactness
constraint has been implemented, verified wire-exact, and measured — none
beats the dense loop. **HP8-R CLOSED (NOT MET).** The open candidate that
could invalidate this disposition is a SIMD-shaped algebraic
reformulation of the mixture/rebalance (an E4-carryover; see the E
record's conclusion).

## Standing consequences

- HP8-C/Q remain HOLD per the operator ledger (no new entry evidence).
- Production wire/backends/portfolio: unchanged. All instrumentation
  env-gated `#[ignore]`.
- Next eligible work per the ledgers: none within the HP8 program —
  the program's remaining open items are promotion-authorized-only
  (backend-search vector reuse; SIMD-shaped reformulation).
