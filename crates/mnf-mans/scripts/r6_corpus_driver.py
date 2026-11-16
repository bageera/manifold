#!/usr/bin/env python3
"""MNF16-X1-L R6.3-R6.6 driver: identity toggle + full portfolio competition.

Phase A (backend 7 disabled): candidates without MIX7, rebuild, encode the
30-file corpus, record (size, sha256) per file, decode-verify every file.
Phase B (backend 7 enabled): restore, rebuild, encode the same corpus +
decode-verify; per-file backend/flags/size accounting.
Verdicts: R6.3 (phase-A == the recorded v1.16.0 sizes/determinism),
R6.4/6.5 (portfolio competition, <= 769,108), R6.6 (reconstruction 30/30).
"""

import json
import os
import struct
import subprocess
import hashlib
import sys
import time
from pathlib import Path

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
sys.path.insert(0, str(ROOT / "scripts"))
from mnf13_r6_extract import cli_encode  # noqa: E402

CORPUS = [
    # 22 traced mix files (from research/manifests/corpus.lock)
] + [m["file"] for m in json.loads((ROOT / "research" / "manifests" / "corpus.lock").read_text())["files"]]
NONMIX = [
    "testdata/gradient_320x240.png", "testdata/heldout/blob_224x160.png",
    "testdata/heldout/blocks_192x128.png", "testdata/heldout/plaid_192.png",
    "testdata/heldout/poster_208x144.png", "testdata/noise_256x256.png",
    "testdata/repeat_256x256.png", "testdata/solid_256x256.png",
]
CORPUS = CORPUS + NONMIX
LOCK = {m["file"]: m for m in json.loads((ROOT / "research" / "manifests" / "corpus.lock").read_text())["files"]}
MNF7_LINE = "        (BACKEND_MIX7, Coder::Zstd(19)),\n"
CAND_FILE = ROOT / "src/mnf8.rs"


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()[:32]


def rebuild():
    env = dict(os.environ)
    env["PATH"] = "/opt/homebrew/opt/rustup/bin:" + env["PATH"]
    env["RUSTUP_HOME"] = os.path.expanduser("~/.rustup")
    r = subprocess.run(["cargo", "build", "--release"], cwd=ROOT,
                       env=env, capture_output=True, text=True, timeout=560)
    if r.returncode != 0:
        print(r.stderr[-500:], flush=True)
        raise SystemExit(1)


def toggle(disable: bool):
    src = CAND_FILE.read_text()
    if disable:
        assert MNF7_LINE in src
        src = src.replace(MNF7_LINE, "")
    else:
        assert MNF7_LINE not in src
        anchor = "fn backend_candidates() -> Vec<(u32, Coder)> {\n    vec![\n"
        src = src.replace(anchor, anchor + MNF7_LINE)
    CAND_FILE.write_text(src)


def phase(tag: str):
    out = {}
    for rel in CORPUS:
        t0 = time.time()
        blob = cli_encode(rel, profile=("mnf8", "maximum"))
        plen, flags, backend = struct.unpack_from("<III", blob, 20)
        dec = subprocess.run(["target/release/manifold", "decode", "/tmp/r6_corpus.mnf",
                              "-o", "/tmp/r6_corpus_dec.png"],
                             cwd=ROOT, capture_output=True, timeout=300)
        # the cli_encode wrote to ITS own temp; redo decode on this blob
        Path("/tmp/r6_corpus.mnf").write_bytes(blob)
        dec = subprocess.run(["target/release/manifold", "decode", "/tmp/r6_corpus.mnf",
                              "-o", "/tmp/r6_corpus_dec.png"],
                             cwd=ROOT, capture_output=True, timeout=300)
        recon = "?"
        if dec.returncode == 0:
            import numpy as np
            from PIL import Image
            src_px = np.asarray(Image.open(rel).convert("RGB"), np.uint8)
            dec_px = np.asarray(Image.open("/tmp/r6_corpus_dec.png").convert("RGB"), np.uint8)
            recon = bool(src_px.shape == dec_px.shape and not (src_px != dec_px).any())
        out[rel] = {"size": len(blob), "backend": backend, "flags": flags,
                    "sha": sha(blob), "recon": recon,
                    "enc_s": round(time.time() - t0, 1)}
        print(json.dumps({"phase": tag, **out[rel], "file": rel}), flush=True)
    return out


def main():
    log = {"r6_3": {}, "r6_4": {}}
    # Phase A: backend 7 disabled
    toggle(disable=True)
    rebuild()
    log["r6_3"] = phase("A-disabled")
    # Phase B: backend 7 enabled
    toggle(disable=False)
    rebuild()
    log["r6_4"] = phase("B-enabled")

    # ---- R6.3 verdict: phase-A == recorded v1.16.0 (sizes from lock/nonmix)
    r53 = list(json.loads((ROOT / "research" / "results" / "x1l_r5.json").read_text())["files"])
    recorded = {r["file"]: r["shipped_B"] for r in r53}
    mismatch = [(f, log["r6_3"][f]["size"], recorded[f])
                for f in CORPUS if log["r6_3"][f]["size"] != recorded[f]]
    print(json.dumps({"R6.3_identity": "PASS" if not mismatch else "FAIL",
                      "mismatches": mismatch}), flush=True)

    # ---- R6.4/6.5
    shipped_total = log["r6_3"].__len__() and sum(v["size"] for v in log["r6_3"].values())
    b7_total = sum(v["size"] for v in log["r6_4"].values())
    flips = [(f, log["r6_3"][f]["size"], log["r6_4"][f]["size"])
             for f in CORPUS if log["r6_3"][f]["size"] != log["r6_4"][f]["size"]
             and log["r6_4"][f]["backend"] == 7]
    print(json.dumps({"R6_4": {
        "files": len(CORPUS), "shipped_total": shipped_total, "b7_total": b7_total,
        "gain": shipped_total - b7_total, "gain_pct": round(100 * (shipped_total - b7_total) / shipped_total, 2),
        "flips_to_7": len(flips), "reference_769108": b7_total <= 769108}}), flush=True)
    # ---- R6.6
    recon_fail = [f for f in CORPUS if not log["r6_4"][f]["recon"]]
    print(json.dumps({"R6_6_reconstruction": "PASS" if not recon_fail else f"FAIL {recon_fail}"}), flush=True)
    (ROOT / "research/results/x1l_r6_corpus.json").write_text(json.dumps(log, indent=1))
    print("WROTE research/results/x1l_r6_corpus.json")


if __name__ == "__main__":
    main()
