"""Validate refimpl.py against official / third-party vectors.

Vectors (copied verbatim into ../vectors, sources):
  mt19937ar.out          http://www.math.sci.hiroshima-u.ac.jp/m-mat/MT/MT2002/CODES/mt19937ar.out
  mt19937-64.out.txt     http://www.math.sci.hiroshima-u.ac.jp/m-mat/MT/mt19937-64.out.txt
  SFMT.<mexp>.out.txt    https://github.com/MersenneTwister-Lab/SFMT (master)
  random123_kat_vectors  https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors
  Threefish-256 (tweak)  Bouncy Castle Threefish256Test.java (Skein 1.3 golden KAT)
  ChaCha20 block         RFC 8439 section 2.3.2
  Salsa20/8 core         RFC 7914 section 8; scrypt cross-check with hashlib.scrypt (OpenSSL)
  PCG32                  pcg-c-basic pcg32-demo (output recorded by compiling the demo)
"""
import hashlib
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from refimpl import *  # noqa

VEC = os.path.join(os.path.dirname(__file__), '..', 'vectors')
fails = 0
passes = 0


def check(name, got, exp):
    global fails, passes
    if list(got) == list(exp):
        passes += 1
        print(f"PASS {name}")
    else:
        fails += 1
        print(f"FAIL {name}\n  got {[hex(x) for x in list(got)[:8]]}\n  exp {[hex(x) for x in list(exp)[:8]]}")


def ints_after(lines, header_pred, count):
    out = []
    started = False
    for line in lines:
        if not started:
            if header_pred(line):
                started = True
            continue
        for tok in line.split():
            try:
                out.append(int(tok))
            except ValueError:
                return out
            if len(out) == count:
                return out
    return out


# --- MT19937 (init_by_array {0x123,0x234,0x345,0x456}) ---
lines = open(os.path.join(VEC, 'mt19937ar.out')).read().splitlines()
exp = ints_after(lines, lambda l: 'genrand_int32' in l, 1000)
mt = MT19937()
mt.init_by_array([0x123, 0x234, 0x345, 0x456])
check('MT19937 init_by_array 1000', [mt.next() for _ in range(1000)], exp)
mt = MT19937(5489)
check('MT19937 seed 5489 first', [mt.next()], [3499211612])

# --- MT19937-64 (init_by_array64 {0x12345,0x23456,0x34567,0x45678}) ---
lines = open(os.path.join(VEC, 'mt19937-64.out.txt')).read().splitlines()
exp = ints_after(lines, lambda l: 'genrand64_int64' in l, 1000)
mt = MT19937_64()
mt.init_by_array([0x12345, 0x23456, 0x34567, 0x45678])
check('MT19937-64 init_by_array 1000', [mt.next() for _ in range(1000)], exp)

# --- SFMT all parameter sets: init_gen_rand(1234), 32-bit outputs ---
for mexp in sorted(SFMT_PARAMS):
    path = os.path.join(VEC, f'SFMT.{mexp}.out.txt')
    lines = open(path).read().splitlines()
    head = lines[0]
    p = SFMT_PARAMS[mexp]
    want_head = f"SFMT-{mexp}:{p[0]}-{p[1]}-{p[2]}-{p[3]}-{p[4]}:{p[5]:08x}-{p[6]:08x}-{p[7]:08x}-{p[8]:08x}"
    check(f'SFMT-{mexp} parameter header', [head == want_head], [True])
    exp = ints_after(lines, lambda l: l.startswith('init_gen_rand'), 1000)
    s = SFMT(mexp)
    s.init_gen_rand(1234)
    check(f'SFMT-{mexp} init_gen_rand(1234) 1000', [s.next32() for _ in range(1000)], exp)

# --- Random123 KAT ---
kat = [l.split() for l in open(os.path.join(VEC, 'random123_kat_vectors.txt')).read().splitlines()
       if l and not l.startswith('#')]


