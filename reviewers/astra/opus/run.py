"""Review harness; all writes stay below this file's directory.

python run.py prepare
python run.py LABEL cargo test --offline --all-features
The cargo working directory is the tracked-source copy in work/source.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.stdout.reconfigure(encoding='utf-8', errors='replace')

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SOURCE = ROOT / ('urng-review_' + HERE.name)
COPY = HERE / 'work' / 'source'

def prepare():
    paths = subprocess.check_output(['git', '-C', str(SOURCE), 'ls-files', '-z']).decode().split('\0')
    for name in paths:
        if not name or not (name.startswith(('src/', 'tests/', 'examples/', '.cargo/')) or name in ('Cargo.toml', 'Cargo.lock', 'build.rs', 'README.md')):
            continue
        dest = COPY / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(SOURCE / name, dest)
    revision = subprocess.check_output(['git', '-C', str(SOURCE), 'rev-parse', 'HEAD'], text=True).strip()
    (HERE / 'logs').mkdir(exist_ok=True)
    (HERE / 'logs' / 'revision.txt').write_text(revision + '\n', encoding='utf-8')
    print('Prepared', COPY, revision)

def run(label, command):
    env = os.environ.copy()
    env['DOTNET_CLI_HOME'] = str(HERE / 'work' / 'dotnet-home')
    env['DOTNET_SKIP_FIRST_TIME_EXPERIENCE'] = '1'
    env['DOTNET_CLI_TELEMETRY_OPTOUT'] = '1'
    env['CARGO_TARGET_DIR'] = str(HERE / 'target' / 'baseline') if COPY.name == 'baseline' else str(HERE / 'target')
    log = HERE / 'logs' / (label + '.log')
    log.parent.mkdir(exist_ok=True)
    start = time.time()
    with log.open('w', encoding='utf-8') as f:
        f.write('cwd: ' + str(COPY) + '\ncommand: ' + json.dumps(command) + '\n')
        f.flush()
        p = subprocess.run(command, cwd=COPY, env=env, stdout=f, stderr=subprocess.STDOUT)
        f.write(f'\nEXIT={p.returncode} ELAPSED={time.time()-start:.3f}s\n')
    print(log.read_text(encoding='utf-8', errors='replace')[-9000:])
    return p.returncode

def install():
    for name in ('astra_review.rs', 'adapter.rs'):
        shutil.copyfile(HERE / name, COPY / 'src' / name)
    lib = COPY / 'src' / 'lib.rs'
    text = lib.read_text(encoding='utf-8')
    marker = '\n#[cfg(test)]\nmod astra_review;\n'
    if marker not in text:
        lib.write_text(text + marker, encoding='utf-8')
    # Test-only state injection for the 64-bit carry boundary. No production
    # method is changed, and no original repository file is written.
    if HERE.name == 'opus':
        salsa = COPY / 'src' / 'crng' / 'b32' / 'salsa.rs'
        text = salsa.read_text(encoding='utf-8')
        if 'fn astra_set_block_counter' not in text:
            text += '\n#[cfg(test)]\nimpl<const R: usize> Salsa<R> { pub(crate) fn astra_set_block_counter(&mut self, c: u64) { self.x[8] = wrap!(c as u32); self.x[9] = wrap!((c >> 32) as u32); self.pos = wrap!(16); } }\n'
            salsa.write_text(text, encoding='utf-8')
    tf = COPY / 'src' / 'wide' / 'threefry.rs'
    text = tf.read_text(encoding='utf-8')
    if 'fn astra_set_counter' not in text:
        for words in (2, 4):
            text += f'\n#[cfg(test)]\nimpl Threefry32x{words}x4 {{ pub(crate) fn astra_set_counter(&mut self, c: u32) {{ self.c0 = u32x4::splat(c); self.pos = {words}; }} }}\n'
        tf.write_text(text, encoding='utf-8')
    print('Installed review-only test module in copy')

def benchmark_install():
    dest = COPY / 'examples' / 'astra_bench.rs'
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(HERE / 'astra_bench.rs', dest)

def csharp_prepare():
    folder = HERE / 'work' / 'csharp'
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'binding_check.csproj').write_text('''<Project Sdk="Microsoft.NET.Sdk">
<PropertyGroup><TargetFramework>net10.0</TargetFramework><AllowUnsafeBlocks>true</AllowUnsafeBlocks><EnableDefaultCompileItems>false</EnableDefaultCompileItems><NuGetAudit>false</NuGetAudit></PropertyGroup>
<ItemGroup><Compile Include="../source/target/release/RngNative.cs" /></ItemGroup>
</Project>''', encoding='utf-8')

def baseline_prepare():
    revision = '4068051' if HERE.name == 'opus' else '386eb4d'
    paths = subprocess.check_output(['git','-C',str(SOURCE),'ls-tree','-r','--name-only',revision], text=True).splitlines()
    for name in paths:
        if name.startswith(('src/', 'examples/')) or name in ('Cargo.toml','Cargo.lock','README.md','build.rs'):
            dest = COPY / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(subprocess.check_output(['git','-C',str(SOURCE),'show',revision+':'+name]))
    benchmark_install()
    print('Prepared baseline', revision, COPY)

if __name__ == '__main__':
    if sys.argv[1].startswith('baseline-'):
        COPY = HERE / 'work' / 'baseline'
    if sys.argv[1] == 'prepare':
        prepare()
    elif sys.argv[1] == 'baseline-prepare':
        baseline_prepare()
    elif sys.argv[1] == 'benchmark-install':
        benchmark_install()
    elif sys.argv[1] == 'csharp-prepare':
        csharp_prepare()
    elif sys.argv[1] == 'install':
        install()
    elif sys.argv[1] == 'matrix':
        results = {}
        for features in ('', 'rand', 'cabi', 'simd', 'wide', 'sampler', 'seedgen', 'cabi,simd', 'full'):
            cmd = ['cargo', 'check', '--offline', '--lib', '--no-default-features']
            if features:
                cmd += ['--features', features]
            results[features or 'default'] = run('check-' + (features or 'default').replace(',', '-'), cmd)
        print(json.dumps(results))
    else:
        sys.exit(run(sys.argv[1], sys.argv[2:]))
