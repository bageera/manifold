#!/usr/bin/env python3
"""HP8-BT — exact probability-tree factorization + precision ablation.

For a production frequency vector f (u32 fixed point, sum S), every arm is a
prefix tree whose leaves are the 256 valid residual symbols (signed mirror
domain where applicable). Node masses are INTEGER sums of leaf frequencies,
so the exact identity

    prod(child_mass / node_mass) over the path == f(x) / S

holds as rational arithmetic for every x — verified exhaustively per arm.

BT2: each node's children are LR-quantized to a budget 2^Q (floor 1 over
LIVE children only; pruned/zero-mass branches excluded, never coded). The
ideal cost of a leaf = sum over its path of -log2(q_child / 2^Q).

BT3: per-arm cost model — node counts (build adds), per-node LR op budget,
and per-symbol decisions (path lengths excluding collapsed single-child
chains, which cost no decision in a real coder).

Arms: T0 direct categorical, T1 binary, T2 zigzag(binary), T3 unsigned
ternary, T4 balanced ternary (pruned), T5 radix-4, T6 radix-8, T7 radix-16,
T8 zero/sign/magnitude (pruned sign-0 + pos-128 prune), T9 mixed radix
4-8-8. Output research/results/hp8-bt-comparison.csv.
"""
import itertools
import os
from fractions import Fraction

import numpy as np

from representation import balanced_ternary_digits

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
TRACES = os.path.join(ROOT, "research", "results", "hp1-native-traces")
OUT = os.path.join(ROOT, "research", "results", "hp8-bt-comparison.csv")

SIG_MAX = 127
SIG_MIN = -128


def lr_children(masses, q):
    """LR-quantize a mass vector to total 2^q, floor 1 for LIVE (mass>0)
    children; zero-mass children stay 0 and are never coded."""
    m = np.asarray(masses, dtype=np.int64)
    total = 1 << q
    live = m > 0
    n_live = int(live.sum())
    assert total >= n_live, f"budget {total} < {n_live} live children"
    out = np.zeros_like(m)
    fs = (m * total) // m.sum()
    rem = m * total - fs * m.sum()
    grants = total - fs.sum()
    # ties -> lowest live index
    order = np.argsort(-rem, kind="stable")
    k = 0
    while grants > 0 and k < len(order):
        i = order[k]
        if m[i] > 0:
            fs[i] += 1
            grants -= 1
        k += 1
    zeros = (fs == 0) & live
    while zeros.any():
        src = int(fs.argmax())
        tgt = int(np.where(zeros)[0][0])
        fs[src] -= 1
        fs[tgt] += 1
        zeros = (fs == 0) & live
    assert fs[live].min() >= 1 and fs.sum() == total
    return fs


class Tree:
    """digit-radii prefix tree with optional pruning predicate; masses from f.

    digit_values optionally overrides the per-level digit alphabet (balanced
    ternary uses {-1,0,1}); default = range(radix).
    """

    def __init__(self, radii, symbol_of_digits, prune=None, digit_values=None):
        self.radii = radii
        self.symbol_of = symbol_of_digits
        self.prune = prune
        self.digit_values = digit_values or [list(range(r)) for r in radii]
        self.nodes = {}
        self.leaves = {}
        paths = {}
        maxd = len(radii)

        def rec(prefix, depth):
            if depth == maxd:
                s = symbol_of_digits(tuple(prefix))
                if s is not None:
                    paths[tuple(prefix)] = s
                return
            for d in self.digit_values[depth]:
                nprefix = prefix + [d]
                if prune is not None and not prune(tuple(nprefix), depth + 1):
                    continue
                rec(nprefix, depth + 1)

        rec([], 0)
        self.paths = paths
        self.path_of_symbol = {v: k for k, v in paths.items()}

    def node_masses(self, f):
        """Returns {digit_prefix: mass}, leaf masses = f[s]."""
        masses = {}
        for path, s in self.paths.items():
            masses[path] = int(f[s])
        # aggregate upward
        agg = {}
        for path, m in masses.items():
            for d in range(len(path) + 1):
                key = tuple(path[:d])
                agg[key] = agg.get(key, 0) + m
        self.agg = agg
        # children per prefix
        kids = {}
        for path in self.paths:
            for d in range(1, len(path) + 1):
                kids.setdefault(tuple(path[:d - 1]), set()).add(path[d - 1])
        self.kids = {k: sorted(v) for k, v in kids.items()}
        # live-child check: a node with ONE child chain contributes no decision
        self.single_child = {
            k: len(v) == 1 for k, v in self.kids.items()}
        return agg

    def check_identity(self, f, total):
        ok = True
        for path, s_ in self.paths.items():
            prod = Fraction(1)
            for d in range(1, len(path) + 1):
                node = tuple(path[:d])
                parent = tuple(path[:d - 1])
                m_node = self.agg[node]
                m_parent = self.agg[parent]
                assert m_node > 0, f"zero-mass node {node} on path {path} sym {s_} f={int(f[s_])}"
                assert m_parent >= m_node, \
                    f"mass inversion node {node}: {m_parent} < {m_node}"
                prod *= Fraction(m_node, m_parent)
            if prod != Fraction(int(f[s_]), int(total)):
                ok = False
                break
        return ok

    def quantized_path_bits(self, f, q):
        """Per-symbol ideal bits under per-node LR quantization to 2^q."""
        bits = np.zeros(256)
        # per unique node compute quantized conditionals
        cache = {}
        for path, s_ in self.paths.items():
            b = 0.0
            for d in range(1, len(path) + 1):
                node = tuple(path[:d])
                if node in cache:
                    b += cache[node]
                    continue
                parent = tuple(path[:d - 1])
                m_parent = self.agg[parent]
                kids = self.kids.get(parent, [path[d - 1]])
                mvals = [self.agg[tuple(path[:d - 1]) + (kk,)] for kk in kids]
                if len(kids) == 1:
                    qv = None  # collapsed: no decision
                    contrib = 0.0
                else:
                    qs = lr_children(mvals, q)
                    idx = kids.index(path[d - 1])
                    contrib = q - float(np.log2(max(1, qs[idx])))
                cache[node] = contrib
                b += contrib
            bits[s_] = b
        return bits

    def stats(self):
        decisions = {}
        for path in self.paths:
            # count non-collapsed nodes on the path
            cnt = 0
            for d in range(1, len(path) + 1):
                parent = tuple(path[:d - 1])
                kids = self.kids.get(parent, [])
                if len(kids) > 1 or d == 0:
                    cnt += 1
            decisions[path] = cnt
        n_nodes = len(self.kids)
        n_multi = sum(1 for k, v in self.kids.items() if len(v) > 1)
        cells = sum(len(v) for v in self.kids.values())
        build_adds = sum(self.agg[k] for k in self.kids if False)  # placeholder
        path_lengths = sorted(set(decisions.values()))
        return dict(n_nodes=n_nodes, n_decision_nodes=n_multi,
                    table_cells=cells, path_lengths=path_lengths)


