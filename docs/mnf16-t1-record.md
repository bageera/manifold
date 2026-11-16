# MNF16-T1 record (2026-11-15) — research throughput optimization, first deliverable

P1 removed redundant computation inside the encoder; T1 removes it
across the research program. The funnel (R0 mathematical feasibility →
R1 frozen production trace → R2 information screening → R3 adaptive
replay → R4 experimental wire → R5 portfolio → R6 release) is adopted
as process; this deliverable builds R0's enforcement, R1's persistence,
and the multi-arm R3 runner — with X1-L as the first consumer.

## Deliverables

1. **Versioned production trace** (scripts/mnf16_t1_trace.py): one pass
   of the invariant computations — per mix file, extract_stream, the
   bridge wire-decode, the shipped-schedule per-byte baseline costs,
   the record-grammar walk (positions, modes, lane ids), the X1
   lane-class array, and prod_keys — stored as compressed npz. 22
   traces, 2,542,946 stream bytes, 0 failures. The npz blobs live on
   disk (23.3 MB total, gitignored; regenerable from the committed
   fixtures); the COMMITTED artifacts are the manifests:
   research/manifests/corpus.lock binds every trace to the v1.16.0
   vintage + bridge identity via sha256 (stream and trace), and
   research/manifests/screening.lock pre-registers the T2 screening
   population by a deterministic documented rule (argmin n, argmax n,
   3 nearest-median-n of the trace manifest): the 5 files above.
2. **Shared causal-extractor contract** (scripts/mnf16_t1_contract.py):
   the executable gate for any registered feature — T1 exhaustive
   256-value target substitution, T2 future mutation, T3 structural
   (PrefixView only), T4 exact fast/reference parity — against a loaded
   trace; the registry starts with the X1 lane-class mapping (pass:
   262,144/262,144 substitutions, parity exact, 0.38 s on a trace that
   previously cost a full experiment setup).
3. **Multi-arm runner** (scripts/mnf16_t1_runner.py): compares multiple
   probability-model arms against the SAME frozen stream without any
   encoder invocation — baseline L0 from the STORED trace bits, each
   candidate arm as an independent adaptive state (rust audits on the
   stored stream), reports model bytes AND entropy-wire bytes per arm.
   --screening runs the pre-registered T2 subset. Results land in
   research/results/ (gitignored artifacts).

## Validation: reuse must not change measurements

The runner's default arms are the frozen X1-L set, executed from the
trace alone, then reconciled file-by-file against the recorded LIVE
run (docs/mnf16-x1l-record.md):

| Metric | max diff across 22 files |
| --- | --- |
| L0 baseline | 0.0 |
| L1/L2/L3 gains | 0.0 / 0.0 / 0.0 |
| wire bytes (L0, L1) | 0 / 0 |

| Run | wall time | mode |
| --- | --- | --- |
| live X1-L harness | 332 s (5.5 min) | encode per file + walk + audits |
| trace runner | 41 s (0.7 min) | audits only |
| acceleration | **8x** this arm set | no encodes ever again |

The contract gate reproduces the X1-0 T0–T4 verdicts from the trace
(PASS, exact).

## Notes and limitations

- Traces are bound to v1.16.0 + the bridge tool; a production vintage
  change or bridge change regenerates them (the committed lock detects
  drift via digests).
- Traces are decoder-order only; features still pass T0–T4 before any
  census — the stored annotations expose no future-dependent shortcut
  (the walk materializes prefix-valid information; the contract keeps
  extractors honest).
- The remaining funnel rungs (screening stopping rules R2, portfolio
  and release separation, parallel workers, the G_net early-termination
  table) are process commitments for subsequent experiments, recorded
  here; not built in this deliverable.
- The X1 synthetic-control limitation (fixtures picked to backend 1)
  carries over to the T1 tier design: T1 synthetic positives/negatives
  for content hypotheses need mix-payload fixtures or a documented
  bridge-level control path.

## Disposition

Per priority table: 1 (persistent traces) done; 2 (shared contract)
done; 3 (automatic rejection gates) partially — the executable prefix
contract plus the funnel order; 4 (multi-arm single-pass) done for
rust-arm sets. X1-L measured at 8x lower marginal experiment cost with
bit-identical results. A1 and every future candidate runs on this
substrate. Production untouched: anchor 810,816 B; no backend, wire or
packaging changes.
