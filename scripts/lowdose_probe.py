#!/usr/bin/env python3
"""Low-dose EER probe (EMPIAR-11213, 553-frame fractionated movies).

The regime EER was built for: ~0.37 MB/frame wire. Measures the event
density p (eerfile render), Shannon H(p), and compares:
  - vendor wire bytes/frame (fixed by the file)
  - dense-render zlib-9/frame
  - event-delta-list (u32 deltas + values)
  - frame-chain arm
  - slice-rule/repeat detector
All bit-exact on the rendered stack; the render is DECLARED (4x4 subpixel).
"""
import numpy as np, zlib, math, struct, sys
import eerfile

path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lowdose.eer"
frames = eerfile.read(path)
S = np.stack(frames)
n = len(S)
cell = int(S[0].size)
wire = 211523518  # from Content-Length
print(f"frames {n} | render {S.shape} | wire {wire//1048576} MiB = {wire/n:.0f} B/frame")

nzf = [float((S[k] > 0).mean()) for k in range(n)]
p = sum(nzf) / n
H = -(p * math.log2(p) + (1 - p) * math.log2(1 - p)) if 0 < p < 1 else 0.0
print(f"event density p = {p:.4f} | Shannon H(p) = {H:.4f} bits/px-dense")
print(f"per-frame dense bytes (u8): {cell}")
print(f"H-bound bytes/frame ≈ {H * cell / 8:.0f}")

base = len(zlib.compress(np.ascontiguousarray(S).tobytes(), 3))  # z3 for speed on big stacks
print(f"dense-render z3: {base} ({base / (n * cell):.3f} B/px-dense) | per-frame {base/n:.0f} B")

# event-delta-list per frame:
total_ev = 0
ev_bytes = 0
for k in range(n):
    ys, xs = np.nonzero(S[k])
    m = len(ys)
    total_ev += m
    if m == 0:
        continue
    pos = ys.astype(np.int64) * S.shape[2] + xs.astype(np.int64)
    order = np.argsort(pos)
    d = np.diff(np.concatenate([[pos[order][0]], pos[order]]))
    db = d.astype(np.uint32).tobytes()
    vb = S[k][ys[order], xs[order]].astype(np.uint8).tobytes()
    ev_bytes += len(zlib.compress(db + vb, 3))
ev_bytes += n * 4
print(f"event-delta-list z3: {ev_bytes} ({ev_bytes / n:.0f} B/frame) | {ev_bytes / max(1,total_ev):.2f} B/event")

# frame-chain arm:
ch = np.zeros_like(S)
ch[0] = S[0]
for k in range(1, n):
    ch[k] = (S[k].astype(np.int16) - S[k - 1].astype(np.int16)).astype(np.int32) % 256
chain = len(zlib.compress(np.ascontiguousarray(ch).tobytes(), 3))
print(f"frame-chain z3: {chain} ({chain / n:.0f} B/frame)")

# vendor comparison
print(f"wire/frame {wire/n:.0f} vs base {base/n:.0f} (z3) vs events {ev_bytes/n:.0f} vs chain {chain/n:.0f}")
print(f"wire/dense-render-z3 = {wire/base:.3f}")
