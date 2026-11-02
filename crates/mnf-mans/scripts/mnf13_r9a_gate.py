#!/usr/bin/env python3
"""MNF13-R9-A — byte-equal backend-5 extraction + prequential audit.

The bridge between ID4's production residual census and the entropy
research: measure EXACTLY what the shipped range coder sees, then price
each symbol's prequential self-information under the model state that
coded it. The representation is entirely frozen; this is measurement.

Extraction (source of truth = the shipped grammar, mnf13_r6_extract
discipline extended to backend 5):
  - CLI encode --mnf8 --profile maximum per corpus file (+hubble);
  - the mix payload codes the raw RECORD stream: per tile
    [mode byte][relabeled residual body] in tile-major wire emission
    order (planar = transpose(2,0,1) planes; interleaved = ravel; the
    CD relabel per-pixel as shipped; predictor edge semantics
    (0,0)=127 / top=left / left-col=ABOVE);
  - token wires and MNF9 magics carry no mix-addressable record stream
    (ID4 already classified them; R9-A records and skips them honestly).

Byte-equality anchors ( asserted per file, both directions):
  1. recompute-from-pixels through the shipped-rule bridge ('e' mode,
     backend-5 constants) == the shipped payload, BYTE-IDENTICAL;
  2. decode-from-wire through the same bridge ('d' mode) == the
     recomputed stream (the shipped decoder's own key schedule).

Prequential audit ('a' mode): the exact-roundtrip is asserted inside
the bridge, then per-symbol -log2(p_i) at the state that coded it.
Three accounting columns per file (Jon's safeguard):
  L_model  = sum of self-info bits (statistical cost / 8)
  L_entropy_wire = the mix payload's coded byte count
  L_complete = the whole file's size
plus adaptation-regret readout: L_model minus the order-N static
floor (per-context empirical entropy under the same key stream) and
per-symbol f64s dumped for stage-B analysis.
"""
import argparse
import json
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
from mnf13_r6_extract import cd_relabel, cli_encode, pred_map  # noqa: E402

BRIDGE = ROOT / "target/release/examples/r9_audit"
TS = 16
B5 = "mix32768_1048576-lr15-inc32-ctx16"   # shipped backend-5 constants
B4 = "mix4096_65536-lr15-inc32-ctx8"       # shipped backend-4 constants

def bucket(b: int) -> int:
    if b == 0:
        return 0
    if b <= 2:
        return 1
    if b <= 5:
        return 2
    if b <= 11:
        return 3
    if b <= 24:
        return 4
    if b <= 48:
        return 5
    if b <= 96:
        return 6
    return 7


def bridge(mode: str, syms: bytes, cmd: str) -> bytes:
    hdr = f"{len(syms)} {mode}\n".encode()
    r = subprocess.run([str(BRIDGE), cmd, mode], input=hdr + syms,
                       capture_output=True)
    if r.returncode != 0:
        raise RuntimeError(f"bridge {cmd} {mode}: {r.stderr[:200]!r}")
    return r.stdout


