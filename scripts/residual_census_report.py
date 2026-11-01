#!/usr/bin/env python3
"""Aggregate the ID4 census CSV into the five research-question readout."""
import ast
import csv
import json
import sys
from collections import defaultdict
from pathlib import Path

rows = list(csv.DictReader(open("/tmp/id4_census.csv")))
tot_max = sum(int(r["size_max"]) for r in rows if not r["file"].startswith("hub_"))
tot_bal = sum(int(r["size_bal"]) for r in rows if not r["file"].startswith("hub_"))
print(f"30-corpus: maximum {tot_max} (anchor 812,180)  balanced {tot_bal}  "
      f"mix6-regret {tot_max - tot_bal}")

# Q1-Q5 buckets
cls_tiles = defaultdict(int)
cls_z9 = defaultdict(int)
mode_tiles = defaultdict(int)
per_file_heavy = []
for r in rows:
    classes = ast.literal_eval(r["classes"]) if r["classes"] else {}
    modes = ast.literal_eval(r["mode_hist"]) if r["mode_hist"] else {}
    for m, n in modes.items():
        mode_tiles[m] += n
    for k, v in classes.items():
        cls_tiles[k] += v["tiles"]
        cls_z9[k] += v["z9"]
    heavy = classes.get("texture-heavy")
    if heavy:
        per_file_heavy.append((r["file"], heavy["tiles"], heavy["z9"],
                               int(r["size_max"])))
print("\nmode histogram (tiles):", dict(sorted(mode_tiles.items())))
print("\nresidual-class census (tiles / z9-proxy share):")
ztot = sum(cls_z9.values()) or 1
for k in sorted(cls_tiles):
    print(f"  {k:14} tiles={cls_tiles[k]:5}  z9={cls_z9[k]:9}  "
          f"({cls_z9[k] / ztot * 100:5.1f}% of proxy)")
print("\ntop texture-heavy files (tiles, proxy B, file B):")
for f, t, z, s in sorted(per_file_heavy, key=lambda x: -x[2])[:10]:
    print(f"  {f:44} tiles={t:4}  z9={z:8}  size={s:8}")