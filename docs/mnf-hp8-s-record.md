# MNF-HP8-S — sparse/structured probability-representation record

Census: research/hp8/structure.py → research/results/hp8-structure.csv over
the same 28-image × 40k-row production-vector population as HP8-P.

## Findings (MEASURED)

| quantity | mean | note |
|---|---|---|
| effective alphabet (participation ratio) | **13.3 symbols** | concentrated |
| top-1 probability mass | 0.444 | — |
| top-8 mass | 0.878 | 8 symbols carry 88% |
| adjacent-equal frequency pairs | **235.9 of 255** | ~93% of the vector is runs of equal values (S3 run-length structure) |
| floor (freq=1) cells | 0 | at 2^24 the estimator never parks a symbol on the floor (S1 decomposition has no floor pressure) |
| signed-mirror symmetry L1 | 0.232 | residuals NOT symmetric (S4 approximate-symmetry would cost 23% mass error) |
| piecewise-constant-16 approximation L1 | 1.37 | fails; S5 parametric families (piecewise/geometric) cannot track the row-to-row shape |
| split-half temporal L1 | 0.072 | vectors temporally stable (S6: per-image aggregates carry most structure) |

## Interpretation

The repeated-equal-value structure (236/255 pairs) comes from the
production estimator's quantized posteriors — the two-rate adapters park
many symbols on identical frequencies. In principle a run-length or
top-k-plus-flat-residual representation stores a comparable vector in far
fewer than 256 cells. BUT:

- Storage is not the cost driver at production precision: the vectors are
  CONSTRUCTED per symbol from counters (HP8-0: ~743 ns/symbol model loop)
  and fed to a coder that consumes them once; serialized per-symbol vectors
  are charged in neither production nor any wire-changing candidate.
- The measured candidates with actual wire economics (HP8-P/Q16) change
  zero bytes; a structured representation would have to REPLACE the model
  state itself (a hierarchical/shared estimator = BT4-class work whose
  gate is unmet per BT3).

## Gate disposition

"Memory, computational or description-length advantage demonstrated": the
census DEMONSTRATES exploitable structure exists in the vectors (S3 run
mass 93%, top-8 mass 88%) but the candidate representations measured do
not connect to a cost that production pays. **Verdict: INCONCLUSIVE** —
the structure census is preserved as the entry evidence for any future
shared-state experiment (HP8-R/HP8-E), with the specific numbers above as
its baseline.
