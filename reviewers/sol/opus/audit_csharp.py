"""Compile generated bindings offline and compare them with the built exports."""
from pathlib import Path
import json
import os
import re
import shutil
import subprocess
import sys

base = Path(__file__).resolve().parent
if "--both" in sys.argv[1:]:
    for target in ("opus", "astra"):
        subprocess.run([sys.executable, str(base.parent/target/"audit_csharp.py")] + [arg for arg in sys.argv[1:] if arg != "--both"], check=True)
    raise SystemExit(0)
baseline = "--baseline" in sys.argv[1:]
suffix = "-before" if baseline else ""
release = base / ("work-before" if baseline else "work") / "target" / "release"
generated = release / "RngNative.cs"
exports = subprocess.check_output(["llvm-readobj", "--coff-exports", str(release/"urng.dll")], text=True)
(base / "logs" / ("dll-exports"+suffix+".log")).write_text(exports, encoding="utf-8")
names = set(re.findall(r"\bName: (\w+)", exports))
imports = set(re.findall(r'EntryPoint = "(\w+)"', generated.read_text(encoding="utf-8")))
result = {"native_exports":len(names),"generated_imports":len(imports),"missing_in_csharp":sorted(names-imports),"missing_in_dll":sorted(imports-names)}
(base / ("csharp-inventory"+suffix+".json")).write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
project = base / ("work-csharp"+suffix)
project.mkdir(exist_ok=True)
shutil.copy2(generated, project/"RngNative.cs")
(project / "review.csproj").write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework><AllowUnsafeBlocks>true</AllowUnsafeBlocks><EnableNETAnalyzers>false</EnableNETAnalyzers></PropertyGroup></Project>', encoding="utf-8")
(project / "NuGet.Config").write_text('<configuration><packageSources><clear /></packageSources></configuration>',encoding="utf-8")
env = os.environ.copy()
env["DOTNET_CLI_HOME"] = str(project)
env["DOTNET_SKIP_FIRST_TIME_EXPERIENCE"] = "1"
env["DOTNET_CLI_TELEMETRY_OPTOUT"] = "1"
env["DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE"] = "1"
env["NUGET_PACKAGES"] = str(project/"packages")
with (base / "logs" / ("csharp-build"+suffix+".log")).open("w",encoding="utf-8") as log:
    p = subprocess.run(["dotnet","build","review.csproj","--nologo","--configfile","NuGet.Config"],cwd=project,env=env,stdout=log,stderr=subprocess.STDOUT)
print(json.dumps({"native_exports":len(names),"generated_imports":len(imports),"missing_in_csharp":len(names-imports),"missing_in_dll":len(imports-names),"csharp_build_exit":p.returncode}),flush=True)
raise SystemExit(p.returncode)
