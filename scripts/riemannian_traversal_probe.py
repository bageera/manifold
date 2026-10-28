import numpy as np, zlib, heapq, math, time

N = 128
def d2xy(n, d):
    rx, ry, t, x, y = 0, 0, int(d), 0, 0
    s = 1
    while s < n:
        rx = 1 & (t // 2); ry = 1 & (t ^ rx)
        if ry == 0:
            if rx == 1:
                x = s-1-x; y = s-1-y
            x, y = y, x
        x += s*rx; y += s*ry
        t //= 4; s *= 2
    return x, y

hil = np.array([d2xy(N, d) for d in range(N*N)])
flat_h = (hil[:,0].astype(np.int64))*N + hil[:,1].astype(np.int64)
assert len(np.unique(flat_h)) == N*N, "hilbert bijection broken"

def morton_order(n):
    xs, ys = np.meshgrid(np.arange(n), np.arange(n), indexing="ij")
    xx = xs.reshape(-1).astype(np.int64); yy = ys.reshape(-1).astype(np.int64)
    keys = np.zeros(n*n, dtype=np.int64)
    for b in range(int(math.log2(n))):
        keys |= ((xx >> b) & 1) << (2*b)
        keys |= ((yy >> b) & 1) << (2*b+1)
    return np.argsort(keys, kind="stable")

def greedy_order(P, weight):
    n = P.shape[0]; NN = n*n
    visited = np.zeros(NN, dtype=bool)
    order = np.empty(NN, dtype=np.int64)
    heap = []
    pi = P.reshape(-1)
    heapq.heappush(heap, (0.0, 0, 0))
    k = 0
    while heap and k < NN:
        cost, tie, pos = heapq.heappop(heap)
        if visited[pos]:
            continue
        visited[pos] = True
        order[k] = pos; k += 1
        y, x = pos // n, pos % n
        for dy, dx in ((1,0),(-1,0),(0,1),(0,-1)):
            ny, nx = y+dy, x+dx
            if 0 <= ny < n and 0 <= nx < n:
                npos = ny*n + nx
                if not visited[npos]:
                    w = 1.0 + weight*abs(int(pi[npos]) - int(pi[pos]))
                    heapq.heappush(heap, (cost + w, npos*4, npos))
    return order

def measure(tag, order, P):
    seq = P.reshape(-1)[order]
    r = (np.diff(seq.astype(np.int16)).astype(np.int32) % 256).astype(np.uint8)
    return len(zlib.compress(r.tobytes() + bytes([int(seq[0])]), 7))

def smooth(seed):
    r = np.random.default_rng(seed)
    c = r.integers(0, 256, (8, 8)).astype(np.float64)
    xs = np.linspace(0, 7, N); ys = np.linspace(0, 7, N)
    im = np.zeros((N, N))
    for yi, y in enumerate(ys):
        for xi, x in enumerate(xs):
            x0, y0 = int(x), int(y); x1, y1 = min(x0+1, 7), min(y0+1, 7)
            fx, fy = x-x0, y-y0
            im[yi, xi] = (c[y0,x0]*(1-fx)*(1-fy) + c[y0,x1]*fx*(1-fy) +
                          c[y1,x0]*fy*(1-fx) + c[y1,x1]*fx*fy)
    return im.clip(0, 255).astype(np.uint8)

def edges():
    P = np.full((N, N), 128, np.uint8)
    P[:, :N//2] = 30
    P[N//2:, :] = 200
    P[:N//3, N//2:] = 90
    return P

cases = {
    "smooth": smooth(11),
    "edges": edges(),
    "plaid": np.clip(np.sin(2*np.pi*np.mgrid[0:N,0:N][1]/16)*60 +
                     np.sin(2*np.pi*np.mgrid[0:N,0:N][0]/16)*60 + 128, 0, 255).astype(np.uint8),
    "random": np.random.default_rng(9).integers(0, 256, (N, N), dtype=np.uint8),
}

biject = len(np.unique(morton_order(N))) == N*N
print("morton bijection:", biject, "| hilbert bijection:", len(np.unique(flat_h)) == N*N,
      "| greedy bijection: N/A (priority traversal, single-visit by construction)")

for name, P in cases.items():
    r0 = measure("r", np.arange(N*N), P)
    rm = measure("m", morton_order(N), P)
    rh = measure("h", flat_h, P)
    rg = measure("g", greedy_order(P, 2.0), P)
    print(f"{name:8s} raster {r0:7d} | morton {rm:7d} | hilbert {rh:7d} | metric-greedy {rg:7d} ({rg/r0:.3f}x)")