# urng 1.0.0 修正版レビュー — Sol 修正版（レビュアー: Opus 5.5）

> 状態: 完了（2026-10-16）。§1〜§2・§4〜§6 は Sol の REVIEW.md / FIX_REPORT.md を参照せずに作成し、その時点で WIP コミット（`70536a5`）した。§3 はその後に両文書を読んで突き合わせた。

## 1. 総合評価

| 項目 | 内容 |
| --- | --- |
| 対象モデル | Sol |
| 対象ディレクトリ | `urng-review_sol/` |
| 対象コミット | `8417b4b`（修正本体 `383625a`、報告書 `ece9e31`、ベース `386eb4d` = urng 1.0.0） |
| 実施環境 | Windows 11 Pro 10.0.26200 / AMD Ryzen 7 9800X3D（Zen 5、AVX2・AVX-512F/VL/DQ/BW 対応、16 論理 CPU）/ 64 GB / rustc 1.98.1 stable（x86_64-pc-windows-msvc, LLVM 22.1.8）、nightly-2026-10-08（`-Zbuild-std` クロスチェック用）/ Python 3.13.15 / gcc（Strawberry）/ llvm-objdump / .NET SDK 10.0.401 |
| 方法 | 原本を `git archive` でハーネス内に展開してビルド（原本ディレクトリでは一切ビルドしていない）。対象コードから独立した参照実装（Python。公式ベクタで事前検証）と公式 C ソースで系列を照合し、C ABI・数値変換・サンプラー・ビルド構成・機械語・性能を実測した |

### 結論の要約

- **アルゴリズムの正確性は非常に高い。** 独立参照実装（MT19937/-64、SFMT 全 10 パラメータ、Random123 Philox/Threefry、Skein Threefish-256、RFC 8439 ChaCha20、Salsa20、PCG32、xoshiro/xoroshiro の公式 C、jump/long_jump は GF(2) 行列冪）と照合し、スカラー 46 型（＋xoroshiro128 の jump/long_jump）・SIMD 19 型の全レーン・wide 38 型の全レーンで **793/793 系列が一致**。1.0.0 の誤実装（Philox の乗算鍵、Threefry の tweak/XOR、ChaCha/Salsa、xoroshiro128++、SFC64、Biski64x8、xoroshiro64**x8/x16、long_jump、wide Threefry の桁上げ等）はすべて正しく直っている。`randi`/`index` は 2^32 語の総当たりで厳密に一様、`randf` は半開区間を保証、shuffle とサンプラーも正しい。
- **安全性も大きく改善。** AVX2 型から AVX-512 命令が機械語レベルで到達しないこと、`Xoshiro256Ssx2` のスタック破壊の解消、C ABI のポインタ・整列・重なり・長さ検証（5,544 チェック）を確認。非 x86_64（aarch64 / i686 / big-endian ppc64）でも全 feature でビルドできる唯一の版。
- **ただし、1.0.0 の `seed | 1` が残っており、ほぼ全生成器でシード 2k と 2k+1 が同一系列になる（SOL-01, High）。** 連番シードを使う並列シミュレーションで系列が重複する。Sol 自身の phase-1 レビューでも見落とされていた。
- C ABI シンボル `sfmt_rand_f64s` の削除（SOL-04）と、生成される C# バインディングがコンパイル不能（SOL-05、1.0.0 ではコンパイルできた）という互換性の後退がある。
- 性能は、スカラー API は概ね同等だが、C ABI 一括生成が 1.0.0（並列・別系列の誤った高速実装）比 4〜18 倍（範囲生成では最大 40 倍）遅く、SIMD 型の一部（`Biski64x8`/`Sfc64x8`/`Xoshiro128Ppx16`、`SplitMix32x16`）はスカラー版や Astra 版より大幅に遅い（SOL-02/03）。

### 重大な問題の件数

| 重要度 | 件数 | ID |
| --- | --- | --- |
| Critical | 0 | — |
| High | 1 | SOL-01 |
| Medium | 4 | SOL-02, SOL-03, SOL-04, SOL-05 |
| Low | 7 | SOL-06 〜 SOL-12 |
| Info | 2 | SOL-13, SOL-14 |

### 検証できた範囲 / 未検証範囲

- 検証済み: 全スカラー・SIMD・wide 生成器の出力系列（独立参照）、C ABI 全 56 ファミリ（Rust 系列との一致・呼び出し分割・浮動小数・範囲・不正入力・状態不変）、`randi`（2^32 語総当たり）、`randf`（160 の敵対的ケース）、`index`、shuffle（χ²）、サンプラー（ゼロ・極端値・不正値・300 通りの分布掃引）、rand_core 連携、64 feature 組み合わせ、非 x86_64 クロスチェック、AVX2 経路の機械語検査、C# バインディングのコンパイル、wide Threefry の 2^32 ブロック周期試験、性能（同一環境・同一ハーネスで base/Sol/Astra、非 LTO/LTO）。
- 未検証: AVX2 のみ・AVX-512 非搭載 CPU での実行（実機・エミュレータなし。機械語の静的検査で代替）、big-endian での実行時動作（クロスコンパイルのみ）、Miri（コンポーネント未導入。ツールチェーンを変更しないため未実施）、C# からの実行、統計検定スイート（PractRand/TestU01 等）、Linux 上の実ビルド（`-Zbuild-std` によるクロスチェックのみ）。

## 2. 発見した問題

