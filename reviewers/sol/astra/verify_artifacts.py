"""Validate saved review evidence without rerunning the target implementations."""
from pathlib import Path
import ast
import csv
import hashlib
import json
import re
import subprocess

base = Path(__file__).resolve().parent
source = base.parents[2] / ("urng-review_" + base.name)
expected = {"opus":(6,196,10,10,7,0),"astra":(9,190,9,11,5,2)}[base.name]
issues,existing_count,probe_ok,probe_fail,cipher_ok,cipher_fail = expected
report = (base/"REVIEW.md").read_text(encoding="utf-8")
headings = re.findall(r"^### SOL-[A-Z]+-\d+ — (High|Medium):",report,re.M)
assert len(headings) == issues, headings
assert len(re.findall(r"^## [1-6]\. ",report,re.M)) == 6
for p in base.glob("*.py"):
    ast.parse(p.read_text(encoding="utf-8"),filename=str(p))
hashes = json.loads((base/"reviewed-source-sha256.json").read_text(encoding="utf-8"))
for name,sha in hashes.items():
    assert hashlib.sha256((source/name).read_bytes()).hexdigest() == sha,name
    assert hashlib.sha256((base/"work"/name).read_bytes()).hexdigest() == sha,"work source differs: "+name
def results(log_name):
    content = (base/"logs"/log_name).read_text(encoding="utf-8")
    values = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed",content)
    return tuple(sum(int(row[i]) for row in values) for i in (0,1))
assert results("existing-full.log") == (existing_count,0)
assert results("probes.log") == (probe_ok,probe_fail)
assert results("cipher-probes.log") == (cipher_ok,cipher_fail)
assert results("counter-probes.log") == (0,2)
for version in ("before","after"):
    with (base/f"benchmark-{version}.csv").open(encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    assert len(rows) == 65
    assert len({(row["case"],row["elements"]) for row in rows}) == 13
status = subprocess.check_output(["git","-C",str(source),"status","--porcelain"],text=True)
assert not status,"original has changed: "+status
summary = {"issues":issues,"severity":{"High":headings.count("High"),"Medium":headings.count("Medium")},"existing_tests":results("existing-full.log"),"probe_tests":results("probes.log"),"counter_tests":results("counter-probes.log"),"cipher_tests":results("cipher-probes.log"),"source_files_unchanged":len(hashes),"original_git_status":"clean","benchmark_rows_each_version":65,"benchmark_conditions":13,"python_syntax":"all valid"}
(base/"verification.json").write_text(json.dumps(summary,indent=2)+"\n",encoding="utf-8")
print(json.dumps(summary),flush=True)
