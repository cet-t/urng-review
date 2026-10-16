"""Exact f32 emulation of the (unchanged 1.0.0) Alias32::build used by Astra,
to find minimal weight vectors whose implied sampling distribution is wrong.

P(i) is computed exactly from the (prob, alias) table as
  P(i) = (1/n) * sum_j [ j==i ] * prob[j] + [ alias[j]==i ] * (1 - prob[j]).
usage: python alias_emulate.py
"""
import itertools
import struct


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def build(weights):
    n = len(weights)
    total = 0.0
    for w in weights:
        total = f32(total + w)
    scale = f32(f32(float(n)) / total)
    p = [f32(w * scale) for w in weights]
    small = [i for i, pi in enumerate(p) if pi < 1.0]
    large = [i for i, pi in enumerate(p) if not pi < 1.0]
    prob = [0.0] * n
    alias = [0] * n
    while True:
        s = small.pop() if small else None          # evaluated first (Rust tuple order)
        l = large[-1] if large else None
        if s is None or l is None:
            break                                     # a popped `s` is lost here
        prob[s] = p[s]
        alias[s] = l
        p[l] = f32(p[l] - f32(1.0 - p[s]))
        if p[l] < 1.0:
            large.pop()
            small.append(l)
    for i in large:
        prob[i] = 1.0
    for i in small:
        prob[i] = 1.0
    return prob, alias


def implied(weights):
    prob, alias = build(weights)
    n = len(weights)
    P = [0.0] * n
    for j in range(n):
        P[j] += prob[j] / n
        P[alias[j]] += (1.0 - prob[j]) / n
    return P


def worst(weights):
    t = sum(weights)
    P = implied(weights)
    return max(abs(P[i] - weights[i] / t) for i in range(len(weights))), P


hits = []
for n in (2, 3):
    for ws in itertools.product(range(1, 13), repeat=n):
        d, P = worst([float(w) for w in ws])
        if d > 0.05:
            hits.append((sum(ws), ws, d, P))
hits.sort()
print(f"{len(hits)} small integer weight vectors (n=2..3, w<=12) with |dP|>0.05; smallest:")
for _, ws, d, P in hits[:10]:
    t = sum(ws)
    print(f"  weights={list(ws)} expected={[round(w / t, 4) for w in ws]} implied={[round(x, 4) for x in P]} max|dP|={d:.3f}")
for ws in ([472 / 7, 574 / 7], [3.4028235e38, 3.4028235e38], [3.4028235e38, 0.0, 3.4028235e38]):
    try:
        d, P = worst([f32(w) for w in ws])
        print(f"weights={ws}: implied={[round(x, 4) for x in P]} max|dP|={d:.3f}")
    except (ZeroDivisionError, OverflowError) as e:
        print(f"weights={ws}: {e!r}")