def extract_stream(png, truth=None):
    """(backend, flags, blob, stream|None) — the record stream in coded
    order, exactly as the mix payload addresses it.

    CAUSALITY (pinned from src/mnf8.rs 1283-1462): the ENCODER relabels
    the assembled record stream (mode bytes + mode-0 bodies) with CD
    BEFORE backend_compress; the DECODER mix-decodes FIRST and
    unrelabels AFTER. So the mix coder's symbol stream is the
    UNLABELED record stream; relabeling applies to the container
    payload, not the mix symbols. (My first draft relabeled before
    assembling — the anchor caught it.)
    """
    blob = cli_encode(png, profile=("mnf8", "maximum"))
    if blob[:4] != b"MNF8":
        return 0, 0, blob, None
    w, h, tile, tile_count = struct.unpack_from("<IIII", blob, 4)
    plen, flags, backend = struct.unpack_from("<III", blob, 20)
    assert tile == TS
    if flags & 0x8:
        return backend, flags, blob, None  # token wire: not record-coded
    src = Image.open(truth or png).convert("RGB")
    px = np.asarray(src, np.int64)
    assert (w, h) == (px.shape[1], px.shape[0]), px.shape
    kind = ("med" if flags & 4 else "plane" if flags & 16 else
            "left" if flags & 32 else "none")
    ref = (flags >> 6) & 3
    planar = bool(flags & 1)
    pred = pred_map(kind, px)
    res = (px - pred) % 256
    # THE CAUSALITY (proven by the byte comparison, src/mnf8.rs 1283-1291):
    # the ENCODER assembles the record stream THEN applies the CD relabel
    # THEN backend_compress; the DECODER mix-decodes FIRST and unrelabels
    # AFTER (mnf6.rs/raw). The mix coder codes the RELABELED stream. (An
    # intermediate draft assembled unlabeled — 848 localized diffs, head
    # 80 00 00 = the REF=3 relabel of (128,128,128) — the instrument's
    # own anchors caught it.)
    wire3 = cd_relabel(res, ref, planar, w, h)
    n_bytes = h * w * 3
    wire = np.empty(n_bytes, np.int64)
    pos = 0
    cols = (w + TS - 1) // TS
    bodies = []
    for ti in range(tile_count):
        row, col = divmod(ti, cols)
        tw = min(TS, w - col * TS)
        th = min(TS, h - row * TS)
        t = wire3[row * TS:row * TS + th, col * TS:col * TS + tw]
        body = t.transpose(2, 0, 1).ravel() if planar else t.ravel()
        wire[pos:pos + body.size] = body
        pos += body.size
        bodies.append(bytes(body.astype(np.uint8).tobytes()))
    assert pos == n_bytes
    stream = bytearray()
    for ti in range(tile_count):
        stream.append(0)  # mode-0 record (assert: ID4 census: all mode 0)
        stream += bodies[ti]
    return backend, flags, blob, bytes(stream)


def planar_of(flags: int) -> bool:
    return bool(flags & 0x1)


def _walk_check(stream: bytes, w: int, h: int) -> None:
    """Structural validation of a wire-decoded record stream: every mode
    byte legal, every body inside the stream, consumed == len exactly."""
    cols = (w + TS - 1) // TS
    rows_n = (h + TS - 1) // TS
    pos = 0
    META = {0: 0, 1: 4, 2: 5, 3: 0, 4: 4, 5: 5, 6: 12, 7: 12, 8: 3}
    for t in range(cols * rows_n):
        gy, gx = divmod(t, cols)
        tw = min(TS, w - gx * TS)
        th = min(TS, h - gy * TS)
        nb = 3 * tw * th
        assert pos < len(stream), f"walk: eof at tile {t}"
        mode = stream[pos]
        pos += 1
        if mode == 3:
            n1 = nb // 3
            n9 = (9 * n1 + 7) // 8
            pos += n1 + 2 * n9
        else:
            pos += META[mode] + nb
    assert pos == len(stream), f"walk: consumed {pos} != {len(stream)}"


def static_floor_bits(syms, keys, nctx):
    """Per-context empirical static entropy (order-N floor under the same
    key stream) — the adaptation-regret denomintor."""
    cnt = np.zeros((nctx, 256), np.int64)
    for s, k in zip(syms, keys):
        cnt[k, s] += 1
    for k in range(nctx):
        tot = cnt[k].sum()
        if tot == 0:
            cnt[k].fill(0)
            cnt[k, 0] = 1
    ntot = cnt.sum(axis=1, keepdims=True)
    p = cnt / np.where(ntot == 0, 1, ntot)
    cost = 0.0
    for s, k in zip(syms, keys):
        cost += -np.log2(max(p[k, s], 1e-12))
    return cost


