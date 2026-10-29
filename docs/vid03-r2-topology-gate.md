# VID03-R2: Topology-Dependent Reconstruction — gate record

Status: **DENIED for compression** (2026-10-28). Instrument:
`scripts/vid03_r2_gate.py` (committed, reproduces the table on rerun).

## The gate's question

VID03-R's pilot established that multi-sheet topology metadata — the
impossible +1+1+1 loop (loop sum 3 ≠ 0, inconsistent with any single
globally defined height function) — is *representable* in 9 B, but the
prototype never let the transitions drive rendered pixels. R2 (Jon's
gate) makes the chart transitions affect the actual reconstructed
frames and compares, with equal metadata budgets, a globally consistent
arrangement against the inconsistent multi-sheet arrangement.

## Arms (all complete-file, container = min(zlib-9, zstd-19))

- **A** whole-sequence zlib (the pilot's control)
- **B** consistent single-sheet: one template + every frame's per-site
  rotation symbols, paid explicitly
- **C** multi-sheet: template + loop/transition record; rotations DERIVED
  decoder-side by the sheet-state evolution (the +1+1+1 loop as stored
  transition info)
- **D** exact-generative formula arm (rotation = (a·f + b) mod 4)
- **E** sparse-event decomposition: (frame, pointer, value) events — the
  attribution control for any C win

Self-check: the formula frames rediscover (2f + j) mod 4 exactly; the
sheet frames' rendered rotations reproduce the evolution path (f=4
pointer-1 transition visible in the recon).

## Results

| fixture             | A zlib | B rots | C sheet | D formula | E sparse |
|---------------------|--------|--------|---------|-----------|----------|
| ordinary            | 2,178  | 626    | n/a     | 615       | n/a      |
| impossible-rendered | 2,905  | 635    | 626     | n/a       | 632      |
| three-templates     | 4,925  | 1,611  | n/a     | 1,602     | n/a      |
| sparse-transitions  | 3,626  | 619    | 631     | n/a       | 620      |

Framing-noise floor at these sizes: ±6 B (KEEP-class deltas).

## Verdict

1. **Topology does not earn rate.** C beats B by 9 B on
   impossible-rendered — inside the floor — and the sparse-event arm
   (632) covers the symbol arm (635) within noise: the topology record
   adds nothing E does not already attribute.
2. **On the sparse-transitions fixture — the class constructed to favor
   events — C loses to B by 12 B.** The container compresses the full
   rotation-symbol stream so well (it is sparse itself, and the zlib
   run-length machinery sees the zero runs) that the derived-state
   machinery's framing cannot repay itself.
3. Where the formula class exists, D ties-or-beats B by 9–11 B — the
   exact-generative framing is again the strongest of the symbolic
   arms, consistent with MNF9's record.

## The standing conclusion (pilot + gate together)

- Multi-sheet topology is **representable** (pilot: 9-B loop record),
  **renderable** (gate: transitions drive pixels, reconstruction exact),
  but **not economical** (gate: no arm separation beyond the noise
  floor; the event-attribution control covers the sheet record).
- The compression value in the constructed video corpus belongs to
  **shared templates + the container's own coding of the transition
  streams** — the same shape as G3's verdict (the win belongs to the
  container, not the shared structure).
- VID03-R2 does NOT advance to deterministic state evolution,
  Navier–Stokes, thermal transport, or Van 't Hoff components on
  compression grounds. If topology is pursued further (e.g. for
  semantic chart annotation), it must present a fixture class where the
  derived representation beats E's sparse events OUTSIDE the noise
  floor — none measured here.