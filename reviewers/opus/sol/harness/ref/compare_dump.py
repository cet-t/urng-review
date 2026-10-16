"""Compare a dump produced by common/dump.rs with the independent reference.

usage: python compare_dump.py <dump.txt> sol|astra [--detail]

The *core* algorithms come from refimpl.py (validated by check_vectors.py and
check_xoshiro_c.py).  Seeding follows each crate's documented / implemented
scheme; the per-variant differences are explicit below so that a mismatch can
be attributed either to the core algorithm or to a seeding change.
"""
import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(__file__))
from refimpl import *  # noqa

dump_path, variant = sys.argv[1], sys.argv[2]
detail = '--detail' in sys.argv
SOL = variant == 'sol'
OR1 = SOL  # Sol keeps urng 1.0.0's `seed | 1` in SplitMix32/64::new


def sm32(seed):
    return SplitMix32(seed, OR1)


def sm64(seed):
    return SplitMix64(seed, OR1)


def take(g, n, f='next'):
    m = getattr(g, f)
    return [m() for _ in range(n)]


def block_stream(block_fn, ctr, words_per_block, nwords, ctr_bits, ctr_words):
    """Counter-mode helper: increments ctr (list of words, LE) with carry."""
    out = []
    ctr = list(ctr)
    mask = (1 << ctr_bits) - 1
    while len(out) < nwords:
        out.extend(block_fn(ctr))
        for i in range(ctr_words):
            ctr[i] = (ctr[i] + 1) & mask
            if ctr[i] != 0:
                break
    return out[:nwords]


# ---------------------------------------------------------------- scalar 32
def exp_scalar32(name, seed, n):
    if name == 'SplitMix32':
        return take(sm32(seed), n)
    if name in ('Xoshiro128Pp', 'Xoshiro128Ss'):
        g = sm32(seed)
        return take(Xoshiro128(take(g, 4), name.endswith('Pp')), n)
    if name == 'Xoroshiro64Ss':
        return take(Xoroshiro64ss(take(sm32(seed), 2)), n)
    if name == 'Jsf32':
        g = sm32(seed)
        b, c, d = take(g, 3)
        return take(Jsf32(0xF1EA5EED, b, c, d), n)
    if name == 'Sfc32':
        a, b, c = take(sm32(seed), 3)
        return take(Sfc32(a, b, c, 1), n)
    if name == 'Xorshift32':
        a = sm32(seed).next()
        if SOL and a == 0:
            a = 1
        return take(Xorshift32(a), n)
    if name == 'Xorshift128':
        return take(Xorshift128(take(sm32(seed), 4)), n)
    if name == 'Xorwow':
        v = take(sm32(seed), 6)
        return take(Xorwow(v[:5], v[5]), n)
    if name == 'Pcg32':
        g = sm64_from_seed32(seed, OR1)
        st = g.next()
        inc = g.next() | 1
        return take(Pcg32(st, inc), n)
    if name == 'Philox32':
        g = sm32(seed)
        c = take(g, 4)
        k = take(g, 2)
        return block_stream(lambda ctr: philox4x32(ctr, k), c, 4, n, 32, 4)
    if name == 'Threefry32x4':
        g = sm32(seed)
        k = take(g, 4)
        if SOL:
            return block_stream(lambda ctr: threefry4x32(ctr, k), [0] * 4, 4, n, 32, 4)
        tw = take(g, 2)
        return block_stream(lambda ctr: threefry4x32_tweaked(ctr, k, tw), [0] * 4, 4, n, 32, 4)
    if name == 'Threefry32x2':
        k = take(sm32(seed), 2)
        return block_stream(lambda ctr: threefry2x32(ctr, k), [0, 0], 2, n, 32, 2)
    if name == 'Squares32':
        k = sm64_from_seed32(seed, OR1).next()
        return [squares32(i, k) for i in range(n)]
    if name in ('ChaCha8', 'ChaCha20'):
        key = take(sm32(seed), 8)
        r = int(name[6:])
        return block_stream(lambda ctr: chacha_block(chacha_state(key, ctr[0], [0, 0, 0]), r),
                            [0], 16, n, 32, 1)
    if name in ('Salsa8', 'Salsa20'):
        key = take(sm32(seed), 8)
        r = int(name[5:])
        return block_stream(lambda ctr: salsa_core(salsa_state(key, [0, 0], ctr[0] | (ctr[1] << 32)), r),
                            [0, 0], 16, n, 32, 2)
    if name == 'Mt19937':
        mt = MT19937(sm32(seed).next())
        return take(mt, n)
    if name.startswith('Sfmt'):
        mexp = int(name[4:])
        s = SFMT(mexp)
        g = sm64_from_seed32(seed, OR1)
        words = []
        for _ in range(s.n * 2):
            v = g.next()
            words += [v & M32, v >> 32]
        s.set_state(words)
        return take(s, n, 'next32')
    raise KeyError(name)


