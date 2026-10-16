"""Independent pure-Python reference implementations used by the Opus review.

Written from the published algorithm descriptions / reference C code, NOT from
the urng sources.  Every core algorithm is first validated against official
vectors by `check_vectors.py` before it is used to judge the crate.

Seeding helpers (`urng_*`) reproduce the *crate-specific* seeding schemes as
read from the reviewed source code; the variant flag `or1` selects whether
SplitMix32/64::new keep the 1.0.0 `seed | 1` behaviour (Sol) or not (Astra).
"""

M32 = 0xFFFFFFFF
M64 = 0xFFFFFFFFFFFFFFFF


def rotl32(x, r):
    r &= 31
    x &= M32
    return ((x << r) | (x >> (32 - r))) & M32 if r else x


def rotl64(x, r):
    r &= 63
    x &= M64
    return ((x << r) | (x >> (64 - r))) & M64 if r else x


# ----------------------------------------------------------------------------
# SplitMix
# ----------------------------------------------------------------------------
class SplitMix64:
    """Steele/Lea/Flood SplitMix64 as in prng.di.unimi.it/splitmix64.c."""

    G = 0x9E3779B97F4A7C15

    def __init__(self, seed, or1=False):
        self.s = (seed | 1) & M64 if or1 else seed & M64

    @staticmethod
    def mix(z):
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M64
        return z ^ (z >> 31)

    def next(self):
        self.s = (self.s + self.G) & M64
        return self.mix(self.s)


class SplitMix32:
    """urng's own 32-bit SplitMix variant (32-bit Weyl state, 64-bit finalizer
    truncated).  There is no external reference for this custom variant."""

    G = 0x9E3779B9
    A = 0xFF51AFD7ED558CCD
    B = 0xC4CEB9FE1A85EC53

    def __init__(self, seed, or1=False):
        self.s = (seed | 1) & M32 if or1 else seed & M32

    @classmethod
    def mix(cls, x):
        z = x & M32
        z = ((z ^ (z >> 16)) * cls.A) & M64
        z = ((z ^ (z >> 16)) * cls.B) & M64
        return (z ^ (z >> 16)) & M32

    def next(self):
        self.s = (self.s + self.G) & M32
        return self.mix(self.s)


def sm64_from_seed32(seed, or1):
    s = SplitMix32(seed, or1)
    hi = s.next()
    lo = s.next()
    return SplitMix64((hi << 32) | lo, or1)


# ----------------------------------------------------------------------------
# xoshiro / xoroshiro family (prng.di.unimi.it reference code)
# ----------------------------------------------------------------------------
class Xoshiro128:
    def __init__(self, s, plusplus):
        self.s = [x & M32 for x in s]
        self.pp = plusplus

    def next(self):
        s = self.s
        if self.pp:
            r = (rotl32((s[0] + s[3]) & M32, 7) + s[0]) & M32
        else:
            r = (rotl32((s[1] * 5) & M32, 7) * 9) & M32
        t = (s[1] << 9) & M32
        s[2] ^= s[0]
        s[3] ^= s[1]
        s[1] ^= s[2]
        s[0] ^= s[3]
        s[2] ^= t
        s[3] = rotl32(s[3], 11)
        return r


class Xoshiro256:
    def __init__(self, s, plusplus):
        self.s = [x & M64 for x in s]
        self.pp = plusplus

    def next(self):
        s = self.s
        if self.pp:
            r = (rotl64((s[0] + s[3]) & M64, 23) + s[0]) & M64
        else:
            r = (rotl64((s[1] * 5) & M64, 7) * 9) & M64
        t = (s[1] << 17) & M64
        s[2] ^= s[0]
        s[3] ^= s[1]
        s[1] ^= s[2]
        s[0] ^= s[3]
        s[2] ^= t
        s[3] = rotl64(s[3], 45)
        return r


class Xoroshiro64ss:
    def __init__(self, s):
        self.s = [x & M32 for x in s]

    def next(self):
        s0, s1 = self.s
        r = (rotl32((s0 * 0x9E3779BB) & M32, 5) * 5) & M32
        s1 ^= s0
        self.s = [rotl32(s0, 26) ^ s1 ^ ((s1 << 9) & M32), rotl32(s1, 13)]
        return r


