# urng 1.0.0 修正版レビュー — Sol 修正版（レビュアー: Opus 5.5）

> 状態: **独立レビュー部分（§1, §2, §4, §5, §6）完了。§3 は独立レビュー完了後に Sol の REVIEW.md / FIX_REPORT.md を参照して記入。**

## 1. 総合評価

| 項目 | 内容 |
| --- | --- |
| 対象モデル | Sol |
| 対象ディレクトリ | `urng-review_sol/` |
| 対象コミット | `8417b4b`（修正本体 `383625a`、文書 `ece9e31`、ベース `386eb4d` = urng 1.0.0） |
| 実施環境 | Windows 11 Pro 10.0.26200 / AMD Ryzen 7 9800X3D（Zen 5、AVX2・AVX-512F/VL/DQ/BW 対応）/ 64 GB / rustc 1.98.1 stable（x86_64-pc-windows-msvc）, nightly-2026-10-08（`-Zbuild-std` のクロスチェック用）/ Python 3.13.15 / gcc（Strawberry）/ LLVM 22（llvm-objdump） |
| 検証方法 | 原本を `git archive` でハーネス内へ展開してビルド（原本ディレクトリでは一切ビルドしていない）。対象コードから独立した参照実装（Python、公式ベクタで事前検証）と公式 C ソースで系列を照合 |

### 結論の要約

- **アルゴリズムの正確性は高い。** 独立参照実装（MT19937/-64、SFMT 全10パラメータ、Random123 Philox/Threefry、Skein Threefish-256、RFC 8439 ChaCha20、Salsa20、PCG32、xoshiro/xoroshiro 公式 C、jump/long_jump の GF(2) 行列冪）と照合し、スカラー 47 型・SIMD 17 型の全レーン・wide 38 型の全レーンで **793/793 系列が一致**。1.0.0 の誤実装（Philox の乗算鍵更新、Threefry の tweak/XOR、ChaCha/Salsa の非標準実装、xoroshiro128++ の式、SFC64、Biski64x8、xoroshiro64**x8/x16、long_jump 等）はすべて正しく修正されている。
- **メモリ安全性も大きく改善。** AVX2 型から AVX-512VL 命令（`vprold` 等）が機械語レベルで到達しないことを確認、`Xoshiro256Ssx2` のスタック破壊を解消、C ABI はポインタ・整列・重なり・長さを検証し `MaybeUninit` で書き込む。C ABI 5,544 チェックで不正入力時も安全に no-op。非 x86_64（aarch64 / i686 / big-endian ppc64）でも全 feature でビルド可能。
- **一方で、1.0.0 のシード展開の欠陥 `seed | 1` が残っており、ほぼ全生成器でシード 2k と 2k+1 が同一系列になる（SOL-01, High）。** 並列シミュレーションで連番シードを使うと系列が重複する。これは本修正版の採用を妨げる最大の問題。
- 性能面では C ABI の一括生成が 1.0.0（並列・別系列）比で 4〜21 倍遅く、SIMD 型の一部（`Biski64x8`/`Sfc64x8`/`Xoshiro128Ppx16`、C ABI の SIMD 経路）はスカラー版より遅い。正確性のための必要コストと最適化不足が混在する（§4）。
- C ABI シンボル `sfmt_rand_f64s` の削除（ABI 破壊）、C# バインディングの欠落など互換性面の課題がある。

### 重大な問題の件数

| 重要度 | 件数 | ID |
| --- | --- | --- |
| Critical | 0 | — |
| High | 1 | SOL-01 |
| Medium | 3 | SOL-02, SOL-03, SOL-04 |
| Low | 8 | SOL-05 〜 SOL-12 |
| Info（未検証の懸念を含む） | 2 | SOL-13, SOL-14 |

### 検証できた範囲 / 未検証範囲

- 検証済み: 全スカラー・SIMD・wide 生成器の出力系列（独立参照）、C ABI 全 56 ファミリ（Rust API 系列との一致・呼び出し境界・不正入力）、`randi`（2^32 語の総当たりで厳密一様性）、`randf` 半開区間、`index`、shuffle 分布、サンプラー（ゼロ・極端値・不正値・分布）、rand_core 連携、全 64 feature 組み合わせ、非 x86_64 クロスチェック、AVX2 経路の機械語検査、性能（同一環境・同一ハーネスで base/Sol/Astra を比較）。
- 未検証: AVX2 のみ / AVX-512 非搭載 CPU での実行（実機・エミュレータなし。機械語の静的検査で代替）、big-endian での実行時動作（クロスコンパイルのみ）、Miri（コンポーネント未導入。ツールチェーンを変更しないため未実施）、C# バインディングの実行時呼び出し（生成物とエクスポート表の照合のみ）、統計検定スイート（PractRand/TestU01 等）。

