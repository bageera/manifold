#!/usr/bin/env python3
"""MNF13-R9-E corpus gate: the shipped portfolio's ACTUAL output per file,
old binary (pre-backend-6, 1baf847) vs new (backend 6 in the pick).

Per file, per binary: CLI encode --mnf8=maximum -> size -> verify (exact)
-> encode wall time + peak RSS (/usr/bin/time -l; ru_maxrss is KILOBYTES
on macOS). Backend id of the NEW pick read from the wire (blob[28]).
The pick's min() self-gates: a losing arm costs only its own trial.
"""
import csv, glob, os, subprocess, tempfile, time

ROOT = "/Users/jonbethea/projects/blacktech/manifold"
OLD = "/tmp/r9e-old-target/release/manifold"
NEW = ROOT + "/target/release/manifold"


def run(bin, png, out):
    t0 = time.perf_counter()
    r = subprocess.run(["/usr/bin/time", "-l", bin, "encode", png, "-o", out,
                        "--mnf8", "--profile", "maximum"],
                       capture_output=True, text=True)
    wall = time.perf_counter() - t0
    assert r.returncode == 0, (bin, png, r.stderr[-300:])
    rss_mb = 0.0
    for line in r.stderr.splitlines():
        parts = line.split()
        # macOS 26: "<bytes>  peak memory footprint" (bytes); older
        # macOS: "maximum resident set size" (KILOBYTES per the POSIX
        # getrusage contract on this platform)
        if "peak memory footprint" in line and len(parts) >= 2:
            rss_mb = int(parts[0]) / 1048576.0
        elif "maximum resident set size" in line and len(parts) >= 2:
            rss_mb = int(parts[0]) / 1024.0
    v = subprocess.run([bin, "verify", png, out], capture_output=True, text=True)
    assert v.returncode == 0, (bin, png, v.stderr[-200:])
    return os.path.getsize(out), wall, rss_mb


def backend_of(path):
    with open(path, "rb") as f:
        blob = f.read(32)
    return int.from_bytes(blob[28:32], "little")


def main():
    files = sorted(glob.glob(ROOT + "/testdata/**/*.png", recursive=True))
    rows = []
    for png in files:
        rel = os.path.relpath(png, ROOT)
        row = {"file": rel}
        for tag, b in (("old", OLD), ("new", NEW)):
            out = tempfile.mktemp(suffix=".mnf")
            size, wall, rss = run(b, png, out)
            row[f"{tag}_size"] = size
            row[f"{tag}_enc_s"] = round(wall, 3)
            row[f"{tag}_rss_mb"] = round(rss, 1)
            if tag == "new":
                row["pick_backend"] = backend_of(out)
            os.remove(out)
        row["delta"] = row["new_size"] - row["old_size"]
        rows.append(row)
        print(f"{rel:44} old {row['old_size']:7} new {row['new_size']:7} "
              f"({row['delta']:+6}) b{row['pick_backend']} "
              f"enc {row['old_enc_s']:.2f}->{row['new_enc_s']:.2f}s", flush=True)
    tot_o = sum(r["old_size"] for r in rows)
    tot_n = sum(r["new_size"] for r in rows)
    wins = sum(1 for r in rows if r["delta"] < 0)
    losses = sum(1 for r in rows if r["delta"] > 0)
    ties = sum(1 for r in rows if r["delta"] == 0)
    b6 = sum(1 for r in rows if r["pick_backend"] == 6)
    print(f"\n== {len(rows)} files: old total {tot_o}  new total {tot_n}  "
          f"delta {tot_n - tot_o:+d} ({(tot_n - tot_o) / tot_o:+.3%})")
    print(f"   wins {wins}  losses {losses}  ties {ties}  backend-6 picks {b6}")
    import statistics
    eo = [r["old_enc_s"] for r in rows]
    en = [r["new_enc_s"] for r in rows]
    print(f"   encode time: old sum {sum(eo):.1f}s  new sum {sum(en):.1f}s "
          f"(median {statistics.median(eo):.2f}s -> {statistics.median(en):.2f}s)")
    out = os.environ.get("R9E_CSV", "/tmp/r9e_gate.csv")
    with open(out, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    print("wrote", out)


if __name__ == "__main__":
    main()