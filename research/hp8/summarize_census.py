#!/usr/bin/env python3
"""HP8-0 — aggregate the fresh P1-B-style stage census into per-backend costs."""
import pandas as pd

# The p1b instrument emits backend-7 rows with an extra field between
# walk_excl and wire_len (the lane-walk note slot). Drop it defensively.
lines = open("research/results/hp8-computational-census.csv").read().splitlines()
header = lines[0].split(",")
fix = []
for ln in lines[1:]:
    f = ln.split(",")
    if len(f) == len(header) + 1:
        # p1b's per-stage layouts carry an extra note slot at index 13
        del f[13]
    assert len(f) == len(header), f"row shape {len(f)}: {ln[:120]}"
    fix.append(f)
df = pd.DataFrame(fix, columns=header)
# numeric coercion; pick/build/relabel rows carry text in timing columns
rec = df[df.stage == "record"].copy()
for c in ("data_len", "t_full", "twin", "twin_prep", "twin_loop", "twin_seal",
          "walk_excl", "wire_len", "model_calls"):
    rec[c] = pd.to_numeric(rec[c], errors="coerce")
rec = rec.dropna(subset=["data_len", "t_full"])
rec["ns_sym"] = rec.t_full / rec.data_len * 1000
rec["ns_sym_loop"] = rec.twin_loop / rec.data_len * 1000
agg = rec.groupby("backend").agg(
    arms=("file", "count"), files=("file", "nunique"),
    ns_sym_mean=("ns_sym", "mean"), ns_sym_min=("ns_sym", "min"),
    ns_sym_max=("ns_sym", "max"), ns_loop_mean=("ns_sym_loop", "mean"),
    model_calls=("model_calls", "sum"), data=("data_len", "sum"))
print(agg.round(1).to_string())
per_file = rec.pivot_table(index="file", columns="backend",
                           values="ns_sym", aggfunc="mean").round(1)
print(per_file.to_string())
agg.round(1).to_csv("research/results/hp8-computational-census-summary.csv")
print("wrote summary")