## 2. 発見した問題

### SOL-01 [High] SplitMix32/64 の `seed | 1` が残存し、シード 2k と 2k+1 がほぼ全生成器で同一系列になる

- **対象**: `src/prng/b32/splitmix.rs:30`（`SplitMix32::new` の `seed | 1`）、`src/prng/b64/splitmix.rs:23`（`SplitMix64::new`）、`src/prng/b64/xoroshiro.rs:23`、`src/prng/b32/splitmix.rs:105`（`SplitMix32x16::new`）、`src/wide/splitmix.rs:33`、`src/wide/pcg.rs:33`、`src/wide/squares.rs:39`
- **技術的原因**: 全生成器のシード展開器である SplitMix32/64 が状態を `seed | 1` で初期化するため、最下位ビットが異なるだけのシードが同一状態へ写像される。SplitMix の Weyl 増分は奇数なので状態を奇数にする必要はなく、`| 1` は単に 1 ビット分のシード空間を捨てている。`sm64_from_seed32!` 経由（Pcg32, Sfmt*, Squares32）も同様。
- **影響範囲**: SplitMix32/64, Xoshiro128/256, Xoroshiro64/128, Jsf32, Sfc32/64, Xorshift32/64/128, Xorwow, Mt19937/Mt1993764, Sfmt* 全種, Pcg32, Philox32/64/4x64, Threefry32x2/x4, Squares32, ChaCha8/20, Salsa8/20, Cet64/256, Threefish256, Biski64, WyHash64, TwistedGFSR、全 SIMD・wide 型、C ABI の全 `_new`。32 ビットシードは実質 2^31 通り、64 ビットシードは 2^63 通りに減る。ワーカー番号・試行番号をシードに使う一般的な並列モンテカルロで、ワーカー 2k と 2k+1 が**完全に同じ乱数列**を使い、標本の重複・分散の過小評価を黙って引き起こす。
- **再現方法**: `harness/common/probe.rs` の `seed_pairs`（`cargo run --release --features v_sol --bin probe -- seed_pairs`）。
- **実際の結果**: 35 型すべてで `new(2) == new(3)`、`new(0x12345678) == new(0x12345679)`（64 ビット型は `new(2^40) == new(2^40+1)`）が `true`。wide（`Pcg32x8`, `Squares32x8`, `Xoshiro128Ppx8`）と C ABI（`xoshiro128pp_new(2)` と `(3)`）も同一。ログ: `harness/out/probe-seed_pairs-sol.log`。
- **期待される結果**: 異なるシードは異なる系列を生成する（Astra 修正版では全て `false`）。
- **修正方針**: `SplitMix32::new` / `SplitMix64::new` / `Xoroshiro128Pp::new` / wide 各型 / `SplitMix32x16::new` から `| 1` を除去する。除去すると `seed = -γ` で状態 0 を経由する経路が生じるため、Xorshift32/64 系のゼロ状態ガード（Sol は実装済み）は維持する。出力系列が変わるため互換性ポリシー（SOL-11）と併せて扱う。
- **検証状態**: 実証済み

### SOL-02 [Medium] C ABI 一括生成の大幅な性能低下（SIMD 経路は最適化不足）

