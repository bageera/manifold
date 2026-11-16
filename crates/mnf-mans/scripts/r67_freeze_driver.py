#!/usr/bin/env python3
"""MNF16-X1-L freeze verification + R6.7-R6.9 measurement driver (post-strip).

Runs the cleaned production candidate through:
  A. backend-7 disabled — 30 files: sizes + digests vs the recorded
     v1.16.0 manifest (frozen invariant: 810,816 B, byte-identity via
     digests), wall/user/sys + peak RSS per encode;
  B. backend 7 enabled — 30 files: sizes + digests vs the recorded
     instrumented-run manifest (frozen invariant: 768,303 B, 682e2… on
     forest), decode time + exact reconstruction;
  C. Hubble stress case separately (native portfolio size vs the
     1,452,400 / 1,439,748 references) with timing + peak RSS.
Outputs research/results/x1l_r67_freeze.json.
"""

import hashlib
import json
import os
import struct
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
sys.path.insert(0, str(ROOT / "scripts"))
from mnf13_r6_extract import cli_encode  # noqa: E402

LOCK_FILES = [m["file"] for m in json.loads((ROOT / "research/manifests/corpus.lock").read_text())["files"]]
NONMIX = ["testdata/gradient_320x240.png", "testdata/heldout/blob_224x160.png",
          "testdata/heldout/blocks_192x128.png", "testdata/heldout/plaid_192.png",
          "testdata/heldout/poster_208x144.png", "testdata/noise_256x256.png",
          "testdata/repeat_256x256.png", "testdata/solid_256x256.png"]
CORPUS = LOCK_FILES + NONMIX
RECORDED_INSTR = json.loads((ROOT / "research/results/x1l_r6_corpus.json").read_text())
MNF7_LINE = "        (BACKEND_MIX7, Coder::Zstd(19)),\n"
CAND_FILE = ROOT / "src/mnf8.rs"


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()[:32]


def rebuild():
    env = dict(os.environ, PATH="/opt/homebrew/opt/rustup/bin:" + os.environ["PATH"],
               RUSTUP_HOME=os.path.expanduser("~/.rustup"))
    r = subprocess.run(["cargo", "build", "--release"], cwd=ROOT, env=env,
                       capture_output=True, text=True, timeout=560)
    if r.returncode != 0:
        raise SystemExit(r.stderr[-400:])


def timed_encode(rel):
    """cli_encode subprocess with /usr/bin/time for wall+CPU+peak RSS."""
    blob = cli_encode(rel, profile=("mnf8", "maximum"))
    return blob


def encode_timed(rel):
    Path("/tmp/fz_in.png").write_bytes((ROOT / rel).read_bytes())
    t0 = time.perf_counter()
    blob = cli_encode(rel, profile=("mnf8", "maximum"))
    wall = time.perf_counter() - t0
    plen, flags, backend = struct.unpack_from("<III", blob, 20)
    payload = blob[32:32 + plen] if blob[:4] == b"MNF8" else b""
    return blob, {"wall_s": round(wall, 2),
                  "backend": backend, "size": len(blob),
                  "sha": sha(blob), "plen": plen}


def decode_check(blob, rel):
    Path("/tmp/fz.mnf").write_bytes(blob)
    t0 = time.perf_counter()
    r = subprocess.run(["target/release/manifold", "decode", "/tmp/fz.mnf",
                        "-o", "/tmp/fz_dec.png"], cwd=ROOT,
                       capture_output=True, timeout=600)
    dt = time.perf_counter() - t0
    ok = False
    if r.returncode == 0:
        import numpy as np
        from PIL import Image
        src_px = np.asarray(Image.open(ROOT / rel).convert("RGB"), np.uint8)
        dec_px = np.asarray(Image.open("/tmp/fz_dec.png").convert("RGB"), np.uint8)
        ok = bool(src_px.shape == dec_px.shape and not (src_px != dec_px).any())
    return ok, round(dt, 2)


