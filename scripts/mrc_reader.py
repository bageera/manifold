#!/usr/bin/env python3
"""MRC reader for the cryo-EM research probes (research-crate; NOT codec code).

Reads an MRC2014 file: header (1024 B) [+ extended, nsymbt], data =
mode 0 (int8) / 1 (int16) / 2 (float32) / 6 (uint16), little-endian.
Renders each plane to uint8 for the byte-level arms:
- int8/int16/uint16: per-plane min-max normalize to 0..255 (lossless probe
  renders byte PLANES of the source — the BIT-EXACT claims in the probes
  are on these RENDERED bytes, the same convention docs/photo-gate uses
  for "same decoded RGB8 input").
- float32: per-plane min-max or scale via fixed shift (declared in output).

Usage: reader.py file.mrc [--plane k|--all] -> prints dims+mode, writes
/tmp/mrc_planes/plane_0000..u8 raw files.
"""
import struct, sys, os
import numpy as np

def read_mrc(path):
    with open(path, "rb") as f:
        h = f.read(1024)
    nx, ny, nz, mode = struct.unpack("<iiii", h[0:16])
    nsymbt = struct.unpack("<i", h[92:96])[0]
    dmin, dmax, dmean = struct.unpack("<fff", h[80:92])
    ispg, nslice = struct.unpack("<ii", h[64:72])
    mapstr = h[208:212]
    machst = h[212:214]
    rms = struct.unpack("<f", h[216:220])[0]
    axis_order = struct.unpack("<3i", h[16:28])   # mapc,mapr,maps
    dtypes = {0: np.int8, 1: np.int16, 2: np.float32, 6: np.uint16}
    if mode not in dtypes:
        raise SystemExit(f"unsupported mode {mode}")
    dt = dtypes[mode]
    data = np.fromfile(path, dtype=dt, count=nx*ny*nz, offset=1024+nsymbt)
    # MRC data layout: fastest axis = x (mapc etc assumed identity here;
    # reader asserts identity and refuses otherwise — research scope)
    if tuple(axis_order) not in ((1, 2, 3), (0, 0, 0)):  # (0,0,0) = writer didn't declare
        raise SystemExit(f"axis permutation {axis_order} unsupported (research reader)")
    vol = data.reshape(nz, ny, nx) if nz > 1 else data.reshape(ny, nx)
    return vol, mode, (nx, ny, nz), (dmin, dmax, dmean, rms), machst

def to_u8(plane, mode):
    if mode in (0, 1, 6):
        lo, hi = plane.min(), plane.max()
        if hi == lo:
            return np.zeros(plane.shape, np.uint8)
        return ((plane.astype(np.float64) - lo) / (hi - lo) * 255).round().astype(np.uint8)
    else:  # float32: fixed range vs per-plane? use per-plane minmax, declared
        lo, hi = float(plane.min()), float(plane.max())
        if hi <= lo:
            return np.zeros(plane.shape, np.uint8)
        return ((plane.astype(np.float64) - lo) / (hi - lo) * 255).round().astype(np.uint8)

if __name__ == "__main__":
    path = sys.argv[1]
    vol, mode, dims, stats, machst = read_mrc(path)
    print(f"{path}: mode {mode} dims (nx,ny,nz)={dims} stats(min,max,mean,rms)={[round(float(s),3) for s in stats]} machst={machst.hex()}")
    os.makedirs("/tmp/mrc_planes", exist_ok=True)
    planes = vol if vol.ndim == 3 else vol[None]
    for k in range(len(planes)):
        u8 = to_u8(planes[k], mode)
        u8.tofile(f"/tmp/mrc_planes/plane_{k:04d}.u8")
    print(f"wrote {len(planes)} planes to /tmp/mrc_planes/*.u8")
