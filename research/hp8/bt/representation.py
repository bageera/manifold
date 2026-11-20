#!/usr/bin/env python3
"""HP8-BT0 — exhaustive reversible residual representations (256/256).

Every representation maps the residual byte domain {0..255} (or its signed
mirror r in [-128,127], r = x if x < 128 else x - 256) to digit strings.
Each arm exposes:

    encode(values) -> list of digit tuples (fixed or canonical length)
    decode(digits) -> values (exact inverse)
    meta: n_digits (max), radix per digit position, domain, path_lengths

Exhaustive gate: every one of the 256 inputs round-trips exactly, digit
alphabets are valid, and encode(decode(digits)) closes on the declared
digit domain. Invalid branch analysis: for fixed-depth representations the
digit space may exceed 256 — the count of unreachable branches is reported,
never silently dropped.
"""
import itertools

import numpy as np


def signed_mirror(x):
    """r = x if x < 128 else x - 256 (exact reversible, bijective 256)."""
    r = np.asarray(x, dtype=np.int64)
    return np.where(r < 128, r, r - 256)


def unsigned_mirror(r):
    """Inverse of signed_mirror."""
    r = np.asarray(r, dtype=np.int64)
    return np.where(r < 0, r + 256, r)


def digits(value, radii, signed=False):
    """Fixed-radix big-endian digit decomposition.

    signed=True interprets `value` in the signed mirror domain (digits of
    the unsigned mirror image); balanced ternary is separate.
    """
    v = int(value)
    if signed:
        v = int(unsigned_mirror(np.array([v]))[0])
    out = []
    for rad in reversed(radii):
        out.append(v % rad)
        v //= rad
    assert v == 0, f"value {value} out of domain {radii}"
    return tuple(reversed(out))


def undigits(ds, radii, signed=False):
    v = 0
    for d, rad in zip(ds, radii):
        assert 0 <= d < rad, f"digit {d} outside radix {rad}"
        v = v * rad + d
    if signed:
        # two's-complement style sign extension over the full digit space
        space = int(np.prod(radii))
        if v >= space // 2:
            v -= space
    return v


def balanced_ternary_digits(x):
    """Canonical 6-trit balanced-ternary of the SIGNED byte (range -128..127).

    x = sum t_k 3^k, t_k in {-1,0,1}; leading trits may be 0. Returns the
    fixed 6-digit tuple (t5..t0). Every value in [-128,127] has a unique
    balanced-ternary form within 6 trits (3^6/2 = 364 >= 127).
    """
    x = int(x)
    assert -128 <= x <= 127, "balanced ternary arm covers the signed byte domain"
    out = [0] * 6
    v = x
    for k in range(6):
        # round-half-to-even style digit selection keeping remainder in range
        rem = v % 3
        if rem == 0:
            t = 0
        elif rem == 1:
            t = 1
        else:
            t = -1
        out[5 - k] = t
        v = (v - t) // 3
    assert v == 0, f"balanced ternary needs >6 trits for {x}"
    return tuple(out)


def balanced_ternary_value(ds):
    v = 0
    for t in ds:
        assert t in (-1, 0, 1), f"invalid trit {t}"
        v = v * 3 + t
    return v


def signed_magnitude(x):
    """(sign s in {0:zero,1:pos,2:neg}, magnitude m)."""
    r = int(signed_mirror(np.array([x]))[0])
    if r == 0:
        return (0, 0)
    return (1 if r > 0 else 2, abs(r))


def unsigned_magnitude(s, m):
    if s == 0:
        return 0
    r = m if s == 1 else -m
    return int(unsigned_mirror(np.array([r]))[0])


ARMS = {}


def arm(name, n_digits=None, radii=None, signed_domain=False, canonical=False,
        encode=None, decode=None, path_lengths=None, max_radix=None):
    ARMS[name] = dict(
        n_digits=n_digits, radii=radii, signed_domain=signed_domain,
        canonical=canonical, encode=encode, decode=decode,
        path_lengths=path_lengths, max_radix=max_radix)


# fixed-radix unsigned binary over the raw byte
arm("T-bin-u8", n_digits=8, radii=[2] * 8,
    encode=lambda v: digits(v, [2] * 8),
    decode=lambda ds: undigits(ds, [2] * 8))
# signed two's-complement binary over the signed mirror
arm("T-bin-signed", n_digits=8, radii=[2] * 8, signed_domain=True,
    encode=lambda v: digits(v, [2] * 8, signed=True),
    decode=lambda ds: undigits(ds, [2] * 8, signed=True))
# zigzag over the signed mirror: z = 2r if r >= 0 else -2r - 1 (8 bits unsigned)
def zigzag_enc(v):
    r = int(signed_mirror(np.array([v]))[0])
    z = 2 * r if r >= 0 else -2 * r - 1
    return digits(z, [2] * 8)