- **対象**: `src/cabi/_internal.rs:34-68`（`BlockRng`）、`src/cabi/prng/**`・`src/cabi/cbrng/**` の全 fill 関数（例 `src/cabi/prng/b32/sfc.rs:349`）、`src/prng/b32/xoshiro.rs:184`・`:281`（`RngV::nextuv`）
- **技術的原因**: (1) 1.0.0 の rayon 並列・チャンク毎再シード・NT ストアを撤去し単一スレッドの逐次生成にした（並列化はチャンク境界で系列が変わる誤りを含んでいたため、撤去自体は正確性確保のための判断）。(2) SIMD 型の fill は `BlockRng` が `RngV::nextuv`（`#[target_feature]` 無しの安全関数）を呼ぶため AVX-512 組み込み関数がインライン化されず関数呼び出し・メモリ経由になり、さらに毎回 `assert!(is_x86_feature_detected!(..))` を実行、ブロックを 1 語ずつ `Rng::nextu` で取り出す。
- **影響範囲**: C/C# から大量の乱数を生成する用途。数値は §4。例（1 M 語、非 LTO）: `xoshiro128pp_next_u32s` 0.18 → 0.82 ns/語（4.6×）、`sfc32x16_next_u32s` 0.027 → 0.38（14×）、`splitmix32x16_next_u32s` 0.17 → 3.1（18×、Astra の 3.4 倍遅い）、`xoshiro128ppx16_next_u32s` 4.4 ns/語（同じ Sol のスカラー `xoshiro128pp` 0.82 より 5 倍遅い）。`RAYON_NUM_THREADS=1` の 1.0.0 と比べるとスカラー fill は同等（0.7〜0.8 ns/語）で、差は並列性の喪失による。SIMD fill は単一スレッド同士でも 1.0.0 より 2.4〜20 倍遅い。
- **再現方法**: `harness/common/bench.rs`（`out/bench-*.csv`, `out/bench-table.md`）。
- **期待される結果**: SIMD fill は対応するスカラー fill より速いこと。系列の厳密性を保った並列化（カウンタ型 Philox/Threefry/Squares/ChaCha はカウンタ分割、xoroshiro128 は jump、xoshiro は jump 実装追加）が可能。
- **修正方針**: SIMD fill 本体を `#[target_feature]` 付き内部関数にして組み込み関数をインライン化し、ブロック単位で `copy_nonoverlapping`。CPU 機能の判定は `_new` 時の 1 回に限定。大容量向けにはカウンタ型・jump 可能型のみ系列を変えない並列 fill を提供する。
- **検証状態**: 実証済み（性能測定）

### SOL-03 [Medium] SIMD 型の Rust API がスカラー版より大幅に遅い（`#[target_feature]` 欠如・毎回の CPU 判定）

- **対象**: `src/prng/b64/biski.rs:116-117`（`Biski64x8::nextu`、`#[inline(always)]` のみ）、`src/prng/b64/sfc.rs:120-121`（`Sfc64x8::nextu`）、`src/prng/b32/xoshiro.rs:184`/`:281`（`Xoshiro128Ppx16`/`Ssx16` の `nextuv`）、`src/prng/b32/jsf.rs`（`Jsf32x16::nextuv`）、`src/prng/b32/splitmix.rs:123-143`（`SplitMix32x16`）
- **技術的原因**: 組み込み関数を `#[target_feature]` 無しの関数から呼ぶため各命令が非インラインの関数呼び出しになる（1.0.0 由来）。Sol はさらに `assert!(is_x86_feature_detected!)` を毎呼び出しに追加した。`SplitMix32x16`（SIMD/wide）は正しいスカラー 64 ビット最終化子に置き換えたためベクトル化されない。
- **影響範囲（非 LTO, ns/語）**: `Biski64x8` 3.94（スカラー `Biski64` 0.40、Astra 0.37）、`Sfc64x8` 6.56（スカラー 0.68、Astra 0.45）、`Xoshiro128Ppx16::nextuv` 4.06（スカラー 0.53、wide 版 0.16）、`SplitMix32x16::nextuv` 1.87（1.0.0 0.91、Astra 0.79）、`wide::SplitMix32x16` 0.68（1.0.0 0.037、ただし 1.0.0 はスカラーと異なる誤った混合関数）。
- **再現方法**: `bench.exe simd::` / `bench.exe wide::`。
- **期待される結果**: SIMD 型はスカラー型より高スループット。
- **修正方針**: Astra と同様に `unsafe fn nextu` 系へ `#[target_feature(enable = "avx512f")]` を付ける。安全メソッドは構築時（`new`）の機能確認を型の不変条件として扱い、ホットパスでの `is_x86_feature_detected!` を外す。`SplitMix32x16` は 64 ビット乗算を `_mm512_mul_epu32` 分解（または AVX-512DQ `vpmullq`）でベクトル化する。
- **検証状態**: 実証済み（性能測定）

### SOL-04 [Medium] C ABI シンボル `sfmt_rand_f64s` の削除（バイナリ互換性の破壊）