def kat_rows(name, rounds):
    for row in kat:
        if row[0] == name and row[1] == str(rounds):
            yield [int(x, 16) for x in row[2:]]


for row in kat_rows('philox4x32', 10):
    check(f'philox4x32-10 {row[0]:08x}', philox4x32(row[0:4], row[4:6]), row[6:10])
for row in kat_rows('philox4x32', 7):
    check(f'philox4x32-7 {row[0]:08x}', philox4x32(row[0:4], row[4:6], 7), row[6:10])
for row in kat_rows('philox2x64', 10):
    check(f'philox2x64-10 {row[0]:016x}', philox2x64(row[0:2], row[2:3]), row[3:5])
for row in kat_rows('philox4x64', 10):
    check(f'philox4x64-10 {row[0]:016x}', philox4x64(row[0:4], row[4:6]), row[6:10])
for row in kat_rows('threefry2x32', 20):
    check(f'threefry2x32-20 {row[0]:08x}', threefry2x32(row[0:2], row[2:4]), row[4:6])
for row in kat_rows('threefry2x32', 13):
    check(f'threefry2x32-13 {row[0]:08x}', threefry2x32(row[0:2], row[2:4], 13), row[4:6])
for row in kat_rows('threefry4x32', 20):
    check(f'threefry4x32-20 {row[0]:08x}', threefry4x32(row[0:4], row[4:8]), row[8:12])
for row in kat_rows('threefry4x32', 72):
    check(f'threefry4x32-72 {row[0]:08x}', threefry4x32(row[0:4], row[4:8], 72), row[8:12])
for row in kat_rows('threefry4x64', 72):
    # Threefry4x64-72 == Threefish-256 with zero tweak
    check(f'threefry4x64-72 (=Threefish-256, tweak 0) {row[0]:016x}',
          threefish256(row[0:4], row[4:8], [0, 0]), row[8:12])

# --- Threefish-256 with tweak (Skein 1.3 golden KAT via Bouncy Castle) ---
def le_words(hexstr):
    b = bytes.fromhex(hexstr)
    return [int.from_bytes(b[i:i + 8], 'little') for i in range(0, len(b), 8)]


check('Threefish-256 KAT zero',
      threefish256([0] * 4, [0] * 4, [0, 0]),
      le_words('84da2a1f8beaee947066ae3e3103f1ad536db1f4a1192495116b9f3ce6133fd8'))
check('Threefish-256 KAT 2',
      threefish256(le_words('FFFEFDFCFBFAF9F8F7F6F5F4F3F2F1F0EFEEEDECEBEAE9E8E7E6E5E4E3E2E1E0'),
                   le_words('101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f'),
                   le_words('000102030405060708090a0b0c0d0e0f')),
      le_words('e0d091ff0eea8fdfc98192e62ed80ad59d865d08588df476657056b5955e97df'))

# --- ChaCha20 RFC 8439 2.3.2 ---
key = [int.from_bytes(bytes(range(4 * i, 4 * i + 4)), 'little') for i in range(8)]
nonce = [int.from_bytes(b, 'little') for b in (bytes([0, 0, 0, 9]), bytes([0, 0, 0, 0x4a]), bytes(4))]
check('ChaCha20 RFC8439 2.3.2 block', chacha_block(chacha_state(key, 1, nonce), 20), [
    0xe4e7f110, 0x15593bd1, 0x1fdd0f50, 0xc47120a3, 0xc7f4d1c7, 0x0368c033, 0x9aaa2204, 0x4e6cd4c3,
    0x466482d2, 0x09aa9f07, 0x05d7c214, 0xa2028bd9, 0xd19c12b5, 0xb94e16de, 0xe883d0cb, 0x4e3c50a2])

# --- Salsa20/8 core RFC 7914 section 8 ---
def le_u32s(hexstr):
    b = bytes.fromhex(hexstr)
    return [int.from_bytes(b[i:i + 4], 'little') for i in range(0, len(b), 4)]


