"""Summarize this review's retained logs without consulting other reviewers."""
from pathlib import Path
import json
import re
import statistics

HERE=Path(__file__).resolve().parent
def bench(name):
    rows={}
    for line in (HERE/'logs'/name).read_text(encoding='utf-8').splitlines():
        if re.match(r'^\w+,\d+,\d+,[\d.]+$',line):
            case,n,trial,value=line.split(',')
            rows.setdefault(case,[]).append(float(value))
    return {k:{'median':statistics.median(v),'min':min(v),'max':max(v),'trials':len(v)} for k,v in rows.items()}

current=bench('bench-native.log');baseline=bench('baseline-bench-native.log')
output={'benchmark':{k:{'current':v,'baseline':baseline[k],'time_ratio':v['median']/baseline[k]['median']} for k,v in current.items()},'tests':{}}
for name in ('existing-full','existing-native','independent-final'):
    text=(HERE/'logs'/(name+'.log')).read_text(encoding='utf-8')
    matches=re.findall(r'test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;',text)
    output['tests'][name]={'passed':sum(int(a)for a,b,c in matches),'failed':sum(int(b)for a,b,c in matches),'ignored':sum(int(c)for a,b,c in matches)}
(HERE/'logs'/'summary.json').write_text(json.dumps(output,indent=2),encoding='utf-8')
print(json.dumps(output['tests']))
for k,v in output['benchmark'].items():
    print(f"| {k} | {v['baseline']['median']:.4f} | {v['current']['median']:.4f} | {v['time_ratio']:.2f}x |")
