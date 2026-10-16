from pathlib import Path
import hashlib
import json
import os
import platform
import subprocess

base = Path(__file__).resolve().parent
root = base.parents[2]
for target in ("opus", "astra"):
    output = base.parent / target
    source = root / ("urng-review_"+target)
    hashes = {}
    for p in sorted(source.joinpath("src").rglob("*.rs")):
        hashes[p.relative_to(source).as_posix()] = hashlib.sha256(p.read_bytes()).hexdigest()
    for name in ("Cargo.toml","build.rs","README.md"):
        hashes[name] = hashlib.sha256((source/name).read_bytes()).hexdigest()
    (output/"reviewed-source-sha256.json").write_text(json.dumps(hashes,indent=2)+"\n",encoding="utf-8")
    versions = {}
    for name, command in [("rustc",["rustc","-Vv"]),("cargo",["cargo","-V"]),("dotnet",["dotnet","--list-sdks"]),("native_cfg",["rustc","--print","cfg","-C","target-cpu=native"])]:
        versions[name] = subprocess.check_output(command,text=True).strip()
    versions.update({"date":"2026-10-16","os":platform.platform(),"cpu":"AMD Ryzen 7 9800X3D 8-Core Processor","logical_processors":os.cpu_count(),"build_flags":"default target CPU; no RUSTFLAGS; release opt=3 lto=fat codegen-units=1 panic=abort","reviewer_assignment":"Sol","coauthor_requested_by_user":"GPT-6.1 Sol <noreply@openai.com>"})
    (output/"environment.json").write_text(json.dumps(versions,indent=2)+"\n",encoding="utf-8")
    for log in (output/"logs").glob("*.log"):
        raw = log.read_bytes()
        if raw.startswith((b"\xff\xfe",b"\xfe\xff")):
            content = raw.decode("utf-16")
        else:
            content = raw.decode("utf-8")
        # Normalize transport line endings, retaining every output line and
        # every test result. Avoid Windows write_text adding a second CR.
        content = content.replace("\r\r\n", "\n").replace("\r\n", "\n")
        content = "\n".join(line.rstrip() for line in content.split("\n")).rstrip() + "\n"
        if raw != content.encode("utf-8"):
            log.write_text(content,encoding="utf-8",newline="\n")
    print(target,"recorded",len(hashes),"source hashes")