### SOL-01 [High] `seed | 1` が残存し、シード 2k と 2k+1 がほぼ全生成器で同一系列になる

- **対象**: `src/prng/b32/splitmix.rs:30`（`SplitMix32::new`）、`src/prng/b64/splitmix.rs:23`（`SplitMix64::new`）、`src/prng/b64/xoroshiro.rs:23`、`src/prng/b32/splitmix.rs:105`（`SplitMix32x16::new`）、`src/wide/splitmix.rs:33`、`src/wide/pcg.rs:33`、`src/wide/squares.rs:39`
- **技術的原因**: 全生成器のシード展開器 SplitMix32/64 が状態を `seed | 1` で初期化する。SplitMix の Weyl 増分は奇数なので状態を奇数にする必要はなく、最下位ビットを捨てているだけ。`sm64_from_seed32!` 経由（Pcg32, Sfmt*, Squares32）も同様。
- **影響範囲**: SplitMix32/64, Xoshiro128/256, Xoroshiro64/128, Jsf32, Sfc32/64, Xorshift32/64/128, Xorwow, Mt19937/Mt1993764, Sfmt 全種, Pcg32, Philox32/64/4x64, Threefry32x2/x4, Squares32, ChaCha8/20, Salsa8/20, Cet64/256, Threefish256, Biski64, WyHash64, TwistedGFSR、全 SIMD・wide 型、C ABI の全 `_new`。32 ビットシードは実質 2^31 通り、64 ビットは 2^63 通り。ワーカー番号・試行番号をシードにする並列モンテカルロで、ワーカー 2k と 2k+1 が完全に同じ乱数列を使い、標本の重複と分散の過小評価を黙って引き起こす。
- **再現方法**: `harness/common/probe.rs` の `seed_pairs`（`cargo run --release --features v_sol --bin probe -- seed_pairs`）。
- **実際の結果**: 35 型すべてで `new(2) == new(3)`、`new(0x12345678) == new(0x12345679)`（64 ビット型は `new(2^40) == new(2^40+1)`）が `true`。wide（`Pcg32x8`, `Squares32x8`, `Xoshiro128Ppx8`）と C ABI（`xoshiro128pp_new(2)` と `(3)`）も同一（`harness/out/probe-seed_pairs-sol.log`）。
- **期待される結果**: 異なるシードは異なる系列（Astra 版は全て `false`）。
- **修正方針**: `| 1` を全箇所から除去する。除去後も Xorshift32/64 系のゼロ状態ガード（Sol は実装済み）は維持する。出力系列が変わるため SOL-11 と併せてリリースする。
- **検証状態**: 実証済み

### SOL-02 [Medium] C ABI 一括生成の大幅な性能低下（SIMD 経路は最適化不足）

- **対象**: `src/cabi/_internal.rs:34-68`（`BlockRng`）、全 fill 関数（例 `src/cabi/prng/b32/sfc.rs:349`）、`src/prng/b32/xoshiro.rs:184`・`:281`（`RngV::nextuv`）
- **技術的原因**: (1) 1.0.0 の rayon 並列・チャンク毎再シード・NT ストアを撤去して単一スレッドの逐次生成にした（1.0.0 の並列化は系列の重複・チャンク境界依存という誤りを含むため、撤去自体は正確性確保の判断）。(2) SIMD 型の fill は `BlockRng` が `RngV::nextuv`（`#[target_feature]` 無しの安全関数）を呼ぶため AVX-512 組み込み関数がインライン化されず、毎回 `assert!(is_x86_feature_detected!(..))` を実行し、ブロックを 1 語ずつ取り出す。
- **影響範囲（1 M 語, ns/語）**: `xoshiro128pp_next_u32s` 0.18 → 0.80（4.5×、`RAYON_NUM_THREADS=1` の 1.0.0 は 0.70）、`sfc32x16_next_u32s` 0.027 → 0.37（13.6×、単一スレッドの 1.0.0 でも 0.080）、`splitmix32x16_next_u32s` 0.17 → 3.08（17.9×、Astra の 3.3 倍遅い）、`xoshiro128ppx16_next_u32s` 4.49（同じ Sol のスカラー `xoshiro128pp` 0.80 の 5.6 倍遅い）。LTO 構成（出荷 cdylib と同条件）でも同傾向（§4）。
- **再現方法**: `harness/common/bench.rs`（`out/bench-table*.md`）。
- **期待される結果**: SIMD fill は対応するスカラー fill より速い。系列を保った並列化（カウンタ型 Philox/Threefry/Squares/ChaCha はカウンタ分割、xoroshiro128 は jump）が可能。
- **修正方針**: SIMD fill 本体を `#[target_feature]` 付き内部関数にしてブロック単位で `copy_nonoverlapping`、CPU 判定は `_new` 時の 1 回に限定。大容量向けに系列不変の並列 fill を提供する。
- **検証状態**: 実証済み（性能測定）

### SOL-03 [Medium] SIMD 型の Rust API がスカラー版より大幅に遅い

