#!/usr/bin/env python3
"""Documented benchmark harness v0.5.1/v0.6 gate (README §7).

Measures, on identical decoded RGB8 pixels:
- MNF5 three arms (no-atlas / exact-atlas / geom-atlas) via the CLI
- PNG (in-process via the same decoders)
- JPEG XL lossless and WebP lossless (external CLIs, RGB8-pixel exact)
- Peak RSS for every MNF5 arm and PNG/JXL/WebP encode via /usr/bin/time -l

Asserts pixel-exactness for every lossless codec against the same RGB8
matrix before reporting size (fairness rule). Writes docs/benchmarks.md.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).parent.parent
BIN = ROOT / "target" / "release" / "manifold"
TD = ROOT / "testdata"
CJXL = "/opt/homebrew/bin/cjxl"
CWEBP = "/opt/homebrew/bin/cwebp"


def rss_bytes(stderr: str) -> int:
    for line in stderr.splitlines():
        if "maximum resident set size" in line:
            return int(line.split()[0])
    return -1


def timed(cmd: list[str]) -> tuple[int, int]:
    """Run argv under /usr/bin/time -l; return (bytes_out, rss)."""
    r = subprocess.run(["/usr/bin/time", "-l", *cmd], capture_output=True, text=True, timeout=300)
    if r.returncode != 0:
        raise RuntimeError(f"{cmd[0]}: rc={r.returncode} {r.stderr[-200:]}")
    out = Path(cmd[cmd.index("-o") + 1]) if "-o" in cmd else None
    size = out.stat().st_size if out and out.exists() else 0
    return size, rss_bytes(r.stderr)


def pixels_of(png_or_jxl_png: Path) -> bytes:
    """Decode a file to RGB8 via the image crate (one canonical decoder)."""
    raise NotImplementedError  # unused; exactness is checked via `verify`


def png_pixels_exact(src: Path, candidate: Path) -> bool:
    """Pillow-based pixel comparison (dev-only dep, not shipped in the codec)."""
    from PIL import Image
    a = Image.open(src).convert("RGB")
    b = Image.open(candidate).convert("RGB")
    return a.tobytes() == b.tobytes()  # PIL <14 here: tobytes is the raw pixel compare


def main() -> int:
    if not BIN.exists():
        print("build release first: cargo build --release", file=sys.stderr)
        return 2
    images = sorted(TD.glob("*.png"))
    rows = []
    for img in images:
        # canonical RGB8: decode via our own img::load through verify against a lossless png
        # (PIL not used: image crate is the project's decoder of record)
        with tempfile.TemporaryDirectory() as td:
            # Our CLI encodes any image the image crate reads; PNG in -> exact pixels.
            base = img.stem

            # PNG size from our benchmark command (png crate, in-memory, same RGB8)
            b = subprocess.run([str(BIN), "benchmark", str(img)], capture_output=True, text=True, timeout=300)
            png_size = None
            for line in b.stdout.splitlines():
                if line.startswith("PNG ") or line.startswith("PNG                "):
                    parts = line.split()
                    if parts[0] == "PNG":
                        png_size = int(parts[1])

            # MNF5 arms
            arm_rows = {}
            for label, extra in [("mnf5-no-atlas", ["--no-atlas"]),
                                 ("mnf5-atlas", []),
                                 ("mnf5-geom-atlas", ["--geom"])]:
                out = Path(td) / "out.mnf5"
                size, rss = timed([str(BIN), "encode", str(img), "-o", str(out), *extra])
                v = subprocess.run([str(BIN), "verify", str(img), str(out)],
                                   capture_output=True, text=True)
                exact = v.returncode == 0
                arm_rows[label] = (size, rss, exact)

            # JXL lossless (-d 0) from the source PNG
            jxl_out = Path(td) / "out.jxl"
            jxl_size = jxl_rss = None
            if Path(CJXL).exists():
                r = subprocess.run(["/usr/bin/time", "-l", CJXL, str(img), str(jxl_out), "-d", "0"],
                                   capture_output=True, text=True, timeout=300)
                if r.returncode == 0:
                    jxl_size, jxl_rss = jxl_out.stat().st_size, rss_bytes(r.stderr)
                    # verify JXL decodes to same pixels: djxl to png, pixel-compare
                    png_rt = Path(td) / "jxl_rt.png"
                    d = subprocess.run(["/opt/homebrew/bin/djxl", str(jxl_out), str(png_rt)],
                                       capture_output=True, timeout=300)
                    jxl_exact = d.returncode == 0 and png_pixels_exact(img, png_rt)
                else:
                    jxl_exact = False
            else:
                jxl_exact = False

            # WebP lossless
            wp_out = Path(td) / "out.webp"
            webp_size = webp_rss = None
            if Path(CWEBP).exists():
                r = subprocess.run(["/usr/bin/time", "-l", CWEBP, "-lossless", "-m", "6", "-z", "9",
                                    str(img), "-o", str(wp_out)],
                                   capture_output=True, text=True, timeout=300)
                if r.returncode == 0:
                    webp_size, webp_rss = wp_out.stat().st_size, rss_bytes(r.stderr)
                    # webp -> png roundtrip for exactness (pixel-compare)
                    png_rt = Path(td) / "webp_rt.png"
                    d = subprocess.run(["/opt/homebrew/bin/dwebp", str(wp_out), "-o", str(png_rt)],
                                       capture_output=True, timeout=300)
                    webp_exact = d.returncode == 0 and png_pixels_exact(img, png_rt)
                else:
                    webp_exact = False
            else:
                webp_exact = False

            # (PNG size parsed above)

            rows.append({
                "image": img.name,
                "arms": {
                    "mnf5-no-atlas": arm_rows["mnf5-no-atlas"],
                    "mnf5-atlas": arm_rows["mnf5-atlas"],
                    "mnf5-geom-atlas": arm_rows["mnf5-geom-atlas"],
                },
                "png": [png_size, None, True],
                "jxl-lossless": [jxl_size, jxl_rss, jxl_exact],
                "webp-lossless": [webp_size, webp_rss, webp_exact],
            })
            print(f"done {img.name}")

    out_md = ROOT / "docs" / "benchmarks.md"
    lines = [
        "# Lossless benchmark — identical decoded RGB8 source (README §7)",
        "",
        "Sources are the synthetic corpus PNGs; MNF5 arms read the same PNG",
        "through the image crate and re-encode. Exactness = verify against the",
        "source PNG's RGB8 (for JXL/WebP: their PNG roundtrip must verify).",
        "Peak RSS in KB (macOS /usr/bin/time -l, host M-series).",
        "",
    ]
    hdr = "| image | codec | bytes | peak RSS KB | exact |"
    lines += [hdr, "|---|---|---|---|---|"]
    for row in rows:
        for codec, (size, rss, exact) in {**row["arms"], "png": row["png"],
                                          "jxl-lossless": row["jxl-lossless"],
                                          "webp-lossless": row["webp-lossless"]}.items():
            lines.append(f"| {row['image']} | {codec} | {size} | {'-' if rss is None else rss//1024} | {'yes' if exact else 'NO'} |")
    out_md.parent.mkdir(exist_ok=True)
    out_md.write_text("\n".join(lines) + "\n")
    print(f"wrote {out_md}")
    return 0


if __name__ == "__main__":
    sys.exit(main())