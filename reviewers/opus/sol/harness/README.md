# Opus 5.5 review harness (shared by the Sol and Astra reviews)

Everything here is review tooling; nothing is built inside the original
`urng-review_*` submodules.

## Setup

```powershell
powershell -File setup.ps1 -Target sol      # or astra; exports HEAD -> _src, 1.0.0 -> _base
```

## Reference implementation (independent of the reviewed code)

```powershell
python -I ref/check_vectors.py                       # 62 official-vector checks
python -I ref/check_xoshiro_c.py <dl_dir> <build>    # official prng.di.unimi.it C sources (+ f2x.c)
```

`ref/refimpl.py` is written from the published specifications; `vectors/` holds
the official outputs (sources listed at the top of `check_vectors.py`).

## Rust probes (run-target = reviewed tree, run-base = urng 1.0.0)

```powershell
cd run-target
cargo build --release --features v_sol   # or v_astra
target/release/dump.exe ../out/dump-sol.txt; python -I ../ref/compare_dump.py ../out/dump-sol.txt sol
target/release/cabi.exe                  # C ABI equivalence / edge cases
target/release/probe.exe <seed_pairs|xorshift32_zero|alias_small|cipher_exhaust|threefry_wrap|addr_offsets>
target/release/numeric.exe <randi_enum|randi64_edge|randf_edge|index|shuffle|sampler|simdconv|randcore>
target/release/bench.exe [filter]        # also: cargo build --profile lto (fat LTO, cgu=1)
```

`ref/bench_table.py out <variants...>` builds the tables in `out/bench-table*.md`.
`build_matrix.ps1 -Tree _src -Log out/build-matrix-<x>.log` runs the 64 feature
combinations, doctests and nightly `-Zbuild-std` cross checks.
`ref/evex_scan.py <urng.dll> <export-prefix>...` statically scans for AVX-512
(EVEX) instructions reachable from C ABI entry points.
