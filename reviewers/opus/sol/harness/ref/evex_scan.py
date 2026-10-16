"""Static machine-code check: are AVX-512 (EVEX-encoded) instructions reachable
from the AVX2-only C ABI entry points?

usage: python evex_scan.py <urng.dll> <export-prefix> [<export-prefix> ...]

Disassembles the DLL with llvm-objdump, then walks every direct call / jump /
fall-through edge starting at the given exports.  Indirect transfers (IAT,
function pointers) are not followed.  In 64-bit mode an instruction whose
first opcode byte is 0x62 is EVEX (AVX-512); `%k` mask registers and `zmm`
operands are reported too.
"""
import re
import subprocess
import sys

dll, prefixes = sys.argv[1], sys.argv[2:]
out = subprocess.run(['llvm-objdump', '-d', dll], capture_output=True, text=True).stdout
ins = {}
order = []
exports = {}
line_re = re.compile(r'^\s*([0-9a-f]+):\s+((?:[0-9a-f]{2} )+)\s*(.*)$')
sym_re = re.compile(r'^([0-9a-f]+) <([^>]+)>:')
for line in out.splitlines():
    m = sym_re.match(line)
    if m:
        exports[m.group(2)] = int(m.group(1), 16)
        continue
    m = line_re.match(line)
    if m:
        addr = int(m.group(1), 16)
        raw = m.group(2).split()
        ins[addr] = (raw, m.group(3).strip())
        order.append(addr)
nxt = {order[i]: order[i + 1] for i in range(len(order) - 1)}
target_re = re.compile(r'\b0x([0-9a-f]+)\b')

LEGACY = {'66', '67', 'f2', 'f3', '2e', '36', '3e', '26', '64', '65', 'f0'}


def is_evex(raw):
    i = 0
    while i < len(raw) and raw[i] in LEGACY:
        i += 1
    return i < len(raw) and raw[i] == '62'


for prefix in prefixes:
    starts = sorted((n, a) for n, a in exports.items() if n.startswith(prefix))
    for name, start in starts:
        seen = set()
        stack = [start]
        evex = []
        while stack:
            a = stack.pop()
            while a is not None and a in ins and a not in seen:
                seen.add(a)
                raw, text = ins[a]
                if is_evex(raw) or '%k' in text or 'zmm' in text:
                    evex.append((a, text))
                mn = text.split()[0] if text else ''
                tgt = target_re.search(text)
                if mn.startswith('j') or mn.startswith('call'):
                    if tgt and '*' not in text:
                        stack.append(int(tgt.group(1), 16))
                    if mn == 'jmp' or mn == 'jmpq':
                        break
                if mn.startswith('ret') or mn == 'ud2' or mn == 'int3':
                    break
                a = nxt.get(a)
        print(f"{name:<32} reachable instructions={len(seen):6d}  EVEX/AVX-512={len(evex)}"
              + ("" if not evex else f"  e.g. {evex[0][1]}"))
