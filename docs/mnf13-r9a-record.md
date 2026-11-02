# MNF13-R9-A — Byte-equal backend-5 extraction + prequential audit: record

Status: MEASURED · the gateway instrument for R9-C/D (context-model
experiments on production streams). The binary gate is MET on every
applicable file; one file carries an honest scope label (below).
Production codec untouched.

## What shipped (instrument)

- `crates/mnf-mans/examples/r9_audit.rs` — the shipped mix coder
  (backend-4 AND backend-5 constants; shipped key rules computed
  internally from prev/prev2) with three modes: `e` wire bytes, `p`
  TRUE payload decode (fresh models, key schedule restarting at
  (0,0)), `a` prequential audit — per-symbol −log2(p_i) under the
  2^24 fixed-point vector that coded it, exact roundtrip asserted on
  every encode/audit run, ALL failures clean `ERR` exits (never
  panics).
- `crates/mnf-mans/scripts/mnf13_r9a_gate.py` — extraction (the ID4
  walker's grammar + r6_extract's CD relabel + shipped predictor edge
  semantics), both-direction anchors, the three accounting columns,
  per-symbol bits saved for stage-B analyses, honest skips never
  merged with anchors.

## Measurement contract — MET (31 files: 30 corpus + hubble)

| file class | n | anchor direction(s) | verdict |
|---|---|---|---|
| backend-5 mode-0-only records (all photos + document + screenshot) | 22 | recompute == payload (byte-identical) AND payload decode == stream | both ANCHORED |
| hubble (record file with 277 law/const/ref tiles) | 1 | recompute ✗ (extraction would need the pick's arm choices = re-running the search); payload decode == wire-decoded stream, structure-walked legal | audited at `anchor_source=wire-decode` |
| backend-1 / token-wire / MNF9 files | 8 | not mix-addressable | honest skip (never counted as anchor anything) |

**Anchor failures: 0.**

## The three columns (sums over the 23 audited files)

| column | bytes | note |
|---|---:|---|
| L_model (prequential Σ −log2 p / 8) | 2,037,050.1 | what the model "thought" it spent |
| L_entropy_wire (payload minus len-words) | 2,037,168 | what the coder actually wrote |
| **coder overhead** | **+117.9** | range-coder rounding + per-stream word padding — **≤ 0.006% of streams** |
| L_complete (whole files) | 2,037,996 | framing included |
| diagnostic static floor | 2,027,379.0 | per-context empirical entropy, zero-model state (NOT achievable transport — context counts would cost bytes) |
| **adaptation regret** | **9,671.1 | 0.47% of L_model** — the price of learning-from-zero; per file 64–2,577 B, hubble 2,576.8 (0.18% of its stream) |

## The R7/R8 hypothesis sharpened (what R9-C/D must beat)

The model-vs-static gap is ALREADY tight (0.47%): the shipped dual-EWA
mix wastes little to adaptation on these streams. The R1 hypothesis
(−7..−8% per photographic file) therefore lives in a different
dimension: NOT less adaptation regret, but RICHER conditionality —
per-symbol bits concentrated where the 16-key rule can't see. R9-A's
attribution already shows WHERE: **ctx 0 (prev-byte bucket 0 = the
zero-runs) carries 85.2% of document's model cost and 80.0% of
screenshot's** — 167k–768k symbols flowing through ONE context whose
statistics the coder must learn per run. That is the concrete target
for R9-C hierarchical sharing (share sparse contexts with their
parents) and R9-D higher-order keys (refine within ctx-0).

## Method honesty (all caught by instrument assertions, all fixed)

1. Bridge decode reused the encode loop's tail (prev, prev2) —
   poisoned the first context when tail-key ≠ first-key
   (`Frontend(InvalidData)` on document; gradient passed by
   coincidence, tail key == first key). Decode now restarts at (0,0)
   as the shipped decoder does.
2. Bridge `d` mode was never a decoder — treated stdin as symbols and
   roundtripped them, returning the input (spurious "decode-side
   mismatch"). Replaced by the true payload decoder (`p` mode);
   first word of body skipped as the length word (the next bug).
3. The driver's first payload-decode call passed `payload[4:]` with a
   stale length header — the decoded "stream" was garbage symbols;
   the 848-diff byte compare (relabel localization) was polluted by
   it and re-run clean.
4. THE CAUSALITY FINDING (proven by byte comparison after 1–3 were
   fixed): the mix coder codes the CD-RELABELED record stream —
   encoder assembles records → relabel → mix-code; decoder
   mix-decodes → unrelabel (`src/mnf8.rs` 1283–1291 ↔ mnf6.rs).
   Extractors must relabel BEFORE assembling the mix symbol stream.
5. Scratch venv `mnfpca2` was PRUNED (24 h idle rule) mid-run —
   rebuilt from the one-step recipe (skill record: venv name is not
   durable state; the rebuild procedure is, and it works).

## Scope of closure

CLOSED (R9-A): byte-equality of a pixels→grammar→relabel→mix
reconstruction against the shipped payload on every mix-addressable
mode-0-only file in the corpus (22 files, both directions); the
prequential cost census reconciles (L_model ≈ L_entropy_wire to
≤ 0.006% overhead); adaptation regret quantified (0.47%); honest
disposition for every non-applicable file (8 skips, 0 failures).

OPEN (explicitly staged): hubble-class RECORD files carrying
non-mode-0 tiles cannot recompute-anchor without reproducing the
pick's arm choices (out of instrument scope by ADR-010(b)); they
audit fine at wire-decode provenance. R9-B (per-symbol statistical
attribution: adaptation trajectory vs context statistics) reads the
saved `/tmp/r9a_bits/*.npy`; R9-C/D experiments use these exact
streams through the same bridge; R9-E gates any winner complete-file
vs 812,180 / 1,452,400 anchors.

Comparator rules imported unchanged: identical symbol streams,
consistent units; the static floor stays diagnostic.