check('Salsa20/8 core RFC7914', salsa_core(le_u32s(
    '7e879a214f3ec9867ca940e641718f26baee555b8c61c1b50df846116dcd3b1d'
    'ee24f319df9b3d8514121e4b5ac5aa3276021d2909c74829edebc68db8b8c25e'), 8), le_u32s(
    'a41f859c6608cc993b81cacb020cef05044b2181a2fd337dfd7b1c6396682f29'
    'b4393168e3c9e6bcfe6bc5b7a06d96bae424cc102c91745c24ad673dc7618f81'))


# scrypt(N=16,r=1,p=1) implemented on top of salsa_core(8) vs OpenSSL's scrypt
def scrypt_ref(password, salt, n, r, p, dklen):
    b = hashlib.pbkdf2_hmac('sha256', password, salt, 1, p * 128 * r)

    def blockmix(blk):
        x = blk[(2 * r - 1) * 64:]
        ys = []
        for i in range(2 * r):
            t = bytes(a ^ c for a, c in zip(x, blk[i * 64:(i + 1) * 64]))
            words = [int.from_bytes(t[k:k + 4], 'little') for k in range(0, 64, 4)]
            x = b''.join(w.to_bytes(4, 'little') for w in salsa_core(words, 8))
            ys.append(x)
        return b''.join(ys[0::2] + ys[1::2])

    out = b''
    for i in range(p):
        x = b[i * 128 * r:(i + 1) * 128 * r]
        v = []
        for _ in range(n):
            v.append(x)
            x = blockmix(x)
        for _ in range(n):
            j = int.from_bytes(x[(2 * r - 1) * 64:(2 * r - 1) * 64 + 8], 'little') % n
            x = blockmix(bytes(a ^ c for a, c in zip(x, v[j])))
        out += x
    return hashlib.pbkdf2_hmac('sha256', password, out, 1, dklen)


for pw, salt in [(b'', b''), (b'password', b'NaCl'), (b'urng-review', b'opus')]:
    check(f'scrypt(Salsa20/8 core) vs OpenSSL pw={pw!r}',
          list(scrypt_ref(pw, salt, 16, 1, 1, 64)),
          list(hashlib.scrypt(pw, salt=salt, n=16, r=1, p=1, dklen=64)))

# --- PCG32 demo (pcg32_srandom_r(&rng, 42u, 54u)) ---
g = Pcg32.srandom(42, 54)
check('PCG32 demo round 1', [g.next() for _ in range(6)],
      [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e])

# --- xoroshiro128 jump: GF(2) matrix power vs published JUMP polynomial ---
def poly_jump(gen, consts):
    acc = [0, 0]
    for c in consts:
        for b in range(64):
            if c >> b & 1:
                acc[0] ^= gen.s[0]
                acc[1] ^= gen.s[1]
            gen.next()
    gen.s = acc


for pp, jump, ljump in [(True, [0x2bd7a6a6e99c2ddc, 0x0992ccaf6a6fca05], [0x360fd5f2cf8d5d99, 0x9c6e6877736c46e3]),
                        (False, [0xdf900294d8f554a5, 0x170865df4b3201fc], [0xd2a98b26625eee7b, 0xdddf9b1090aa7ac1])]:
    st = [0x0123456789ABCDEF, 0xFEDCBA9876543210]
    a = Xoroshiro128(st, pp); poly_jump(a, jump)
    b = Xoroshiro128(st, pp); b.jump_pow2(64)
    check(f'xoroshiro128{"++" if pp else "**"} jump == T^(2^64)', a.s, b.s)
    a = Xoroshiro128(st, pp); poly_jump(a, ljump)
    b = Xoroshiro128(st, pp); b.jump_pow2(96)
    check(f'xoroshiro128{"++" if pp else "**"} long_jump == T^(2^96)', a.s, b.s)

print(f"\n{passes} passed, {fails} failed")
sys.exit(1 if fails else 0)