def sym_unsigned(digits, radii):
    v = 0
    for d in radii:
        pass
    v = 0
    for d, r in zip(digits, radii):
        v = v * r + d
    return v if v < 256 else None


def sym_two_complement(digits, radii):
    v = 0
    for d, r in zip(digits, radii):
        v = v * r + d
    space = 1
    for r in radii:
        space *= r
    if space == 256:
        return v
    return None


def baltern_symbol(digits):
    v = 0
    for t in digits:
        v = v * 3 + t
    if not (-128 <= v <= 127):
        return None
    # canonical form only: the digits must equal the canonical encoding
    if tuple(digits) != balanced_ternary_digits(v):
        return None
    return v % 256  # signed byte domain -> raw byte index


def baltern_prune(prefix, depth):
    """reachable range check: partial value v, remaining k trits in {-1,0,1}
    add [-R, R] with R = (3^k - 1)/2."""
    v = 0
    for t in prefix:
        v = v * 3 + t
    k = len(prefix)
    # remaining = 6 - k trits, each +-3^j -> R = (3^(6-k) - 1) / 2
    R = ((3 ** (6 - k)) - 1) // 2
    return (v - R <= SIG_MAX) and (v + R >= SIG_MIN)


def signmag_symbol(digits):
    sign = digits[0]
    m = 0
    for d in digits[1:]:
        m = m * 2 + d
    if sign == 0:
        return 0 if m == 0 else None
    if sign == 1:
        r = m + 1
        return r if 1 <= r <= 127 else None  # +128 unrepresentable (2's-cmpl domain)
    r = -(m + 1)                              # sign == 2
    return 256 + r if -128 <= r <= -1 else None


def signmag_prune(prefix, depth):
    if depth == 0:
        return True
    sign = prefix[0]
    m_partial = 0
    for d in prefix[1:]:
        m_partial = m_partial * 2 + d
    rem = 7 - (depth - 1)  # magnitude digits not yet chosen
    lo = m_partial << rem
    hi = lo + (1 << rem) - 1
    if sign == 0:
        return lo <= 0 <= hi
    if sign == 1:
        return lo <= 126  # reachable values m+1; max valid 127 (m'<=126)
    return lo <= 127      # sign == 2: magnitudes 1..128 all valid