- **対象**: `src/cabi/prng/b64/mersenne.rs:175`（`sfmt1993764_rand_f64s` に改名、旧名なし）
- **技術的原因**: 1.0.0 で誤った名前でエクスポートされていた関数を改名し、旧シンボルを残していない。
- **影響範囲**: 1.0.0 の DLL に対して `sfmt_rand_f64s` を呼ぶ既存の C/C# 利用者は、動的リンク時のロード失敗（C）または呼び出し時の `EntryPointNotFoundException`（C#）になる。
- **再現方法**: `llvm-objdump -p urng.dll` のエクスポート表比較（1.0.0: 320、Sol: 336、1.0.0 にあり Sol に無いもの = `sfmt_rand_f64s` のみ）。
- **期待される結果**: 新名を追加しつつ旧名を非推奨エイリアスとして残す。
- **修正方針**: `#[unsafe(no_mangle)] pub unsafe extern "C" fn sfmt_rand_f64s(..)` を新関数への委譲として残す。
- **検証状態**: 実証済み

### SOL-05 [Low] C# バインディングに 54 関数が欠落

- **対象**: `build.rs:90-135`（csbindgen 入力に `src/cabi/prng/b32/sfc.rs`、`src/cabi/prng/b32/xoroshiro.rs`、`src/cabi/prng/b64/biski.rs` 等が無い）、`build.rs:79`（`rerun-if-changed` が `Cargo.toml` のみ）
- **影響範囲**: C# から `sfc32*`、`sfc32x4/x8/x16*`、`xoroshiro64ss*`、`xoroshiro64ssx8/x16*`、`biski64*`、`biski64x8*` の 54 関数が使えない。ソース変更時に再生成されないため生成物が古くなる。
- **再現方法**: クリーンビルドで生成された `target/release/RngNative.cs` の `EntryPoint` と DLL エクスポートを照合（282/336）。Astra 版は 320/320。
- **修正方針**: 入力ファイルを追加し、`cargo:rerun-if-changed=src` を出力する。
- **検証状態**: 実証済み

### SOL-06 [Low] C ABI の SIMD fill が呼び出し単位でブロックを破棄し、出力が分割方法に依存する

- **対象**: `src/cabi/_internal.rs:34-68`、`src/cabi/mod.rs:14-15`（文書化済み）
- **技術的原因**: `BlockRng` を fill 呼び出しごとに生成し、最後のブロックの残りを捨てる。
- **影響範囲**: 同じシードでも `count=16` を 1 回と `count=1` を 16 回では結果が異なる。小刻みに呼ぶ C/C# 利用者はブロックのうちレーン 0..count-1 しか使わず、生成量の最大 (N-1)/N を捨てる（`sfc32x16` で 1 語ずつ取ると 15/16 が無駄）。スカラー fill は呼び出し境界非依存なので API 内で仕様が不統一。
- **再現方法**: `harness/common/cabi.rs`（`out/cabi-sol.log` の "chunked-fill semantics: Block"）。
- **修正方針**: Astra 版のように未消費語をハンドル内に保持する（追加コストはほぼ無い）。
- **検証状態**: 実証済み（文書化された仕様だが設計上の問題）

### SOL-07 [Low] `RngV::randfv` が有効な入力で panic する（1.0.0 からの挙動変更）

- **対象**: `src/rngv.rs:47-53`（`x[i].to_randf(min[i], min[i] + scale[i])`）
- **技術的原因**: 上限を `min + scale` の丸め結果で作り、スカラー `to_randf` の `min < max` 検証に渡すため、`min + scale` が `min` に丸まると panic する。`scale <= 0`・非有限も panic（文書に記載なし）。
- **再現方法**: `numeric.exe simdconv`：`randfv(scale=1, min=16777216)`、`randfv(scale=1e-30, min=1)` が PANIC（`out/numeric-simdconv-sol.log`）。
- **期待される結果**: 区間 `[min, min+scale)` が数学的に非空なら `min` を返す等、panic しない。panic 条件を文書化する。
- **補足**: `randiv` は `min + offset` を折り返し加算するため結果が `[min, min+scale)` 外（負数）になり得る（`randiv(scale=10, min=i32::MAX-2)` → `-2147483643`）。文書化済みだが意外性が高い。
- **検証状態**: 実証済み

### SOL-08 [Low] `Rng::randi` / `index` / shuffle が呼び出し毎に除算を行う

