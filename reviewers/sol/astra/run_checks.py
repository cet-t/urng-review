"""Run review commands with reproducible UTF-8 logs, preserving failing tests."""
from pathlib import Path
import json
import shutil
import subprocess
import sys

base = Path(__file__).resolve().parent
work = base / "work"
mode = sys.argv[1] if len(sys.argv) > 1 else "probes"
if not work.exists():
    raise SystemExit("Run prepare.py first")
if (base / "probes.rs").exists():
    shutil.copy2(base / "probes.rs", work / "tests" / "sol_review.rs")
if (base / "cipher_probes.rs").exists():
    shutil.copy2(base / "cipher_probes.rs", work / "tests" / "sol_cipher_review.rs")
    (work / "tests" / "support").mkdir(exist_ok=True)
    shutil.copy2(base / "cipher_reference.rs", work / "tests" / "support" / "sol_cipher_reference.rs")
if mode == "rand" and (base / "rand64_probe.rs").exists():
    shutil.copy2(base / "rand64_probe.rs", work / "tests" / "sol_rand64_review.rs")
elif (work / "tests" / "sol_rand64_review.rs").exists():
    (work / "tests" / "sol_rand64_review.rs").unlink()
source = work / "src" / "wide" / "threefry.rs"
original = base.parents[2] / ("urng-review_" + base.name) / "src" / "wide" / "threefry.rs"
if "mod sol_counter_probes" in source.read_text(encoding="utf-8"):
    source.write_bytes(original.read_bytes())
if mode == "counters":
    content = source.read_text(encoding="utf-8")
    if "mod sol_counter_probes" not in content:
        source.write_text(content + "\n" + (base / "counter_probes.rs").read_text(encoding="utf-8"), encoding="utf-8")
commands = {
    "probes": [("probes", ["cargo", "test", "--offline", "--features", "full", "--test", "sol_review", "--", "--nocapture"])],
    "existing": [("existing-full", ["cargo", "test", "--offline", "--lib", "--features", "full"] + [arg for p in sorted((work / "tests").glob("*.rs")) if not p.name.startswith("sol_") for arg in ("--test", p.stem)])],
    "features": [("feature-" + (feature or "default"), ["cargo", "check", "--offline", "--lib"] + (["--features", feature] if feature else [])) for feature in ("", "rand", "simd", "cabi", "wide", "sampler", "seedgen", "cabi,simd", "full")],
    "docs": [("doctests", ["cargo", "test", "--offline", "--doc", "--features", "full"])],
    "alltargets": [("alltargets", ["cargo", "check", "--offline", "--all-targets", "--features", "full"])],
    "build": [("build-release", ["cargo", "build", "--offline", "--release", "--features", "full"])],
    "counters": [("counter-probes", ["cargo", "test", "--offline", "--features", "wide", "--lib", "sol_counter_probes", "--", "--nocapture"])],
    "ciphers": [("cipher-probes", ["cargo", "test", "--offline", "--test", "sol_cipher_review", "--", "--nocapture"])],
    "rand": [("rand64-probe", ["cargo", "test", "--offline", "--features", "rand", "--test", "sol_rand64_review"])],
}
commands["quality"] = commands["docs"] + commands["alltargets"] + commands["build"]
results = []
for name, command in commands[mode]:
    log_path = base / "logs" / (name + ".log")
    with log_path.open("w", encoding="utf-8") as log:
        log.write("COMMAND: " + subprocess.list2cmdline(command) + "\n")
        log.flush()
        process = subprocess.run(command, cwd=work, stdout=log, stderr=subprocess.STDOUT)
        log.write(f"\nEXIT: {process.returncode}\n")
    result = {"name": name, "command": command, "exit": process.returncode}
    results.append(result)
    print(json.dumps(result), flush=True)
(base / "logs" / (mode + "-summary.json")).write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
if mode == "counters":
    source.write_bytes(original.read_bytes())
if mode == "rand":
    (work / "tests" / "sol_rand64_review.rs").unlink(missing_ok=True)
raise SystemExit(1 if any(result["exit"] != 0 for result in results) else 0)