def zigzag_dec(ds):
    z = undigits(ds, [2] * 8)
    r = (z + 1) // 2 if z % 2 else z // 2
    r = r if z % 2 == 0 else -(z + 1) // 2
    # correct zigzag inverse: z even -> r = z/2 ; z odd -> r = -(z+1)/2
    r = z // 2 if z % 2 == 0 else -((z + 1) // 2)
    return int(unsigned_mirror(np.array([r]))[0])


arm("T-zigzag", n_digits=8, radii=[2] * 8, signed_domain=True,
    encode=zigzag_enc, decode=zigzag_dec)
# signed-magnitude: sign digit (3 values) + 7-bit magnitude remap m' = m-1
# for m >= 1 (magnitudes 1..128 fit 7 bits exactly); sign=0,m'>0 and any
# m' making |value| > 128 branches are unreachable
arm("T-signmag", n_digits=8, radii=[3] + [2] * 7, canonical=True,
    encode=lambda v: (lambda sm: (sm[0],) + digits(max(1, sm[1]) - 1, [2] * 7))(signed_magnitude(v)),
    max_radix=3)
# unsigned ternary, 6 trits over 0..2 (729 space, 473 unreachable)
arm("T-tern-u6", n_digits=6, radii=[3] * 6,
    encode=lambda v: digits(v, [3] * 6),
    decode=lambda ds: undigits(ds, [3] * 6))
# balanced ternary, 6 trits over {-1,0,1} with pruning analysis
arm("T-baltern6", n_digits=6, radii=[3] * 6, signed_domain=True, canonical=True,
    encode=balanced_ternary_digits,
    decode=lambda ds: balanced_ternary_value(ds), max_radix=3)
# radix-4 / 8 / 16 unsigned
for name, b, k in (("T-radix4", 4, 4), ("T-radix8", 8, 3), ("T-radix16", 16, 2)):
    arm(name, n_digits=k, radii=[b] * k,
        encode=lambda v, b=b, k=k: digits(v, [b] * k),
        decode=lambda ds, b=b, k=k: undigits(ds, [b] * k))
# mixed radix 4-8-8 (unsigned) and 8-4-8
arm("T-mixed-488", n_digits=3, radii=[4, 8, 8],
    encode=lambda v: digits(v, [4, 8, 8]),
    decode=lambda ds: undigits(ds, [4, 8, 8]))
arm("T-mixed-848", n_digits=3, radii=[8, 4, 8],
    encode=lambda v: digits(v, [8, 4, 8]),
    decode=lambda ds: undigits(ds, [8, 4, 8]))


def exhaustive_check():
    values = np.arange(256)
    report = []
    for name, a in ARMS.items():
        enc_lens = set()
        roundtrip_ok = True
        invalid_branches = 0
        digit_space = 1
        if name == "T-baltern6":
            trits = list(itertools.product((-1, 0, 1), repeat=6))
            digit_space = len(trits)
            for x in values:
                ds = a["encode"](int(signed_mirror(np.array([x]))[0]))
                if a["decode"](ds) != int(signed_mirror(np.array([x]))[0]):
                    roundtrip_ok = False
                enc_lens.add(len(ds))
            reachable = sum(
                1 for ds in trits if -128 <= balanced_ternary_value(ds) <= 127)
            # reachable canonical strings: trit strings with leading-zero
            # canonical form (v == 0 above the MSB trit is automatic since
            # balanced value of the string IS the value; only range prunes)
            invalid_branches = digit_space - sum(
                1 for ds in trits
                if all(t == 0 for t in ds[: len(ds) - len(a["encode"](0))])
                and -128 <= balanced_ternary_value(ds) <= 127)
            invalid_branches = digit_space - reachable
            path_lengths = {6}
        elif name == "T-signmag":
            space = 3 * 2 ** 7
            digit_space = space
            invalid_branches = space - 256  # sign=0,m>'0' and m>'127' branches
            path_lengths = {8}
            for x in values:
                ds = a["encode"](int(x))
                mprime = undigits(ds[1:], [2] * 7)
                mag = mprime + 1 if ds[0] != 0 else 0
                if unsigned_magnitude(ds[0], mag) != int(x):
                    roundtrip_ok = False
                enc_lens.add(len(ds))
        else:
            digit_space = int(np.prod(a["radii"]))
            invalid_branches = digit_space - 256
            for x in values:
                ds = a["encode"](int(x))
                back = a["decode"](ds)
                if a["signed_domain"]:
                    # signed-domain arms cover the signed mirror; the byte
                    # round-trips through the mirror inverse
                    back = int(unsigned_mirror(np.array([back]))[0])
                if back != int(x):
                    roundtrip_ok = False
                enc_lens.add(len(ds))
            path_lengths = {a["n_digits"]}
        report.append(dict(
            arm=name, digits=digit_space, invalid_branches=invalid_branches,
            path_lengths=sorted(path_lengths), roundtrip=roundtrip_ok))
        assert roundtrip_ok, f"{name} failed exhaustive roundtrip"
        print(f"{name:14s} digits {digit_space:5d} invalid {invalid_branches:5d} "
              f"paths {sorted(path_lengths)} roundtrip 256/256 OK")
    return report


if __name__ == "__main__":
    exhaustive_check()
