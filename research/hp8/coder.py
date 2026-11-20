#!/usr/bin/env python3
"""HP8 — screening-only range coder (constriction-0.5-inspired).

SUPERSEDED for wire accounting by tests/hp8_wire_probe.rs, which drives the
PRODUCTION constriction coder directly (this Python port matches byte
lengths but not bit-exact bytes; its inverted-situation handling differs
by the ±1 deferred-word bookkeeping). Kept for ideal-cost screening.

  encode:  scale = range >> P; range = scale*p; lower += scale*cum (wrap 2^64)
           then if range < 2^32: emit word (lower>>32); lower <<= 32; range <<= 32
  decode:  quantile = (point - lower (wrap)) // scale; update same; then
           renorm: lower <<= 32; range <<= 32; point = (point<<32) | next word
  seal:    append point_word = (lower + 2^32 - 1) >> 32   (normal situation)
  bytes:   u32 words little-endian (production's `to_le_bytes` packing)

The no-wrap invariant lower + range <= 2^64-1 holds by induction
(scale*(c+p) <= (range>>P)<<P <= range) and is asserted at every step.

Stats count every mul/div/shift-emit for the screening op-count currency.
"""
import numpy as np

M64 = (1 << 64) - 1
W32 = 1 << 32


class RangeEncoder:
    """Production encoder incl. the inverted-situation bulk-word deferral
    (constriction stream/queue.rs encode_symbol/seal, Normal<->Inverted)."""

    NORMAL, INV = 0, 1

    def __init__(self):
        self.lower = 0
        self.range = M64
        self.words = []
        self.situation = (self.NORMAL, None)  # (kind, (num_inverted, first_word))
        self.stats = dict(mul=0, div=0, emit=0, renorm=0, calls=0)

    def encode(self, cum, p, precision):
        assert 1 <= p <= (1 << precision), "p in [1, 2^P]"
        assert cum + p <= (1 << precision), "cdf within total"
        s = self.stats
        s["calls"] += 1
        scale = self.range >> precision
        new_lower = (self.lower + scale * cum) & M64
        s["mul"] += 2
        no_wrap = ((new_lower + self.range) & M64) > new_lower
        if self.situation[0] == self.INV and no_wrap:
            n, first = self.situation[1]
            first = (first + 1) if new_lower < self.lower else first
            self.words.append(first)
            for _ in range(n - 1):
                self.words.append(0xFFFFFFFF)
            s["emit"] += n
            self.situation = (self.NORMAL, None)
        self.lower = new_lower
        self.range = scale * p
        assert self.range >= 1
        if self.range < W32:
            lower_word = (self.lower >> 32) & 0xFFFFFFFF
            self.lower = (self.lower << 32) & M64
            self.range = (self.range << 32) & M64
            s["renorm"] += 1
            if self.situation[0] == self.INV:
                n, first = self.situation[1]
                self.situation = (self.INV, (n + 1, first))
            elif (self.lower + self.range) & M64 > self.lower:
                self.words.append(lower_word)
                s["emit"] += 1
            else:
                self.situation = (self.INV, (1, lower_word))

    def seal(self):
        lower, rng = self.lower, self.range
        point = (lower + W32 - 1) & M64
        point_word = (point >> 32) & 0xFFFFFFFF
        upper_word = (((lower + rng) & M64) >> 32) & 0xFFFFFFFF
        if self.situation[0] == self.NORMAL:
            self.words.append(point_word)
            if point_word == upper_word:
                self.words.append(0)
        else:
            n, first = self.situation[1]
            if point >= lower:
                seq = [first] + [0xFFFFFFFF] * (n - 1)
            else:
                seq = [(first + 1) & 0xFFFFFFFF] + [0] * (n - 1)
            self.words.extend(seq)
            self.words.append(point_word)
            if point_word == upper_word:
                self.words.append(0)
        return self.packed()

    def packed(self):
        out = bytearray()
        for w in self.words:
            out.extend(w.to_bytes(4, "little"))
        return bytes(out)


