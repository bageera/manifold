# GSE-09 original measured results — verbatim from the GSE-09 report

PROVENANCE: the numbers below are quoted VERBATIM from the GSE-09
experiment report (received 2026-10-30, original session). The
original prototype package was not recoverable on this machine; these
tables are the primary provenance for the original measurements. They
are NOT regenerated and MUST NOT be re-labeled as outputs of
`prototype_reproduction.py` (that script produces its own, separately
labeled JSON files).

## 1. Measured compression results (original session)

128×128 byte arrays · 16 KiB per dataset · full serialized sizes ·
lower is better.

| Dataset                 | GSE-09   | Change vs. zstd-19 |
|-------------------------|----------|--------------------|
| Constant regions        | 94 B     | +62.1%             |
| Mathematical regions    | 94 B     | −97.9%             |
| 80 sparse edits         | 293 B    | −94.6%             |
| Mixed structured/random | 8,254 B  | −20.9%             |
| Smooth noisy data       | 12,099 B | −6.6%              |
| Uniform random          | 16,394 B | −0.02%             |

Note from the original report: results are from deterministic
synthetic fixtures, not independent real-world datasets; the strong
structured-data results reflect fixtures matching the generator
family.

## 2. Larger 64 KiB validation (original session)

256×256, same model family, newly generated test data.

| Dataset      | zstd-19  | GSE-09   | Outcome          |
|--------------|----------|----------|------------------|
| Constant     | 124 B    | 350 B    | Lost             |
| Mathematical | 18,097 B | 350 B    | Won              |
| Sparse edits | 19,402 B | 1,156 B  | Won              |
| Mixed        | 41,156 B | 30,950 B | Won              |
| Smooth noise | 48,146 B | 48,436 B | Lost             |
| Random       | 65,549 B | 65,546 B | Effectively tied |

Mixed-data improvement survived at 64 KiB: 24.8% smaller than
zstd-19. Negative result preserved: the noisy fixture's small
advantage at 16 KiB disappeared at 64 KiB — method is sensitive to
data structure and block size.

## 3. Query performance (original session)

For the 128×128 mixed fixture:

| Query method            | Median latency |
|-------------------------|----------------|
| Direct NumPy region sum | 1,928 ns       |
| Full prefix-sum index   | 403 ns         |
| GSE-09 region sum       | 33,816 ns      |

GSE-09 was ~17.5× slower than direct scanning for arbitrary rectangle
queries. Quadtree cached aggregates cover complete regions only;
partial-region queries remained unoptimized. THE COMBINED
STORAGE-AND-QUERY GATE FAILED — this negative verdict is part of the
frozen record.

## 4. What GSE-09 established (original report, preserved)

- Recursive partitioning works: different regions select different
  exact representations.
- Sparse exceptions viable: modified bytes represented without
  storing entire replacement blocks.
- Mixed-data compression improves; advantage persists at 64 KiB.
- Simultaneous compression and traversal acceleration NOT achieved.
- Aleph-null addressing remains separate: GSE-09 tests finite
  recursive regions only.

## 5. Original reproduction command (preserved)

    python gse09.py --side 256 --min-tile 8 --queries 80 --repeats 3 \
      --output results_256.json

Disposition block from the original report: GSE-09 passes exact
reconstruction and synthetic mixed-data compression, fails the
combined storage-and-query-speed gate; recommends GSE-10 analytical
query engine in Rust retaining the GSE-09 format.