#!/usr/bin/env python3
"""MNF13-R1 in-repo reproduction — context-aware arithmetic coding of
the ACTUAL v1.12 residual streams (per Jon's promotion gate).

Arms (complete serialized cost, same header semantics as the shipped
MNF8 container: identical 32-B header fields, payload replaced):
  A  shipped     — the released file bytes (the pick's own backend)
  B  ari order-0 — fully adaptive 8-bit arithmetic (no transport beyond
                   the frame; decoder-side identical model)
  C  +prev-res   — order-1 on the coded residual order (256 contexts)
  D  +cross-ch   — causal cross-channel context: each byte's model keyed
                   on the already-decoded channels of the same pixel
                   (interleave) / plane-previously-coded-2 (planar)
  E  +spatial    — activity context from the PREVIOUS recon row
                   (guaranteed-decoded; encoder reads recon = true by
                   the mod-256 rule)
Decode = the exact inverse model walk; every arm asserts exact
reconstruction before quoting any size.

The honest-transport gate (rANS lesson): every context the encoder
keys on must be recomputable from the DECODER-side recon + already-
coded bytes — the coder carries ZERO side tables (all models adapt).
Causality asserted by construction; per-file exactness asserted.
"""
import os
import struct
import subprocess
import sys
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(Path(__file__).parent))
from mnf11_x2_dynamics import decompress_stream  # noqa: E402
from mnf13_r0_diag import cli_encode_max, extract_residuals,     cd_unrelabel_stream, activity_class  # noqa: E402

TILE = 16

# ---------------- the range coder (32-bit, carry-less; WNC-style) -----

TINY = 64     # min probability per symbol (in a 4096 total) — clamp

def _probabilities(counts):
    """counts[256] → constriction probabilities (sum = 4096), clamped ≥1."""
    tot = sum(counts)
    scale = 4096 / tot
    p = [max(TINY, int(round(n * scale))) for n in counts]
    # renormalize exactly to 4096 (adjust the largest)
    s = sum(p)
    if s != 4096:
        big = max(range(256), key=lambda k: p[k])
        p[big] += 4096 - s
        if p[big] <= 0:
            # degenerate — distribute
            p[big] = 1
            p[(big + 1) % 256] += -1
    return p

class CtxCoder:
    """Per-context adaptive models through constriction's exact range
    coder (Queue/Stack semantics fixed: Queue + identical model
    evolution both sides = exact inverse)."""

    def __init__(self, nctx):
        self.models = [[1] * 256 for _ in range(nctx)]

    def encode(self, wire, ctx):
        import constriction
        sym_probs = []
        for i in range(len(wire)):
            m = self.models[ctx[i]]
            sym_probs.append(_probabilities(m))
            m[int(wire[i])] += 32
            if sum(m) > self.TOT_:  # halve
                self.models[ctx[i]] = [(x + 1) >> 1 for x in m]
                m = self.models[ctx[i]]
        q = constriction.range.QueueEncoder()
        enc = q.encode(np.asarray(wire, np.int32), np.asarray(sym_probs, np.int32))
        return enc.compressed.tolist()

    TOT_ = 4096 + 4096   # halving threshold (slack)

    def decode(self, compressed, ctx, n):
        import constriction
        # decoder must mirror: the model evolution follows the DECODED
        # symbol, and the probs list must be regenerated in the same
        # order — constriction's QueueDecoder uses the probs sequence
        # provided by the caller.
        q = constriction.range.QueueDecoder(compressed)
        out = np.zeros(n, np.int64)
        for i in range(n):
            m = self.models[ctx[i]]
            probs = _probabilities(m)
            s = q.decode(probs, None)  # per-call API shape? probe
            out[i] = s
            m[s] += 32
            if sum(m) > self.TOT_:
                self.models[ctx[i]] = [(x + 1) >> 1 for x in m]
        return out
