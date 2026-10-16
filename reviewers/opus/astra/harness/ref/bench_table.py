"""Combine bench CSVs (two runs per variant) into a markdown table.
usage: python bench_table.py <out_dir> <variant> [<variant> ...]
Each line: name,unit,median,min,max  (name may contain commas)
Reported value = mean of the per-run medians; spread = |run1-run2|/mean.
"""
import os
import sys

out_dir, variants = sys.argv[1], sys.argv[2:]


def load(path):
    rows = {}
    if not os.path.exists(path):
        return rows
    for line in open(path, encoding='utf-8-sig'):
        line = line.strip()
        if not line:
            continue
        parts = line.rsplit(',', 4)
        rows[parts[0]] = float(parts[2])
    return rows


data = {}
for v in variants:
    r1 = load(os.path.join(out_dir, f'bench-{v}-run1.csv'))
    r2 = load(os.path.join(out_dir, f'bench-{v}-run2.csv'))
    data[v] = {}
    for k in r1:
        vals = [r1[k]] + ([r2[k]] if k in r2 else [])
        mean = sum(vals) / len(vals)
        spread = (max(vals) - min(vals)) / mean if len(vals) > 1 else float('nan')
        data[v][k] = (mean, spread)

names = list(data[variants[0]].keys())
hdr = '| case | ' + ' | '.join(f'{v} ns/word' for v in variants) + ' | ' + \
      ' | '.join(f'{v}/{variants[0]}' for v in variants[1:]) + ' | max run spread |'
print(hdr)
print('|' + '---|' * (1 + len(variants) * 2))
for k in names:
    cells = []
    spreads = []
    for v in variants:
        if k in data[v]:
            cells.append(f'{data[v][k][0]:.4f}')
            spreads.append(data[v][k][1])
        else:
            cells.append('-')
    ratios = []
    b = data[variants[0]].get(k, (None,))[0]
    for v in variants[1:]:
        if k in data[v] and b:
            ratios.append(f'{data[v][k][0] / b:.2f}')
        else:
            ratios.append('-')
    sp = max((s for s in spreads if s == s), default=float('nan'))
    print(f'| `{k}` | ' + ' | '.join(cells) + ' | ' + ' | '.join(ratios) + f' | {sp:.1%} |')