- **対象**: `src/prng/b64/biski.rs:116-117`（`Biski64x8::nextu`。`#[inline(always)]` のみ）、`src/prng/b64/sfc.rs:120-121`（`Sfc64x8::nextu`）、`src/prng/b32/xoshiro.rs:184`/`:281`（`Xoshiro128Ppx16`/`Ssx16::nextuv`）、`src/prng/b32/jsf.rs`（`Jsf32x16::nextuv`）、`src/prng/b32/splitmix.rs:123-143`（`SplitMix32x16`）
- **技術的原因**: 組み込み関数を `#[target_feature]` 無しの関数から呼ぶため各命令が非インラインの関数呼び出しになる（1.0.0 由来）。Sol は毎呼び出しの `assert!(is_x86_feature_detected!)` を加えた。`SplitMix32x16` は正しいスカラー最終化子（64 ビット乗算）を `#[target_feature]` 関数経由で呼ぶ。
- **影響（全レーン消費, 非 LTO, ns/語）**: `Biski64x8` 4.00（スカラー `Biski64` 0.39、Astra 0.39）、`Sfc64x8` 6.85（スカラー 0.68、Astra 0.46）、`Xoshiro128Ppx16::nextuv` 4.09（スカラー 0.53、wide 版 0.28）、`SplitMix32x16::nextuv` 1.88（1.0.0 0.90、Astra 0.80）。
- **再現方法**: `bench.exe simd::`（`out/bench-table-lanes*.md`）。
- **期待される結果**: SIMD 型はスカラー型より高スループット。
- **修正方針**: Astra と同様に `unsafe fn nextu` 系へ `#[target_feature(enable = "avx512f")]` を付与。安全メソッドは構築時の機能確認を型の不変条件として扱い、ホットパスの `is_x86_feature_detected!` を外す。
- **検証状態**: 実証済み（性能測定）

### SOL-04 [Medium] C ABI シンボル `sfmt_rand_f64s` の削除（バイナリ互換性の破壊）

- **対象**: `src/cabi/prng/b64/mersenne.rs:175`（`sfmt1993764_rand_f64s` に改名、旧名なし）
- **技術的原因**: 1.0.0 で誤った名前でエクスポートされていた関数を改名し、旧シンボルを残していない。
- **影響範囲**: 1.0.0 の `sfmt_rand_f64s` を使う既存の C/C# 利用者は、動的リンク時のロード失敗（C）または `EntryPointNotFoundException`（C#）。FIX_REPORT の「C の関数名・引数型・呼出し規約は保持」という記述と矛盾する。
- **再現方法**: `llvm-objdump -p urng.dll` のエクスポート比較（1.0.0: 320、Sol: 336。1.0.0 にあり Sol に無いのは `sfmt_rand_f64s` のみ）。
- **修正方針**: 旧名を新関数への委譲（非推奨エイリアス）として残す。
- **検証状態**: 実証済み

### SOL-05 [Medium] 生成される C# バインディングがコンパイル不能、かつ 54 関数が欠落

- **対象**: `build.rs:90-135`（csbindgen 入力）、`build.rs:79`（`rerun-if-changed` が `Cargo.toml` のみ）、`src/prng/b32/mersenne.rs:352-542`（SFMT 型はマクロ生成）、`src/cabi/prng/b32/mersenne.rs`（SFMT の C ABI を明示関数として展開）
- **技術的原因**: SFMT の C ABI 関数を csbindgen が読める形に展開した一方、`Sfmt607`〜`Sfmt216091` の構造体はマクロ生成のため csbindgen に見えず、生成 C# が未宣言の型を参照する。また `sfc32*`、`xoroshiro64ss*`、`biski64*` の C ABI ファイルが入力に無い。
- **影響範囲**: クリーンビルドで生成される `RngNative.cs` を C# プロジェクトに入れると **CS0246 が 108 件**（`Sfmt607, Sfmt1279, Sfmt2281, Sfmt4253, Sfmt11213, Sfmt44497, Sfmt86243, Sfmt132049, Sfmt216091` が見つからない）でビルド失敗。1.0.0 の生成物はコンパイルできた（関数は不足していた）ので後退。加えて 336 中 54 関数（sfc32 系・xoroshiro64ss 系・biski64 系）が生成されない。
- **再現方法**: `dotnet build`（net10.0, `AllowUnsafeBlocks`）に生成 `RngNative.cs` を入れる（`harness/out/csharp-build-sol.log`）。DLL エクスポートとの照合は 282/336。
- **修正方針**: SFMT 型を csbindgen が読める形で宣言する（またはハンドルを `void*` にする）、入力ファイルを追加、`cargo:rerun-if-changed=src` を出力、CI で生成 C# をコンパイルする。
- **検証状態**: 実証済み

### SOL-06 [Low] C ABI の SIMD fill が呼び出しごとにブロック残りを破棄し、出力が分割方法に依存する

- **対象**: `src/cabi/_internal.rs:34-68`、`src/cabi/mod.rs:14-15`（文書化済み）
- **影響範囲**: 同じシードでも `count=16` 1 回と `count=1` 16 回で結果が異なる。小刻みに呼ぶ利用者はブロックの先頭レーンしか使わず、生成量の最大 (N-1)/N を捨てる。スカラー fill は分割非依存なので API 内で仕様が不統一。
- **再現方法**: `harness/common/cabi.rs`（`out/cabi-sol.log` の "chunked-fill semantics: Block"）。
- **修正方針**: Astra 版のように未消費語をハンドル内に保持する。
- **検証状態**: 実証済み（文書化された仕様だが設計上の問題）

### SOL-07 [Low] `RngV::randfv` が有効な入力で panic する（1.0.0 からの挙動変更）

