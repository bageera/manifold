#!/usr/bin/env python3
"""BZ16-R2 scalar reference engine (stdlib-only, pure Python ints/lists).

Companion to the BZ16 memo chain and docs/bz16-r2-record.md. This is the
in-repo *reference* implementation of the pinned integer Boltzmann model:
scalar integers and lists only, no NumPy, so no vectorization-semantics
dependency (the BZ16-I6 methodology, reproduced in-repo).

Provenance split (authoritative version in docs/bz16-r2-record.md):
- FROZEN by memo: LUT formula + 1/256 energy resolution + k in [0,4096];
  256 symbols; three circular-distance features; Q12 coefficients;
  two inverse-temperature experts; four integer fixed-share mixture
  weights; half-even rounding; frequencies/cumulative u32, products u64;
  no new mathematical families.
- PINNED by R2 (underdetermined by the memos, awaiting the accepted
  research-environment reference; each choice must be re-pointed on
  arrival if the accepted engine differs): feature definitions,
  temperature shifts, init state, mixture/freeze components, update
  step shifts, tie-break rules.

Golden export format (pinned contract, one fixture per file):
  h <fixture> <window> <W_SUM> <stream_len>     header
  p <t> <o> <E_hi(o)> <E_lo(o)> <k_hi(o)> <k_lo(o)>
     <c_hi0..2> <c_lo0..2> <w0..3>              carry state BEFORE t + E/k at o
  f <256 frequencies>                           model output at t
  F <257 cumulative frequencies>                coding intervals at t
  z <c_hi0..2> <c_lo0..2> <w0..3>               carries AFTER the full run

The rANS section is a BZ16-R2-G verification instrument only (memo §6:
the arithmetic coder stays out of the production surface).
"""
import hashlib
import math
import os
import sys

# --- frozen: LUT (memo section 3, R1-confirmed) -------------------------
K_MAX = 4096
TWO_P24 = 1 << 24


def make_lut():
    return [max(1, round(TWO_P24 * math.exp(-k / 256.0)))
            for k in range(K_MAX + 1)]


LUT = make_lut()

W_SUM = 4096            # fixed-share weight sum (Q12)
W_INIT = W_SUM // 4     # four mixture weights, init uniform
ALPHA_NUM, ALPHA_DEN = 64, 4096   # fixed-share rate (pinned 1/64)
STEP_SHIFT = 15         # gradient step denominator 2^15 (pinned)
C_LIMIT = 4 * 4096      # frozen conservative coefficient bound |c| <= 16384
E_LIMIT = 3 * C_LIMIT * 4096    # = 201,326,592 (memo section 5 frozen value)
FREQ_SUM = 4096         # Q12 probability mass
WINDOW = 512            # audited positions per fixture (memo section 7)
CHUNK_BITS = 16
RENORM = FREQ_SUM << CHUNK_BITS   # 4096 << 16 = 2^28


def cdist(a, b):
    d = abs(a - b)
    return d if d <= 128 else 256 - d


def clamp_k(e, shift):
    # frozen: k = energy in 1/256 units, index clamp [0, 4096]
    k = e >> shift
    if k < 0:
        return 0
    return k if k <= K_MAX else K_MAX


