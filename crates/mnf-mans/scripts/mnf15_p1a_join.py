#!/usr/bin/env python3
"""MNF15-P1-A: per-file encoder search-cost attribution (no reruns).

Joins the R9-E corpus-gate per-file CSV (.research/r9/r9e_rows.csv —
release-CLI encode/verify per file, old = pre-backend-6 binary 1baf847,
new = v1.15.0) into a decision table keyed by the file-level pick, and
attributes the added Maximum-profile encode time to the sides of the
mix6 won/lost split. Answers P1-A's question: does the added search
cost land on files whose winner is backend 6, or on files where backend
6 is evaluated and declines?

Reads ONLY recorded measurements — nothing is re-encoded. Anchors
(total bytes 812,180 / 810,816 and summed times 870.2 / 1145.3 s from
docs/mnf13-r9e-record.md) are asserted, not assumed.

Outputs:
  docs/mnf15-p1a-decision-table.csv  (per-file rows, committed)
  docs/mnf15-p1a-gate.log            (this run's printed summary)
Exits nonzero on any assert failure.
"""
import csv
import re
import statistics
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
ROWS = REPO / ".research/r9/r9e_rows.csv"
OUT_CSV = REPO / "docs/mnf15-p1a-decision-table.csv"
OUT_LOG = REPO / "docs/mnf15-p1a-gate.log"


def dims_from(path: str):
    m = re.search(r"_(\d+)x(\d+)", path)
    return (int(m.group(1)), int(m.group(2))) if m else None


def main() -> int:
    rows = list(csv.DictReader(ROWS.open()))
    assert len(rows) == 30, f"corpus count: expected 30, got {len(rows)}"

    old_total = sum(int(r["old_size"]) for r in rows)
    new_total = sum(int(r["new_size"]) for r in rows)
    assert old_total == 812_180, f"old anchor: {old_total}"
    assert new_total == 810_816, f"new anchor: {new_total}"
    assert new_total - old_total == -1_364

    old_s = sum(float(r["old_enc_s"]) for r in rows)
    new_s = sum(float(r["new_enc_s"]) for r in rows)
    assert 869.0 < old_s < 871.0, f"old time anchor: {old_s:.1f}"
    assert 1144.0 < new_s < 1146.0, f"new time anchor: {new_s:.1f}"

    for r in rows:
        d = dims_from(r["file"])
        r["pixels"] = d[0] * d[1] if d else ""
        r["delta_bytes"] = int(r["new_size"]) - int(r["old_size"])
        r["delta_s"] = round(float(r["new_enc_s"]) - float(r["old_enc_s"]), 3)
        r["delta_rss_mb"] = round(float(r["new_rss_mb"]) - float(r["old_rss_mb"]), 3)
        r["win_backend"] = r["pick_backend"]
        db = r["delta_bytes"]
        r["s_per_kb_saved"] = (
            round(-r["delta_s"] / (db / 1024.0), 1) if db < 0 else ""
        )

    won = [r for r in rows if r["win_backend"] == "6"]
    lost = [r for r in rows if r["win_backend"] != "6"]
    assert len(won) == 15 and len(lost) == 15, (len(won), len(lost))
    won_db = sum(int(r["delta_bytes"]) for r in won)
    lost_db = sum(int(r["delta_bytes"]) for r in lost)
    assert won_db == -1_364 and lost_db == 0, (won_db, lost_db)
    won_ds = sum(r["delta_s"] for r in won)
    lost_ds = sum(r["delta_s"] for r in lost)

    pts = [
        (int(r["pixels"]), r["delta_s"])
        for r in rows
        if r["pixels"] != ""
    ]
    n = len(pts)
    mx = sum(p for p, _ in pts) / n
    my = sum(d for _, d in pts) / n
    cov = sum((p - mx) * (d - my) for p, d in pts) / n
    sx = (sum((p - mx) ** 2 for p, _ in pts) / n) ** 0.5
    sy = (sum((d - my) ** 2 for _, d in pts) / n) ** 0.5
    corr = cov / sx / sy
    s_per_mpx = my / (mx / 1e6)

    wins = {}
    for r in rows:
        wins[r["win_backend"]] = wins.get(r["win_backend"], 0) + 1

    rss_old = max(float(r["old_rss_mb"]) for r in rows)
    rss_new = max(float(r["new_rss_mb"]) for r in rows)
    rss_dmax = max(r["delta_rss_mb"] for r in rows)

    top = sorted(rows, key=lambda r: r["delta_s"], reverse=True)[:8]

    outcsv = OUT_CSV.open("w", newline="")
    fields = [
        "file", "pixels", "win_backend", "old_size", "new_size",
        "delta_bytes", "old_enc_s", "new_enc_s", "delta_s",
        "old_rss_mb", "new_rss_mb", "delta_rss_mb", "s_per_kb_saved",
    ]
    w = csv.DictWriter(outcsv, fieldnames=fields)
    w.writeheader()
    w.writerows({k: r[k] for k in fields} for r in rows)
    outcsv.close()

    lines = []
    lines.append(f"files: {len(rows)} (asserted 30)")
    lines.append(f"sizes: {old_total} -> {new_total} dB {new_total - old_total} (asserted)")
    lines.append(
        f"time s: {old_s:.1f} -> {new_s:.1f}  d {new_s - old_s:.1f} "
        f"+{(new_s / old_s - 1) * 100:.1f}%"
    )
    lines.append(f"s per kB saved (corpus): {(new_s - old_s) / ((old_total - new_total) / 1024):.1f}")
    lines.append(
        f"medians s: {statistics.median(float(r['old_enc_s']) for r in rows):.1f}"
        f" -> {statistics.median(float(r['new_enc_s']) for r in rows):.1f}"
    )
    lines.append(
        f"mix6-won files: {len(won)}  dtime {won_ds:.1f} s  dbytes {won_db}"
    )
    lines.append(
        f"mix6-lost files: {len(lost)}  dtime {lost_ds:.1f} s  dbytes {lost_db}"
    )
    lines.append(
        f"lost-share of added time: {lost_ds / (new_s - old_s) * 100:.1f}%"
    )
    lines.append(
        f"won-side s/kB: {won_ds / (-won_db / 1024):.1f}"
    )
    lines.append(
        f"corr(pixels, dtime) = {corr:.2f} (n={n}, dims-less row: plaid)"
    )
    lines.append(f"dtime per Mpixel: {s_per_mpx:.1f} s")
    lines.append(f"backend wins: {dict(sorted(wins.items()))}")
    lines.append(f"RSS max: {rss_old:.1f} -> {rss_new:.1f} MB, per-file dmax {rss_dmax:.1f}")
    lines.append("top time penalties:")
    for r in top:
        lines.append(
            f"  {Path(r['file']).name:44s} b{r['win_backend']}"
            f" dbytes {int(r['delta_bytes']):+6d} ds {r['delta_s']:+7.1f}s"
            f" s/kB {r['s_per_kb_saved']}"
        )
    text = "\n".join(lines) + "\n"
    OUT_LOG.write_text(text)
    print(text, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
