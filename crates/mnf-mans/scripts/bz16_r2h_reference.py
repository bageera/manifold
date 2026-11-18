#!/usr/bin/env python3
"""BZ16-R2H recovered-model scalar reference (stdlib-only, pure ints/lists).

The BZ16 research-transfer package does not exist on this machine (verified
by exhaustive filesystem, remote and session-history search), so R2H
executes fully in-repo re-running R2's dual-implementation methodology on
the RECOVERED original semantics. This file implements the recovered model
per the operator memo's R2H-B reconciliation table (the authoritative
recovered description available here); the Rust twin
(`crates/mnf-mans/examples/bz16_r2h_parity.rs`) replays it independently and
the two must agree bit-for-bit, including the nine incremental full-stream
SHA-256 probability-trace digests (R2H-D).

Recovered semantics (memo R2H-B rows; each overrides the R2 pin):
- feature order: circular distance to zero, upper residual, left residual
  (recorded interpretation: cdist(v,0), cdist(v,upper), cdist(v,left) on the
  128x128 torus; pre-stream pad = symbol 0)
- initial coefficients: two rows of three zeros
- expert inverse temperatures: 1 and 2 (k_beta = clamp(beta*E >> (n-4)))
- probability total: 32,768 (fixed across Q8/Q12/Q16)
- mixture weight total: 65,536
- fixed-share: normalize posterior, x999, +16384, normalize again
- gradient rounding: signed nearest, ties away from zero (step 1/32768)
- normalization remainder: first maximum of original input order
- counters: all-ones init, +1 per observation, never rescaled

Components (4): hi expert, lo expert, counter unigram, uniform.
Golden export format: the R2 contract (h/p/f/F/z), one file per
(fixture, precision), with the counters state hashed per position (n line).
p/n lines carry the carries AFTER the update at t (counters AFTER the
t-th increment); E/k at o are pre-update. A `d` header line records the
R2H-D incremental full-stream trace digest.
"""
import hashlib
import math
import os
import sys

K_MAX = 4096
TWO_P24 = 1 << 24
PROB_TOTAL = 32768          # recovered
W_SUM = 65536               # recovered
W_INIT = W_SUM // 4         # 16,384
FS_NUM, FS_ADD, FS_DEN = 999, 16384, 1000   # recovered fixed-share
GRAD_DEN = 32768            # gradient step = 1/prob_total
FREQ_SUM = PROB_TOTAL
WINDOW = 512
GOLDEN_T = (0, 13, 255, 511)   # the four golden positions per fixture/Qn
SIZE = 128
STREAM_LEN = SIZE * SIZE


def make_lut():
    return [max(1, round(TWO_P24 * math.exp(-k / 256.0)))
            for k in range(K_MAX + 1)]


LUT = make_lut()


def cdist(a, b):
    d = abs(a - b)
    return d if d <= 128 else 256 - d


def clamp_k(e, shift):
    k = e >> shift
    if k < 0:
        return 0
    return k if k <= K_MAX else K_MAX


def fnv1a_le(values):
    h = 0xcbf29ce484222325
    for v in values:
        for b in (v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF,
                  (v >> 24) & 0xFF):
            h ^= b
            h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h