- **対象**: `src/rng.rs:94-110`（`threshold = range.wrapping_neg() % range` を毎回計算）、`src/rng.rs:113-131`（`sample_index` は 64 ビット剰余を毎回計算し、32 ビット経路ではさらに 32 ビット剰余）
- **影響**: `Sfc32::randi(0,999)` 1.05 ns（1.0.0 の偏りのある乗算シフト 0.74 ns の 1.42×）、`shuffle(1M)` 2.33 ns/要素（2.0×）。Lemire の "nearly divisionless" 法（低位語が `range` 未満のときだけ剰余を計算）で大半の呼び出しから除算を除ける。
- **分類**: 正確性確保に必要なコスト（拒否サンプリング）＋最適化不足
- **検証状態**: 実証済み（性能測定。正確性は 2^32 語総当たりで確認済み）

### SOL-09 [Low] `TwistedGFSR::nextu` が `#[inline]` を失い 1.0.0 比 4.2 倍遅い

- **対象**: `src/prng/b64/twisted_gfsr.rs:53`、`:68`
- **技術的原因**: 64 ビット語を 32 ビット出力 2 個から作る修正（正しい）で 2 倍のコストは必要だが、`#[inline]` を外したためクレート外からインライン化されない。
- **影響**: 0.80 → 3.32 ns/語（非 LTO）。
- **修正方針**: `#[inline]` を付与。
- **検証状態**: 実証済み（性能測定）

### SOL-10 [Low] ChaCha8/20 の 32 ビットブロックカウンタ枯渇で panic

- **対象**: `src/crng/b32/chacha.rs:74`, `:85-90`
- **影響**: `ChaCha8::new(seed)` は 2^32 ブロック（2^36 語 = 256 GiB）後に panic する。ChaCha8 は数 GB/s で生成でき、長時間のシミュレーションで到達し得る。`panic = "abort"` 設定の利用者ではプロセス停止。RFC 8439 の鍵ストリーム用途では正しい挙動だが、RNG の便宜コンストラクタとしては 64 ビットカウンタ（DJB 版 / rand_chacha と同じ）が望ましい。Salsa は 64 ビットカウンタ。
- **検証状態**: 静的解析で確認（Astra の同等テストで `from_key_nonce(.., u32::MAX)` の panic は再現される挙動）

### SOL-11 [Low] バージョンが 1.0.0 のまま（互換性を破る変更を含む）

- 既定シード系列・`randi`/`index`/shuffle の消費語数・PCG の増分・C ABI の Rust 側シグネチャ（安全→`unsafe`）・シンボル削除（SOL-04）・新たな panic 条件を含むが `Cargo.toml` の `version = "1.0.0"` は不変。semver 上 2.0.0 相当。README には系列変更の注記あり。
- **検証状態**: 静的解析で確認

### SOL-12 [Low] rand_core 連携: `try_next_u64` の語順が rand_core の慣例・自身の `try_fill_bytes` と不一致

- **対象**: `src/_internal.rs:665-669`
- **内容**: 32 ビット生成器で `next_u64 = (第1語 << 32) | 第2語`。rand_core 0.10 の `utils::next_u64_via_u32` は第 1 語を下位に置き、`try_fill_bytes` も LE で第 1 語が下位バイト。`numeric.exe randcore` で `next_u64 = 0x480b2fc4b55dd249`、`fill_bytes(8)` の LE 値 `0xb55dd249480b2fc4` を確認。1.0.0 から不変（Astra も同じ）。rand_core の指針に反し `Copy`・時刻シードの `Default` も実装している。
- **検証状態**: 実証済み

### SOL-13 [Info] SFMT の 128 ビットシフトがネイティブエンディアン依存（big-endian で仕様と不一致の懸念）

- **対象**: `src/prng/b32/mersenne.rs:32-39`、`src/prng/b64/mersenne.rs:32-39`（`bytemuck::cast::<[u32;4], u128>` 後にシフト）
- **内容**: SFMT 参照実装は `u[0]` を最下位語とする明示的な語順で 128 ビットシフトを行う。Rust 実装はネイティブ `u128` へ再解釈するため big-endian では語順が逆になり出力が仕様と異なるはず。Sol は ppc64 でビルド可能になったため顕在化し得る。1.0.0 由来。
- **検証状態**: 未検証の懸念（big-endian 実機・エミュレータ未使用。静的解析のみ）

### SOL-14 [Info] `#[target_feature]` 関数内の `assert!(is_x86_feature_detected!)` は安全性を保証しない

