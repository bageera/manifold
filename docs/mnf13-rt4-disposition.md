# MNF13-RT4/RT3 disposition — rectangle references vs the shipped matchers (2026-10-29)

The RT3/RT4 question (one rectangle-reference instruction vs the
existing MATCH/NEAR sequence for the same repeated region) is
answered by measurements already in the repo, rerun fresh on the
v1.12.0 release:

## The shipped representations

- The RECORD grammar's mode-1 exact references ARE one-instruction
  tile-rectangle copies (1 B mode + u32 index; the atlas holds whole
  16x16 tiles; T-transform variants mode-2/4/5). Shipped since the
  MNF5 atlas, pick-competed per file at complete cost ever since.
- The TOKEN grammar's tag-1 MATCH (6 B, up to 85 px) / tag-2 NEAR
  (4 B) copy the MED-residual plane's residual triplets byte-forward;
  cap 85 px per op with consecutive-token emission for longer runs.

## The ablation (G4-revised harness rerun on the v1.12 binary)

`g4_gate.py` (arm B = one-op-per-rect: shared template + tile-rect
references + full sparse residual payloads + left-predicted
complement, complete serialized cost) vs the release pick:

| fixture | release pick | tile-rect arm | verdict |
|---|---|---|---|
| screenshot (tokens win; 987 exact causal full-tile residual dups measured in-repo) | 667 | 817 (n=71) | **+150 LOSE** |
| document | 923 (balanced row) | 793 (n=5) | −130 — but this IS the pick's record arm: the shipped mode-1 refs, not a new token op |
| poster | 1,141 / 751 | same | record arm again |
| guard-strip | 311 | 320 | +9 LOSE |
| hero-pair-identical | 220 | 176 | −44 (plain template reuse) |
| solid / gradient | 36 / 254 | no win | container exploits it |

(The release pick above = the balanced-profile column of the rerun;
the maximum pick is never worse by construction.)

## Reading

1. RT4's own port-level census matches in-repo reality: the real token
   files carry 98-100% matched-pixel coverage (screenshot: 3,080 exact
   refs + 3,044 nears; blocks: 639 nears/98.4%); the uncovered literal
   gap = 19-391 px.
2. Where whole-residual-tile duplication dominates (screenshot, 987
   exact causal tile dups measured on the coded residual plane), the
   one-op-per-rect record arm still loses by +150 complete-file
   bytes: per-tile refs must carry or reference residual payloads
   the near-form's byte-forward copies get free, and template
   metadata outweighs the ~2 matched-token savings per tile.
3. Where the rect form wins (document/poster/solid classes), it is
   ALREADY the shipped pick (record arms with mode-1 refs).

## Verdict

The rectangle-reference branch CLOSES: the representation exists
(mode-1), the pick prices it per file against the token stream, and
the measured deltas go both ways by content class with the exact
class where a NEW rect token would help showing +150 instead. Any
future grouping op needs a complete-wire win over BOTH existing
grammars' pick — the burden the evidence now numbers. Standing open
branch per the previous record: statistical/approximate spatial
dependencies (the shared probability-model line, honest-transport
gated).
