"""Copy review inputs without modifying originals or reading other reports."""
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[3]
for target in (Path(__file__).resolve().parent.name,):
    source = root / f"urng-review_{target}"
    output = root / "reviewers" / "sol" / target
    output.mkdir(parents=True, exist_ok=True)
    work = output / "work"
    work.mkdir(exist_ok=True)
    for name in ("src", "tests", "examples"):
        shutil.copytree(source / name, work / name, dirs_exist_ok=True)
    for name in ("Cargo.toml", "Cargo.lock", "build.rs", "README.md"):
        if (source / name).exists():
            shutil.copy2(source / name, work / name)
    if (output / "Cargo.review.lock").exists():
        shutil.copy2(output / "Cargo.review.lock", work / "Cargo.lock")
    revision = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    (output / "source-revision.txt").write_text(revision + "\n", encoding="utf-8")
    (output / "logs").mkdir(exist_ok=True)
    print(target, revision, work)