- **対象**: 例 `src/prng/b32/xoshiro.rs:156`（`#[target_feature(enable = "avx512f")] unsafe fn new` 内）、`src/prng/b32/sfc.rs` の `Sfc32x8::nextuv`
- **内容**: 機能非対応 CPU で `#[target_feature]` 関数を呼ぶこと自体が UB であり、関数本体先頭の assert では防げない（コンパイラは assert より前に AVX-512 命令を配置し得る）。契約上は `unsafe` の前提条件として正しく文書化されているので実害はないが、防御として機能するのは `#[target_feature]` 無しの安全メソッド内の assert のみ。後者は SOL-03 の性能低下要因。
- **検証状態**: 静的解析で確認

## 3. 既存修正の評価

（独立レビュー完了後に記入）

## 4. 性能評価

### 測定条件

- 同一マシン・同一ハーネス（`harness/common/bench.rs`）を 1.0.0（ベース）・Sol・Astra の各ソースに対してビルド。`opt-level=3`、非 LTO（依存クレートとして利用する一般的な構成）。各ケースは 150 ms 以上の反復を 7 回計測した中央値、さらに実行順を入れ替えた 2 回（base→Sol→Astra、Astra→Sol→base）の平均。2 回の差は 86 行中 81 行で 10% 未満。
- C ABI はハーネスから Rust 関数として直接呼び出し。1.0.0 は既定で rayon 並列（`RAYON_NUM_THREADS=1` でも測定）。
- **注意: 1.0.0 は多くの項目で誤った実装・異なる系列（C ABI はチャンク毎再シード）であり、「誤った高速実装」と「正しい実装」の比較である。** 該当行に明記する。

### 主要結果（ns/語、小さいほど速い。比 = 各版 / 1.0.0）

| ケース | 1.0.0 | Sol | Astra | Sol 比 | 備考 / 分類 |
| --- | ---: | ---: | ---: | ---: | --- |
| `Xoshiro256Pp::nextu` | 0.498 | 0.500 | 0.493 | 1.00 | 変化なし |
| `Sfc64::nextu` | 0.506 | 0.681 | 0.675 | 1.35 | 1.0.0 は誤った SFC64。正確性に必要なコスト |
| `Xoroshiro128Pp::nextu` | 0.688 | 0.787 | 0.769 | 1.14 | 1.0.0 は誤った式。必要なコスト |
| `Philox32::nextu` | 4.135 | 1.755 | 1.588 | 0.42 | 1.0.0 は 20 ラウンド・乗算鍵（誤り） |
| `ChaCha20::nextu` | 7.168 | 4.717 | 3.911 | 0.66 | 1.0.0 は 40 ラウンド（誤り） |
| `TwistedGFSR::nextu` | 0.796 | 3.320 | 3.117 | 4.17 | 2 語連結は必要（〜2×）、残りは `#[inline]` 欠如（SOL-09, 最適化不足） |
| `Sfc32::randi(0,999)` | 0.737 | 1.049 | 3.092 | 1.42 | 1.0.0 は偏りあり。拒否法のコスト＋毎回除算（SOL-08） |
| `Xoshiro256Pp::randf(0,1)` | 0.624 | 0.787 | 0.603 | 1.26 | 半開区間保証の分岐。最適化不足の可能性 |
| `Sfc64::shuffle(1M)`（ns/要素） | 1.160 | 2.329 | 3.626 | 2.01 | 1.0.0 は偏ったシャッフル。必要なコスト＋除算（SOL-08） |
| `Alias32::sample(16)` | 5.681 | 6.649 | 8.526 | 1.17 | 拒否法 `index` のコスト |
| `simd::Sfc32x8::nextu` | 0.752 | 0.392 | 0.807 | 0.52 | 1.0.0 は AVX-512VL 依存（AVX2 CPU で不正命令） |
| `simd::Philox32x4x4::nextu` | 0.344 | 1.244 | 1.283 | 3.61 | 1.0.0 の SIMD Philox は誤り。正しいがスカラー化（最適化不足の可能性） |
| `simd::SplitMix32x16::nextuv` | 0.905 | 1.868 | 0.786 | 2.06 | 1.0.0 は別アルゴリズム。Sol は assert＋非インライン（SOL-03） |
| `simd::Biski64x8::nextu` | 3.680 | 3.944 | 0.367 | 1.07 | Sol は 1.0.0 同様に非 `target_feature`（SOL-03） |
| `simd::Sfc64x8::nextu` | 6.843 | 6.561 | 0.454 | 0.96 | 同上 |
| `simd::Xoshiro128Ppx16::nextuv` | 3.998 | 4.055 | 3.949 | 1.01 | 3 版とも非インライン（スカラーの 8 倍遅い） |
| `wide::SplitMix32x16::nextu` | 0.037 | 0.683 | 0.696 | 18.46 | 1.0.0 は誤った混合関数。正しい 64 ビット最終化子のコスト＋ベクトル化不足 |
| `wide::Xoshiro128Ppx16::randi(0,999)` | 0.493 | 2.198 | 3.255 | 4.46 | 1.0.0 は偏りあり。レーン毎の除算 |
| `cabi::xoshiro128pp_next_u32s[1M]` | 0.180 | 0.820 | 1.274 | 4.55 | 1.0.0 は並列・別系列。単一スレッド 1.0.0 は 0.699 |
| `cabi::mt19937_next_u32s[64M]` | 0.152 | 0.900 | 0.995 | 5.93 | 同上（単一スレッド 1.0.0: 0.802） |
| `cabi::philox32_next_u32s[64M]` | 0.306 | 2.364 | 2.420 | 7.72 | 同上＋1.0.0 は誤った Philox |
| `cabi::sfc32x16_next_u32s[1M]` | 0.027 | 0.383 | 0.360 | 14.11 | 単一スレッド 1.0.0 でも 0.080（SOL-02, 最適化不足） |
| `cabi::splitmix32x16_next_u32s[1M]` | 0.171 | 3.122 | 0.927 | 18.24 | 1.0.0 は別アルゴリズム。Sol は Astra の 3.4 倍遅い（SOL-02） |
| `cabi::philox32x4x4_next_u32s[1M]` | 0.163 | 1.333 | 1.319 | 8.20 | 1.0.0 は誤り。スカラー Philox ×4 |
| `cabi::xoshiro128ppx16_next_u32s[1M]` | 4.204 | 4.421 | 4.374 | 1.05 | SIMD 経路がスカラー（0.82）より遅い（SOL-02） |
| `cabi::sfc32x16_rand_i32s(0,999)[1M]` | 0.031 | 1.269 | 1.844 | 40.41 | 1.0.0 は偏りあり・並列 |
| `cabi::xoshiro256pp_rand_f64s(0,1)[1M]` | 0.136 | 1.191 | 1.563 | 8.78 | 1.0.0 は並列・半開保証なし |

