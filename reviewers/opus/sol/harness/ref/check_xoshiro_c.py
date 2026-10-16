"""Compile the official xoshiro/xoroshiro C sources with xoshiro_driver.c and
compare their output with refimpl.py.  Usage: python check_xoshiro_c.py <dl_dir> <build_dir>
(dl_dir holds the downloaded official .c files in per-file subdirectories)."""
import os
import random
import subprocess
import sys

sys.path.insert(0, os.path.dirname(__file__))
from refimpl import *  # noqa

dl, build = sys.argv[1], sys.argv[2]
os.makedirs(build, exist_ok=True)
driver = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'xoshiro_driver.c')

cases = [
    ('xoshiro128plusplus.c', 32, 4, lambda st: Xoshiro128(st, True), True, True),
    ('xoshiro128starstar.c', 32, 4, lambda st: Xoshiro128(st, False), True, True),
    ('xoshiro256plusplus.c', 64, 4, lambda st: Xoshiro256(st, True), True, True),
    ('xoshiro256starstar.c', 64, 4, lambda st: Xoshiro256(st, False), True, True),
    ('xoroshiro64starstar.c', 32, 2, lambda st: Xoroshiro64ss(st), False, False),
    ('xoroshiro128plusplus.c', 64, 2, lambda st: Xoroshiro128(st, True), True, True),
    ('xoroshiro128starstar.c', 64, 2, lambda st: Xoroshiro128(st, False), True, True),
]
fails = 0
rnd = random.Random(20261016)
for fname, w, ns, mk, hj, hl in cases:
    src = None
    for root, _, files in os.walk(dl):
        if fname in files:
            src = os.path.join(root, fname)
    exe = os.path.join(build, fname.replace('.c', '.exe'))
    flags = [f'-DSRC="{src.replace(os.sep, "/")}"', f'-DNS={ns}']
    if hj:
        flags.append('-DHAS_JUMP')
    if hl:
        flags.append('-DHAS_LONG_JUMP')
    subprocess.check_call(['gcc', '-O2', *flags, '-o', exe, driver])
    for trial in range(5):
        mask = (1 << w) - 1
        st = [rnd.getrandbits(w) for _ in range(ns)]
        out = subprocess.check_output([exe, '2000', *[format(x, 'x') for x in st]], text=True).split('\n')
        g = mk(list(st))
        exp = [format(g.next(), 'x') for _ in range(2000)]
        ok = out[:2000] == exp
        if not ok:
            fails += 1
        print(f"{'PASS' if ok else 'FAIL'} {fname} trial {trial} outputs")
        # Jumps: compare the C state after jump() with the GF(2) matrix power
        # applied in Python (independent of the published constants)
        if fname.startswith('xoroshiro128'):
            pp = 'plusplus' in fname
            g = Xoroshiro128(list(st), pp)
            for _ in range(2000):
                g.next()
            g.jump_pow2(64)
            j = [int(x, 16) for x in out[2000].split()[1:]]
            ok = j == g.s
            fails += 0 if ok else 1
            print(f"{'PASS' if ok else 'FAIL'} {fname} trial {trial} jump == T^(2^64)")
            g.jump_pow2(96)
            l = [int(x, 16) for x in out[2001].split()[1:]]
            ok = l == g.s
            fails += 0 if ok else 1
            print(f"{'PASS' if ok else 'FAIL'} {fname} trial {trial} long_jump == T^(2^96)")
print('fails', fails)
sys.exit(1 if fails else 0)
