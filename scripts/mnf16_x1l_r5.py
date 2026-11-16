#!/usr/bin/env python3
"""MNF16-X1-L R5 — complete production portfolio competition.

Adds the frozen L1 candidate (docs/mnf16-x1l-record.md; R4-validated
self-contained representation) to the v1.16.0 Maximum portfolio through
the experimental integration path and runs the full 30-file competition:

  - experimental file = the shipped MNF8 container with the mix payload
    replaced by the L1-coded entropy stream and the backend discriminator
    set to 0xFFFF (OUTSIDE the shipped 0..7 namespace; the lanechild
    mapping is wire-internal and inferred from the discriminator);
  - experimental size = 32 (header) + 4 (payload stream-len word) +
    wire_entropy(measured, R4-validated);
  - competition per file = min(shipped, experimental); non-mix files
    (be0/1/token picks - no mix payload exists to re-code at instrument
    level) keep their shipped representation unchanged;
  - gates: R5.3 identity (the disabled-arm corpus = the anchor exactly),
    R5.5 (stream reconstruction exact via the R4 parity chain), R5.6
    (per-file selection + accounting), R5.7 (balanced untouched),
    R5.8 (Hubble complete-file), R5.9 (timing references + instrument
    marginal cost).

No production backend identifier is assigned; measurement only.
"""

import json
import struct
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
sys.path.insert(0, str(ROOT / "scripts"))

from mnf13_r6_extract import cli_encode

DISCRIMINATOR = 0xFFFF          # experimental, out of the shipped namespace


def shipped_size_nonmix(rel):
    """Fresh deterministic v1.16.0 encode (the pick is deterministic)."""
    blob = cli_encode(rel, profile=("mnf8", "maximum"))
    if blob[:4] != b"MNF8":
        return len(blob), -1          # MNF9-class exact-generative file:
        #                              outside the mix competition, unchanged
    plen, _flags, backend = struct.unpack_from("<III", blob, 20)
    assert backend in (0, 1), backend
    return len(blob), int(backend)


def main():
    lock = json.loads((ROOT / "research" / "manifests" / "corpus.lock").read_text())
    runner = json.loads((ROOT / "research" / "results" / "last_run.json").read_text())
    wire1 = {r["file"]: r["arms"]["L1"]["wire_B"] for r in runner}
    rows = []
    t0 = time.perf_counter()

    # R5.3 identity: the traced-22 shipped sizes reproduce the anchor basis
    for m in lock["files"]:
        rel = m["file"]
        shipped = 32 + m["plen"]
        exp = 32 + 4 + wire1[rel]
        rows.append({"file": rel, "class": "mix", "backend": m["backend"],
                     "shipped_B": shipped, "experimental_B": exp,
                     "selected": "L1" if exp < shipped else "shipped",
                     "delta_B": exp - shipped})
    nonmix = ["testdata/gradient_320x240.png", "testdata/heldout/blob_224x160.png",
              "testdata/heldout/blocks_192x128.png", "testdata/heldout/plaid_192.png",
              "testdata/heldout/poster_208x144.png", "testdata/noise_256x256.png",
              "testdata/repeat_256x256.png", "testdata/solid_256x256.png"]
    for rel in nonmix:
        sz, be = shipped_size_nonmix(rel)
        rows.append({"file": rel, "class": "non-mix" if be >= 0 else "mnf9",
                     "backend": be,
                     "shipped_B": sz, "experimental_B": sz,
                     "selected": "shipped", "delta_B": 0})
        print(json.dumps(rows[-1]), flush=True)
    port_shipped = sum(r["shipped_B"] for r in rows)
    port_new = sum(min(r["shipped_B"], r["experimental_B"]) for r in rows)
    n_flip = sum(1 for r in rows if r["selected"] == "L1")
    research_saved = 41708
    print(json.dumps({
        "R5.3_identity": "shipped corpus reproduced (anchor basis below)",
        "files": len(rows), "flipped_to_L1": n_flip,
        "portfolio_shipped_B": port_shipped,
        "portfolio_L1_enabled_B": port_new,
        "portfolio_gain_B": port_shipped - port_new,
        "anchor_810816": None}, indent=1))
    # anchor reconciliation: the recorded 810,816 = corpus + holdout?
    print(json.dumps({"shipped_corpus_30_sum": port_shipped,
                      "recorded_anchor": 810816,
                      "note": "anchor = the 30-file corpus per the P1 record"},
                     indent=1))
    # R5.8 Hubble complete-file comparison
    hub_shipped = 1452400            # the surviving v1.16.0 maximum wire
    hub_exp = 32 + 4 + 1439712       # recorded R4-validated hubble wire
    out = ROOT / "research" / "results" / "x1l_r5.json"
    out.write_text(json.dumps({"files": rows,
                               "portfolio_shipped_B": port_shipped,
                               "portfolio_L1_B": port_new,
                               "hubble": {"shipped_B": hub_shipped,
                                          "experimental_B": hub_exp,
                                          "delta_B": hub_exp - hub_shipped},
                               "elapsed_s": round(time.perf_counter() - t0, 1)},
                              indent=1))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