全 86 ケースの表: `harness/out/bench-table.md`（生データ `out/bench-{base,sol,astra}-run{1,2}.csv`、`out/bench-base-rayon1.csv`）。

### 性能低下の分類

| 分類 | 該当 |
| --- | --- |
| 正確性・安全性確保に必要なコスト | SFC64/xoroshiro128++ の正しい式、拒否サンプリング（randi/index/shuffle/sampler）、TwistedGFSR の 64 ビット化（約 2×まで）、C ABI のスカラー fill の並列撤去（系列の厳密化）、wide/SIMD SplitMix32 の正しい最終化子 |
| 最適化不足の可能性 | SIMD 型・SIMD C ABI の非インライン組み込み関数と毎回の CPU 判定（SOL-02/03）、`TwistedGFSR` の `#[inline]` 欠如（SOL-09）、毎回の除算（SOL-08）、Philox32x4x4/SplitMix32x16 のスカラー化、系列を保った並列 fill の未提供 |
| 原因不明 | `Xoshiro128Pp::nextu` 1.03×、`Jsf32::nextu` 1.06×（コード変更なし。2 回の差 8〜13% で測定誤差の範囲） |
| 測定誤差の可能性 | 1.0.0 の並列 C ABI（1M 語）の一部で 2 回の差が 18〜30%（rayon のスケジューリング揺らぎ） |

## 5. 実行した検証

