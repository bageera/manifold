# MNF16-X1-L record (2026-11-15) — lane-class conditioning, coder-level

One bounded coder experiment, authorized after the X1 content census
closed the cross-channel content hypotheses. Hypothesis: the SEMANTIC
ROLE of a wire symbol (its lane class in the record grammar) carries
predictive information backend 6 does not model — `I(S; L | C6) > 0`
where L is decoder-available, not transmitted.

## Frozen spec (in the instrument docstring before the corpus run)

Class mapping = the EXACT X1 availability classification: class 1 =
residual byte at lane > 0; class 0 = lane-0 residual or framing byte.
Arm L-x: every base context 0..19 gains ONE lane child (context 20+k,
space 40) used by class-1 bytes; class-0 bytes keep the production
model. Children carry NO feature content — the mapping is structural
metadata. Lane bytes are decoder-derivable from the record walk (the
X1-0 T0–T4 proof, scripts/mnf16_x1.py PrefixView) and are NOT
transmitted (the 'e' wire accounting excludes them). L1 children fresh
(parent 0xFF, no propagation); L2 children parented to their base
context with child-local ancestor propagation (`laneparent` token —
the shipped hierarchical-refine pattern; base-key share semantics
untouched). L3 = within-file permutation of the class array (counts
exactly preserved; seed 17). L-H = frozen L1 on Hubble, conditional on
corpus success. Additive instrument tokens (`lanechild`, `laneparent`)
in r9_audit.rs; every mode without them is untouched.

## Gates

| Gate | Result |
| --- | --- |
| Identity (L0) | PASS — L0 total = 583,366.1 B, the S3-0 corpus anchor EXACTLY (+0.0); Hubble L0 = 1,452,320.1 B, the recorded holdout anchor exactly |
| Prefix invariance | PASS — lane class is a pure function of the record walk state (w/h/tile_count/flags + mode+meta+body offset): the X1-0 classification, validated exhaustively in the X1-0 record (5.77M target substitutions; fast/reference parity) |
| Decoder availability | PASS — the walk regenerates the class at probability time from the decoded prefix + container metadata; zero transmission; wire accounting excludes it |
| Adaptive model | PASS — corpus +41,720.9 B prequential; holdout +12,653.0 B |
| Control | PASS — L3 shuffled: corpus −5,222.0 B; holdout −1,461.0 B (real-vs-shuffled separation ≈ +46.9 kB corpus) |
| Wire accounting | PASS — 'e'-mode bytes including framing: corpus 583,440 → 541,732 (**+41,708 B**); Hubble 1,452,364 → 1,439,712 (**+12,652 B**) |
| Resources | lane-class generation: python walk 7.6 s/224×160-class file, ~365 s on Hubble's 2.63M bytes (instrument cost; a Rust walk is trivial and deferred per the standing guardrail); audit 6.2 s / ~98 MB RSS on Hubble; audit 1–11 s per corpus file |

## Results

Corpus (22 mix files, 2,542,946 stream bytes; L0 = the exact shipped
schedules):

| Arm | model cost B | wire B | gain vs L0 |
| --- | --- | --- | --- |
| L0 shipped | 583,366.1 | 583,440 | — |
| L1 lane children (fresh) | 541,645.2 | 541,732 | **+41,720.9 / +41,708 (7.15%)** |
| L2 hierarchical | 562,042.6 | 562,116 | +21,323.5 |
| L3 shuffled | 588,588.1 | 588,668 | −5,222.0 |

L1 is positive on **22/22 files** (+17.4 … +4,008.5 B). L2's hierarchy
halves the gain — fresh children win.

Hubble holdout (L-H, frozen L1, single shot; backend-5 stream; L0 = its
own shipped schedule): model **+12,653.0 B (0.87%)**, wire +12,652,
L2 +3,628, shuffled −1,461.

## Interpretation

The estimator's availability number (+3,989 B) was a large
UNDERESTIMATE: static per-file histograms cannot see the locally
adaptive structure that per-(context, lane-class) ONLINE models
capture. At the actual adaptive coder the symbol-role split is worth
7.15% of the mix payload corpus-wide at zero transmission cost,
positive on every file with a mix payload, and confirmed on the frozen
holdout with the shuffled control negative everywhere.

This confirms the broader hypothesis: Manifold's record grammar
contains statistical information about the MEANING of a symbol (its
lane role) that the entropy model currently treats only as a numerical
value. The distinction between symbol history and symbol role is now a
measured, wire-realizable, zero-transmission information source.

## Disposition

X1-L closes positive. X1 closes. Per the promotion contract: an
independent self-contained decoding proof and complete Maximum
portfolio accounting are REQUIRED before any promotion decision; no
production backend identifier, no production wire change; the 810,816 B
anchor stands untouched. MNF16-A1 (acquisition signatures) is next in
queue. Run artifacts: /tmp/mnf16_x1l_full.log, /tmp/mnf16_x1l_hubble.log,
/tmp/mnf16_x1l.json.