class Xoroshiro128:
    """xoroshiro128++ (a=49,b=21,c=28) and xoroshiro128** (a=24,b=16,c=37)."""

    def __init__(self, s, plusplus):
        self.s = [x & M64 for x in s]
        self.pp = plusplus

    def next(self):
        s0, s1 = self.s
        if self.pp:
            r = (rotl64((s0 + s1) & M64, 17) + s0) & M64
            s1 ^= s0
            self.s = [rotl64(s0, 49) ^ s1 ^ ((s1 << 21) & M64), rotl64(s1, 28)]
        else:
            r = (rotl64((s0 * 5) & M64, 7) * 9) & M64
            s1 ^= s0
            self.s = [rotl64(s0, 24) ^ s1 ^ ((s1 << 16) & M64), rotl64(s1, 37)]
        return r

    # Independent jump: compute T^(2^k) over GF(2) by repeated squaring of the
    # 128x128 state-transition matrix (does not use the published constants).
    def transition_matrix(self):
        cols = []
        for bit in range(128):
            st = [0, 0]
            st[bit // 64] = 1 << (bit % 64)
            g = Xoroshiro128(st, self.pp)
            g.next()
            cols.append(g.s[0] | (g.s[1] << 64))
        return cols  # column j = image of basis vector j

    @staticmethod
    def mat_apply(cols, v):
        out = 0
        j = 0
        while v:
            if v & 1:
                out ^= cols[j]
            v >>= 1
            j += 1
        return out

    @classmethod
    def mat_mul(cls, a, b):  # (a*b) applied as a(b(x))
        return [cls.mat_apply(a, c) for c in b]

    def jump_pow2(self, k):
        m = self.transition_matrix()
        for _ in range(k):
            m = self.mat_mul(m, m)
        v = self.s[0] | (self.s[1] << 64)
        v = self.mat_apply(m, v)
        self.s = [v & M64, v >> 64]


# ----------------------------------------------------------------------------
# PCG32 (pcg-c-basic)
# ----------------------------------------------------------------------------
class Pcg32:
    MULT = 6364136223846793005

    def __init__(self, state, inc):
        self.state = state & M64
        self.inc = inc & M64

    @classmethod
    def srandom(cls, initstate, initseq):
        g = cls(0, ((initseq << 1) | 1) & M64)
        g.next()
        g.state = (g.state + initstate) & M64
        g.next()
        return g

    def next(self):
        old = self.state
        self.state = (old * self.MULT + self.inc) & M64
        xs = (((old >> 18) ^ old) >> 27) & M32
        rot = old >> 59
        return ((xs >> rot) | (xs << ((-rot) & 31))) & M32


# ----------------------------------------------------------------------------
# PractRand SFC32/SFC64, Jenkins JSF32
# ----------------------------------------------------------------------------
class Sfc32:
    def __init__(self, a, b, c, counter):
        self.a, self.b, self.c, self.ctr = a & M32, b & M32, c & M32, counter & M32

    def next(self):
        tmp = (self.a + self.b + self.ctr) & M32
        self.ctr = (self.ctr + 1) & M32
        self.a = self.b ^ (self.b >> 9)
        self.b = (self.c + (self.c << 3)) & M32
        self.c = (rotl32(self.c, 21) + tmp) & M32
        return tmp


class Sfc64:
    def __init__(self, a, b, c, counter):
        self.a, self.b, self.c, self.ctr = a & M64, b & M64, c & M64, counter & M64

    def next(self):
        tmp = (self.a + self.b + self.ctr) & M64
        self.ctr = (self.ctr + 1) & M64
        self.a = self.b ^ (self.b >> 11)
        self.b = (self.c + (self.c << 3)) & M64
        self.c = (rotl64(self.c, 24) + tmp) & M64
        return tmp


class Jsf32:
    def __init__(self, a, b, c, d):
        self.a, self.b, self.c, self.d = a & M32, b & M32, c & M32, d & M32

    def next(self):
        e = (self.a - rotl32(self.b, 27)) & M32
        self.a = self.b ^ rotl32(self.c, 17)
        self.b = (self.c + self.d) & M32
        self.c = (self.d + e) & M32
        self.d = (e + self.a) & M32
        return self.d


# ----------------------------------------------------------------------------
# Marsaglia xorshift family
# ----------------------------------------------------------------------------
class Xorshift32:
    def __init__(self, a):
        self.a = a & M32

    def next(self):
        x = self.a
        x ^= (x << 13) & M32
        x ^= x >> 17
        x ^= (x << 5) & M32
        self.a = x
        return x


class Xorshift64:
    def __init__(self, a):
        self.a = a & M64

    def next(self):
        x = self.a
        x ^= (x << 13) & M64
        x ^= x >> 7
        x ^= (x << 17) & M64
        self.a = x
        return x


class Xorshift128:
    def __init__(self, x):
        self.x = [v & M32 for v in x]

    def next(self):
        t = self.x[3]
        t ^= (t << 11) & M32
        t ^= t >> 8
        s = self.x[0]
        self.x[3], self.x[2], self.x[1] = self.x[2], self.x[1], s
        self.x[0] = t ^ s ^ (s >> 19)
        return self.x[0]


class Xorwow:
    def __init__(self, x, c):
        self.x = [v & M32 for v in x]
        self.c = c & M32

    def next(self):
        t = self.x[4]
        s = self.x[0]
        self.x[4], self.x[3], self.x[2], self.x[1] = self.x[3], self.x[2], self.x[1], s
        t ^= t >> 2
        t ^= (t << 1) & M32
        t ^= s ^ ((s << 4) & M32)
        self.x[0] = t
        self.c = (self.c + 362437) & M32
        return (t + self.c) & M32


# ----------------------------------------------------------------------------
# wyhash64 (Lemire testingRNG), biski64, CET (urng custom), TT800
# ----------------------------------------------------------------------------
class WyHash64:
    def __init__(self, s):
        self.s = s & M64

    def next(self):
        self.s = (self.s + 0x60BEE2BEE120FC15) & M64
        t = self.s * 0xA3B195354A39B70D
        m1 = ((t >> 64) ^ t) & M64
        t = m1 * 0x1B03738712FAD5C9
        return ((t >> 64) ^ t) & M64


class Biski64:
    """github.com/danielcota/biski64 reference next()."""

    def __init__(self, fast_loop, mix, loop_mix):
        self.f, self.m, self.l = fast_loop & M64, mix & M64, loop_mix & M64

    def next(self):
        out = (self.m + self.l) & M64
        old_l = self.l
        self.l = self.f ^ self.m
        self.m = (rotl64(self.m, 16) + rotl64(old_l, 40)) & M64
        self.f = (self.f + 0x9999999999999999) & M64
        return out


CET_SP1 = 0xFFFFFFFFFFFFFF43
CET_SP2 = 0xFFFFFFFFFFFFFF1B
CET_P1 = 0x94D049BB133111EB


def cet_mix(x):
    x ^= x >> 30
    x = (x * CET_SP2) & M64
    x ^= x >> 27
    x = (x * CET_P1) & M64
    return x ^ (x >> 31)


class Cet64:
    def __init__(self, s):
        self.s = s & M64

    def next(self):
        self.s = (self.s + CET_SP1) & M64
        return cet_mix(self.s)


class Cet256:
    def __init__(self, s):
        self.s = [v & M64 for v in s]

    def next(self):
        v = self.s[0] | (self.s[1] << 64) | (self.s[2] << 128) | (self.s[3] << 192)
        v = (v + CET_SP1) & ((1 << 256) - 1)
        self.s = [(v >> (64 * i)) & M64 for i in range(4)]
        x = ((self.s[0] ^ self.s[3]) + rotl64(self.s[1], 17)) & M64
        return cet_mix(x)


class TT800:
    """Matsumoto & Kurita TT800 core (twist + 1996 tempering)."""

    N, M = 25, 7
    MAG = (0, 0x8EBFD028)

    def __init__(self, x, index=25):
        self.x = [v & M32 for v in x]
        self.k = index

    def next32(self):
        if self.k >= self.N:
            x = self.x
            for kk in range(self.N - self.M):
                x[kk] = x[kk + self.M] ^ (x[kk] >> 1) ^ self.MAG[x[kk] & 1]
            for kk in range(self.N - self.M, self.N):
                x[kk] = x[kk + self.M - self.N] ^ (x[kk] >> 1) ^ self.MAG[x[kk] & 1]
            self.k = 0
        y = self.x[self.k]
        y ^= (y << 7) & 0x2B5B2500
        y ^= (y << 15) & 0xDB8B0000
        y &= M32
        y ^= y >> 16
        self.k += 1
        return y


# ----------------------------------------------------------------------------
# Mersenne Twister MT19937 / MT19937-64
# ----------------------------------------------------------------------------
class MT19937:
    N, M = 624, 397

    def __init__(self, mt0=None):
        self.mt = [0] * self.N
        self.mti = self.N + 1
        if mt0 is not None:
            self.init_genrand(mt0)

    def init_genrand(self, s):
        self.mt[0] = s & M32
        for i in range(1, self.N):
            p = self.mt[i - 1]
            self.mt[i] = (1812433253 * (p ^ (p >> 30)) + i) & M32
        self.mti = self.N

    def init_by_array(self, key):
        self.init_genrand(19650218)
        i, j = 1, 0
        n = self.N
        for _ in range(max(n, len(key))):
            p = self.mt[i - 1]
            self.mt[i] = ((self.mt[i] ^ ((p ^ (p >> 30)) * 1664525)) + key[j] + j) & M32
            i += 1
            j += 1
            if i >= n:
                self.mt[0] = self.mt[n - 1]
                i = 1
            if j >= len(key):
                j = 0
        for _ in range(n - 1):
            p = self.mt[i - 1]
            self.mt[i] = ((self.mt[i] ^ ((p ^ (p >> 30)) * 1566083941)) - i) & M32
            i += 1
            if i >= n:
                self.mt[0] = self.mt[n - 1]
                i = 1
        self.mt[0] = 0x80000000

    def next(self):
        if self.mti >= self.N:
            mt = self.mt
            for kk in range(self.N):
                y = (mt[kk] & 0x80000000) | (mt[(kk + 1) % self.N] & 0x7FFFFFFF)
                mt[kk] = mt[(kk + self.M) % self.N] ^ (y >> 1) ^ (0x9908B0DF if y & 1 else 0)
            self.mti = 0
        y = self.mt[self.mti]
        self.mti += 1
        y ^= y >> 11
        y ^= (y << 7) & 0x9D2C5680
        y ^= (y << 15) & 0xEFC60000
        y ^= y >> 18
        return y & M32


class MT19937_64:
    NN, MM = 312, 156
    A = 0xB5026F5AA96619E9
    UM, LM = 0xFFFFFFFF80000000, 0x7FFFFFFF

    def __init__(self, mt0=None):
        self.mt = [0] * self.NN
        self.mti = self.NN + 1
        if mt0 is not None:
            self.init_genrand(mt0)

    def init_genrand(self, s):
        self.mt[0] = s & M64
        for i in range(1, self.NN):
            p = self.mt[i - 1]
            self.mt[i] = (6364136223846793005 * (p ^ (p >> 62)) + i) & M64
        self.mti = self.NN

    def init_by_array(self, key):
        self.init_genrand(19650218)
        i, j = 1, 0
        nn = self.NN
        for _ in range(max(nn, len(key))):
            p = self.mt[i - 1]
            self.mt[i] = ((self.mt[i] ^ ((p ^ (p >> 62)) * 3935559000370003845)) + key[j] + j) & M64
            i += 1
            j += 1
            if i >= nn:
                self.mt[0] = self.mt[nn - 1]
                i = 1
            if j >= len(key):
                j = 0
        for _ in range(nn - 1):
            p = self.mt[i - 1]
            self.mt[i] = ((self.mt[i] ^ ((p ^ (p >> 62)) * 2862933555777941757)) - i) & M64
            i += 1
            if i >= nn:
                self.mt[0] = self.mt[nn - 1]
                i = 1
        self.mt[0] = 1 << 63

    def next(self):
        if self.mti >= self.NN:
            mt = self.mt
            for i in range(self.NN):
                x = (mt[i] & self.UM) | (mt[(i + 1) % self.NN] & self.LM)
                mt[i] = mt[(i + self.MM) % self.NN] ^ (x >> 1) ^ (self.A if x & 1 else 0)
            self.mti = 0
        x = self.mt[self.mti]
        self.mti += 1
        x ^= (x >> 29) & 0x5555555555555555
        x ^= (x << 17) & 0x71D67FFFEDA60000
        x ^= (x << 37) & 0xFFF7EEE000000000
        x &= M64
        x ^= x >> 43
        return x


# ----------------------------------------------------------------------------
# SFMT (Saito & Matsumoto), parameter sets from SFMT-params*.h
# ----------------------------------------------------------------------------
SFMT_PARAMS = {
    # mexp: (pos1, sl1, sl2, sr1, sr2, msk1..4, parity1..4)
    607: (2, 15, 3, 13, 3, 0xfdff37ff, 0xef7f3f7d, 0xff777b7d, 0x7ff7fb2f, 0x00000001, 0x00000000, 0x00000000, 0x5986f054),
    1279: (7, 14, 3, 5, 1, 0xf7fefffd, 0x7fefcfff, 0xaff3ef3f, 0xb5ffff7f, 0x00000001, 0x00000000, 0x00000000, 0x20000000),
    2281: (12, 19, 1, 5, 1, 0xbff7ffbf, 0xfdfffffe, 0xf7ffef7f, 0xf2f7cbbf, 0x00000001, 0x00000000, 0x00000000, 0x41dfa600),
    4253: (17, 20, 1, 7, 1, 0x9f7bffff, 0x9fffff5f, 0x3efffffb, 0xfffff7bb, 0xa8000001, 0xaf5390a3, 0xb740b3f8, 0x6c11486d),
    11213: (68, 14, 3, 7, 3, 0xeffff7fb, 0xffffffef, 0xdfdfbfff, 0x7fffdbfd, 0x00000001, 0x00000000, 0xe8148000, 0xd0c7afa3),
    19937: (122, 18, 1, 11, 1, 0xdfffffef, 0xddfecb7f, 0xbffaffff, 0xbffffff6, 0x00000001, 0x00000000, 0x00000000, 0x13c9e684),
    44497: (330, 5, 3, 9, 3, 0xeffffffb, 0xdfbebfff, 0xbfbf7bef, 0x9ffd7bff, 0x00000001, 0x00000000, 0xa3ac4000, 0xecc1327a),
    86243: (366, 6, 7, 19, 1, 0xfdbffbff, 0xbff7ff3f, 0xfd77efff, 0xbf9ff3ff, 0x00000001, 0x00000000, 0x00000000, 0xe9528d85),
    132049: (110, 19, 1, 21, 1, 0xffffbb5f, 0xfb6ebf95, 0xfffefffa, 0xcff77fff, 0x00000001, 0x00000000, 0xcb520000, 0xc7e91c7d),
    216091: (627, 11, 3, 10, 1, 0xbff7bff7, 0xbfffffff, 0xbffffa7f, 0xffddfbfb, 0xf8000001, 0x89e80709, 0x3bd2b64b, 0x0c64b1e4),
}


class SFMT:
    def __init__(self, mexp):
        (self.pos1, self.sl1, self.sl2, self.sr1, self.sr2, m1, m2, m3, m4,
         p1, p2, p3, p4) = SFMT_PARAMS[mexp]
        self.msk = [m1, m2, m3, m4]
        self.parity = [p1, p2, p3, p4]
        self.n = mexp // 128 + 1
        self.n32 = self.n * 4
        self.st = [0] * self.n32  # flat little-endian u32 view
        self.idx = self.n32

    def init_gen_rand(self, seed):
        self.st[0] = seed & M32
        for i in range(1, self.n32):
            p = self.st[i - 1]
            self.st[i] = (1812433253 * (p ^ (p >> 30)) + i) & M32
        self.idx = self.n32
        self.period_certification()

    def set_state(self, words32):
        assert len(words32) == self.n32
        self.st = [w & M32 for w in words32]
        self.idx = self.n32
        self.period_certification()

    def period_certification(self):
        inner = 0
        for i in range(4):
            inner ^= self.st[i] & self.parity[i]
        i = 16
        while i > 0:
            inner ^= inner >> i
            i >>= 1
        if inner & 1:
            return
        for i in range(4):
            work = 1
            for _ in range(32):
                if work & self.parity[i]:
                    self.st[i] ^= work
                    return
                work = (work << 1) & M32

    def _w(self, i):
        return (self.st[4 * i] | (self.st[4 * i + 1] << 32) | (self.st[4 * i + 2] << 64)
                | (self.st[4 * i + 3] << 96))

    def _set_w(self, i, v):
        for k in range(4):
            self.st[4 * i + k] = (v >> (32 * k)) & M32

    def gen_rand_all(self):
        M128 = (1 << 128) - 1
        n = self.n
        r1 = self._w(n - 2)
        r2 = self._w(n - 1)
        for i in range(n):
            a = self._w(i)
            b = self._w((i + self.pos1) % n)
            x = (a << (self.sl2 * 8)) & M128
            y = r1 >> (self.sr2 * 8)
            out = 0
            for k in range(4):
                ak = (a >> (32 * k)) & M32
                bk = (b >> (32 * k)) & M32
                dk = (r2 >> (32 * k)) & M32
                v = ak ^ ((x >> (32 * k)) & M32) ^ ((bk >> self.sr1) & self.msk[k]) \
                    ^ ((y >> (32 * k)) & M32) ^ ((dk << self.sl1) & M32)
                out |= v << (32 * k)
            self._set_w(i, out)
            r1, r2 = r2, out

    def next32(self):
        if self.idx >= self.n32:
            self.gen_rand_all()
            self.idx = 0
        v = self.st[self.idx]
        self.idx += 1
        return v

    def next64(self):
        # gen_rand64: idx counts u32 words in SFMT C; here count u64 words
        if self.idx >= self.n32:
            self.gen_rand_all()
            self.idx = 0
        v = self.st[self.idx] | (self.st[self.idx + 1] << 32)
        self.idx += 2
        return v


# ----------------------------------------------------------------------------
# Random123: Philox, Threefry; Skein Threefish-256
# ----------------------------------------------------------------------------
def philox4x32(ctr, key, rounds=10):
    x = list(ctr)
    k = list(key)
    for r in range(rounds):
        if r:
            k = [(k[0] + 0x9E3779B9) & M32, (k[1] + 0xBB67AE85) & M32]
        p0 = 0xD2511F53 * x[0]
        p1 = 0xCD9E8D57 * x[2]
        x = [((p1 >> 32) ^ x[1] ^ k[0]) & M32, p1 & M32,
             ((p0 >> 32) ^ x[3] ^ k[1]) & M32, p0 & M32]
    return x


def philox2x64(ctr, key, rounds=10):
    x = list(ctr)
    k = key[0]
    for r in range(rounds):
        if r:
            k = (k + 0x9E3779B97F4A7C15) & M64
        p = 0xD2B74407B1CE6E93 * x[0]
        x = [((p >> 64) ^ x[1] ^ k) & M64, p & M64]
    return x


def philox4x64(ctr, key, rounds=10):
    x = list(ctr)
    k = list(key)
    for r in range(rounds):
        if r:
            k = [(k[0] + 0x9E3779B97F4A7C15) & M64, (k[1] + 0xBB67AE8584CAA73B) & M64]
        p0 = 0xD2E7470EE14C6C93 * x[0]
        p1 = 0xCA5A826395121157 * x[2]
        x = [((p1 >> 64) ^ x[1] ^ k[0]) & M64, p1 & M64,
             ((p0 >> 64) ^ x[3] ^ k[1]) & M64, p0 & M64]
    return x


R32x2 = [13, 15, 26, 6, 17, 29, 16, 24]
R32x4 = [(10, 26), (11, 21), (13, 27), (23, 5), (6, 20), (17, 11), (25, 10), (18, 20)]
R64x4 = [(14, 16), (52, 57), (23, 40), (5, 37), (25, 33), (46, 12), (58, 22), (32, 32)]


def threefry2x32(ctr, key, rounds=20):
    ks = [key[0], key[1], 0x1BD11BDA ^ key[0] ^ key[1]]
    x = [(ctr[0] + ks[0]) & M32, (ctr[1] + ks[1]) & M32]
    for r in range(rounds):
        x[0] = (x[0] + x[1]) & M32
        x[1] = rotl32(x[1], R32x2[r % 8]) ^ x[0]
        if r % 4 == 3:
            s = (r + 1) // 4
            x[0] = (x[0] + ks[s % 3]) & M32
            x[1] = (x[1] + ks[(s + 1) % 3] + s) & M32
    return x


def threefry4x32(ctr, key, rounds=20):
    ks = list(key) + [0x1BD11BDA ^ key[0] ^ key[1] ^ key[2] ^ key[3]]
    x = [(ctr[i] + ks[i]) & M32 for i in range(4)]
    for r in range(rounds):
        ra, rb = R32x4[r % 8]
        if r % 2 == 0:
            x[0] = (x[0] + x[1]) & M32
            x[1] = rotl32(x[1], ra) ^ x[0]
            x[2] = (x[2] + x[3]) & M32
            x[3] = rotl32(x[3], rb) ^ x[2]
        else:
            x[0] = (x[0] + x[3]) & M32
            x[3] = rotl32(x[3], ra) ^ x[0]
            x[2] = (x[2] + x[1]) & M32
            x[1] = rotl32(x[1], rb) ^ x[2]
        if r % 4 == 3:
            s = (r + 1) // 4
            for i in range(4):
                x[i] = (x[i] + ks[(s + i) % 5]) & M32
            x[3] = (x[3] + s) & M32
    return x


def threefry4x32_tweaked(ctr, key, tw, rounds=20):
    """urng 1.0.0 (base) variant: Threefish-style tweak schedule + counter XOR.
    Not a Random123 algorithm; used only to identify what the code computes."""
    ks = list(key) + [0x1BD11BDA ^ key[0] ^ key[1] ^ key[2] ^ key[3]]
    t = [tw[0], tw[1], tw[0] ^ tw[1]]

    def inj(x, s):
        x[0] = (x[0] + ks[s % 5]) & M32
        x[1] = (x[1] + ks[(s + 1) % 5] + t[s % 3]) & M32
        x[2] = (x[2] + ks[(s + 2) % 5] + t[(s + 1) % 3]) & M32
        x[3] = (x[3] + ks[(s + 3) % 5] + s) & M32

    x = list(ctr)
    inj(x, 0)
    for r in range(rounds):
        ra, rb = R32x4[r % 8]
        if r % 2 == 0:
            x[0] = (x[0] + x[1]) & M32
            x[1] = rotl32(x[1], ra) ^ x[0]
            x[2] = (x[2] + x[3]) & M32
            x[3] = rotl32(x[3], rb) ^ x[2]
        else:
            x[0] = (x[0] + x[3]) & M32
            x[3] = rotl32(x[3], ra) ^ x[0]
            x[2] = (x[2] + x[1]) & M32
            x[1] = rotl32(x[1], rb) ^ x[2]
        if r % 4 == 3:
            inj(x, (r + 1) // 4)
    return [x[i] ^ ctr[i] for i in range(4)]


def threefish256(block, key, tweak, xor_ctr=False):
    """Skein 1.3 Threefish-256 encryption (72 rounds)."""
    ks = list(key) + [0x1BD11BDAA9FC1A22 ^ key[0] ^ key[1] ^ key[2] ^ key[3]]
    t = [tweak[0], tweak[1], tweak[0] ^ tweak[1]]

    def subkey(s):
        return [ks[s % 5], (ks[(s + 1) % 5] + t[s % 3]) & M64,
                (ks[(s + 2) % 5] + t[(s + 1) % 3]) & M64, (ks[(s + 3) % 5] + s) & M64]

    v = list(block)
    for d in range(72):
        if d % 4 == 0:
            sk = subkey(d // 4)
            v = [(v[i] + sk[i]) & M64 for i in range(4)]
        ra, rb = R64x4[d % 8]
        e0 = (v[0] + v[1]) & M64
        e1 = rotl64(v[1], ra) ^ e0
        e2 = (v[2] + v[3]) & M64
        e3 = rotl64(v[3], rb) ^ e2
        f = [e0, e1, e2, e3]
        v = [f[0], f[3], f[2], f[1]]  # permutation pi = (0,3,2,1)
    sk = subkey(18)
    out = [(v[i] + sk[i]) & M64 for i in range(4)]
    if xor_ctr:
        out = [out[i] ^ block[i] for i in range(4)]
    return out


# ----------------------------------------------------------------------------
# Squares (Widynski 2020)
# ----------------------------------------------------------------------------
def squares32(ctr, key):
    x = y = (ctr * key) & M64
    z = (y + key) & M64
    x = (x * x + y) & M64
    x = ((x >> 32) | (x << 32)) & M64
    x = (x * x + z) & M64
    x = ((x >> 32) | (x << 32)) & M64
    x = (x * x + y) & M64
    x = ((x >> 32) | (x << 32)) & M64
    return ((x * x + z) & M64) >> 32


# ----------------------------------------------------------------------------
# ChaCha (RFC 8439) and Salsa20 (Bernstein)
# ----------------------------------------------------------------------------
def chacha_block(state, rounds):
    x = list(state)

    def qr(a, b, c, d):
        x[a] = (x[a] + x[b]) & M32; x[d] = rotl32(x[d] ^ x[a], 16)
        x[c] = (x[c] + x[d]) & M32; x[b] = rotl32(x[b] ^ x[c], 12)
        x[a] = (x[a] + x[b]) & M32; x[d] = rotl32(x[d] ^ x[a], 8)
        x[c] = (x[c] + x[d]) & M32; x[b] = rotl32(x[b] ^ x[c], 7)

    for _ in range(rounds // 2):
        qr(0, 4, 8, 12); qr(1, 5, 9, 13); qr(2, 6, 10, 14); qr(3, 7, 11, 15)
        qr(0, 5, 10, 15); qr(1, 6, 11, 12); qr(2, 7, 8, 13); qr(3, 4, 9, 14)
    return [(x[i] + state[i]) & M32 for i in range(16)]


def chacha_state(key_words, counter, nonce_words):
    return [0x61707865, 0x3320646E, 0x79622D32, 0x6B206574] + list(key_words) + \
           [counter & M32] + list(nonce_words)


def salsa_core(state, rounds):
    x = list(state)

    def qr(a, b, c, d):
        x[b] ^= rotl32((x[a] + x[d]) & M32, 7)
        x[c] ^= rotl32((x[b] + x[a]) & M32, 9)
        x[d] ^= rotl32((x[c] + x[b]) & M32, 13)
        x[a] ^= rotl32((x[d] + x[c]) & M32, 18)

    for _ in range(rounds // 2):
        qr(0, 4, 8, 12); qr(5, 9, 13, 1); qr(10, 14, 2, 6); qr(15, 3, 7, 11)
        qr(0, 1, 2, 3); qr(5, 6, 7, 4); qr(10, 11, 8, 9); qr(15, 12, 13, 14)
    return [(x[i] + state[i]) & M32 for i in range(16)]


def salsa_state(key_words, nonce_words, counter):
    c = [0x61707865, 0x3320646E, 0x79622D32, 0x6B206574]
    k = key_words
    return [c[0], k[0], k[1], k[2], k[3], c[1], nonce_words[0], nonce_words[1],
            counter & M32, (counter >> 32) & M32, c[2], k[4], k[5], k[6], k[7], c[3]]


# ----------------------------------------------------------------------------
# Float conversions used by the crate (documented bit-trick)
# ----------------------------------------------------------------------------
import struct


def f01_32(w):
    return struct.unpack('<f', struct.pack('<I', (w >> 9) | 0x3F800000))[0] - 1.0


def f01_64(w):
    return struct.unpack('<d', struct.pack('<Q', (w >> 11) | 0x3FF0000000000000))[0] - 1.0
