"""MNF11 relabel-stream walker (ADR-008 experiment): walks the shipped
1.10.0 files' record/token streams, applies the per-pixel channel-diff
relabel [r,g,b] -> [g, r-g, b-g] (mode-0 bodies + token tag-0 literals, the
wire-shape-preserving forms), recompresses with the FILE'S OWN backend, and
reports complete-file deltas. Consumption==stream asserted per file."""

import numpy as np, zlib, struct, subprocess, os, glob, io
import zstandard
from PIL import Image
ROOT = "/Users/jonbethea/projects/blacktech/manifold"
env = {**os.environ, "PATH": "/opt/homebrew/opt/rustup/bin:"+os.path.expanduser("~/.rustup/toolchains/stable-aarch64-apple-darwin/bin")+":"+os.environ["PATH"], "RUSTUP_HOME": os.path.expanduser("~/.rustup")}
zdec = zstandard.ZstdDecompressor()
def cli_file(rel):
    subprocess.run([f"{ROOT}/target/release/manifold","encode",rel,"-o","/tmp/m11p.mnf5","--mnf8"],capture_output=True,env=env,check=True,cwd=ROOT)
    return open("/tmp/m11p.mnf5","rb").read()
def cd_relabel_stream(blob):
    if blob[:4] != b"MNF8":
        return None, ("mnf9",)
    plen, flags, backend = struct.unpack_from("<III", blob, 20)
    payload = blob[32:32+plen]
    stream = zdec.stream_reader(io.BytesIO(payload)).read() if backend == 1 else zlib.decompress(payload)
    w, h, tile, tile_count = struct.unpack_from("<IIII", blob, 4)
    if flags & 0x8:   # TOKEN wire: relabel tag-0 literal triplets only
        out = bytearray(); pos = 0; npx = w*h; ppos = 0
        while ppos < npx and pos < len(stream):
            tag = stream[pos]; pos += 1; out.append(tag)
            if tag == 0:
                t3 = np.frombuffer(stream[pos:pos+3], np.uint8).astype(np.int64)
                pos += 3
                g = t3[1] % 256
                rg = (t3[0] - t3[1]) % 256
                bg = (t3[2] - t3[1]) % 256
                out += np.array([g, rg, bg], np.uint8).tobytes()
                ppos += 1
            elif tag == 1:
                out += stream[pos:pos+5]; pos += 5; ppos += stream[pos-1]
            elif tag == 2:
                out += stream[pos:pos+3]; pos += 3; ppos += stream[pos-1]
            else:
                raise SystemExit(f"token tag {tag} at pos {pos}/{len(stream)} ppos {ppos}/{npx}")
        assert ppos == npx, "token consumption"
        s = bytes(out)
        sz = (len(zstandard.ZstdCompressor(level=19, write_checksum=True).compress(s)) if backend==1 else len(zlib.compress(s,9))) + 32
        return sz, ("token",)
    cols = (w + tile - 1)//tile
    out = bytearray(); pos = 0
    n0 = n1 = n2 = n3 = 0
    for t in range(tile_count):
        gy, gx = divmod(t, cols)
        tw = min(tile, w - gx*tile); th = min(tile, h - gy*tile)
        nb = 3*tw*th
        mode = stream[pos]; pos += 1
        out.append(mode)
        if mode == 0:
            n0 += 1
            pxv = np.frombuffer(stream[pos:pos+nb], np.uint8).astype(np.int64).reshape(-1, 3)
            pos += nb
            g = pxv[:, 1] % 256
            rg = (pxv[:, 0] - pxv[:, 1]) % 256
            bg = (pxv[:, 2] - pxv[:, 1]) % 256
            out += np.stack([g, rg, bg], axis=1).astype(np.uint8).reshape(-1).tobytes()
        else:
            if mode == 1: n1 += 1; extra = 4 + nb
            elif mode == 2: n2 += 1; extra = 5 + nb
            elif mode == 3: n3 += 1; extra = nb
            else: raise SystemExit(f"mode {mode}")
            out += stream[pos:pos+extra]; pos += extra
    assert pos == len(stream), "consumption check"
    s = bytes(out)
    sz = (len(zstandard.ZstdCompressor(level=19, write_checksum=True).compress(s)) if backend==1
          else len(zlib.compress(s, 9))) + 32
    return sz, (n0, n1, n2, n3)
tot0 = 0; tot1 = 0
files = (sorted(glob.glob(os.path.join(ROOT, "testdata/*.png"))) +
         sorted(glob.glob(os.path.join(ROOT, "testdata/heldout/*.png"))) +
         sorted(glob.glob(os.path.join(ROOT, "testdata/heldout-photos/*.png"))) +
         sorted(glob.glob(os.path.join(ROOT, "testdata/photos/*.png"))))
wins = 0
for f in files:
    rel = os.path.relpath(f, ROOT)
    blob = cli_file(rel)
    sz, modes = cd_relabel_stream(blob)
    if sz is None:
        print(f"{os.path.basename(f):42s} MNF9 file (skipped)")
        continue
    d = sz - len(blob)
    tot0 += len(blob); tot1 += sz
    if d < 0: wins += 1
    print(f"{os.path.basename(f):42s} shipped={len(blob):>7d} cd-relabel={sz:>7d} ({d:+6d}) modes={modes}")
print(f"\nTOTAL {tot0} -> {tot1} ({tot1-tot0:+d}) wins {wins}/30")
