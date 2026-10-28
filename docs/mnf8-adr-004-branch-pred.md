# ADR-004 — MNF8 branch-prediction flags: PRED_PLANE / PRED_LEFT
# (status: ACCEPTED, 2026-10-28)

## Context

The lattice ablation (docs/mnf9-lattice-probe.md) measured the causal
lattices as losses but its CONTROLS exposed a bigger effect: the
predictor's BRANCH choice dominates the neighborhood choice — forcing
LOCO-I's plane branch (a+b−cc, clamped) beats full MED by −4.6%/corpus
and a pure left predictor wins flat/edge classes (poster −59%). The
retention gate ran per the spec (docs/mnf9-branch-gate.md): per-file
pick over MNF8's full candidate set + the two new arms — **PASSED**,
−49,341 on the real wire (fires 27/30, never-worse per file, every arm
decode-asserted exactly). Note the wire number (−49k) is the honest one;
the python isolation gate's −71k ignored the atlas arms' existing wins
on the same classes.

## Decision

**MNF8 flags gain bits 4-5** (ADR-003's bit-3 mechanism, same pattern):

    bit 4  FLAG_PRED_PLANE  image-wide predictor = LOCO-I plane branch
    bit 5  FLAG_PRED_LEFT   image-wide predictor = pure left

- Exactly ONE of {PRED_MED, PRED_PLANE, PRED_LEFT} may be set; multiple
  → decoder rejects.
- These bits apply to mode-0 literal bodies (same as PRED_MED's v1.2
  semantics). Plane/Left files are PURE prediction arms: the encoder
  emits mode-0 records ONLY (the measured winning form — no atlas, no
  YCoCg candidates under these flags), and the decoder's ADR-004 walk
  rejects non-mode-0 records in such files.
- TOKENS (bit 3) stays MED-only by definition (its grammar's payload is
  the MED-residual wire); TOKENS+{PLANE,LEFT} → decoder rejects.
- Decode: Plane/Left payloads route to `decode_pred_walk` — per-tile
  causal inversion with the selected predictor's interior rule
  (`PredKind::predict`); Med payloads keep the frozen MNF7 walker;
  None payloads = raw bodies. Edges: (0,0)=127, top row = left, left
  column = above — identical to the MED walk everywhere except the
  interior rule.
- Encode: the pick loop measures 4 predictor kinds × interleave/planar ×
  backend (ADR-003's token arm unchanged), complete serialized cost —
  never-worse by construction. **Encode-side neighbor reads come from
  TRUE pixels; the decode walk reads reconstructed pixels; the mod-256
  induction makes them agree (reconstructed-earlier == true). The
  gradient-gate bug — reading the prediction stream instead of the pixel
  stream (chaining predictions) — is the reason this rule is explicit.**

## Measured basis (the two records this ADR implements)

- docs/mnf9-lattice-probe.md: branch vs neighborhood isolation, with
  the control set (cart-plane −4.6%, left-only −2.7% vs full MED).
- docs/mnf9-branch-gate.md: the retention gate — PASSED (−49,341
  real-wire, 27/30 fires, per-file never-worse).

## Consequences

- No new magic (MNF9 stays reserved for new-record-grammar work); MNF8
  files only, flag-space bit costs ~0; every old file decodes unchanged.
- The per-file maintenance question ("do both predictors earn their
  bit?") answered by the wire gate: Left wins flat/edge classes
  (poster/blob/document), Plane wins smooth/photo gradients (13 files).
  Both fire; both retained.
- Conformance: mnf8_branch_left_poster golden vector + two rejection
  vectors (two-predictors, tokens+left); lib pins for the pick firing
  (poster) and the never-worse rule; full-corpus real-wire gate
  #[ignore]d for CI (837 s, run explicitly when touching these arms).