- **対象**: `src/rngv.rs:47-53`（`x[i].to_randf(min[i], min[i] + scale[i])`）
- **技術的原因**: 上限 `min + scale` の丸め結果をスカラー `to_randf` の `min < max` 検証に渡すため、`min + scale` が `min` に丸まると panic。`scale <= 0`・非有限も panic（trait 文書に記載なし）。
- **再現方法**: `numeric.exe simdconv`：`randfv(scale=1, min=16777216)`、`randfv(scale=1e-30, min=1)` が PANIC（`out/numeric-simdconv-sol.log`）。
- **補足**: `randiv` は `min + offset` を折り返し加算するため結果が負になり得る（`randiv(scale=10, min=i32::MAX-2)` → `-2147483643`）。文書化済み。
- **修正方針**: 非空の数学的区間では `min` を返す等で panic を避け、panic 条件を文書化する。
- **検証状態**: 実証済み

### SOL-08 [Low] `randi` / `index` / shuffle / `randf` の呼び出しごとのコスト

- **対象**: `src/rng.rs:94-110`（`threshold = range.wrapping_neg() % range` を毎回計算）、`src/rng.rs:113-131`（`sample_index` は 64 ビット剰余を毎回、32 ビット経路ではさらに 32 ビット剰余）、`src/rng.rs:74-85`（`to_randf` の検証と分岐）
- **影響（非 LTO, 実行時に決まる範囲, 4 回の中央値, ns/語）**: `Sfc32::randi(0,999)` 1.10 → 1.78（1.6×）、`Xoshiro256Pp::randi` 1.28 → 2.45（1.9×）、`Sfc32::randf` 0.91 → 1.98（2.2×）、`shuffle(1M)` 1.13 → 2.24（2.0×）。定数範囲ではコンパイラが閾値を畳み込むため 1.43×。
- **分類**: 正確性確保（拒否法・半開区間保証）の必要コスト＋最適化不足。Lemire の nearly-divisionless 法（低位語が `range` 未満のときだけ剰余）で大半の除算を省ける。
- **検証状態**: 実証済み（性能測定。正確性は総当たりで確認済み）

### SOL-09 [Low] `TwistedGFSR::nextu` が `#[inline]` を失い 1.0.0 比 4 倍遅い

- **対象**: `src/prng/b64/twisted_gfsr.rs:53`、`:68`
- **影響**: 0.78 → 3.16 ns/語。32 ビット出力 2 個の連結（正しい修正）で約 2 倍は必要だが、残りはクレート外でインライン化されないため。
- **修正方針**: `#[inline]` を付与。
- **検証状態**: 実証済み（性能測定）

### SOL-10 [Low] ChaCha8/20 の 32 ビットブロックカウンタ枯渇で panic

- **対象**: `src/crng/b32/chacha.rs:74`, `:85-90`
- **影響**: `ChaCha8::new(seed)` は 2^32 ブロック（2^36 語 = 256 GiB）後に panic。RNG として数分〜数十分の連続生成で到達し得る。RFC 8439 鍵ストリームとしては正しいが、RNG の便宜コンストラクタには 64 ビットカウンタ（DJB 版 / rand_chacha）が望ましい。
- **再現方法**: `probe.exe cipher_exhaust`：`ChaCha20::from_key_nonce(key, nonce, u32::MAX)` の 1 ブロック後に panic（`out/probe-cipher_exhaust-sol.log`）。
- **検証状態**: 実証済み（`from_key_nonce` 経由。`new` 経由の 2^32 ブロックは静的解析）

### SOL-11 [Low] バージョンが 1.0.0 のまま（互換性を破る変更を含む）

- 既定のシード系列・`randi`/`index`/shuffle の消費語数・PCG 増分・C ABI の Rust 側シグネチャ（安全→`unsafe`）・シンボル削除（SOL-04）・新たな panic 条件を含むが `version = "1.0.0"`。semver 上 2.0.0 相当。README/FIX_REPORT に系列変更の注記はある。
- **検証状態**: 静的解析で確認

### SOL-12 [Low] rand_core 連携: `try_next_u64` の語順が rand_core の慣例・自身の `try_fill_bytes` と不一致

- **対象**: `src/_internal.rs:665-669`
- **内容**: 32 ビット生成器で `next_u64 = (第1語 << 32) | 第2語`。rand_core 0.10 の `utils::next_u64_via_u32` は第 1 語を下位に置き、`try_fill_bytes` も LE で第 1 語が下位。実測 `next_u64 = 0x480b2fc4b55dd249`、`fill_bytes(8)` の LE 値 `0xb55dd249480b2fc4`（`out/numeric-randcore-sol.log`）。rand_core の指針に反して `Copy` と時刻シードの `Default` も実装。1.0.0 から不変。
- **検証状態**: 実証済み

### SOL-13 [Info] SFMT の 128 ビットシフトがネイティブエンディアン依存（big-endian で仕様と不一致の懸念）

- **対象**: `src/prng/b32/mersenne.rs:32-39`、`src/prng/b64/mersenne.rs:32-39`（`bytemuck::cast::<[u32;4], u128>` 後にシフト）
- **内容**: SFMT 参照実装は `u[0]` を最下位語とする明示的語順でシフトする。ネイティブ `u128` への再解釈は big-endian で語順が逆になる。Sol は ppc64 でビルド可能になったため顕在化し得る。1.0.0 由来。
- **検証状態**: 未検証の懸念（静的解析のみ）

### SOL-14 [Info] `#[target_feature]` 関数内の `assert!(is_x86_feature_detected!)` は安全性を保証しない