# ---------------------------------------------------------------- scalar 64
def exp_scalar64(name, seed, n):
    if name == 'SplitMix64':
        return take(sm64(seed), n)
    if name in ('Xoshiro256Pp', 'Xoshiro256Ss'):
        return take(Xoshiro256(take(sm64(seed), 4), name.endswith('Pp')), n)
    if name == 'Sfc64':
        a, b, c = take(sm64(seed), 3)
        return take(Sfc64(a, b, c, 1), n)
    if name == 'Philox64':
        k = take(sm64(seed), 2)
        return block_stream(lambda ctr: philox2x64(ctr, k[:1]), [1, 0], 2, n, 64, 2)
    if name == 'Philox4x64':
        k = take(sm64(seed), 2)
        return block_stream(lambda ctr: philox4x64(ctr, k), [3, 2, 1, 0], 4, n, 64, 4)
    if name == 'Xorshift64':
        a = sm64(seed).next()
        return take(Xorshift64(a or 1), n)
    if name == 'Xoroshiro128Pp':
        g = SplitMix64(seed | 1, OR1) if SOL else sm64(seed)
        return take(Xoroshiro128(take(g, 2), True), n)
    if name == 'Xoroshiro128Ss':
        return take(Xoroshiro128(take(sm64(seed), 2), False), n)
    if name == 'TwistedGFSR':
        if SOL:
            g = sm64(seed)
            x = [g.next() & M32 for _ in range(25)]
        else:
            g = SplitMix32(seed & M32, False)
            x = take(g, 25)
        t = TT800(x)
        return [(t.next32() << 32) | t.next32() for _ in range(n)]
    if name == 'Cet64':
        return take(Cet64(sm64(seed).next()), n)
    if name == 'Cet256':
        return take(Cet256(take(sm64(seed), 4)), n)
    if name == 'Threefish256':
        g = sm64(seed)
        k = take(g, 4)
        tw = take(g, 2)
        return block_stream(lambda ctr: threefish256(ctr, k, tw, xor_ctr=not SOL), [0] * 4, 4, n, 64, 4)
    if name == 'Biski64':
        return take(Biski64(*take(sm64(seed), 3)), n)
    if name == 'WyHash64':
        return take(WyHash64(sm64(seed).next()), n)
    if name == 'Mt1993764':
        return take(MT19937_64(sm64(seed).next()), n)
    if name == 'Sfmt1993764':
        s = SFMT(19937)
        g = sm64(seed)
        words = []
        for _ in range(s.n * 2):
            v = g.next()
            words += [v & M32, v >> 32]
        s.set_state(words)
        return take(s, n, 'next64')
    if name in ('Xoroshiro128Pp_jumps', 'Xoroshiro128Ss_jumps'):
        pp = 'Pp' in name
        g = (SplitMix64(seed | 1, OR1) if (SOL and pp) else sm64(seed))
        x = Xoroshiro128(take(g, 2), pp)
        out = []
        for step in range(3):
            out += take(x, 3)
            if step == 0:
                x.jump_pow2(64)
            elif step == 1:
                x.jump_pow2(96)
        return out
    raise KeyError(name)


SCALAR32 = {'SplitMix32', 'Xoshiro128Pp', 'Xoshiro128Ss', 'Xoroshiro64Ss', 'Jsf32', 'Sfc32',
            'Xorshift32', 'Xorshift128', 'Xorwow', 'Pcg32', 'Philox32', 'Threefry32x4', 'Threefry32x2',
            'Squares32', 'ChaCha8', 'ChaCha20', 'Salsa8', 'Salsa20', 'Mt19937', 'Sfmt607', 'Sfmt1279',
            'Sfmt2281', 'Sfmt4253', 'Sfmt11213', 'Sfmt19937', 'Sfmt44497', 'Sfmt86243', 'Sfmt132049',
            'Sfmt216091'}