| # | コマンド / ハーネス | 結果 | 保存先 |
| --- | --- | --- | --- |
| 1 | `cargo test --all-features --locked --no-fail-fast`（スクラッチコピー） | 成功: lib 161, cabi_regression 59, cipher_regression 4, numeric_regression 13, sampler_rand_regression 4, doctest 107（1 ignored）+1。失敗 0 | `logs/cargo-test-all-features.log` |
| 2 | `python ref/check_vectors.py`（参照実装の公式ベクタ照合） | 62/62 成功 | `harness/out/check_vectors.log` |
| 3 | `python ref/check_xoshiro_c.py`（公式 C ソースをコンパイルして照合、jump は GF(2) 行列冪で独立検証） | 全成功 | `harness/out/check_xoshiro_c.log` |
| 4 | `dump.exe` + `ref/compare_dump.py sol`（スカラー・SIMD 全レーン・wide 全レーン・jump/long_jump） | 793/793 系列一致 | `harness/out/compare-sol.log` |
| 5 | `cabi.exe`（56 ファミリ、Rust 系列一致・分割呼び出し・浮動小数・範囲・不正入力・状態不変） | 5,544 チェック全成功 | `harness/out/cabi-sol.log` |
| 6 | `probe.exe seed_pairs` / `xorshift32_zero` | SOL-01 を再現 / ゼロ状態シードなし | `harness/out/probe-*-sol.log` |
| 7 | `numeric.exe randi_enum`（2^32 語総当たり、7 範囲） | 全範囲で拒否数・各バケット数が理論値と完全一致 | `harness/out/numeric-randi_enum-sol.log` |
| 8 | `numeric.exe randi64_edge / randf_edge / index / shuffle / sampler / simdconv / randcore` | randf 160 ケース全て半開区間内、index(3) 総当たり一様、shuffle χ²=116.9（df=119）、サンプラー正常、SOL-07/12 を再現 | `harness/out/numeric-*-sol.log` |
| 9 | `build_matrix.ps1`（64 feature 組み合わせ、doctest、`build --all-targets`、`doc`、nightly `-Zbuild-std` で aarch64/i686/ppc64 クロスチェック） | 全成功（1.0.0 は非 x86_64 + simd で失敗） | `harness/out/build-matrix-sol.log`, `build-matrix-base.log` |
| 10 | `ref/evex_scan.py`（AVX2 C ABI 入口から到達する EVEX 命令の静的探索） | Sol: 0（1.0.0 は `vprold` に到達＝陽性対照） | 本文 §1 |
| 11 | エクスポート表・C# バインディング照合（`llvm-objdump -p`、生成 `RngNative.cs`） | SOL-04, SOL-05 | 本文 |
| 12 | `bench.exe`（base/Sol/Astra × 2 回、非 LTO。LTO 版は追記予定） | §4 | `harness/out/bench-*.csv`, `bench-table.md` |

- 参照実装・期待値: `harness/ref/refimpl.py`（独自実装）、`harness/vectors/`（MT19937/MT19937-64 公式出力、SFMT 全 10 パラメータ公式出力、Random123 `kat_vectors`）、Bouncy Castle の Skein 1.3 Threefish-256 KAT、RFC 8439 §2.3.2、RFC 7914 §8 と OpenSSL scrypt、pcg-c-basic `pcg32-demo` の実行出力、prng.di.unimi.it の公式 C ソース。
- 実行できなかった検証: AVX-512 非搭載 CPU 上の実行、big-endian 実機、Miri、C# 実行、統計検定スイート（§1 参照）。

## 6. 総括

- **採用上の重大な障害**: SOL-01（シード 2k/2k+1 の系列同一）。修正は数行（`| 1` の除去）だが全生成器の出力が変わるため、採用するなら同時に行うべき。
- **残存リスク**: C ABI・SIMD 経路の性能（SOL-02/03）、ABI 互換性（SOL-04）、C# バインディング欠落（SOL-05）、big-endian での SFMT（SOL-13）。
- **優先して修正すべき事項**: (1) SOL-01、(2) SOL-04（旧シンボルのエイリアス追加）、(3) SOL-02/03（`#[target_feature]` 化・ホットパスの CPU 判定除去・SIMD fill のブロック単位コピー）、(4) SOL-05、(5) SOL-06（未消費語の保持）。
- **追加検証の推奨**: AVX2 のみの CPU（または Intel SDE）での実行試験、big-endian での SFMT 出力確認、Miri による C ABI スカラー経路の検査、PractRand/TestU01 による統計検定、C# からの P/Invoke 実行試験。

## 作業ログ（再開用）

- ハーネス: `harness/setup.ps1 -Target sol` で `_src/`（HEAD）・`_base/`（386eb4d）を展開。`run-target/`（Sol）・`run-base/`（1.0.0）で `cargo build --release --features v_sol --bin <dump|cabi|probe|numeric|bench>`。
- ベース `386eb4d` の `src/` ツリーは Astra の `19bc224` と同一（ツリーハッシュ `ef46088…`）。
- 未完了: LTO 構成での C ABI 性能（実行中）、§3。
