#!/usr/bin/env python3
"""R6.7 post-pass: peak-RSS captures + Hubble decode/reconstruction digest."""
import hashlib, json, os, subprocess, time, struct
from pathlib import Path

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
OUT = {}
ENV = dict(os.environ, PATH="/opt/homebrew/opt/rustup/bin:" + os.environ["PATH"],
           RUSTUP_HOME=os.path.expanduser("~/.rustup"))


def timed(cmd):
    """Run a command under /usr/bin/time -l, return (wall, maxrss_kb, rc)."""
    t0 = time.perf_counter()
    r = subprocess.run(["/usr/bin/time", "-l"] + cmd, cwd=ROOT, env=ENV,
                       capture_output=True, text=True, timeout=1200)
    wall = round(time.perf_counter() - t0, 1)
    rss = 0
    for ln in r.stderr.splitlines():
        if "maximum resident set size" in ln:  # macOS: "<kb>  maximum resident set size"
            rss = int(ln.split()[0])
    return wall, rss, r.returncode


import sys
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
from mnf13_r6_extract import cli_encode  # noqa: E402

def enc(rel, out):
    return ["target/release/manifold", "encode", rel, "-o", out, "--mnf8=maximum"]

# 1. backend-7 enabled binary is current; representative encodes
for tag, rel in [("b7_forest", "testdata/photos/forest_224x160_s900002.png"),
                 ("b7_document", "testdata/photos/document_200x280.png"),
                 ("b1_blobs", "testdata/repeat_256x256.png"),
                 ("b7_terrain_max", "testdata/photos/terrain_224x160_s900005.png")]:
    w, rss, rc = timed(enc(rel, "/tmp/rss.mnf"))
    OUT[tag] = {"wall_s": w, "rss_mb": round(rss / 1048576, 1), "rc": rc}
    print(json.dumps({tag: OUT[tag]}), flush=True)

# 2. decode timing + RSS on a backend-7 container
w, rss, rc = timed(["target/release/manifold", "decode", "/tmp/rss.mnf", "-o", "/tmp/rss_dec.png"])
OUT["decode_b7"] = {"wall_s": w, "rss_mb": round(rss / 1048576, 1), "rc": rc}
print(json.dumps({"decode_b7": OUT["decode_b7"]}), flush=True)

# 3. Hubble: re-encode under time (RSS), verify byte-identity, then decode + digest
w, rss, rc = timed(enc("/tmp/hf1_hubble_dec.png", "/tmp/hubble_r67.mnf"))
OUT["hubble_encode"] = {"wall_s": w, "rss_mb": round(rss / 1048576, 1), "rc": rc}
blob = Path("/tmp/hubble_r67.mnf").read_bytes()
OUT["hubble_encode"]["sha16"] = hashlib.sha256(blob).hexdigest()[:16]
OUT["hubble_encode"]["size"] = len(blob)
print(json.dumps({"hubble_encode": OUT["hubble_encode"]}), flush=True)

w, rss, rc = timed(["target/release/manifold", "decode", "/tmp/hubble_r67.mnf", "-o", "/tmp/hubble_r67_dec.png"])
OUT["hubble_decode"] = {"wall_s": w, "rss_mb": round(rss / 1048576, 1), "rc": rc}
import numpy as np
from PIL import Image
src_px = np.asarray(Image.open("/tmp/hf1_hubble_dec.png").convert("RGB"), np.uint8)
dec_px = np.asarray(Image.open("/tmp/hubble_r67_dec.png").convert("RGB"), np.uint8)
OUT["hubble_recon"] = {
    "exact": bool(src_px.shape == dec_px.shape and not (src_px != dec_px).any()),
    "sha": hashlib.sha256(dec_px.tobytes()).hexdigest()[:16],
    "src_sha": hashlib.sha256(src_px.tobytes()).hexdigest()[:16]}
plen, flags, backend = struct.unpack_from("<III", blob, 20)
OUT["hubble_wirespec"] = {"plen": plen, "flags": flags, "backend": backend}
print(json.dumps({"hubble_decode": OUT["hubble_decode"], "hubble_recon": OUT["hubble_recon"],
                  "hubble_wirespec": OUT["hubble_wirespec"]}), flush=True)
(Root := ROOT / "research/results/x1l_r67_rss.json").write_text(json.dumps(OUT, indent=1))
print("WROTE", Root)