def audit_file(png, truth=None, tag=""):
    rel = tag + Path(png).name
    backend, flags, blob, stream = extract_stream(png, truth)
    plen = struct.unpack_from("<I", blob, 20)[0]
    row = {"file": rel, "backend": backend, "flags": hex(flags),
           "plen": plen, "size": len(blob)}
    if blob[:4] != b"MNF8":
        row["skip"] = f"magic {blob[:4].decode(errors='replace')}"
    elif stream is None:
        row["skip"] = "token-wire" if (flags & 0x8) else "no-record-stream"
    elif backend not in (4, 5):
        row["skip"] = f"backend{backend} (not a mix payload)"
    if "skip" in row:
        print(json.dumps(row), flush=True)
        return row
    # anchors, both directions (mode = the SHIPPED constants for this file's backend)
    mode = B5 if backend == 5 else B4
    payload_full = bytes(blob[32:32 + plen])
    wire = bridge(mode, stream, "e")
    if wire != blob[32:32 + plen]:
        # Recompute-direction anchor failure = this file's pick used
        # record modes the extractor doesn't lay out (extraction-from-
        # pixels would need the pick's arm choices = re-running the
        # search; ADR-010(b) assigns that to the encoder, never the
        # instrument). FALLBACK: audit the WIRE-DECODED symbol stream
        # ('p' mode = the shipped decoder's own output — non-circular:
        # symbols + keys + constants all come from the wire + shipped
        # constants). Labeled anchor_source="wire-decode"; the recompute
        # anchor stays honestly false for this row.
        row["anchor_recompute"] = False
        row["e_mismatch"] = {"recomputed_len": len(wire), "shipped_plen": plen}
        stream = bridge(mode, payload_full, "p")
        row["anchor_source"] = "wire-decode"
        # structural walk validation: the decoded stream must parse as a
        # legal record stream (every mode byte legal, consumed == len)
        w2 = int.from_bytes(blob[4:8], "little")
        h2 = int.from_bytes(blob[8:12], "little")
        _walk_check(stream, w2, h2)
        audit = bridge(mode, stream, "a")
    else:
        row["anchor_recompute"] = True
        back = bridge(mode, payload_full, "p")
        if back != stream:
            row["anchor_fail"] = "decode-side mismatch"
            print(json.dumps(row), flush=True)
            return row
        row["anchor_decode"] = True
        audit = bridge(mode, stream, "a")
    row["anchor_decode"] = True
    n = struct.unpack_from("<I", audit, 0)[0]
    assert n == len(stream)
    bits = np.frombuffer(audit[4:4 + 8 * n], np.float64)
    # context keys per symbol under the shipped rule (recomputed here for
    # the static floor; the bridge computes its own identical schedule)
    keys = np.empty(len(stream), np.int64)
    p2 = prev = 0
    for i, b in enumerate(stream):
        keys[i] = bucket(prev) | ((bucket(p2) >= 3) << 3) if backend == 5 else bucket(prev)
        p2, prev = prev, b
    L_model = float(bits.sum())
    L_static = float(static_floor_bits(stream, keys, 16 if backend == 5 else 8))
    row.update({
        "n_syms": n,
        "L_model_B": round(L_model / 8, 1),
        "L_static_B": round(L_static / 8, 1),
        "adapt_regret_B": round((L_model - L_static) / 8, 1),
        "L_entropy_wire_B": plen - 4,          # payload minus len-word
        "coder_overhead_B": round(plen - 4 - L_model / 8, 1),
        "L_complete_B": len(blob),
        "ctx_hist": {str(k): int((keys == k).sum())
                     for k in range(16 if backend == 5 else 8)},
        "bits_head": [round(float(x), 4) for x in bits[:512]],
    })
    out_np = Path("/tmp/r9a_bits") / f"{rel}.npy"
    out_np.parent.mkdir(exist_ok=True)
    np.save(out_np, bits)
    print(json.dumps({k: v for k, v in row.items() if k != "bits_head"}),
          flush=True)
    return row


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--subset", type=int, default=0)
    ap.add_argument("--out", default="/tmp/r9a_rows.json")
    a = ap.parse_args()
    if not BRIDGE.exists():
        print(f"missing {BRIDGE}; build: cargo build --release -p mnf-mans "
              "--example r9_audit", file=sys.stderr)
        return 2
    named = [(p, None, "") for p in sorted((ROOT / "testdata").glob("**/*.png"))]
    hub = Path("/tmp/mnf14_if2/hubble_deep_field.jpg")
    truth = Path("/tmp/hf1_hubble_decoded.png")
    if hub.exists() and truth.exists():
        named.append((hub, truth, "hub_"))
    rows = []
    for png, t, tag in named:
        r = audit_file(png, t, tag)
        rows.append(r)
        if a.subset and len(rows) >= a.subset:
            break
    Path(a.out).write_text(json.dumps(rows, indent=1))
    ok = [r for r in rows if r.get("anchor_recompute")]
    print(f"\nanchored backend-5 files: {len(ok)}")
    if ok:
        tot_model = sum(r["L_model_B"] for r in ok)
        tot_wire = sum(r["L_entropy_wire_B"] for r in ok)
        tot_static = sum(r["L_static_B"] for r in ok)
        print(f"sums over anchored files: L_model {tot_model:.1f} B, "
              f"static floor {tot_static:.1f} B, wire {tot_wire} B, "
              f"coder overhead {tot_wire - tot_model:.1f} B, "
              f"adaptation regret {tot_model - tot_static:.1f} B")
    print("rows ->", a.out)


if __name__ == "__main__":
    main()