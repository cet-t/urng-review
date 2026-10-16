# urng 1.0.0 修正版レビュー — Astra 修正版（レビュアー: Opus 5.5）

> **状態: 作業中（WIP）**。本ファイルは進行に合わせて随時更新する。

## 1. 総合評価

- 対象モデル: Astra
- 対象ディレクトリ: `urng-review_astra/`
- 対象コミット: `cb07a9a`（修正コミット `3379c33`、ベース `19bc224` = urng 1.0.0 + phase 1 REVIEW.md）
- レビュー実施環境: Windows 11 Pro 10.0.26200 / AMD Ryzen 7 9800X3D（Zen 5, AVX2 + AVX-512F/VL/DQ/BW 対応）/ rustc 1.98.1 stable (x86_64-pc-windows-msvc), nightly-2026-10-08 / Python 3.13.15 / gcc (Strawberry) / clang (LLVM) / .NET
- 結論の要約: （調査中）
- 重大な問題の件数: （調査中）
- 検証範囲: （調査中）

## 2. 発見した問題（暫定）

### AST-01 [Medium] Xorshift32 系がゼロ状態に落ちるシードが存在する（未修正）
- 対象: `src/prng/b32/xorshift.rs:27-32`（`Xorshift32::new`）、同 `Xorshift32x8::new`、`src/wide/xorshift.rs`（`Xorshift32xN::new`）、C ABI `xorshift32_new`
- 原因: 状態を SplitMix32 の出力そのままで初期化し、0 を除外していない。Astra は `SplitMix32::new` から `| 1` を除いた一方、Xorshift64 にだけゼロ回避を追加した。
- 再現: `harness/common/probe.rs xorshift32_zero`（全 2^32 シードを総当たり）。`Xorshift32::new(0x61C88647)`、`(0xF7AA06C3)`、`(0xD7BDEF1B)` が永久に 0 を出力。C ABI と `wide::Xorshift32x4` のレーン0でも同様。
- 状態: 実証済み

### AST-02 [Low] `TwistedGFSR::new(u64)` がシード上位32ビットを無視
- `TwistedGFSR::new(5) == TwistedGFSR::new(5 | 0xDEADBEEF<<32)`。ドキュメントに注記はあるが 64 ビットシード API として衝突。
- 状態: 実証済み

### AST-03 [Low] `wide::Squares32x{4,8}` に `seed | 1` が残存（2k と 2k+1 が同一系列）
- 他の生成器では `| 1` を除去しており不整合。
- 状態: 実証済み

（以下調査中）

## 3. 既存修正の評価

（独立レビュー完了後に記入。FIX_REPORT.md は独立レビュー完了まで未参照）

## 4. 性能評価

（未測定）

## 5. 実行した検証（途中経過）

| 検証 | 結果 |
| --- | --- |
| `cargo test --all-features --locked`（スクラッチコピー） | 全成功（lib 114, algorithms 8, cabi 59, packaging 1, ranges 6, simd_conversions 2, doctest 104+1） |
| 独立参照実装の公式ベクタ照合 `ref/check_vectors.py` | 62/62 成功 |
| 公式 xoshiro/xoroshiro C ソースとの照合 | 全成功 |
| 生成系列ダンプ照合 `common/dump.rs` + `ref/compare_dump.py` | 793/793 系列一致（Astra のシード方式。ただし Threefry32x4 / Threefish256 / wide Threefry32x4 は 1.0.0 の tweak・XOR 付き変種として一致） |
| C ABI 照合 `common/cabi.rs` | 4,982 チェック全成功（全ファミリで呼び出し境界非依存の連続ストリーム） |

## 6. 総括

（未記入）

## 作業ログ（再開用）

- 原本は `git archive` でハーネス内 `_src/`・`_base/` へ展開してビルド（`harness/setup.ps1 -Target astra`）。
- ベース `19bc224` の `src/` は Sol のベース `386eb4d`（urng 1.0.0）と同一。