- **対象**: 例 `src/prng/b32/xoshiro.rs:156`（`#[target_feature(enable = "avx512f")] unsafe fn new` 内）、`Sfc32x8::nextuv` 等
- **内容**: 非対応 CPU で `#[target_feature]` 関数を呼ぶこと自体が UB で、本体先頭の assert では防げない（コンパイラは assert より前に拡張命令を配置し得る）。`unsafe` の前提条件として文書化されているので実害はないが、防御として有効なのは `#[target_feature]` 無しの安全メソッド内の assert のみで、後者は SOL-03 の性能低下要因。
- **検証状態**: 静的解析で確認

## 3. 既存修正の評価

Sol の phase-1 REVIEW.md（指摘 1〜22）と FIX_REPORT.md の主張を、上記の独立検証と突き合わせた。

### 正しく修正されていると確認できた問題

| Sol 指摘 | 内容 | 独立検証での確認 |
| --- | --- | --- |
| 1 Critical | `Xoshiro256Ssx2` の 16 バイト配列への 64 バイト書き込み | 2 レーンが独立 xoshiro256** 参照と一致（dump）。広幅ストア無し |
| 2 High | C ABI が safe な extern として公開 | 全 fill/free が `unsafe`。null/不整列/長さ溢れ/重なり/不正範囲を no-op（cabi 5,544 チェック） |
| 3 High | AVX2 経路の AVX-512VL 命令・CPU 検査 | 機械語検査で AVX2 入口から EVEX 0（1.0.0 は `vprold`）。intrinsic 型を x86_64 に限定（クロスチェック成功）。※SOL-14 参照 |
| 4 High | NT ストアの fence 欠落 | NT 経路そのものを撤去 |
| 5 High | 並列チャンクの系列再利用 | 逐次生成。C ABI 出力が Rust 系列と一致 |
| 6 High | wide SplitMix の重複ブロック | wide/SIMD の平坦化出力がスカラー系列と一致（重複なし） |
| 7 High | ChaCha/Salsa の非標準構造 | RFC 8439 / RFC 7914 / OpenSSL scrypt で検証した参照と一致。※SOL-10 |
| 8 High | SFC64・xoroshiro128++・Philox32/x4x4・xoroshiro64**x8/16・Biski64x8 | すべて独立参照と一致 |
| 9 Medium | `Readme.md` の大小文字 | `include_str!("../README.md")` と Git 上のファイル名 `README.md` の一致を静的に確認（クロスチェックは Windows のファイルシステム上で行ったため大小文字の実証にはならない） |
| 10 Medium | PCG 偶数増分・Xorshift ゼロ状態 | 全 inc が奇数（dump）。2^32 シード総当たりでゼロ状態なし |
| 11 Medium | Philox4x64/Threefish256 の初期ゼロ出力 | 最初の語から正しいブロック |
| 12 Medium | `long_jump` の誤り | GF(2) 行列冪 T^(2^96) と一致 |
| 13 Medium | wide Threefry の桁上げ欠如 | 2^32 ブロック後に系列が反復しないことを実行で確認（62.7 秒の試験） |
| 14 Medium | 偏ったシャッフル | n=5 の 120 置換で χ²=116.9（df=119） |
| 15 Medium | 全幅範囲の panic/定数出力 | `randi(i32::MIN, i32::MAX)` の総当たり、C ABI 全幅 |
| 16 Medium | 乗算シフトの偏り | 2^32 語総当たりで拒否数・バケット数が理論値と完全一致 |
| 17 Medium | SSE `to_randi` の奇数レーン | `simdconv` で全レーン整合 |
| 18 Medium | TwistedGFSR の 32 ビット出力 | 64 ビット語を生成。※SOL-09（性能） |
| 19 Medium | サンプラーの溢れ・ゼロ重み | `[MAX, MAX]`、ゼロ重み、不正値、300 通りの分布掃引すべて正常 |
| 20 Medium | 64 ビット型の `TryRng` 欠如 | 実装済み。※SOL-12（語順） |
| 21 Low | `randf` が上端を返す | 160 の敵対的ケースで全て半開区間内 |
| 22 Low | README の `WRng` | 修正。README を doctest に含め全構成で成功 |
| 追加 | Threefry32x4 の tweak/XOR、Threefish256 の XOR | Random123 / Skein の KAT 相当の参照と一致 |

### 修正が不完全な問題

- **シード処理（指摘 10 関連）**: ゼロ状態は直したが、`seed | 1` によるシード衝突（SOL-01）が残る。Sol の phase-1 レビュー自体がこの問題を検出していない（Astra の phase-1 レビューは C.2 で指摘している）。
- **指摘 3**: 安全メソッドの CPU 検査をホットパスの `assert!` で行い、組み込み関数は非インラインのまま（SOL-03, SOL-14）。
- **指摘 5**: 系列の誤りは解消したが、SIMD fill は呼び出しごとにブロック残りを捨てる（SOL-06）。
- **指摘 18 / 20**: 機能は正しいが `TwistedGFSR` の性能（SOL-09）、rand_core の語順（SOL-12）。
- **指摘 22**: README のその他の誤記（`WRng`、C シグネチャ、`SeedGen` パス）も修正済みで、この点は完了。

### 修正によって新たに発生した問題

