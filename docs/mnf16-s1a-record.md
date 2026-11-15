MNF16-S1-A/S1-B record (2026-11-15) — incremental conditional-MI census,
stage 1: shipped balanced record streams

Scope (per Jon's S1 program): measure whether residual symbols carry
information beyond their stream context — S1-A predictor disagreement
and uncertainty, and the S1-B cross-scale gradient preview — using ONLY
decoder-causal features and the SHIPPED wire, decision discipline =
causal prequential evaluation on held-out stream suffixes. No wire
changes; no format proposal. Measurement only.

Method
------
Census substrate: the shipped balanced-profile files (backend 1 =
zstd container whose payload decompresses to the RAW per-tile record
stream — scripts/residual_census.py's ID4 walk, reused as-is with the
lossless mode-0 recon anchors asserted per tile). Feature families, per
residual symbol of every mode-0 tile (law-mixed tiles walked but
excluded from the population):

  C  = bucket(prev wire symbol, 16 bins), stream order.
  D  = max-min of {a, b, cc, plane, MED} over the SAME plane's causal
       recon (the recon == the image under losslessness), 8 buckets.
  G8 = mean |west8| + |north8| same-plane gradient, 7 buckets.

  I(R; F | C) = H(R|C) - H(R|C, F)

Histograms built on the first 80% of each stream (wire order = decode
order); the improvement is EVALUATED on the final 20% (prequential;
cross-entropy under train-built models on eval occupancy). In-sample
numbers are the upper bound. Instrument:
crates/mnf-mans/scripts/mnf16_s1a_census.py; per-symbol csv at
/tmp/mnf16_s1a_census.csv (26 files / 2,535,168 symbols).

Result (aggregate, per-symbol millibits x8 and bytes)
-----------------------------------------------------
  Feature    in-sample        prequential
  D (disagreement)  0.863 mb8/sym   0.301 mb8/sym  (273,631 / 95,254 B)
  G8 (8px gradient) 1.264 mb8/sym   0.916 mb8/sym  (400,449 / 290,408 B)

Decision-relevant reading
-------------------------
1. BOTH families carry prequential-positive information beyond the
   one-symbol context — G8 (cross-scale conditioning) carries ~3x the
   mass of predictor disagreement on this corpus. S1-B's preview is the
   stronger signal, matching Jon's stated hypothesis ordering.
2. CAVEAT (the ceiling of stage 1): C here is a 16-bin
   first-previous-symbol context — WEAK relative to the production
   mix-coder context schedules (backend-6's 20-ctx prev3-split, etc.).
   Monotonicity: information measured against a weaker C is an
   UPPER BOUND on information beyond the stronger production C. The
   numbers justify the next stage; they do not justify a wire change.
3. Population honesty: the census reads mode-0 residual tiles only
   (the law-mode tiles are the shipped conditional machinery already —
   their bodies are smaller and are excluded; coverage stats in the
   csv). Coverage: 26/30 censused; 3 files are token-wire at the
   balanced pick (heldout/blocks, heldout/document, screenshot — no
   record stream by grammar) and solid_256x256's balanced container did
   not frame as raw zlib (skip-error, recorded).
4. Wire accounting is NOT attempted at this stage (Jon's stage gate):
   the prequential bits are gross conditional-MI, not net encode
   savings; entropy-coder overhead, feature transmission (all features
   are decoder-causal — zero transmission), and the production context
   join all sit in stage 2.

Stage gates for the next step (per the S1 method)
-------------------------------------------------
  1. Rust-bridge stage: decode the mix coder's payload (r9_audit 'p'
     mode + backend-6 constants already parametrized) to replicate the
     EXACT production context per symbol, re-measure I(R; D|C_prod) and
     I(R; G8|C_prod) — the promotion-deciding measurement.
  2. Only surviving candidates advance to a wire experiment with the
     full acceptance gates (byte identity, fuzz, corpus, hubble, no
     Balanced regression, reproducibility).
Instrument runs live with the shipped binary at the v1.16 vintage
(release binary unchanged; no repository behavior change).
