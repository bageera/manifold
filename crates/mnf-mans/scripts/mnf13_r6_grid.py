"""R6 FINISH: the ctx16+longcaps candidates on ALL 20 backend-4 files
(single process = the grid optimum per instrument rules), then the
per-file min-pick corpus verdict vs the shipped 20-file total."""
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
assert len(files) == 20 and all(r["backend"] == 4 for r in files)

# the grid: caps around (4096..16384, 131072..1048576) x ctx16, anchored to
# the shipped everything-else
GRID = [(4096, 131072), (4096, 262144), (4096, 524288),
        (8192, 262144), (8192, 524288), (16384, 524288),
        (16384, 1048576), (32768, 1048576)]
totals = {g: 0 for g in GRID}
per_file = {}
for r in files:
    png = str(ROOT / "testdata" / r["file"])
    be, fl, kind, ref, planar, stream, blob = residual_wire(png,
                                                           profile=("mnf8", "maximum"))
    k16 = file_ctx_keys(stream, "ctx16")
    best = (r["A_shipped"], "shipped")
    per_file[r["file"]] = {}
    for g in GRID:
        out = bridge(f"mix{g[0]}_{g[1]}-lr15-inc32-s24-ctx16", stream, k16)
        totals[g] += len(out)
        per_file[r["file"]][g] = len(out)
        if len(out) < best[0]:
            best = (len(out), f"mix{g[0]}_{g[1]}")
    per_file[r["file"]]["best"] = best
ship_total = sum(r["A_shipped"] for r in files)
print("shipped total:", ship_total)
for g in sorted(GRID, key=lambda g: totals[g]):
    print(f"mix{g[0]}_{g[1]} ctx16 total {totals[g]} ({totals[g] - ship_total:+d})")
# per-file min pick (incl. shipped as an arm — the never-worse rule)
min_total = sum(min([r["A_shipped"]] + [per_file[r["file"]][g] for g in GRID])
                for r in files)
print("per-file min-pick total (grid + shipped):", min_total,
      f"({min_total - ship_total:+d}, -{(ship_total - min_total) / ship_total * 100:.2f}%)")
fires = sum(1 for r in files if min(per_file[r["file"]][g] for g in GRID)
            < r["A_shipped"])
print("files where the grid fires:", fires, "/ 20")
best_arm = min(GRID, key=lambda g: totals[g])
solo = sum(1 for r in files if per_file[r["file"]][best_arm] < r["A_shipped"])
print(f"single best arm mix{best_arm[0]}_{best_arm[1]}:",
      totals[best_arm], f"({totals[best_arm] - ship_total:+d})", "fires", solo)
# corpus-level honest total: 20 backend-4 files improve; the other 10 keep
# their shipped bytes, so the released-corpus delta = the backend-4 delta.
print("released-corpus projection (30 files, 10 unchanged):",
      816341 + (totals[best_arm] - ship_total), "B")
json.dump({"totals": {f"{g[0]}_{g[1]}": totals[g] for g in GRID},
           "per_file": {f: {f"{g[0]}_{g[1]}": v for g, v in per_file[f].items()
                            if g != "best"}
                         for f in per_file}},
          open(ROOT / "docs/mnf13-r6-caps-grid.log", "w"), indent=1)
print("wrote docs/mnf13-r6-caps-grid.log")