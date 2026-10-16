"""Build historical source and current copy; measure sequentially, never in parallel."""
from pathlib import Path
import csv
import io
import json
import os
import shutil
import statistics
import subprocess
import sys
import tarfile

base = Path(__file__).resolve().parent
root = base.parents[2]
target = base.name
source = root / f"urng-review_{target}"
baseline = {"opus": "4068051", "astra": "19bc224"}[target]

def run(name, command, cwd):
    with (base / "logs" / (name + ".log")).open("w", encoding="utf-8") as log:
        log.write("COMMAND: " + subprocess.list2cmdline(command) + "\n")
        log.flush()
        p = subprocess.run(command, cwd=cwd, stdout=log, stderr=subprocess.STDOUT)
        log.write(f"\nEXIT: {p.returncode}\n")
    print(name, p.returncode, flush=True)
    return p.returncode

work_before = base / "work-before"
work_before.mkdir(exist_ok=True)
if sys.argv[1:] == ["prepare"]:
    archive = subprocess.check_output(["git", "-C", str(source), "archive", baseline, "Cargo.toml", "build.rs", "README.md", "src"])
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        tar.extractall(work_before, filter="data")
    shutil.copy2(base / "work" / "Cargo.lock", work_before / "Cargo.lock")
    (base / "baseline-revision.txt").write_text(subprocess.check_output(["git", "-C", str(source), "rev-parse", baseline], text=True), encoding="utf-8")
    raise SystemExit(run("baseline-build", ["cargo", "build", "--offline", "--release", "--features", "full"], work_before))

(base / "bin").mkdir(exist_ok=True)
for version, work in [("before",work_before), ("after",base/"work")]:
    release = work / "target" / "release"
    executable = base / "bin" / f"bench-{version}.exe"
    command = ["rustc", "--edition=2024", "-O", "-C", "panic=abort", "-A", "unused_unsafe", str(base/"bench.rs"), "--extern", f"urng={release/'urng.rlib'}", "-L", f"dependency={release/'deps'}", "-o", str(executable)]
    # Cargo's Windows rlib is named urng.rlib, Unix uses liburng.rlib.
    if not (release/"urng.rlib").exists():
        command[command.index("--extern")+1] = f"urng={release/'liburng.rlib'}"
    if run("benchmark-compile-"+version, command, work):
        raise SystemExit(1)
    with (base / f"benchmark-{version}.csv").open("w", encoding="utf-8") as output:
        p = subprocess.run([str(executable)], cwd=work, stdout=output, stderr=subprocess.PIPE, text=True)
    if p.returncode:
        raise SystemExit(p.stderr)
    print("benchmark finished", version, flush=True)

data = {}
for version in ("before", "after"):
    with (base / f"benchmark-{version}.csv").open(encoding="utf-8") as f:
        for row in csv.DictReader(f):
            key = (row["case"],int(row["elements"]))
            data.setdefault(key, {}).setdefault(version, []).append(float(row["ns_per_word"]))
with (base / "benchmark-comparison.csv").open("w", newline="", encoding="utf-8") as f:
    writer = csv.writer(f)
    writer.writerow(["case","elements","before_median_ns","after_median_ns","after_over_before_time","before_min_ns","before_max_ns","after_min_ns","after_max_ns"])
    for (case,n), versions in data.items():
        b,a = versions["before"], versions["after"]
        bm,am = statistics.median(b),statistics.median(a)
        writer.writerow([case,n,bm,am,am/bm,min(b),max(b),min(a),max(a)])
metadata = {"baseline":baseline,"target":target,"seed":1,"repetitions":5,"profile":"release opt=3 lto=fat codegen-units=1, harness rustc -O, default target CPU","processors":os.cpu_count(),"processor":os.environ.get("PROCESSOR_IDENTIFIER"),"execution":"before and after sequential, no concurrent benchmark, no CPU pinning"}
(base / "benchmark-metadata.json").write_text(json.dumps(metadata,indent=2)+"\n",encoding="utf-8")