# ---------------------------------------------------------------- SIMD lanes
def exp_simd(name, seed, lanes, n):
    """Return list of per-lane expected streams."""
    if name.startswith('Sfc32x'):
        g = sm32(seed)
        st = [take(g, 3) for _ in range(lanes)]
        return [take(Sfc32(a, b, c, 1), n) for a, b, c in st]
    if name.startswith('Jsf32x'):
        g = sm32(seed)
        b, c, d = take(g, lanes), take(g, lanes), take(g, lanes)
        return [take(Jsf32(0xF1EA5EED, b[i], c[i], d[i]), n) for i in range(lanes)]
    if name.startswith('Xoroshiro64Ssx'):
        g = sm32(seed)
        st = [take(g, 2) for _ in range(lanes)]
        return [take(Xoroshiro64ss(s), n) for s in st]
    if name.startswith('Xoshiro128'):
        g = sm32(seed)
        s = [take(g, lanes) for _ in range(4)]
        return [take(Xoshiro128([s[0][i], s[1][i], s[2][i], s[3][i]], 'Pp' in name), n)
                for i in range(lanes)]
    if name == 'SplitMix32x16':
        flat = take(sm32(seed), lanes * n)
        return [[flat[t * lanes + i] for t in range(n)] for i in range(lanes)]
    if name == 'Pcg32x8':
        g = sm64_from_seed32(seed, OR1)
        st = take(g, lanes)
        inc = [x | 1 for x in take(g, lanes)]
        return [take(Pcg32(st[i], inc[i]), n) for i in range(lanes)]
    if name == 'Xorshift32x8':
        g = sm32(seed)
        a = take(g, lanes)
        if SOL:
            a = [x or 1 for x in a]
        return [take(Xorshift32(x), n) for x in a]
    if name == 'Philox32x4x4':
        g = sm32(seed)
        c = take(g, 16)
        k = take(g, 8)
        res = []
        for b in range(4):
            key = [k[2 * b], k[2 * b + 1]]
            words = block_stream(lambda ctr: philox4x32(ctr, key), c[4 * b:4 * b + 4], 4, 4 * n, 32, 4)
            for j in range(4):
                res.append(words[j::4][:n])
        return res
    if name == 'Squares32x8':
        g = sm64_from_seed32(seed, OR1)
        k = take(g, 8)
        return [[squares32(i + 8 * t, k[i]) for t in range(n)] for i in range(8)]
    if name == 'Biski64x8':
        g = sm64(seed)
        st = [take(g, 3) for _ in range(8)]
        return [take(Biski64(*s), n) for s in st]
    if name == 'Sfc64x8':
        g = sm64(seed)
        st = [take(g, 3) for _ in range(8)]
        return [take(Sfc64(a, b, c, 1), n) for a, b, c in st]
    if name == 'Cet64x8':
        g = sm64(seed)
        return [take(Cet64(x), n) for x in take(g, 8)]
    if name == 'Cet256x2':
        g = sm64(seed)
        s = take(g, 8)
        return [take(Cet256(s[:4]), n), take(Cet256(s[4:]), n)]
    if name == 'Xoshiro256Ssx2':
        g = sm64(seed)
        s = take(g, 8)
        return [take(Xoshiro256(s[:4], False), n), take(Xoshiro256(s[4:], False), n)]
    raise KeyError(name)


