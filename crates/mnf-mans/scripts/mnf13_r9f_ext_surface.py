#!/usr/bin/env python3
"""MNF13-R9-F — the released backend-6 pick's EXTERNAL-surface sweep.

Closes the boundary R9-E's record names (its sweep is the 30-file
production corpus only): the hubble-class external asset at the release
CLI, old binary (pre-backend-6, 1baf847) vs v1.15.0 (backend 6 in the
pick), maximum AND balanced profiles, plus the ID4 wire-anchor
cross-check and the census re-walk (mode mix + regret surface) of the
new pick so the external read answers both open items:

  1. does backend 6 fire (and how does the pick move) off-corpus;
  2. does the record-grammar floor verdict (ID4: everything mode 0)
     survive on the external surface under the v1.15 pick.

Usage: mnfpca2 python mnf13_r9f_ext_surface.py
Instruments: scripts/residual_census.py (census_file, reused directly).
"""
import glob
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, "/Users/jonbethea/projects/blacktech/manifold/scripts")
import residual_census  # the ID4 instrument, reused as-is

ROOT = "/Users/jonbethea/projects/blacktech/manifold"
OLD = "/tmp/r9e-old-target/release/manifold"
NEW = ROOT + "/target/release/manifold"
TRUTH = "/tmp/hf1_hubble_decoded.png"
ID4_ANCHOR = "/tmp/id4_files/hub_hubble_deep_field.jpg.max.mnf8"


def run(binp, png, out, profile):
    r = subprocess.run([binp, "encode", png, "-o", out, "--mnf8",
                        "--profile", profile], capture_output=True, text=True)
    assert r.returncode == 0, (binp, png, profile, r.stderr[-300:])
    v = subprocess.run([binp, "verify", png, out], capture_output=True, text=True)
    assert v.returncode == 0, (binp, png, v.stderr[-200:])
    return os.path.getsize(out)


def backend_of(path):
    with open(path, "rb") as f:
        blob = f.read(32)
    return int.from_bytes(blob[28:32], "little")


def main():
    if not os.environ.get("R9F_CENSUS_ONLY"):
        files = [TRUTH]  # the external surface at the R9-E boundary: hubble only
        rows = []
        for png in files:
            rel = os.path.basename(png)
            row = {"file": rel}
            for prof in ("maximum", "balanced"):
                for tag, b in (("old", OLD), ("new", NEW)):
                    out = f"/tmp/r9f_{tag}_{prof}.mnf8"
                    size = run(b, png, out, prof)
                    row[f"{tag}_{prof}_size"] = size
                if prof == "maximum":
                    # ID4 wire anchor: the pre-backend-6 pick must reproduce it
                    with open(ID4_ANCHOR, "rb") as f:
                        anchor = f.read()
                    with open(f"/tmp/r9f_old_maximum.mnf8", "rb") as f:
                        fresh_old = f.read()
                    row["id4_anchor_match"] = str(anchor == fresh_old)
                    row["pick_backend"] = backend_of(f"/tmp/r9f_new_maximum.mnf8")
                row[f"delta_{prof}"] = (row[f"new_{prof}_size"]
                                        - row[f"old_{prof}_size"])
            rows.append(row)
            print(f"{rel:40} max {row['old_maximum_size']:7}->{row['new_maximum_size']:7} "
                  f"({row['delta_maximum']:+6}) b{row['pick_backend']} "
                  f"id4_anchor_match={row['id4_anchor_match']} "
                  f"bal {row['old_balanced_size']:7}->{row['new_balanced_size']:7} "
                  f"({row['delta_balanced']:+6})", flush=True)

    # census re-walk of the NEW pick's maximum hubble wire (ID4 instrument,
    # reused directly): mode histogram + regret = the grammar-floor verdict
    Path("/tmp/r9f_files").mkdir(parents=True, exist_ok=True)
    r = residual_census.census_file(Path(TRUTH), Path("/tmp/r9f_files"),
                                    Path(TRUTH), "r9f_")
    for k, v in r.items():
        print(f"census {k}: {v if not isinstance(v, bytes) else '<bin>'}")


if __name__ == "__main__":
    main()