- **SOL-04**: `sfmt_rand_f64s` の削除。FIX_REPORT の「C の関数名…は保持」は事実と異なる。
- **SOL-05**: C# バインディングがコンパイル不能に後退（SFMT 型の未宣言）。FIX_REPORT は C# 実行を未検証と明記しているが、生成物のコンパイルも確認されていない。
- **SOL-07**: `randfv` の新たな panic 条件。
- **SOL-02/03 の一部**: `SplitMix32x16`（SIMD/C ABI）が `assert` と非インラインの最終化子で Astra 版の 2.3〜3.3 倍遅い。`TwistedGFSR::nextu` の `#[inline]` 消失（SOL-09）。

### 仕様変更として妥当なもの

- アルゴリズム修正による出力系列の変更、Rust から C ABI を呼ぶ際の `unsafe` 化、Rust API の不正範囲・不正重みでの panic と C ABI の no-op、`RngV` の `scale=0` を 2^32 幅とする規約、偏りのある 1 語変換 `Word::to_randi/to_index` の明文化、ChaCha/Salsa の `from_key_nonce` 追加、intrinsic 型の x86_64 限定。いずれも README / FIX_REPORT に記載がある。ただしバージョン番号の更新が必要（SOL-11）。

### FIX_REPORT の記述について

- 性能比較は 11 ケースのみで、退行が最も大きい SIMD 型・SIMD C ABI（`splitmix32x16` 17.9×、`sfc32x16_rand_i32s` 40×、`Biski64x8`/`Sfc64x8` がスカラーより 10 倍遅い等）を含まない。`wide SplitMix32x16` 2.17× は本レビューの全レーン測定（2.1×）と整合。
- 「129 構成すべて成功」「`cargo doc` 警告ゼロ」は本レビューでも再現（64 組み合わせ + doc 警告 0）。

## 4. 性能評価

### 測定条件

- 同一マシン・同一ハーネス（`harness/common/bench.rs`）を 1.0.0・Sol・Astra の各ソースにリンク。`opt-level=3` の非 LTO（依存クレートとしての一般的な構成）と、出荷 cdylib と同じ fat LTO・codegen-units=1 の 2 構成。各ケースは 150 ms 以上の反復を 7 回計測した中央値、さらに実行順を入れ替えた 2 回（base→Sol→Astra / Astra→Sol→base）の平均。実行時に決まる範囲のケースは 4 回の中央値（デスクトップ常駐アプリによる揺らぎが大きかったため）。
- SIMD/wide は全レーンを消費する計測に修正済み（1 レーンだけ使う初版では LLVM が未使用レーンを除去し、同一コードで最大 18 倍の見かけ上の差が出たため除外した）。
- **注意: 1.0.0 は多くの項目で誤った実装、または異なる系列（C ABI はチャンク毎再シード・rayon 並列）である。「誤った高速実装」と「正しい実装」の比較になる行は備考に明記した。** C ABI の 1.0.0 は 16 論理 CPU の並列実行で、括弧内は `RAYON_NUM_THREADS=1` の値。

### 主要結果（ns/語。比 = Sol / 1.0.0）