def largest_remainder(num, den):
    """Quantize num/den to a 256-vector summing FREQ_SUM (ties: remainder
    desc, then lower symbol index). Zero-repair: f==0 pulls 1 from the
    current argmax (ties on the argmax: lowest index)."""
    fs = [(n * FREQ_SUM) // den for n in num]
    rems = [(n * FREQ_SUM) - f * den for n, f in zip(num, fs)]
    deficit = FREQ_SUM - sum(fs)
    order = sorted(range(256), key=lambda s: (-rems[s], s))
    for s in order[:deficit]:
        fs[s] += 1
    repairs = 0
    for s in range(256):
        if fs[s] == 0:
            repairs += 1
            m = max(range(256), key=lambda i: (fs[i], -i))
            fs[m] -= 1
            fs[s] += 1
    assert sum(fs) == FREQ_SUM and min(fs) >= 1, "normalization invariant"
    return fs, repairs


def predict(state, prev3):
    """Mixture stage for one position. Returns (x_hi, x_lo, f, cums, reps)."""
    c_hi, c_lo, w, f_prev, x_prev = state
    x_unif = TWO_P24 // 256
    x_hi = [0] * 256
    x_lo = [0] * 256
    m = [0] * 256
    for s in range(256):
        d1 = cdist(s, prev3[2]) << 5
        d2 = cdist(s, prev3[1]) << 5
        d3 = cdist(s, prev3[0]) << 5
        x_hi[s] = LUT[clamp_k(c_hi[0] * d1 + c_hi[1] * d2 + c_hi[2] * d3, 4)]
        x_lo[s] = LUT[clamp_k(c_lo[0] * d1 + c_lo[1] * d2 + c_lo[2] * d3, 5)]
        m[s] = (w[0] * x_hi[s] + w[1] * x_lo[s] + w[2] * x_unif
                + w[3] * x_prev[s]) >> 12
    total = sum(m)
    f, repairs = largest_remainder(m, total)
    cums = [0] * 257
    acc = 0
    for s in range(256):
        cums[s] = acc
        acc += f[s]
    cums[256] = acc
    return x_hi, x_lo, f, cums, repairs


def observe(state, o, x_hi, x_lo, f, prev3):
    """Adaptation stage: gradient update + integer fixed-share."""
    c_hi, c_lo, w, f_prev, x_prev = state
    d1 = cdist(o, prev3[2]) << 5
    d2 = cdist(o, prev3[1]) << 5
    d3 = cdist(o, prev3[0]) << 5
    for cs, xs, shift in ((c_hi, x_hi, 4), (c_lo, x_lo, 5)):
        err = f[o] - (xs[o] >> 12)
        assert -FREQ_SUM < err <= FREQ_SUM, "gradient err bound"
        for i, di in enumerate((d1, d2, d3)):
            num = err * di
            assert abs(num) <= FREQ_SUM * 4096, "gradient num bound"
            cs[i] = max(-C_LIMIT, min(C_LIMIT, cs[i] - (num >> STEP_SHIFT)))
    losses = (abs(f[o] - (x_hi[o] >> 12)), abs(f[o] - (x_lo[o] >> 12)),
              abs(f[o] - 16), abs(f[o] - f_prev[o]))
    best = 0
    for j in range(1, 4):
        if losses[j] < losses[best]:
            best = j
    keep = ALPHA_DEN - ALPHA_NUM
    for j in range(4):
        w[j] = (w[j] * keep + (W_SUM * ALPHA_NUM if j == best else 0)) >> 12
    drift = W_SUM - sum(w)
    if drift > 0:
        j = max(range(4), key=lambda i: (w[i], -i))
        w[j] += drift
    elif drift < 0:
        j = min(range(4), key=lambda i: (w[i], i))
        w[j] += drift
    assert sum(w) == W_SUM and all(0 <= v <= W_SUM for v in w), "weights"
    for s in range(256):
        x_prev[s] = f[s] << 12
        f_prev[s] = f[s]


def new_state():
    return ([0, 0, 0], [0, 0, 0], [W_INIT] * 4, [16] * 256,
            [16 << 12] * 256)


def run_prefix(stream, window):
    hi_neg = lo_neg = hi_max = lo_max = repairs = 0
    state = new_state()
    prev3 = (0, 0, 0)
    lines = []
    cost = 0.0
    for t in range(len(stream)):
        o = stream[t]
        c_hi, c_lo, w = state[0], state[1], state[2]
        x_hi, x_lo, f, cums, rep = predict(state, prev3)
        d1 = cdist(o, prev3[2]) << 5
        d2 = cdist(o, prev3[1]) << 5
        d3 = cdist(o, prev3[0]) << 5
        e_hi = c_hi[0] * d1 + c_hi[1] * d2 + c_hi[2] * d3
        e_lo = c_lo[0] * d1 + c_lo[1] * d2 + c_lo[2] * d3
        assert abs(e_hi) <= E_LIMIT and abs(e_lo) <= E_LIMIT, "energy bound"
        k_hi = clamp_k(e_hi, 4)
        k_lo = clamp_k(e_lo, 5)
        hi_neg += 1 if e_hi < 0 else 0
        lo_neg += 1 if e_lo < 0 else 0
        hi_max += 1 if k_hi == K_MAX else 0
        lo_max += 1 if k_lo == K_MAX else 0
        repairs += rep
        cost += 12.0 - math.log2(f[o])
        if t < window:
            lines.append(
                "p %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d"
                % (t, o, e_hi, e_lo, k_hi, k_lo, c_hi[0], c_hi[1], c_hi[2],
                   c_lo[0], c_lo[1], c_lo[2], w[0], w[1], w[2], w[3]))
            lines.append("f " + " ".join(str(v) for v in f))
            lines.append("F " + " ".join(str(v) for v in cums))
        observe(state, o, x_hi, x_lo, f, prev3)
        prev3 = (prev3[1], prev3[2], o)
    c_hi, c_lo, w = state[0], state[1], state[2]
    lines.append("z %d %d %d %d %d %d %d %d %d %d"
                 % (c_hi[0], c_hi[1], c_hi[2], c_lo[0], c_lo[1], c_lo[2],
                    w[0], w[1], w[2], w[3]))
    return lines, cost, (hi_neg, lo_neg, hi_max, lo_max, repairs)


# --- R2-G verification instrument: CACM87 carry-less range coder --------
# 16-bit window, 8-bit renorm equivalent (bit-level), underflow counted
# per the classic E0/E1/E3 rules so no carry propagation is needed.
R_HALF = 0x8000
R_QUARTER = 0x4000
R_3QUARTER = 0xC000


def ac_encode(stream):
    """Returns the emitted bit list over the pinned F_t vectors."""
    low = 0
    high = R_HALF * 2 - 1            # 0xFFFF
    pending = 0
    bits = []
    state = new_state()
    prev3 = (0, 0, 0)

    def emit(bit):
        nonlocal pending
        bits.append(bit)
        while pending > 0:
            bits.append(1 - bit)
            pending -= 1

    for o in stream:
        x_hi, x_lo, f, cums, _ = predict(state, prev3)
        assert 1 <= f[o] <= FREQ_SUM, "coder-fit frequency"
        rng = high - low + 1
        high = low + (rng * cums[o + 1]) // FREQ_SUM - 1
        low = low + (rng * cums[o]) // FREQ_SUM
        while True:
            if high < R_HALF:
                emit(0)
            elif low >= R_HALF:
                emit(1)
                low -= R_HALF
                high -= R_HALF
            elif low >= R_QUARTER and high < R_3QUARTER:
                pending += 1
                low -= R_QUARTER
                high -= R_QUARTER
            else:
                break
            low <<= 1
            high = (high << 1) | 1
        observe(state, o, x_hi, x_lo, f, prev3)
        prev3 = (prev3[1], prev3[2], o)
    pending += 1
    emit(0 if low < R_QUARTER else 1)
    return bits


def ac_decode(bits, n):
    """Forward decode; the engine evolves from decoded symbols only."""
    low = 0
    high = R_HALF * 2 - 1
    code = 0
    pos = 0                             # bit reader cursor (MSB-first)
    out = []
    state = new_state()
    prev3 = (0, 0, 0)

    def get_bit():
        nonlocal pos
        b = bits[pos] if pos < len(bits) else 0
        pos += 1
        return b

    # preload 16 bits
    for _ in range(16):
        code = (code << 1) | get_bit()
    for _ in range(n):
        x_hi, x_lo, f, cums, _ = predict(state, prev3)
        rng = high - low + 1
        value = ((code - low + 1) * FREQ_SUM - 1) // rng
        s = 0
        while s < 255 and cums[s + 1] <= value:
            s += 1
        assert 0 <= s <= 255 and f[s] >= 1, "decode symbol range"
        high = low + (rng * cums[s + 1]) // FREQ_SUM - 1
        low = low + (rng * cums[s]) // FREQ_SUM
        while True:
            if high < R_HALF:
                pass
            elif low >= R_HALF:
                code -= R_HALF
                low -= R_HALF
                high -= R_HALF
            elif low >= R_QUARTER and high < R_3QUARTER:
                code -= R_QUARTER
                low -= R_QUARTER
                high -= R_QUARTER
            else:
                break
            code = (code << 1) | get_bit()
            low <<= 1
            high = (high << 1) | 1
        out.append(s)
        observe(state, s, x_hi, x_lo, f, prev3)
        prev3 = (prev3[1], prev3[2], s)
    return out


def fixture_camera(n):
    return [((97 * (i // 128) + 31 * (i % 128)
              + (((i // 128) * (i % 128)) >> 2)) & 0xFF) for i in range(n)]


def fixture_brick(n):
    return [((37 * (i // 1024) + (i % 128) + ((i // 128) % 8) * 3) & 0xFF)
            for i in range(n)]


def fixture_noise(n, seed=12345):
    out = []
    s = seed
    for _ in range(n):
        s = (s * 1103515245 + 12345) & 0x7FFFFFFF
        out.append((s >> 16) & 0xFF)
    return out


def main():
    outdir = sys.argv[1] if len(sys.argv) > 1 else "."
    os.makedirs(outdir, exist_ok=True)
    n = 128 * 128
    fixtures = (("camera", bytes(fixture_camera(n))),
                ("brick", bytes(fixture_brick(n))),
                ("noise", bytes(fixture_noise(n))))
    for name, data in fixtures:
        digest = hashlib.sha256(data).hexdigest()
        lines, cost, stats = run_prefix(data, WINDOW)
        path = os.path.join(outdir, "bz16-r2-golden-%s.txt" % name)
        with open(path, "w") as fh:
            fh.write("h %s %d %d %d\n" % (name, WINDOW, W_SUM, n))
            fh.write("\n".join(lines) + "\n")
        gd = hashlib.sha256(open(path, "rb").read()).hexdigest()
        win = list(data[:WINDOW])
        bittot = ac_encode(win)
        decoded = ac_decode(bittot, len(win))
        assert decoded == win, "R2-G roundtrip: " + name
        print("%s stream-sha256=%s golden-sha256=%s vectors=%d stats=%s "
              "ac=OK(%d bits) cost=%.2f"
              % (name, digest, gd, (len(lines) - 1) // 3, stats,
                 len(bittot), cost))
    print("E_LIMIT %d (frozen memo value)" % E_LIMIT)
    print("B-check k-table:", [(e, clamp_k(e, 4), clamp_k(e, 5)) for e in
                               (0, 1, 4095, 4096, 65535, 65536, -1, E_LIMIT)])


if __name__ == "__main__":
    main()
