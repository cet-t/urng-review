# urng 1.0.0 修正版レビュー — Sol 修正版（レビュアー: Opus 5.5）

> **状態: 作業中（WIP）**。本ファイルは進行に合わせて随時更新する。

## 1. 総合評価

- 対象モデル: Sol
- 対象ディレクトリ: `urng-review_sol/`
- 対象コミット: `8417b4b`（修正コミット `383625a`、ベース `386eb4d` = urng 1.0.0）
- レビュー実施環境: Windows 11 Pro 10.0.26200 / AMD Ryzen 7 9800X3D（Zen 5, AVX2 + AVX-512F/VL/DQ/BW 対応）/ rustc 1.98.1 stable (x86_64-pc-windows-msvc), nightly-2026-10-08 / Python 3.13.15 / gcc (Strawberry) / clang (LLVM) / .NET
- 結論の要約: （調査中）
- 重大な問題の件数: （調査中）
- 検証範囲: （調査中）

## 2. 発見した問題（暫定）

### SOL-01 [High] SplitMix32/64 の `seed | 1` が残存し、ほぼ全生成器でシード 2k と 2k+1 が同一系列になる
- 対象: `src/prng/b32/splitmix.rs:28-32`（`SplitMix32::new`）、`src/prng/b64/splitmix.rs:22-24`（`SplitMix64::new`）、`src/prng/b64/xoroshiro.rs:23`（`SplitMix64::new(seed | 1)`）、`src/wide/pcg.rs:33` / `src/wide/squares.rs:39` / `src/wide/splitmix.rs:33`、`src/prng/b32/splitmix.rs` の `SplitMix32x16::new`（`(seed | 1)`）
- 原因: 1.0.0 の `seed | 1` がシード展開器に残っているため、最下位ビットのみ異なるシードが同じ内部状態に写像される。
- 再現: `harness/common/probe.rs seed_pairs`（`out/probe-seed_pairs-sol.log`）。Rust API の 35 型すべて、wide 型、C ABI（`xoshiro128pp_new(2)` と `(3)`）で `new(2)==new(3)` が真。
- 状態: 実証済み

### SOL-02 [Medium] C ABI シンボル `sfmt_rand_f64s` の削除（ABI 互換性の破壊）
- 1.0.0 は `sfmt_rand_f64s` をエクスポートしていたが、Sol は `sfmt1993764_rand_f64s` に改名し旧シンボルを残していない。実 DLL のエクスポート表比較で確認。
- 状態: 実証済み（`llvm-objdump -p` によるエクスポート比較）

### SOL-03 [Low] C# バインディングに 54 関数が欠落（sfc32*, xoroshiro64ss*, biski64*）
- `build.rs` の csbindgen 入力に該当ファイルが無い。生成された `RngNative.cs` と DLL エクスポートの差分で確認。
- 状態: 実証済み

（以下調査中）

## 3. 既存修正の評価

（独立レビュー完了後に記入。FIX_REPORT.md は独立レビュー完了まで未参照）

## 4. 性能評価

（未測定）

## 5. 実行した検証（途中経過）

| 検証 | 結果 |
| --- | --- |
| `cargo test --all-features --locked`（スクラッチコピー） | 全成功（lib 161, cabi_regression 59, cipher 4, numeric 13, sampler_rand 4, doctest 107+1） |
| 独立参照実装の公式ベクタ照合 `ref/check_vectors.py` | 62/62 成功（MT19937/-64, SFMT 全10パラメータ, Random123 KAT, Threefish-256 KAT, ChaCha20 RFC8439, Salsa20/8 RFC7914 + OpenSSL scrypt 照合, PCG32 demo, xoroshiro128 jump の GF(2) 行列冪照合） |
| 公式 xoshiro/xoroshiro C ソースとの照合 `ref/check_xoshiro_c.py` | 全成功 |
| 生成系列ダンプ照合 `common/dump.rs` + `ref/compare_dump.py` | 793/793 系列一致（スカラー 47 型、SIMD 17 型の全レーン、wide 38 型の全レーン、jump/long_jump） |
| C ABI 照合 `common/cabi.rs` | 5,544 チェック全成功（56 ファミリ。スカラー fill は呼び出し境界非依存、SIMD fill はブロック境界で再開する仕様どおり） |

## 6. 総括

（未記入）

## 作業ログ（再開用）

- 原本は `git archive` でハーネス内 `_src/`（対象 HEAD）・`_base/`（1.0.0）へ展開してビルド（`harness/setup.ps1 -Target sol`）。原本ディレクトリでは一切ビルドしない。
- ベース `386eb4d` の `src/` ツリーは Astra の `19bc224` と同一（ツリーハッシュ `ef46088…`）。
- 参照ベクタは `harness/vectors/`（入手元 URL は `ref/check_vectors.py` 冒頭に記載）。
