# Cryo-EM research branch — experiment contracts (2026-10-27)

Status: research plan only; frozen formats untouched; retention-gate rules
apply to every arm. The framing that makes this branch HONEST: cryo-EM is a
source of mechanisms, and its distinctive prior — many projections of one
shared structure — is exactly the precondition that FAILED on photographs
(shared-texture experiments lose on photos 20/20) but HOLDS in cryo-EM data
by construction.

## Arm P (multi-view projection coding) — the distinctive one

Hypothesis: K related views of a common structure compress better through a
shared latent + per-view corrections than as K independent streams
(measured vs. (a) per-view best single-view mode, (b) K-independent-MNF8,
(c) zstd-19 on concatenated views).

Contract:
- Construct K views of one integer-analytic density: rotate a fixed 3D
  trilinear-voxel field rho (integer arithmetic), project along z by
  column sums, discretize to RGB8 — the generator is EXACT and reproducible
  (no fp path into the codec).
- Discovery arm: recover per-view phase/orientation by block-matching
  against the running atlas (the v12 machinery), never from the generator.
- Exactness: decode(encode(X)) == X per view; per-view residual ring
  identity asserted.
- Retention: mode retained only if L(shared + residuals + metadata) <
  min(K-independent-MNF8, per-view-best) with a margin > transport noise.
- Precondition honest note: applies to MULTI-VIEW sets; single photographs
  remain denied (existing verdict).

## Arm S (sparse-event mode)

Hypothesis: sparse masks/scientific rasters compress better as event lists
{(x, y, t?), payload} than as dense streams.
Contract: synthetic sparse corpus (density sweeps 0.1%..5%); baseline =
PNG, zstd-19, and MONF8-literals; event container = existing record
grammar extension candidate ONLY if the gate passes with metadata counted.
Bit-exact only; EER's precision-bounded lossy ideas stay EXCLUDED (the
scientific-mode table in the proposal: separate, explicitly labeled — not
part of manifold-codec's lossless contract).

## Arm F (Fourier-space structure detection)

Hypothesis: a measured per-region representation class (spatial/edge/
periodic/projection) beats any single fixed class — measurable WITHOUT a
format change by scoring the existing 4 classes per region against real
corpora. This arm is cheap (no codec work) and reuses the ablation
machinery.

## Infrastructure prerequisites (shared)

- MRC reader (research-crate only): MRC headers (int8/int16/float32, stacks,
  spacing) -> RGB8/gray rendering for tests; NEVER in the codec crate.
- EMPIAR fetch script (curl; datasets chosen by API, small entries first).
- The bench matrix records losses beside wins per the standing contract.

## Non-goals (standing)

- fp anywhere in decode paths.
- Uncertainty-bounded lossy mode inside manifold-codec (out of scope; a
  separate research artifact if ever).
- Voxel/octree containers (arm A/B of the proposal) — no measured case yet
  where single-image volumetric modeling beats in-plane prediction; gate
  first via arm P's machinery.