| ケース | 1.0.0 | Sol | Astra | Sol 比 | 備考 / 分類 |
| --- | ---: | ---: | ---: | ---: | --- |
| `Xoshiro256Pp::nextu` | 0.498 | 0.500 | 0.493 | 1.00 | 変化なし |
| `Sfc64::nextu` | 0.506 | 0.681 | 0.675 | 1.35 | 1.0.0 は誤った式。必要なコスト |
| `Xoroshiro128Pp::nextu` | 0.683 | 0.783 | 0.769 | 1.15 | 1.0.0 は誤った式。必要なコスト |
| `Philox32::nextu` | 4.075 | 1.721 | 1.588 | 0.42 | 1.0.0 は 20 ラウンド・乗算鍵（誤り） |
| `ChaCha20::nextu` | 7.129 | 4.616 | 3.911 | 0.65 | 1.0.0 は 40 ラウンド（誤り） |
| `TwistedGFSR::nextu` | 0.783 | 3.164 | 3.117 | 4.04 | 2 語連結で約 2× は必要。残りは `#[inline]` 欠如（SOL-09） |
| `Sfc32::randi(0,999)`（定数範囲） | 0.729 | 1.040 | 3.092 | 1.43 | 1.0.0 は偏りあり |
| `Sfc32::randi`（実行時範囲） | 1.099 | 1.782 | 4.502 | 1.62 | 拒否法＋毎回除算（SOL-08） |
| `Xoshiro256Pp::randi`（実行時範囲） | 1.283 | 2.451 | 3.395 | 1.91 | 同上 |
| `Sfc32::randf`（実行時範囲） | 0.910 | 1.976 | 2.260 | 2.17 | 1.0.0 は上端を返し得る。検証と補正のコスト |
| `Sfc64::shuffle(1M)`（ns/要素） | 1.129 | 2.241 | 3.626 | 1.98 | 1.0.0 は偏ったシャッフル |
| `Alias32::sample(16)` | 5.613 | 6.589 | 8.526 | 1.17 | 拒否法 `index` |
| `simd::Sfc32x8::nextu` | 0.750 | 0.407 | 0.816 | 0.54 | 1.0.0 は AVX2 CPU で不正命令になる実装 |
| `simd::SplitMix32x16::nextuv` | 0.904 | 1.879 | 0.803 | 2.08 | 1.0.0 は別アルゴリズム。assert＋非インライン（SOL-03） |
| `simd::Philox32x4x4::nextu` | 0.483 | 1.328 | 1.362 | 2.75 | 1.0.0 は誤った Philox。スカラー×4 |
| `simd::Biski64x8::nextu` | 3.681 | 4.003 | 0.392 | 1.09 | 非 `target_feature`（SOL-03） |
| `simd::Sfc64x8::nextu` | 6.928 | 6.848 | 0.465 | 0.99 | 同上 |
| `simd::Xoshiro128Ppx16::nextuv` | 3.997 | 4.089 | 3.988 | 1.02 | 3 版とも非インライン |
| `wide::SplitMix32x16::nextu` | 0.370 | 0.773 | 0.782 | 2.09 | 1.0.0 は重複ブロック・別の混合関数。必要なコスト |
| `wide::Xoshiro128Ppx16::randi(0,999)` | 0.753 | 2.236 | 3.571 | 2.97 | 1.0.0 は偏りあり。レーン毎の拒否と除算 |
| `cabi::xoshiro128pp_next_u32s[1M]` | 0.179 (0.699) | 0.801 | 1.334 | 4.48 | 並列撤去（系列の厳密化） |
| `cabi::mt19937_next_u32s[64M]` | 0.152 (0.802) | 0.883 | 1.006 | 5.83 | 同上 |
| `cabi::philox32_next_u32s[64M]` | 0.295 (2.970) | 2.360 | 2.446 | 8.00 | 同上 |
| `cabi::xoshiro256pp_next_u64s[1M]` | 0.088 (0.543) | 1.212 | 1.426 | 13.85 | 同上 |
| `cabi::sfc32x16_next_u32s[1M]` | 0.027 (0.080) | 0.368 | 0.356 | 13.62 | SIMD 経路の最適化不足（SOL-02） |
| `cabi::splitmix32x16_next_u32s[1M]` | 0.172 (0.168) | 3.078 | 0.920 | 17.86 | 1.0.0 は別アルゴリズム。Sol 固有の遅さ（SOL-02/03） |
| `cabi::philox32x4x4_next_u32s[1M]` | 0.160 (0.167) | 1.303 | 1.319 | 8.16 | 1.0.0 は誤った Philox |
| `cabi::xoshiro128ppx16_next_u32s[1M]` | 4.226 (4.263) | 4.492 | 4.451 | 1.06 | SIMD 経路がスカラーより遅い |
| `cabi::pcg32x8_next_u32s[1M]` | 0.178 (0.297) | 0.550 | 0.535 | 3.09 | |
| `cabi::sfc32x16_rand_i32s(0,999)[1M]` | 0.032 (0.091) | 1.286 | 1.840 | 40.62 | 1.0.0 は偏りあり・並列 |
| `cabi::xoshiro256pp_rand_f64s(0,1)[1M]` | 0.136 (0.802) | 1.191 | 1.563 | 8.78 | 1.0.0 は並列・半開保証なし |

LTO 構成（出荷 cdylib と同条件）の主要値: `xoshiro128pp_next_u32s[1M]` 0.178 / 1.165 / 1.277、`xoshiro256pp_next_u64s[1M]` 0.093 / 0.781 / 1.253、`splitmix32x16_next_u32s[1M]` 0.177 / 1.995 / 0.943、`sfc32x16_next_u32s[1M]` 0.029 / 0.367 / 0.405（1.0.0 / Sol / Astra）。全表: `harness/out/bench-table.md`（非 LTO）、`bench-table-lto.md`（LTO）、`bench-table-lanes*.md`（全レーン SIMD/wide）、`bench-table-bb-median.txt`（実行時範囲）。生データは `out/bench-*.csv`。

### 性能低下の分類

| 分類 | 該当 |
| --- | --- |
| 正確性・安全性確保に必要なコスト | SFC64/xoroshiro128++ の正しい式、拒否サンプリングと半開区間保証（randi/index/shuffle/sampler/randf）、TwistedGFSR の 64 ビット化（約 2× まで）、C ABI スカラー fill の並列撤去（系列の厳密化）、wide SplitMix32 の正しい最終化子（約 2×） |
| 最適化不足の可能性 | SIMD 型・SIMD C ABI の非インライン組み込み関数と毎回の CPU 判定（SOL-02/03）、`TwistedGFSR` の `#[inline]` 欠如（SOL-09）、毎回の除算（SOL-08）、Philox32x4x4/SplitMix32x16 のスカラー化、系列を保った並列 fill の未提供 |
| 原因不明 | なし（`Xoshiro128Pp`/`Jsf32` の 1.03〜1.06× は 2 回の差 8〜13% の範囲で測定誤差と判断） |
| 測定誤差の可能性 | 1.0.0 の並列 C ABI（1M 語）の一部で 2 回の差 18〜30%、実行時範囲ケースの個別回の揺らぎ（中央値で評価） |

### Sol のベンチマーク自体の妥当性（`examples/fix-bench.rs`）

- 9 回の中央値・ウォームアップ・出力の `black_box`・全レーン計数と手法は妥当。ただし対象が 11 ケースで、最大の退行がある SIMD 型 Rust API と SIMD C ABI を含まず、範囲生成は定数範囲のみ（閾値計算がコンパイル時に畳み込まれ得る）。fat LTO の値のみで、LTO 無しで使う下流利用者の性能は示していない。

## 5. 実行した検証

