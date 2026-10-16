"""Demonstrate that Astra's Threefry32x4 / Threefish256 outputs are NOT the
Random123 Threefry4x32-20 / Skein Threefish-256 functions of the derived key,
but the 1.0.0 tweak + counter-XOR variant.

usage: python threefry_kat_demo.py <dump-astra.txt>
"""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from refimpl import *  # noqa

dump = {}
for line in open(sys.argv[1]):
    p = line.split()
    dump[(p[0], int(p[1], 16))] = [int(x, 16) for x in p[2:]]

for seed in (0, 1, 42):
    g = SplitMix32(seed, False)
    key = [g.next() for _ in range(4)]
    tw = [g.next() for _ in range(2)]
    got = dump[('Threefry32x4', seed)][:4]
    r123 = threefry4x32([0, 0, 0, 0], key)
    variant = threefry4x32_tweaked([0, 0, 0, 0], key, tw)
    print(f"Threefry32x4::new({seed}) first block: {[hex(x) for x in got]}")
    print(f"   Random123 threefry4x32-20(ctr=0, key) : {[hex(x) for x in r123]}  match={got == r123}")
    print(f"   1.0.0 tweak+XOR variant               : {[hex(x) for x in variant]}  match={got == variant}")

for seed in (0, 1):
    g = SplitMix64(seed, False)
    key = [g.next() for _ in range(4)]
    tw = [g.next() for _ in range(2)]
    got = dump[('Threefish256', seed)][:4]
    pure = threefish256([0, 0, 0, 0], key, tw)
    xored = threefish256([0, 0, 0, 0], key, tw, xor_ctr=True)
    print(f"Threefish256::new({seed}) first block == Threefish-256 E_k,t(ctr) : {got == pure}; == E(ctr) XOR ctr : {got == xored}")
    # ctr=0 makes XOR invisible; compare block 1 as well
    got1 = dump[('Threefish256', seed)][4:8]
    pure1 = threefish256([1, 0, 0, 0], key, tw)
    xored1 = threefish256([1, 0, 0, 0], key, tw, xor_ctr=True)
    print(f"   block 1 == E(ctr=1): {got1 == pure1}; == E(1) XOR 1: {got1 == xored1}")
