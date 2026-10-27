import numpy as np, zlib, struct, collections, os

S = np.load("/tmp/eer_stack.npy")
one = S.shape[0]
BL = int(S.nbytes)
out = []

f0 = S[0].tobytes()
out.append(f"frame0 z9: {len(zlib.compress(f0, 9))} z3: {len(zlib.compress(f0, 3))} z6: {len(zlib.compress(f0, 6))}")
out.append(f"wire/frame: {113.9*1048576/29/1048576:.2f} MiB")

ys, xs = np.nonzero(S[0])
out.append(f"frame0 nonzero: {len(ys)} ({len(ys)/S[0].size:.4f})")
vals = S[0][ys, xs]
out.append("value histogram: " + str(dict(sorted(collections.Counter(vals.tolist()).items())[:8])))

# event-list recode: (dx, dy) deltas from previous event in scan order (RLE's own idea) —
# 32-bit delta packs + value byte:
order = np.lexsort((xs, ys))  # scan order = y-major, x-minor (matches the EER memory order)
yy, xx = ys[order], xs[order]
vv = vals[order]
pos = (yy.astype(np.int64) * S.shape[2] + xx.astype(np.int64))
d = np.diff(np.concatenate([[pos[0]], pos]))
dbytes = d.astype(np.uint32).tobytes()
vbytes = vv.tobytes()
e9 = len(zlib.compress(b"".join([struct.pack("<I", len(dbytes)), dbytes, vbytes]), 9))
out.append(f"event-delta-list z9: {e9} B = {e9/len(ys):.2f} B/event (dense-z9 = {len(zlib.compress(f0,9))/777000:.2f} B/px)")

# chain: f_k vs f_0 alignment via shift search on downsampled rows (drift estimate) —
# the torus arm on the real stack (measured)
res_all = 0
tex = S[0]
for k in range(1, 29):
    best = None
    sub = S[k]
    # search dy in +-4 only (frame drift is small): proxy = abs diff count on a 1-px grid
    for dy in range(-4, 5):
        shifted = np.roll(tex, dy, axis=0)
        c = int(np.count_nonzero(sub != shifted))
        if best is None or c < best:
            best = c
    res_all += best
out.append(f"phase-arm (dy scan +-4, != count): best total {res_all} for 29 frames (raw would be 28*int((S[1]!=S[0]).sum()))")

raw_diff = int((S[1] != S[0]).sum())
out.append(f"unshifted != count f1 f0: {raw_diff}")

open("/tmp/eer_matrix.txt", "w").write("\n".join(out))
print("\n".join(out))