| # | コマンド / ハーネス | 結果 | 保存先 |
| --- | --- | --- | --- |
| 1 | `cargo test --all-features --locked --no-fail-fast`（スクラッチコピー） | 成功: lib 161, cabi_regression 59, cipher_regression 4, numeric_regression 13, sampler_rand_regression 4, doctest 107（1 ignored）+1。失敗 0 | `logs/cargo-test-all-features.log` |
| 2 | `python ref/check_vectors.py`（参照実装の公式ベクタ照合） | 62/62 成功 | `harness/out/check_vectors.log` |
| 3 | `python ref/check_xoshiro_c.py`（公式 C ソースをビルドして照合。jump は GF(2) 行列冪） | 全成功 | `harness/out/check_xoshiro_c.log` |
| 4 | `dump.exe` + `ref/compare_dump.py sol` | 793/793 系列一致 | `harness/out/compare-sol.log` |
| 5 | `cabi.exe`（56 ファミリ） | 5,544 チェック全成功 | `harness/out/cabi-sol.log` |
| 6 | `probe.exe seed_pairs / xorshift32_zero / alias_small / cipher_exhaust / threefry_wrap / addr_offsets` | SOL-01・SOL-10 を再現、ゼロ状態なし、Alias 正常、wide Threefry 非反復、C ABI 出力はアドレス非依存 | `harness/out/probe-*-sol.log` |
| 7 | `numeric.exe randi_enum`（2^32 語総当たり、7 範囲） | 全範囲で理論値と完全一致 | `harness/out/numeric-randi_enum-sol.log` |
| 8 | `numeric.exe randi64_edge / randf_edge / index / shuffle / sampler / simdconv / randcore` | randf 160 ケース全て半開区間内、index(3) 総当たり一様、χ²=116.9、サンプラー正常、SOL-07/12 を再現 | `harness/out/numeric-*-sol.log` |
| 9 | `build_matrix.ps1`（64 feature 組み合わせ、doctest、`build --all-targets`、`doc`、nightly `-Zbuild-std` で aarch64/i686/ppc64） | 全成功（1.0.0 は非 x86_64 + simd で失敗） | `harness/out/build-matrix-sol.log`, `build-matrix-base.log` |
| 10 | `ref/evex_scan.py`（AVX2 C ABI 入口から到達する EVEX 命令の静的探索） | Sol: 0（1.0.0 は `vprold` に到達＝陽性対照） | 本文 §2/§3 |
| 11 | DLL エクスポート比較（`llvm-objdump -p`）、生成 `RngNative.cs` の照合と `dotnet build` | SOL-04、SOL-05（CS0246×108） | `harness/out/csharp-build-sol.log` |
| 12 | `bench.exe`（base/Sol/Astra、非 LTO と LTO、各 2〜4 回） | §4 | `harness/out/bench-*.csv`, `bench-table*.md` |

- 参照実装・期待値: `harness/ref/refimpl.py`（独自実装）、`harness/vectors/`（MT19937/MT19937-64 公式出力、SFMT 全 10 パラメータ公式出力、Random123 `kat_vectors`）、Bouncy Castle の Skein 1.3 Threefish-256 KAT、RFC 8439 §2.3.2、RFC 7914 §8 と OpenSSL scrypt、pcg-c-basic `pcg32-demo` の実行出力、prng.di.unimi.it の公式 C ソース、biski64 公式 README の参照式。
- 実行できなかった検証: AVX-512 非搭載 CPU 上の実行、big-endian 実機、Miri、C# の P/Invoke 実行、統計検定スイート、Linux 実機ビルド（§1 参照）。

## 6. 総括

- **採用上の重大な障害**: SOL-01（シード 2k/2k+1 の系列同一）。修正は数行だが全生成器の出力が変わるため、採用するなら同時に行うべき。互換性面では SOL-04（シンボル削除）と SOL-05（C# バインディングのコンパイル不能）。
- **長所**: 3 版の中で、アルゴリズム修正の網羅性（Threefry/Threefish の標準化、wide Threefry の桁上げ、Alias の修正を含む）、移植性（非 x86_64 ビルド）、C ABI の防御的検証（重なり検査・`MaybeUninit`）が最も優れている。
- **残存リスク**: C ABI・SIMD 経路の性能（SOL-02/03）、big-endian の SFMT（SOL-13）、ChaCha のカウンタ枯渇（SOL-10）。
- **優先して修正すべき事項**: (1) SOL-01、(2) SOL-04（旧シンボルのエイリアス）、(3) SOL-05（SFMT 型の宣言・入力追加・生成 C# のコンパイル CI）、(4) SOL-02/03（`#[target_feature]` 化とホットパスの CPU 判定除去、SIMD fill のブロック単位コピー）、(5) SOL-06（未消費語の保持）、(6) SOL-11（2.0.0 としてリリース）。
- **追加検証の推奨**: AVX2 のみの CPU（または Intel SDE）での実行、big-endian での SFMT 出力、Miri による C ABI スカラー経路、PractRand/TestU01、C# からの P/Invoke 実行、Linux 実機の CI。

## 作業ログ（再開用）

- ハーネス: `harness/setup.ps1 -Target sol` で `_src/`（HEAD）・`_base/`（386eb4d）を展開。`run-target/`（Sol）・`run-base/`（1.0.0）で `cargo build --release --features v_sol --bin <dump|cabi|probe|numeric|bench>`（LTO は `--profile lto`）。
- ベース `386eb4d` の `src/` ツリーは Astra の `19bc224` と同一（ツリーハッシュ `ef46088…`）。
- 性能データは Sol/Astra 両レビュー共通の同一測定（`harness/out/` に Astra 版と同じ CSV を保存）。
