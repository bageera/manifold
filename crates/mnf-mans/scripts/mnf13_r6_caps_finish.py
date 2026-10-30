"""R6 finish: per-file min-pick corpus verdict + the ctx16 caps grid."""
import json
import re
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from mnf13_r6_extract import residual_wire
from mnf13_r6_gate import bridge, file_ctx_keys

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")

log = open(ROOT / "docs/mnf13-r6-gate.log").read()
rows = json.loads(re.search(r"\[\s*\n.*?\n\s*\]", log, re.S).group())
files = [r for r in rows if not r.get("skip") and not r.get("anchor_fail")]
ship_total = sum(r["A_shipped"] for r in files)
names = ["B_caps_1024_16384", "B_caps_1024_65536", "B_caps_4096_16384",
         "B_caps_4096_262144", "B_caps_16384_65536", "C_lr005", "C_lr025",
         "C_lr050", "D_inc16", "D_inc64", "E_s20", "F_ctx16"]
min_total = sum(min([r["A_shipped"]] + [r[n] for n in names]) for r in files)
print("shipped total (20 backend-4 files):", ship_total)
print("per-file min pick total:", min_total, f"({min_total - ship_total:+d})")

for png in ["photos/slate_224x160_s900008.png", "heldout/document_200x280.png",
            "heldout-photos/dusk_224x160_s3100002.png"]:
    be, fl, kind, ref, planar, stream, blob = residual_wire(
        str(ROOT / "testdata" / png), profile=("mnf8", "maximum"))
    k16 = file_ctx_keys(stream, "ctx16")
    best = (1 << 30, None)
    for f, s in [(4096, 131072), (4096, 524288), (2048, 262144),
                 (8192, 262144), (4096, 262144), (16384, 1048576)]:
        mode = f"mix{f}_{s}-lr15-inc32-s24-ctx16"
        out = bridge(mode, stream, k16)
        tag = f"mix{f}_{s}"
        print(f"  {png.split('/')[-1][:30]:30s} {tag:20s} ctx16 -> {len(out):6d}")
        if len(out) < best[0]:
            best = (len(out), mode)
    print(f"  BEST {best[1]} = {best[0]}")