class RangeDecoder:
    def __init__(self, data: bytes):
        assert len(data) % 4 == 0
        self.words = [int.from_bytes(data[i:i + 4], "little")
                      for i in range(0, len(data), 4)]
        self.pos = 0
        self.lower = 0
        self.range = M64
        w0 = self._word()
        w1 = self._word()
        self.point = (w0 << 32) | w1
        self.stats = dict(mul=0, div=0, cmp=0, calls=0)

    def _word(self):
        w = self.words[self.pos] if self.pos < len(self.words) else 0
        self.pos += 1
        return w

    def decode(self, precision, cdf):
        """cdf = int cumulative array (len nsym+1, cdf[-1] = 2^P).
        Returns symbol index."""
        s = self.stats
        s["calls"] += 1
        scale = self.range >> precision
        quantile = ((self.point - self.lower) & M64) // scale
        s["div"] += 1
        assert quantile < (1 << precision), "invalid data (quantile out of range)"
        scaled = np.asarray(cdf, dtype=np.int64) * scale
        idx = int(np.searchsorted(scaled, quantile, side="right")) - 1
        assert 0 <= idx < len(cdf) - 1, f"cell {idx}"
        p = int(cdf[idx + 1] - cdf[idx])
        cum = int(cdf[idx])
        s["mul"] += 2
        self.lower = (self.lower + scale * cum) & M64
        self.range = scale * p
        assert self.lower + self.range <= M64
        if self.range < W32:
            s["renorm"] += 1
            self.lower = (self.lower << 32) & M64
            self.range = (self.range << 32) & M64
            self.point = (self.point << 32) & M64
            self.point |= self._word()
        return idx


def encode_stream(symbols, probs, precision):
    """probs = (n,256) u32 array with rows summing to 2^precision.
    Returns (bytes, enc_stats, ideal_bits)."""
    enc = RangeEncoder()
    ideal = 0.0
    P = 1 << precision
    sf = np.log2(P)
    cums = np.zeros((257), dtype=np.int64)
    for i, s in enumerate(symbols):
        row = probs[i]
        # production-style cdf from the prob row
        np.cumsum(row, out=cums[1:])
        enc.encode(int(cums[s]), int(row[s]), precision)
        ideal += sf - np.log2(row[s])
    return enc.seal(), enc.stats, float(ideal)


def decode_stream(data, probs, precision, n):
    dec = RangeDecoder(data)
    cums = np.zeros((257), dtype=np.int64)
    out = np.empty(n, dtype=np.int64)
    for i in range(n):
        np.cumsum(probs[i], out=cums[1:])
        out[i] = dec.decode(precision, cums)
    return out, dec.stats


def self_test():
    rng = np.random.default_rng(7)
    for precision in (8, 10, 12, 14, 16, 20, 24):
        P = 1 << precision
        # uniform
        probs = np.full((300, 256), max(1, P // 256), dtype=np.int64)
        probs[:, 0] += P - probs.sum(axis=1)
        syms = rng.integers(0, 256, 300)
        data, st, ideal = encode_stream(syms, probs, precision)
        back, dst = decode_stream(data, probs, precision, 300)
        assert (back == syms).all(), f"uniform roundtrip P={precision}"
        # concentrated
        probs = np.ones((300, 256), dtype=np.int64)
        probs[:, 3] = P - 255
        syms = rng.choice(256, 300, p=np.zeros(256) + 1 / 256)
        syms[:150] = 3
        data, _, _ = encode_stream(syms, probs, precision)
        back, _ = decode_stream(data, probs, precision, 300)
        assert (back == syms).all(), f"concentrated roundtrip P={precision}"
        # dirichlet random
        row = rng.dirichlet(np.ones(256) * 0.3)
        row *= P
        row = largest_remainder_row(row.astype(np.int64), P)
        probs = np.tile(row, (300, 1))
        syms = rng.choice(256, 300)
        data, _, _ = encode_stream(syms, probs, precision)
        back, _ = decode_stream(data, probs, precision, 300)
        assert (back == syms).all(), f"dirichlet roundtrip P={precision}"
    print("coder self-test: uniform+concentrated+dirichlet roundtrips OK at "
          "precisions 8,10,12,14,16,20,24")


def largest_remainder_row(m, total):
    m = np.asarray(m, dtype=np.int64)
    m = np.maximum(m, 0)
    fs = (m * total // m.sum()).astype(np.int64)
    rem = m * total - fs * m.sum()
    grants = int(total - fs.sum())
    if grants > 0:
        fs[np.argsort(-rem, kind="stable")[:grants]] += 1
    zeros = fs == 0
    while zeros.any():
        src = int(fs.argmax())
        tgt = int(np.where(zeros)[0][0])
        fs[src] -= 1
        fs[tgt] += 1
        zeros = fs == 0
    assert fs.sum() == total and fs.min() >= 1
    return fs


if __name__ == "__main__":
    self_test()
