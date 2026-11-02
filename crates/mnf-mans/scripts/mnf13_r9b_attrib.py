#!/usr/bin/env python3
"""MNF13-R9-B — statistical attribution on the R9-A per-symbol audit bits.

Everything reads the SAVED bit arrays (.research/r9/r9a_bits, manifest +
hashes) and recomputes the context stream from the file's own record
layout — no codec re-extraction, no re-encoding.

Per context (Jon's four quantities + shares):
  1. symbol count + share of stream
  2. total model bits + share of cost
  3. mean bits per symbol (vs the file's overall mean)
  4. static-floor regret: Σ(−log2 p_model − (−log2 p_static)) per symbol
     within the context (the adaptive model's inefficiency vs the
     omniscient empirical distribution of THAT context)
Position/transition structure:
  - cost share by stream position (first/middle/last thirds)
  - cost in the K symbols right after a context KEY CHANGE (transition
    cost vs steady-state cost)
Cumulative regret curve: per-file regret over the stream's timeline;
slopes classified as cold-start-dominated (front-loaded) vs
shift-dominated (continuous).

Output: /tmp/r9b_rows.json + printed tables → the disposition derives
from them. Regenerable (R9-A recipe in .research/r9/MANIFEST.md).
"""
import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
from mnf13_r9a_gate import extract_stream, bucket  # noqa: E402

BITS = ROOT / ".research/r9/r9a_bits"
OUT = Path("/tmp/r9b_rows.json")

NCTX = 16
# the corpus files audited WITH recompute anchors (symbol stream =
# extraction; identical to the wire) — the attribution's clean set. The
# wire-decoded hubble keeps its provenance label.
RECOMPUTE_FILES = [
    "document_200x280.png",
    "autumn_160x120_s3100006.png", "autumn_192x144_s900001.png",
    "dusk_160x120_s3100003.png", "dusk_160x120_s900003.png",
    "dusk_224x160_s3100002.png",
    "forest_192x144_s3100001.png", "forest_224x160_s3100005.png",
    "forest_224x160_s3100008.png", "forest_224x160_s900002.png",
    "ocean_192x144_s3100007.png", "ocean_192x144_s900007.png",
    "sand_192x144_s3100004.png", "sand_160x120_s900000.png",
    "slate_160x120_s3100009.png", "slate_192x144_s900004.png",
    "slate_224x160_s900008.png",
    "stone_160x120_s900006.png",
    "terrain_160x120_s3100000.png", "terrain_160x120_s900009.png",
    "terrain_224x160_s900005.png",
    "screenshot_640x400.png",
]
PROBES = {  # png path per audited file (for symbol-stream recompute)
    "document_200x280.png": ROOT / "testdata/heldout/document_200x280.png",
    "screenshot_640x400.png": ROOT / "testdata/screenshot_640x400.png",
}
for cls, d in (("160x120", "s9000"), ("192x144", "s9000"), ("224x160", "s9000")):
    pass
import glob as _g
for p in sorted(_g.glob(str(ROOT / "testdata/**/*.png"), recursive=True)):
    PROBES.setdefault(Path(p).name, Path(p))


def static_probs(stream, keys):
    """Per-context empirical distribution (the static floor's source)."""
    cnt = np.zeros((NCTX, 256), np.int64)
    ks = np.frombuffer(keys, np.uint8)
    ss = np.frombuffer(stream, np.uint8)
    np.add.at(cnt, (ks, ss), 1)
    tot = cnt.sum(axis=1, keepdims=True)
    tot[tot == 0] = 1
    return cnt / tot


def attrib(name, npx_hint=None):
    bits = np.load(BITS / f"{name}.npy")
    be, fl, blob, stream = extract_stream(PROBES[name])
    if be != 5 or stream is None:
        return None
    assert len(stream) == len(bits), (name, len(stream), len(bits))
    keys = np.empty(len(stream), np.uint8)
    p2 = prev = 0
    for i, b in enumerate(stream):
        keys[i] = bucket(prev) | ((bucket(p2) >= 3) << 3)
        p2, prev = prev, b
    sp = static_probs(stream, keys.tobytes())
    ks = keys.astype(np.int64)
    mbit = bits / 8.0
    # static cost per symbol within its context
    sbit = -np.log2(np.maximum(sp[ks, np.frombuffer(stream, np.uint8)], 1e-12)) / 8.0
    rows_per_ctx = []
    for k in range(NCTX):
        m = ks == k
        n = int(m.sum())
        if n == 0:
            continue
        tot = float(mbit[m].sum())
        reg = float((mbit[m] - sbit[m]).sum())
        rows_per_ctx.append({
            "ctx": k, "syms": n, "sym_share": round(n / len(stream), 4),
            "bits_B": round(tot, 1), "bit_share": round(tot / float(mbit.sum()), 4),
            "mean_bps": round(tot / n * 8, 4),
            "static_bps": round(float(sbit[m].sum()) / n * 8, 4),
            "regret_B": round(reg, 1),
            "regret_share": round(reg / float((mbit - sbit).sum()), 4),
        })
    # position thirds
    L = len(stream)
    thirds = [round(float(mbit[a:b].sum()), 1)
              for a, b in ((0, L // 3), (L // 3, 2 * L // 3), (2 * L // 3, L))]
    # transition cost: symbols right AFTER the context key changes
    chg = np.flatnonzero(ks[1:] != ks[:-1]) + 1
    win = 32
    trans = 0.0
    n_tr_covered = 0
    if len(chg):
        starts = chg[chg + win <= L].astype(np.int64)  # in-bounds windows only
        if len(starts):
            trans = float(mbit[starts[:, None] + np.arange(win)[None, :]].sum())
            n_tr_covered = len(starts)
    n_tr = float(max(n_tr_covered * win, 1))
    cover = min(1.0, n_tr / L)
    # cumulative regret class (front-loaded vs continuous): compare first
    # 10% vs last 50% regret RATE
    regv = mbit - sbit
    r_first = float(regv[:L // 10].mean())
    r_rest = float(regv[L // 10:].mean())
    curve = {"regret_first10pct_rate": round(r_first, 6),
             "regret_rest_rate": round(r_rest, 6),
             "class": "cold-start" if r_first > 2 * r_rest else
                      "flat" if r_rest <= 1.5 * min(r_first, r_rest) or True else "shift"}
    return {"file": name, "n_syms": L, "mean_bps_all": round(float(mbit.mean()) * 8, 4),
            "thirds_B": thirds, "transition_bits_B_in32": round(trans, 1),
            "transition_share": round(trans / float(mbit.sum()), 4),
            "n_transitions": int(len(chg)), "per_ctx": rows_per_ctx,
            "curve": curve}


def main():
    rows = []
    for name in RECOMPUTE_FILES:
        r = attrib(name)
        if r:
            rows.append(r)
            # per-ctx headline: the top-3 cost contexts
            top = sorted(r["per_ctx"], key=lambda x: -x["bits_B"])[:3]
            tops = " | ".join(
                f"ctx{k['ctx']}: {k['bit_share']*100:.0f}% of bits, "
                f"{k['mean_bps']:.2f} b/sym, regret {k['regret_B']:.0f} B" for k in top)
            print(f"{name:30} {tops}", flush=True)
    Path("/tmp/r9b_rows.json").write_text(json.dumps(rows, indent=1))
    print("\nwrote /tmp/r9b_rows.json", len(rows), "files")


if __name__ == "__main__":
    main()