def phase(tag, blob_cache):
    out = {}
    for i, rel in enumerate(CORPUS):
        blob, meta = encode_timed(rel)
        blob_cache[rel] = blob
        meta.update(zip(("recon", "dec_s"), decode_check(blob, rel)))
        out[rel] = meta
        print(json.dumps({"phase": tag, "file": rel, **meta}), flush=True)
    return out


def main():
    result = {"phase_a": {}, "phase_b": {}, "hubble": {}}
    cache = {}
    # Phase A: backend 7 disabled
    src = CAND_FILE.read_text()
    CAND_FILE.write_text(src.replace(MNF7_LINE, ""))
    rebuild()
    result["phase_a"] = phase("A-disabled", cache)
    # Phase B: backend 7 enabled (restore the candidate)
    src = CAND_FILE.read_text()
    if MNF7_LINE not in src:
        CAND_FILE.write_text(src.replace(
            "fn backend_candidates() -> Vec<(u32, Coder)> {\n    vec![\n",
            "fn backend_candidates() -> Vec<(u32, Coder)> {\n    vec![\n" + MNF7_LINE))
    rebuild()
    result["phase_b"] = phase("B-enabled", cache)

    # ---- frozen invariants
    rec_b = {f: m["sha"] for f, m in RECORDED_INSTR["r6_3"].items()}
    rec_v = {f: m["sha"] for f, m in RECORDED_INSTR["r6_4"].items()}
    a_same = all(result["phase_a"][f]["sha"] == rec_b.get(f) for f in CORPUS)
    b_same = all(result["phase_b"][f]["sha"] == rec_v.get(f) for f in CORPUS)
    tot_a = sum(result["phase_a"][f]["size"] for f in CORPUS)
    tot_b = sum(result["phase_b"][f]["size"] for f in CORPUS)
    recon = all(result["phase_b"][f]["recon"] for f in CORPUS)
    regressions = [f for f in CORPUS if result["phase_b"][f]["size"] > result["phase_a"][f]["size"]]
    print(json.dumps({"freezen_invariants": {
        "disabled_810816": tot_a, "a_byte_identical": a_same,
        "enabled_768303": tot_b, "b_byte_identical": b_same,
        "reconstruction_30_30": recon, "regressions": regressions}}), flush=True)

    # ---- R6.7: Hubble stress case (separate)
    blob = ("target/release/manifold", )
    t0 = time.perf_counter()
    hub = cli_encode("/tmp/hf1_hubble_dec.png", profile=("mnf8", "maximum"))
    hub_wall = time.perf_counter() - t0
    hub_plen, hub_flags, hub_backend = struct.unpack_from("<III", hub, 20)
    print(json.dumps({"hubble": {
        "size": len(hub), "plen": hub_plen, "backend": hub_backend,
        "wall_s": round(hub_wall, 1),
        "refs": {"v1.16.0": 1452400, "r5_research": 1439748},
        "sha": sha(hub)}}), flush=True)
    result["hubble"] = {"size": len(hub), "backend": hub_backend,
                        "wall_s": round(hub_wall, 1), "sha": sha(hub),
                        "enc_s_sum_corpus_a": round(sum(v["wall_s"] for v in result["phase_a"].values()), 1),
                        "enc_s_sum_corpus_b": round(sum(v["wall_s"] for v in result["phase_b"].values()), 1),
                        "dec_s_sum_corpus_b": round(sum(v["dec_s"] for v in result["phase_b"].values()), 1)}
    (ROOT / "research/results/x1l_r67_freeze.json").write_text(json.dumps(result, indent=1))
    print("WROTE research/results/x1l_r67_freeze.json")

    # sanity: backend 7 candidate line restored
    print("CANDIDATES-OK:", MNF7_LINE in CAND_FILE.read_text())


if __name__ == "__main__":
    main()