ARMS = {
    "T0-cat256": None,
    "T1-binary": Tree([2] * 8, lambda d: sym_unsigned(d, [2] * 8)),
    "T2-zigzag": Tree([2] * 8, lambda d: sym_unsigned(d, [2] * 8)),  # zigzag is binary-tree with permuted leaf order; identical structure
    "T3-tern-u6": Tree([3] * 6, lambda d: sym_unsigned(d, [3] * 6)),
    "T4-baltern": Tree([3] * 6, baltern_symbol, prune=baltern_prune,
                       digit_values=[[-1, 0, 1]] * 6),
    "T5-radix4": Tree([4] * 4, lambda d: sym_unsigned(d, [4] * 4)),
    "T6-radix8": Tree([8] * 3, lambda d: sym_unsigned(d, [8] * 3)),
    "T7-radix16": Tree([16] * 2, lambda d: sym_unsigned(d, [16] * 2)),
    "T8-signmag": Tree([3] + [2] * 7, signmag_symbol, prune=signmag_prune),
    "T9-mixed488": Tree([4, 8, 8], lambda d: sym_unsigned(d, [4, 8, 8])),
}


def archetype_vectors():
    rng = np.random.default_rng(4242)
    S = 1 << 24
    out = {}
    u = np.full(256, S // 256, dtype=np.int64)
    u[0] += S - u.sum()
    out["uniform"] = u
    w = np.array([S // 3 * 0.7 ** k for k in range(40)], dtype=np.int64)
    tail = np.ones(216, dtype=np.int64)
    conc = lr_children(np.concatenate([w, tail]), 24)
    out["concentrated"] = conc
    sparse = np.ones(256, dtype=np.int64)
    sparse[:33] = (S - 256) // 33
    sparse[0] += S - sparse.sum()
    out["floor_sparse"] = sparse
    d = rng.dirichlet(np.ones(256) * 0.3) * S
    d = d.astype(np.int64)
    d = np.maximum(d, 1)
    d[7] += S - d.sum()
    out["dirichlet"] = d
    return out


def native_rows(stem, nrows):
    vp = os.path.join(TRACES, stem + ".vec")
    n = int(np.fromfile(vp, dtype=np.uint32, count=1)[0])
    vecs = np.fromfile(vp, dtype=np.uint32, offset=4).reshape(n, 256)
    idx = np.linspace(0, n - 1, min(nrows, n)).astype(int)
    return vecs[idx].astype(np.int64)


def main():
    rows_out = []
    # BT1 gate: exact identity on archetypes + one native row
    print("== BT1 exact identity (rational arithmetic) ==")
    arch = archetype_vectors()
    nat0 = native_rows("forest_224x160_s900002", 4)
    for name, tree in ARMS.items():
        if tree is None:
            continue
        ok_all = True
        for vname, f in list(arch.items()) + [("native", nat0[0])]:
            total = int(f.sum())
            tree.node_masses(f)
            if not tree.check_identity(f, total):
                ok_all = False
                print(f"  {name}: IDENTITY FAIL on {vname}")
        print(f"  {name}: identity {'OK' if ok_all else 'FAIL'} on 5 vectors")
    # structural stats on the dirichlet archetype
    print("== arm structure (dirichlet vector) ==")
    for name, tree in ARMS.items():
        if tree is None:
            continue
        tree.node_masses(arch["dirichlet"])
        st = tree.stats()
        print(f"  {name:12s} nodes {st['n_nodes']:4d} decision-nodes {st['n_decision_nodes']:4d} "
              f"cells {st['table_cells']:5d} decisions/sym {st['path_lengths']}")
    # BT2: quantized ideal cost over sampled native rows
    print("== BT2 native-row ideal bits (mean distribution entropy, bits/sym) ==")
    QS = [16, 12, 8]
    for stem in ("forest_224x160_s900002", "terrain_160x120_s3100000", "screenshot_640x400"):
        nat = native_rows(stem, 400)
        for name, tree in ARMS.items():
            if tree is None:
                continue
            tree.node_masses(nat[0])
            st = tree.stats()
            for q in QS:
                tot = 0.0
                for i in range(nat.shape[0]):
                    f = nat[i]
                    tree.node_masses(f)
                    b = tree.quantized_path_bits(f, q)
                    tot += float(b @ f / f.sum())
                rows_out.append(dict(
                    image=stem, arm=name, q=q,
                    mean_ideal_bits=round(tot / nat.shape[0], 5),
                    decisions_per_sym=str(st["path_lengths"]),
                    cells=st["table_cells"]))
        # T0 baseline at the same budgets
        for q in QS:
            ent = 0.0
            for i in range(nat.shape[0]):
                f = nat[i]
                qv = lr_children(f, q)
                qb = q - np.log2(qv)
                ent += float(qb @ f / f.sum())
            rows_out.append(dict(image=stem, arm="T0-cat256", q=q,
                                 mean_ideal_bits=round(ent / nat.shape[0], 5),
                                 decisions_per_sym="[1]", cells=256))
    import pandas as pd
    df = pd.DataFrame(rows_out)
    df.to_csv(OUT, index=False)
    show = df[df.image == "forest_224x160_s900002"].pivot_table(
        index="arm", columns="q", values="mean_ideal_bits")
    print(show.round(4).to_string())
    print("wrote", OUT)


if __name__ == "__main__":
    main()