def exp_wide(name, seed, lanes, n):
    if name.startswith('SplitMix32x'):
        flat = take(sm32(seed), lanes * n)
        return [[flat[t * lanes + i] for t in range(n)] for i in range(lanes)]
    if name.startswith('Jsf32x'):
        flat = take(sm32(seed), 3 * lanes)
        b, c, d = flat[:lanes], flat[lanes:2 * lanes], flat[2 * lanes:]
        return [take(Jsf32(0xF1EA5EED, b[i], c[i], d[i]), n) for i in range(lanes)]
    if name.startswith('Sfc32x'):
        g = sm32(seed)
        a, b, c, ctr = (take(g, lanes) for _ in range(4))
        return [take(Sfc32(a[i], b[i], c[i], ctr[i]), n) for i in range(lanes)]
    if name.startswith('Xoroshiro64Ssx'):
        g = sm32(seed)
        s0, s1 = take(g, lanes), take(g, lanes)
        return [take(Xoroshiro64ss([s0[i], s1[i]]), n) for i in range(lanes)]
    if name.startswith('Xorshift32x'):
        a = take(sm32(seed), lanes)
        if SOL:
            a = [x or 1 for x in a]
        return [take(Xorshift32(x), n) for x in a]
    if name.startswith('Xorshift128x'):
        g = sm32(seed)
        x = [take(g, lanes) for _ in range(4)]
        return [take(Xorshift128([x[j][i] for j in range(4)]), n) for i in range(lanes)]
    if name.startswith('Xorwowx'):
        g = sm32(seed)
        x = [take(g, lanes) for _ in range(5)]
        c = take(g, lanes)
        return [take(Xorwow([x[j][i] for j in range(5)], c[i]), n) for i in range(lanes)]
    if name.startswith('Xoshiro128'):
        g = sm32(seed)
        s = [take(g, lanes) for _ in range(4)]
        return [take(Xoshiro128([s[j][i] for j in range(4)], 'Pp' in name), n) for i in range(lanes)]
    if name.startswith('Threefry32x2x'):
        g = sm32(seed)
        k0, k1 = take(g, lanes), take(g, lanes)
        res = []
        for i in range(lanes):
            words = block_stream(lambda ctr: threefry2x32(ctr, [k0[i], k1[i]]), [0, 0], 2, n, 32,
                                 2 if SOL else 1)  # Astra: no carry c0 -> c1
            res.append(words)
        return res
    if name.startswith('Threefry32x4x'):
        g = sm32(seed)
        k = [take(g, lanes) for _ in range(4)]
        tw = [take(g, lanes) for _ in range(2)] if not SOL else None
        res = []
        for i in range(lanes):
            key = [k[j][i] for j in range(4)]
            if SOL:
                words = block_stream(lambda ctr: threefry4x32(ctr, key), [0] * 4, 4, n, 32, 4)
            else:
                t = [tw[0][i], tw[1][i]]
                words = block_stream(lambda ctr: threefry4x32_tweaked(ctr, key, t), [0] * 4, 4, n, 32, 1)
            res.append(words)
        return res
    if name.startswith('Pcg32x'):
        def pcg_lanes(sd, nl):
            g = SplitMix64(sd | 1, OR1) if SOL else sm64(sd)
            st = take(g, nl)
            inc = [x | 1 for x in take(g, nl)]
            return [take(Pcg32(st[i], inc[i]), n) for i in range(nl)]
        if lanes == 16:
            return pcg_lanes(seed, 8) + pcg_lanes(SplitMix64.mix(seed ^ 0x9E3779B97F4A7C15), 8)
        return pcg_lanes(seed, lanes)
    if name.startswith('Squares32x'):
        g = SplitMix64(seed | 1, OR1)  # both variants keep `seed | 1` here
        k = take(g, lanes)
        return [[squares32(i + lanes * t, k[i]) for t in range(n)] for i in range(lanes)]
    raise KeyError(name)


LANES = {}
ok = defaultdict(int)
bad = defaultdict(list)
groups = defaultdict(dict)
for line in open(dump_path):
    parts = line.split()
    name, seed = parts[0], int(parts[1], 16)
    vals = [int(x, 16) for x in parts[2:]]
    if '[' in name:
        base, lane = name[:-1].split('[')
        groups[(base, seed)][int(lane)] = vals
        continue
    try:
        exp = exp_scalar32(name, seed, len(vals)) if name in SCALAR32 else exp_scalar64(name, seed, len(vals))
    except KeyError:
        print('no reference for', name)
        continue
    if exp == vals:
        ok[name] += 1
    else:
        first = next(i for i in range(len(vals)) if vals[i] != exp[i])
        bad[name].append((seed, first, vals[first], exp[first]))

for (base, seed), lanes in groups.items():
    kind, name = base.split('::')
    nl = len(lanes)
    n = len(lanes[0])
    try:
        exp = exp_simd(name, seed, nl, n) if kind == 'simd' else exp_wide(name, seed, nl, n)
    except KeyError:
        print('no reference for', base)
        continue
    mism = [i for i in range(nl) if lanes[i] != exp[i]]
    if not mism:
        ok[base] += 1
    else:
        i = mism[0]
        first = next(j for j in range(n) if lanes[i][j] != exp[i][j])
        bad[base].append((seed, f'lane{i}@{first} ({len(mism)}/{nl} lanes)', lanes[i][first], exp[i][first]))

names = sorted(set(ok) | set(bad))
print(f'variant={variant}')
for nm in names:
    if bad[nm]:
        s = bad[nm][0]
        print(f'MISMATCH {nm}: ok={ok[nm]} bad={len(bad[nm])}  e.g. seed={s[0]:#x} at {s[1]}: got {s[2]:#x} expected {s[3]:#x}')
        if detail:
            for s in bad[nm]:
                print(f'    seed={s[0]:#x} at {s[1]}: got {s[2]:#x} exp {s[3]:#x}')
    else:
        print(f'ok       {nm}: {ok[nm]} streams')
print(f'TOTAL ok={sum(ok.values())} mismatched_streams={sum(len(v) for v in bad.values())}')