def largest_remainder(num, total, sum_to):
    """Largest remainder (ties: remainder desc, then first maximum of the
    original input order). Zero repair pulls from the first maximum.
    Returns (values, repairs). The deficit grant is the sort-based form of
    the iterative first-max rule (each granted remainder drops below zero,
    so no symbol is revisited; ties grant lowest index first)."""
    fs = [(n * sum_to) // total for n in num]
    rems = [(n * sum_to) - f * total for n, f in zip(num, fs)]
    deficit = sum_to - sum(fs)
    order = sorted(range(len(fs)), key=lambda s: (-rems[s], s))
    for s in order[:deficit]:
        fs[s] += 1
    repairs = 0
    for s in range(len(fs)):
        if fs[s] == 0:
            repairs += 1
            m = 0
            for i in range(1, len(fs)):
                if fs[i] > fs[m]:
                    m = i
            fs[m] -= 1
            fs[s] += 1
    assert sum(fs) == sum_to and min(fs) >= 1, "remainder invariant"
    return fs, repairs


def round_half_away(n, d):
    """Signed nearest, ties away from zero."""
    if n >= 0:
        return (2 * n + d) // (2 * d)
    return -((-2 * n + d) // (2 * d))


def fixture_rasters():
    cam = [((97 * (i // SIZE) + 31 * (i % SIZE)
             + (((i // SIZE) * (i % SIZE)) >> 2)) & 0xFF)
           for i in range(STREAM_LEN)]
    brick = [((37 * (i // 1024) + (i % SIZE)
               + (3 * ((i // SIZE) % 8)) & 0xFF)) for i in range(STREAM_LEN)]
    s = 12345
    noise = []
    for _ in range(STREAM_LEN):
        s = (s * 1103515245 + 12345) & 0x7FFFFFFF
        noise.append((s >> 16) & 0xFF)
    return (("camera", cam), ("brick", brick), ("noise", noise))


def neighbors(t, stream):
    """Circular context: upper = row-1 mod 128, left = col-1 mod 128;
    pre-stream pad = symbol 0."""
    if t < SIZE:
        up = 0
    else:
        up = stream[t - SIZE]
    if t % SIZE == 0:
        left = 0
    else:
        left = stream[t - 1]
    return up, left


def mixture(c_hi, c_lo, w, counts, up, left, shift):
    """One position's probability model. Returns (p_hi, p_lo, p_cnt,
    energy-at-o triples for every candidate)."""
    x_unif = 128
    p_hi = [0] * 256
    p_lo = [0] * 256
    h_neg = l_neg = h_max = l_max = 0
    e_min = 1 << 62
    e_max = -(1 << 62)
    for v in range(256):
        d0 = cdist(v, 0) << 5
        d1 = cdist(v, up) << 5
        d2 = cdist(v, left) << 5
        e_hi = c_hi[0] * d0 + c_hi[1] * d1 + c_hi[2] * d2
        e_lo = c_lo[0] * d0 + c_lo[1] * d1 + c_lo[2] * d2
        k_hi = clamp_k(e_hi, shift)
        k_lo = clamp_k(2 * e_lo, shift)
        p_hi[v] = LUT[k_hi] >> 9
        p_lo[v] = LUT[k_lo] >> 9
        h_neg += 1 if e_hi < 0 else 0
        l_neg += 1 if 2 * e_lo < 0 else 0
        h_max += 1 if k_hi == K_MAX else 0
        l_max += 1 if k_lo == K_MAX else 0
        lo_e = min(e_hi, 2 * e_lo)
        hi_e = max(e_hi, 2 * e_lo)
        e_min = lo_e if e_min is None or lo_e < e_min else e_min
        e_max = hi_e if e_max is None or hi_e > e_max else e_max
    cnt_fs, _ = largest_remainder(counts, sum(counts), FREQ_SUM)
    m = [(w[0] * p_hi[v] + w[1] * p_lo[v] + w[2] * cnt_fs[v]
          + w[3] * x_unif) >> 16 for v in range(256)]
    return p_hi, p_lo, cnt_fs, x_unif, m, (h_neg, l_neg, h_max, l_max,
                                           e_max - e_min)


def cums_of(f):
    cums = [0] * 257
    acc = 0
    for s in range(256):
        cums[s] = acc
        acc += f[s]
    cums[256] = acc
    return cums


def adapt(c_hi, c_lo, w, counters, o, ds, p_hi, p_lo, cnt_fs, x_unif, f,
          n_q):
    """Recovered adaptation: gradient (signed nearest, ties away, step
    1/32768) + fixed-share (normalize posterior, x999, +16384, normalize
    again). Counters are updated by the caller (+1 per observation). ds =
    the three feature distances at o, pre-scaled (<<5)."""
    c_lim = 4 << n_q
    s_lim = 3 << n_q   # recovered asymmetric clamp scale S
    for cs, xs in ((c_hi, p_hi), (c_lo, p_lo)):
        err = f[o] - xs[o]
        assert -FREQ_SUM < err <= FREQ_SUM, "gradient err bound"
        for i, di in enumerate(ds):
            num = err * di
            assert abs(num) <= FREQ_SUM * 4096, "gradient num bound"
            # recovered clamp polarity: -S <= b <= 4S with S = 3<<n_q
            cs[i] = max(-s_lim, min(4 * s_lim,
                                    cs[i] - round_half_away(num, GRAD_DEN)))
    posts = [w[0] * p_hi[o], w[1] * p_lo[o], w[2] * cnt_fs[o], w[3] * x_unif]
    post, _ = largest_remainder(posts, sum(posts), W_SUM)
    for i in range(4):
        w[i] = round_half_away(FS_NUM * post[i] + FS_ADD, FS_DEN)
    drift = W_SUM - sum(w)
    if drift:
        m0 = 0
        for i in range(1, 4):
            if w[i] > w[m0]:
                m0 = i
        assert w[m0] + drift >= 0, "drift preserves non-negativity"
        w[m0] += drift
    assert sum(w) == W_SUM and min(w) >= 0, "weights invariant"


def engine_pass(stream, n_q, window):
    """Single pass: model, adapt, and emit everything (goldens + digest)."""
    shift = n_q - 4
    c_lim = 4 << n_q
    s_lim = 3 << n_q                      # recovered asymmetric scale S
    e_lim = 3 * c_lim * 4096
    # Energy-difference live ceiling at the recovered asymmetric clamp
    # [−S, 4S] (S = 3<<n_q). E = Σ3 c_k·d_k, d_k <= 4096, so Σd <= 12288:
    # the lo expert's energy enters doubled, max 2·4S·12288, the hi
    # expert's floor is −S·12288, and the cross-expert spread ceiling is
    # (8S + S)·12288 = 9·S·12288. The memo §5 value 5·e_lim/4 assumes a
    # single feature magnitude; the three-feature engine needs the wider
    # exact ceiling. Widened at the multiplication, per the memo.
    diff_lim = 9 * s_lim * 12288
    c_hi, c_lo, w = [0, 0, 0], [0, 0, 0], [W_INIT] * 4
    counters = [1] * 256
    lines = []
    cost = 0.0
    stat = [0, 0, 0, 0, 0, 0]     # hi_neg, lo_neg, hi_max, lo_max, repairs,
                                  # max observed energy spread
    h = hashlib.sha256()          # R2H-D incremental digest
    for t, o in enumerate(stream):
        up, left = neighbors(t, stream)
        c_sum = sum(counters)
        p_hi, p_lo, cnt_fs, x_unif, m, bump = mixture(
            c_hi, c_lo, w, counters, up, left, shift)
        for i in range(4):
            stat[i] += bump[i]
        assert bump[4] <= diff_lim, "energy-difference bound (t=%d)" % t
        stat[5] = max(stat[5], bump[4])
        total = sum(m)
        if total > 0:
            f, repairs = largest_remainder(m, total, FREQ_SUM)
        else:
            # R2H pin: zero-mixture fallback (reached under extreme LUT
            # saturation, all mixture numerators flooring to 0); uniform
            # frequency vector keeps the contract exact.
            f = [FREQ_SUM // 256] * 256
            repairs = 0
        stat[4] += repairs
        cums = cums_of(f)
        h.update(b"".join(v.to_bytes(2, "big") for v in f))   # R2H-D
        # per-candidate energies at the observed symbol (for the export)
        d0 = cdist(o, 0) << 5
        d1 = cdist(o, up) << 5
        d2 = cdist(o, left) << 5
        e_hi_o = c_hi[0] * d0 + c_hi[1] * d1 + c_hi[2] * d2
        e_lo_o = c_lo[0] * d0 + c_lo[1] * d1 + c_lo[2] * d2
        # asymmetric domain: hi in [-S, 4S]·Σd, lo doubled
        assert -s_lim * 12288 <= e_hi_o <= 4 * s_lim * 12288
        assert -2 * s_lim * 12288 <= 2 * e_lo_o <= 8 * s_lim * 12288
        k_hi = clamp_k(e_hi_o, shift)
        k_lo = clamp_k(2 * e_lo_o, shift)
        adapt(c_hi, c_lo, w, counters, o,
              (cdist(o, 0) << 5, cdist(o, up) << 5, cdist(o, left) << 5),
              p_hi, p_lo, cnt_fs, x_unif, f, n_q)
        counters[o] += 1                     # all-ones init, +1, no rescale
        cost += 15.0 - math.log2(f[o])
        if t < window:
            lines.append(
                "p %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d %d"
                % (t, o, e_hi_o, e_lo_o, k_hi, k_lo,
                   c_hi[0], c_hi[1], c_hi[2],
                   c_lo[0], c_lo[1], c_lo[2],
                   w[0], w[1], w[2], w[3]))
            lines.append("n " + str(fnv1a_le(counters)))
            lines.append("f " + " ".join(str(v) for v in f))
            lines.append("F " + " ".join(str(v) for v in cums))
    lines.append("z %d %d %d %d %d %d %d %d %d %d"
                 % (c_hi[0], c_hi[1], c_hi[2], c_lo[0], c_lo[1], c_lo[2],
                    w[0], w[1], w[2], w[3]))
    return lines, cost, tuple(stat), h.hexdigest()


def r2g_roundtrip(stream, n_q):
    """R2-G-equivalent carry-less CACM87 instrument (window only): the
    decoder recomputes all model state from the decoded symbols alone."""
    shift = n_q - 4
    c_hi, c_lo, w = [0, 0, 0], [0, 0, 0], [W_INIT] * 4
    counters = [1] * 256

    def model_for(prefix, t):
        """Model state at position t over the coded/decoded prefix."""
        up = prefix[t - SIZE] if t >= SIZE else 0
        left = prefix[t - 1] if t % SIZE != 0 else 0
        p_hi, p_lo, cnt_fs, x_unif, m, _ = mixture(
            c_hi, c_lo, w, counters, up, left, shift)
        total = sum(m)
        if total > 0:
            f, _ = largest_remainder(m, total, FREQ_SUM)
        else:
            f = [FREQ_SUM // 256] * 256       # zero-mixture fallback
        return up, left, p_hi, p_lo, cnt_fs, x_unif, f, cums_of(f)

    # ---- encode ----
    # 18-bit window: the CACM87 exit-zone leaves a minimum interval of
    # window/4 + 1; at the recovered FREQ_SUM = 2^15 that floor must stay
    # >= 2*F (16-bit windows invert f=1 sub-intervals and the renorm loop
    # never terminates). 2^16+1 = 65,537 >= 2*32,768.
    low, high, pending = 0, 0x3FFFF, 0
    bits = []

    def emit(bit):
        nonlocal pending
        bits.append(bit)
        while pending > 0:
            bits.append(1 - bit)
            pending -= 1

    for t, o in enumerate(stream):
        up, left, p_hi, p_lo, cnt_fs, x_unif, f, cums = model_for(stream, t)
        assert 1 <= f[o] <= FREQ_SUM
        rng = high - low + 1
        high = low + (rng * cums[o + 1]) // FREQ_SUM - 1
        low = low + (rng * cums[o]) // FREQ_SUM
        assert low <= high, "inverted interval (encoder, t=%d)" % t
        it = 0
        while True:
            it += 1
            assert it <= 256, "renorm loop (encoder, t=%d)" % t
            if high < 0x20000:
                emit(0)
            elif low >= 0x20000:
                emit(1)
                low -= 0x20000
                high -= 0x20000
            elif low >= 0x10000 and high < 0x30000:
                pending += 1
                low -= 0x10000
                high -= 0x10000
            else:
                break
            low <<= 1
            high = (high << 1) | 1
        adapt(c_hi, c_lo, w, counters, o,
              (cdist(o, 0) << 5, cdist(o, up) << 5, cdist(o, left) << 5),
              p_hi, p_lo, cnt_fs, x_unif, f, n_q)
        counters[o] += 1
    pending += 1
    emit(0 if low < 0x10000 else 1)

    # ---- decode: state from decoded symbols only ----
    low, high, code = 0, 0x3FFFF, 0
    pos = 0
    out = []
    c_hi, c_lo, w = [0, 0, 0], [0, 0, 0], [W_INIT] * 4
    counters = [1] * 256
    for _ in range(18):
        b = bits[pos] if pos < len(bits) else 0
        pos += 1
        code = (code << 1) | b
    for _ in range(len(stream)):
        t = len(out)
        up, left, p_hi, p_lo, cnt_fs, x_unif, f, cums = model_for(out, t)
        rng = high - low + 1
        value = ((code - low + 1) * FREQ_SUM - 1) // rng
        s = 0
        while s < 255 and cums[s + 1] <= value:
            s += 1
        assert 0 <= s <= 255
        high = low + (rng * cums[s + 1]) // FREQ_SUM - 1
        low = low + (rng * cums[s]) // FREQ_SUM
        assert low <= high, "inverted interval (decoder, t=%d)" % t
        it = 0
        while True:
            it += 1
            assert it <= 256, "renorm loop (decoder, t=%d)" % t
            if high < 0x20000:
                pass
            elif low >= 0x20000:
                code -= 0x20000
                low -= 0x20000
                high -= 0x20000
            elif low >= 0x10000 and high < 0x30000:
                code -= 0x10000
                low -= 0x10000
                high -= 0x10000
            else:
                break
            b = bits[pos] if pos < len(bits) else 0
            pos += 1
            code = (code << 1) | b
            low <<= 1
            high = (high << 1) | 1
        adapt(c_hi, c_lo, w, counters, s,
              (cdist(s, 0) << 5, cdist(s, up) << 5, cdist(s, left) << 5),
              p_hi, p_lo, cnt_fs, x_unif, f, n_q)
        out.append(s)
        counters[s] += 1
    assert out == list(stream), "R2H instrument roundtrip"
    return bits


def main():
    outdir = sys.argv[1] if len(sys.argv) > 1 else "."
    os.makedirs(outdir, exist_ok=True)
    for name, data in fixture_rasters():
        stream_sha = hashlib.sha256(bytes(data)).hexdigest()
        for n_q in (8, 12, 16):
            base = "bz16-r2h-%s-Q%d" % (name, n_q)
            lines, cost, stats, trace = engine_pass(data, n_q, WINDOW)
            path = os.path.join(outdir, base + ".txt")
            with open(path, "w") as fh:
                fh.write("h %s %d %d %d %d %d\n"
                         % (name, n_q, WINDOW, W_SUM, FREQ_SUM,
                            len(data)))
                fh.write("d %s\n" % trace)
                fh.write("\n".join(lines) + "\n")
            bits = r2g_roundtrip(data[:WINDOW], n_q)
            print("%s stream-sha256=%s trace-digest=%s ac=OK(%d bits) "
                  "cost=%.2f stats=%s"
                  % (base, stream_sha, trace, len(bits), cost, stats))
    print("recovered bounds: Q8 %d Q12 %d Q16 %d (gradient x16 %d)"
          % (3 * (4 << 8) * 4096, 3 * (4 << 12) * 4096,
             3 * (4 << 16) * 4096,
             16 * 32768 * 4096 * 4096 * 3))
    # R2H-E clamp reconciliation: the recovered asymmetric clamp [−S, 4S]
    # (S = 3<<n_q) is now the implemented polarity, per the memo. The §5
    # identity 5·e_lim/4 assumes a single feature magnitude; the
    # three-feature engine's exact live ceiling is 9·S·12288 (hi floor
    # −S·12288 vs the lo expert's doubled max 8S·12288), asserted here.
    assert 5 * (3 * (4 << 8) * 4096) // 4 == 15_728_640
    assert 5 * (3 * (4 << 12) * 4096) // 4 == 251_658_240
    assert 5 * (3 * (4 << 16) * 4096) // 4 == 4_026_531_840
    assert 9 * (3 << 8) * 12288 == 84_934_656
    assert 9 * (3 << 12) * 12288 == 1_358_954_496
    assert 9 * (3 << 16) * 12288 == 21_743_271_936
    print("energy-difference live ceiling (asymmetric [−S,4S]): "
          "9·S·12288 = %d/%d/%d; memo §5 single-feature 5·e_lim/4 = "
          "15728640/251658240/4026531840"
          % (84_934_656, 1_358_954_496, 21_743_271_936))


if __name__ == "__main__":
    main()
