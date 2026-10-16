# urng 1.0.0 修正版レビュー — Astra 修正版（レビュアー: Opus 5.5）

> 状態: 完了（2026-10-16）。§1〜§2・§4〜§6 は Astra の REVIEW.md / FIX_REPORT.md を参照せずに作成し、その時点で WIP コミット（`e8843b8`）した。§3 はその後に両文書を読んで突き合わせた。

## 1. 総合評価

| 項目 | 内容 |
| --- | --- |
| 対象モデル | Astra |
| 対象ディレクトリ | `urng-review_astra/` |
| 対象コミット | `cb07a9a`（修正本体 `3379c33`、報告書 `cb07a9a`、ベース `19bc224` = urng 1.0.0 + phase 1 REVIEW.md。`src/` は urng 1.0.0 と同一ツリー） |
| 実施環境 | Windows 11 Pro 10.0.26200 / AMD Ryzen 7 9800X3D（Zen 5、AVX2・AVX-512F/VL/DQ/BW 対応、16 論理 CPU）/ 64 GB / rustc 1.98.1 stable（x86_64-pc-windows-msvc, LLVM 22.1.8）、nightly-2026-10-08（`-Zbuild-std` クロスチェック用）/ Python 3.13.15 / gcc（Strawberry）/ llvm-objdump / .NET SDK 10.0.401 |
| 方法 | 原本を `git archive` でハーネス内に展開してビルド（原本ディレクトリでは一切ビルドしていない）。対象コードから独立した参照実装（Python。公式ベクタで事前検証）と公式 C ソースで系列を照合し、C ABI・数値変換・サンプラー・ビルド構成・機械語・性能を実測した |

### 結論の要約

- **修正した箇所の正確性は高い。** 独立参照実装と照合し、スカラー 46 型（＋xoroshiro128 の jump/long_jump）・SIMD 19 型の全レーン・wide 38 型の全レーンで 793/793 系列が Astra のシード方式どおりに一致。Philox（4x32/2x64/4x64）、ChaCha20（RFC 8439）、Salsa20、PCG32、SFC64、xoroshiro128++/long_jump、Biski64x8、xoroshiro64**x8/x16 等の 1.0.0 の誤りは正しく修正。`randi`/`index` は 2^32 語総当たりで厳密に一様、`randf` は半開区間、shuffle は不偏。`seed | 1` によるシード衝突も（wide Squares を除き）解消した。
- **C ABI の設計は 3 版で最も良い。** 未消費語をハンドルに保持し、スカラー・SIMD とも呼び出し分割に依存しない連続ストリーム（4,982 チェック全成功）。AVX2 型から AVX-512 命令は機械語レベルで到達しない。`Biski64x8`/`Sfc64x8` に `#[target_feature]` を付け、SIMD 版が実際にスカラーより速くなったのも Astra だけ。
- **一方で、修正対象外に残った 1.0.0 の欠陥が重大。** (1) `Alias32`/`Alias64` が重み `[2, 2, 3]` のような単純な入力でも誤った確率で標本化し（28.6% であるべき要素が 61.9%）、ゼロ重み要素も選ばれ得る（AST-01, High）。(2) 3 つの u32 シードで `Xorshift32` が永久に 0 を出力（AST-02）。(3) `simd`（したがって `full`）feature が x86_64 以外でコンパイル不能（AST-03）。(4) `Threefry32x4`（スカラー・wide）は文書に反して Random123 Threefry ではなく、wide Threefry は 2^32 ブロックで系列が反復する（AST-04、実行で確認）。(5) 生成される C# バインディングがコンパイル不能（AST-07）。
- 性能面では `Rng::randi` が u128 剰余により実行時範囲で 1.0.0 比 3〜4 倍・Sol 比 1.4〜2.5 倍遅く、shuffle・サンプラー・C ABI 範囲生成に波及（AST-05）。C ABI 一括生成も 1.0.0（並列・別系列）比で大幅に遅く、スカラー fill は Sol よりも遅い（AST-06）。

### 重大な問題の件数

| 重要度 | 件数 | ID |
| --- | --- | --- |
| Critical | 0 | — |
| High | 1 | AST-01 |
| Medium | 6 | AST-02, AST-03, AST-04, AST-05, AST-06, AST-07 |
| Low | 8 | AST-08 〜 AST-15 |
| Info | 2 | AST-16, AST-17 |

### 検証できた範囲 / 未検証範囲

- 検証済み: 全スカラー・SIMD・wide 生成器の出力系列（独立参照）、C ABI 全 56 ファミリ、`randi`（2^32 語総当たり）、`randf`（160 の敵対的ケース）、`index`、shuffle（χ²）、サンプラー（ゼロ・極端値・不正値・300 通りの分布掃引・厳密 f32 エミュレーション）、rand_core 連携、64 feature 組み合わせ、非 x86_64 クロスチェック、AVX2 経路の機械語検査、C# バインディングのコンパイル、wide Threefry の 2^32 ブロック周期試験、性能（同一環境・同一ハーネスで base/Sol/Astra、非 LTO/LTO）。
- 未検証: AVX2 のみの CPU での実行（機械語の静的検査で代替）、big-endian での実行時動作、Miri、C# からの実行、統計検定スイート、Linux 実機ビルド（`-Zbuild-std` クロスチェックのみ）。

## 2. 発見した問題

### AST-01 [High] `Alias32` / `Alias64` が誤った分布で標本化する（ゼロ重み要素も選ばれ得る、未修正）

- **対象**: `src/sampler/b32/alias.rs:36-74`（特に `:56` の `while let (Some(s), Some(l)) = (small.pop(), large.last().copied())`、`:39` の検証なしの総和、`:66-70`）、`src/sampler/b64/alias.rs` の同じ行
- **技術的原因**: タプル左辺の `small.pop()` が `large` の空判定より先に評価されるため、`large` が空になった時点で `small` から取り出した要素が失われる（`prob=0`、`alias=0` のまま）。浮動小数点の丸めで最後の large 要素の残差が 1.0 をわずかに下回ると、この経路に入る。失われた列の確率質量は常に要素 0 へ流れる。さらに重みの検証が無く（負値・NaN・∞・総和の溢れ・全ゼロ）、`f32::MAX` 同士の総和は ∞ になる。
- **影響範囲**: `Alias32`/`Alias64` によるすべての重み付き抽出。
- **再現方法**: `harness/common/probe.rs alias_small`、`harness/common/numeric.rs sampler`、厳密な f32 エミュレーション `harness/ref/alias_emulate.py`。
- **実際の結果**:
  - `Alias32::new(&mut rng, &[2.0, 2.0, 3.0])` を 100 万回: 観測 `[0.6191, 0.2856, 0.0953]`、期待 `[0.2857, 0.2857, 0.4286]`。`Alias64` も `[0.6189, 0.2860, 0.0951]`。`Alias32` の `[1, 5, 7]` は `[0.4105, 0.0511, 0.5384]`（期待 `[0.0769, 0.3846, 0.5385]`）（`out/probe-alias_small-astra.log`）。
  - 重み 1〜12 の整数で n=2..3 のベクトル 164 通りが確率誤差 0.05 超（エミュレーション、`out/alias_emulate-astra.log`）。ランダム重み 300 通りの掃引で Alias32・Alias64 とも 73 通りが誤差 1% 超、最悪 0.50。
  - `[f32::MAX, 0, f32::MAX]` でゼロ重みの要素 1 を 1/3 の確率で返す。`[f32::MAX, f32::MAX]` は常に 0。`[NaN, 1]`、`[-1, 2]`、`[0, 0]`、`[∞, 1]` も panic せず任意の結果。
  - 同じ入力で Sol 修正版は正しい分布。
- **期待される結果**: 各要素が重みに比例した確率で選ばれ、ゼロ重みは選ばれない。不正な重みは拒否。
- **修正方針**: 両スタックの非空を確認してから pop する（`while !small.is_empty() && !large.is_empty()`）。重みを最大値で正規化して総和の溢れを防ぎ、有限・非負・正の総和を検証する。
- **検証状態**: 実証済み

### AST-02 [Medium] `Xorshift32` 系が永久ゼロ状態になるシードが存在する（未修正）

- **対象**: `src/prng/b32/xorshift.rs:27-30`（`Xorshift32::new`）、同 `:117-`（`Xorshift32x8::new`）、`src/wide/xorshift.rs:34`（`Xorshift32xN::new`）、C ABI `xorshift32_new`
- **技術的原因**: 状態を SplitMix32 の 1 出力で初期化し 0 を除外していない。Xorshift64 にだけゼロ回避を追加し 32 ビット版を漏らした。`0x61C88647 = -γ` は状態 `seed+γ = 0` を経由し、残り 2 つは SplitMix32 最終化子の零点。
- **影響範囲**: 該当シードの `Xorshift32` は 0 しか出力しない（`randi` は常に最小値、`nextf` は常に 0.0）。SIMD/wide 版は該当レーンが 0 固定。
- **再現方法**: `probe.exe xorshift32_zero`（全 2^32 シードを総当たり）。
- **実際の結果**: `Xorshift32::new(0x61C88647)`, `(0xF7AA06C3)`, `(0xD7BDEF1B)` → `[0, 0, 0, ...]`。C ABI と `wide::Xorshift32x4` のレーン 0 も同じ（`out/probe-xorshift32_zero-astra.log`）。Sol 修正版は該当シードなし。
- **修正方針**: Xorshift64 と同じ `if state == 0 { 1 } else { state }` を Xorshift32 / Xorshift32x8 / wide の各レーンに適用。
- **検証状態**: 実証済み

### AST-03 [Medium] `simd`（および `full`）feature が x86_64 以外でコンパイル不能（未修正）

- **対象**: `src/lib.rs:9-12`（`#[cfg(feature = "simd")] mod rngv;` に `target_arch` 条件なし）、`src/rngv.rs:1`（`use std::arch::x86_64::*;`）、`src/cbrng/b32/philox.rs:2`、`src/prng/**` の `#[cfg(feature = "simd")] pub mod simd`、`src/prng/b32/xorshift.rs:118`
- **影響範囲**: Apple Silicon・ARM Linux・32 ビット x86・big-endian で `features = ["full"]` や `simd` を有効にした依存クレートがビルド不能。README は「`simd` targets x86_64」と記すが、非対応ターゲットではハードエラーになる。
- **再現方法**: `cargo +nightly check --lib -Zbuild-std=core,std --target aarch64-unknown-linux-gnu --features simd`（`harness/build_matrix.ps1`）。
- **実際の結果**: aarch64 31 エラー、i686 22 エラー、powerpc64 31 エラー（`--all-features` も同じ。`out/build-matrix-astra.log`）。1.0.0 も失敗。Sol は全ターゲットで成功。
- **修正方針**: SIMD 関連のモジュール・再エクスポート・テストを `#[cfg(all(feature = "simd", target_arch = "x86_64"))]` で囲む。
- **検証状態**: 実証済み

### AST-04 [Medium] `Threefry32x4`（スカラー・wide）が Random123 Threefry ではなく、wide Threefry は 2^32 ブロックで反復する

- **対象**: `src/cbrng/b32/threefry.rs:45-47`（tweak を SplitMix32 から生成）、`:129-132`（出力をカウンタと XOR）、`src/wide/threefry.rs:176-177`, `:240-243`（同）、`src/wide/threefry.rs:100`, `:255`（`c0 += 1` のみで上位語へ桁上げしない）、`src/cbrng/b64/threefish.rs:137-140`（Threefish256 の出力 XOR）
- **技術的原因**: 1.0.0 の tweak 付き・カウンタ XOR 付き変種をそのまま残している（Random123 Threefry は tweak 無し・出力 XOR 無し）。wide 版の 2x32/4x32 は最下位カウンタ語だけを加算する。
- **影響範囲**: (1) 型の文書「A Threefry4x32 random number generator (Random123 family)」と実装が不一致で、Random123 の KAT や他ライブラリの Threefry 系列と照合・再現できない。(2) `wide::Threefry32x2x{4,8,16}` / `Threefry32x4x{4,8,16}` は各レーンで 2^32 ブロック（2x32 は 2^33 語、4x32 は 2^34 語）後に同じ系列を繰り返す。(3) `Threefish256` は `E_k,t(ctr) XOR ctr` を返し Threefish-256 の暗号文そのものではない。
- **再現方法**: `python ref/threefry_kat_demo.py out/dump-astra.txt`、`probe.exe threefry_wrap`。
- **実際の結果**: `Threefry32x4::new(0)` の第 1 ブロック `[adac674d, 893a65e2, 90317864, aa2cae8b]` は、同じ鍵での Random123 threefry4x32-20 `[bce7d479, 9f84e6fa, 8f889494, d5aa49f0]` と一致せず、1.0.0 の tweak+XOR 変種と一致（`out/threefry_kat_demo-astra.log`）。`wide::Threefry32x2x4::new(7)` はブロック 2^32, 2^32+1 がブロック 0, 1 と完全一致（60.8 秒の実行、`out/probe-threefry_wrap-astra.log`。Sol 版は不一致＝正しく桁上げ）。
- **期待される結果**: Random123 Threefry4x32-20（tweak=0、XOR 無し）。wide のカウンタは多語として桁上げ。
- **修正方針**: tweak を 0、出力 XOR を削除。wide のカウンタに桁上げ（`simd_eq(0)` マスクで伝播）を追加。Threefish256 の XOR は仕様として文書化するか削除。
- **検証状態**: 実証済み

### AST-05 [Medium] `Rng::randi` が候補語ごとに u128 剰余を計算し大幅に遅い

- **対象**: `src/rng.rs:87-95`（`try_randi` 内で `(1u128 << $bits) % range` を毎回計算）、`:141-151`（`randi` のループ）、`:182-196`（`array_randi`）、`src/rngv.rs:57-`
- **技術的原因**: 32 ビット語でも 128 ビット剰余（`__umodti3` 呼び出し）を候補語ごとに行う。閾値は範囲だけで決まり、呼び出しごとに 1 回（Lemire 法なら大半の呼び出しで 0 回）で済む。
- **影響（ns/語、実行時に決まる範囲は 4 回の中央値）**: 実行時範囲の `Sfc32::randi(0,999)` 4.50（1.0.0: 1.10、Sol: 1.78）、`Xoshiro256Pp::randi` 3.40（1.28 / 2.45）。定数範囲でも非 LTO で 3.09（0.73 / 1.04）。`shuffle(1M)` 3.63 ns/要素（1.13 / 2.24）、`Alias32::sample` 8.53（5.61 / 6.59）、`wide::Xoshiro128Ppx16::randi` 3.57（0.75 / 2.24）、C ABI `sfc32x16_rand_i32s` 1.84（Sol 1.29）。fat LTO で定数範囲の場合のみ 1.11 まで縮むが、実行時範囲では LTO でも 4.19。
- **修正方針**: 32 ビットでは u32、64 ビットでは u64 の `wrapping_neg() % range` を呼び出しごとに 1 回だけ計算し、低位語が `range` 以上なら剰余を省略する Lemire 法を使う。
- **分類**: 拒否法の必要コストを大きく超える最適化不足
- **検証状態**: 実証済み（性能測定。正確性は総当たりで確認済み）

### AST-06 [Medium] C ABI 一括生成の大幅な性能低下（スカラー fill は Sol より遅い）

- **対象**: `src/cabi/stream.rs:30-75`（`Stream` の `[W; 1]` 保留バッファを 1 語ごとに経由）、全 fill 関数
- **技術的原因**: 1.0.0 の rayon 並列・チャンク毎再シードを撤去（系列を厳密にするための判断）したうえ、スカラー生成器も N=1 の保留バッファと位置判定を 1 語ごとに通す。SIMD 型は `RngV::nextuv`（非 `target_feature`）経由で組み込み関数が非インライン。
- **影響（1 M 語, ns/語, 非 LTO）**: `xoshiro128pp_next_u32s` 1.33（1.0.0 並列 0.18 / 単一スレッド 0.70、Sol 0.80）、`xoshiro256pp_next_u64s` 1.43（Sol 1.21）、`mt19937` 1.08（Sol 0.87）、`sfc32x16` 0.36（1.0.0 単一スレッド 0.080）、`xoshiro128ppx16` 4.45（同じ Astra のスカラー版 1.33 より遅い）。LTO 構成でも `xoshiro128pp` 1.28、`xoshiro256pp` 1.25（Sol 1.17 / 0.78）。
- **修正方針**: スカラー型は保留バッファを経由しない直接ループにする（スカラーには保留語が生じない）。SIMD 型は `#[target_feature]` 付き内部関数でブロック単位にコピーし、保留語は先頭・末尾だけで処理。系列を保った並列化はカウンタ型・jump 可能型で提供できる。
- **検証状態**: 実証済み（性能測定）

### AST-07 [Medium] 生成される C# バインディングがコンパイル不能（1.0.0 からの後退）

- **対象**: `build.rs:91-145`（csbindgen 入力）、`src/prng/b32/mersenne.rs:352-542`（SFMT 型はマクロ生成）、`src/cabi/prng/b32/mersenne.rs`（SFMT の C ABI を明示関数として展開）
- **技術的原因**: SFMT の C ABI 関数を csbindgen が読める形に展開した一方、`Sfmt607`〜`Sfmt216091` の構造体はマクロ生成のため csbindgen に見えず、生成 C# が未宣言の型を参照する。
- **影響範囲**: クリーンビルドの `RngNative.cs` を C# プロジェクトに入れると **CS0246 が 108 件**（`Sfmt607, Sfmt1279, Sfmt2281, Sfmt4253, Sfmt11213, Sfmt44497, Sfmt86243, Sfmt132049, Sfmt216091`）でビルド失敗。1.0.0 の生成物はコンパイルできた（関数は不足）。FIX_REPORT の「320/320 一致（C# 宣言を含む）」は宣言の存在の照合で、C# としてのコンパイルは検証されていない。
- **再現方法**: `dotnet build`（net10.0、`AllowUnsafeBlocks`）に生成 `RngNative.cs` を入れる（`out/csharp-build-astra.log`）。
- **修正方針**: SFMT 型を csbindgen が読める形で宣言する（またはハンドルを `void*` にする）、CI で生成 C# をコンパイルする。
- **検証状態**: 実証済み

### AST-08 [Low] `RngV::randiv` が全幅（scale=0）や i32 を超える範囲で panic し、`to_randi`（折り返し）と不整合

- **対象**: `src/rngv.rs:57-81`（`i32::try_from(max).expect("integer lane range exceeds i32")`）、`:90`（`randfv` は上限保証なし）
- **再現**: `numeric.exe simdconv`：`randiv(scale=0, min=0)`、`randiv(scale=10, min=i32::MAX-2)`、`randiv(scale=u32::MAX, min=0)` が PANIC。同じ引数の `to_randi` は値を返す。`randfv(scale=1, min=2^24)` は上限 `min+scale` と等しい値を返す（「範囲保証なし」と文書化済み）。1.0.0 の `randiv` は panic しなかった。
- **修正方針**: 折り返しに統一するか、panic 条件を trait 文書に明記する。
- **検証状態**: 実証済み

### AST-09 [Low] `TwistedGFSR::new(u64)` がシード上位 32 ビットを無視し、`nextu` が `#[inline]` を失って 4 倍遅い

- **対象**: `src/prng/b64/twisted_gfsr.rs:31`（`SplitMix32::new(seed as u32)`、文書化済み）、`:56`/`:61`
- **再現**: `TwistedGFSR::new(5) == TwistedGFSR::new(5 | 0xDEADBEEF << 32)`（`out/probe-seed_pairs-astra.log`）。`nextu` 0.78 → 3.12 ns/語（2 語連結で約 2 倍は必要、残りは非インライン）。
- **検証状態**: 実証済み

### AST-10 [Low] `wide::Squares32x{4,8}` に `seed | 1` が残存

- **対象**: `src/wide/squares.rs:39`
- **再現**: `wide::Squares32x8::new(2) == new(3)`。他の生成器では `| 1` を除去済みで不整合。
- **検証状態**: 実証済み

### AST-11 [Low] `Bst32`/`Bst64` が有限の重みでも総和が溢れると panic

- **対象**: `src/sampler/b32/bst.rs:41`、`src/sampler/b64/bst.rs` 同等箇所
- **再現**: `Bst32::new(&mut rng, &[f32::MAX, f32::MAX])` が panic（`numeric.exe sampler`）。最大値で正規化すれば受理できる（Sol は受理）。
- **検証状態**: 実証済み

### AST-12 [Low] README の誤記が残存

- `README.md:89-90`: wide の例が存在しないトレイト `WRng`（実際は `RngW`）、`rust,norun` は無効な指定。
- `README.md:238`: `void* mt19937_new(uint32_t seed, size_t warm);`（実際は `seed` のみ）。
- `README.md:149`: `SeedGen` のモジュールを `urng::seedgen` と記載（非公開。実際は `urng::SeedGen`）。
- `README.md:35`: 32 ビット表の `Philox32x4`（実在しない型名。実際は `Philox32`）。`README.md:32-33` 等の SFC32/SFC64 の周期表記も不正確。
- **検証状態**: 静的解析で確認

### AST-13 [Low] rand_core 連携が不完全・慣例と不一致（未修正）

- 64 ビット生成器（`Xoshiro256Pp` 等 16 型）は `SeedableRng` のみで `TryRng` 未実装（`src/rand/b64/mod.rs`）。rand の RNG として渡せない（Sol は実装済み）。
- `try_next_u64` が `(第1語 << 32) | 第2語`（`src/_internal.rs:641-644`）で、rand_core の `utils::next_u64_via_u32`（第 1 語が下位）および自身の `try_fill_bytes`（LE）と不一致（`out/numeric-randcore-astra.log`）。
- `SeedableRng::from_seed` が `u32::from_ne_bytes`（`src/_internal.rs:678`）でエンディアン依存。
- **検証状態**: 実証済み（語順・未実装）、静的解析（エンディアン）

### AST-14 [Low] ChaCha8/20 の 32 ビットブロックカウンタ枯渇で panic

- **対象**: `src/crng/b32/chacha.rs:98-101`
- **再現**: `ChaCha20::from_key_nonce(key, nonce, u32::MAX)` の 1 ブロック後に panic（`probe.exe cipher_exhaust`）。`ChaCha8::new(seed)` は 2^32 ブロック（256 GiB）で panic し、長時間の RNG 利用で到達し得る。RNG 用途には 64 ビットカウンタが望ましい。
- **検証状態**: 実証済み（`from_key_nonce` 経由）

### AST-15 [Low] バージョンが 1.0.0 のまま（互換性を破る変更を含む）

- 出力系列（シード展開・`randi`/`index`/shuffle・PCG 増分）、C ABI の Rust 側シグネチャ（安全→`unsafe`）、ハンドルの内部構造（旧版のポインタとの非互換）、新たな panic 条件を含むが `version = "1.0.0"`。README/FIX_REPORT に系列変更の注記あり。
- **検証状態**: 静的解析で確認

### AST-16 [Info] SFMT の 128 ビットシフトがネイティブエンディアン依存（big-endian で仕様と不一致の懸念）

- `src/prng/b32/mersenne.rs:32-39`、`src/prng/b64/mersenne.rs:32-39`。1.0.0 由来・未修正。`simd` 無しなら big-endian でもビルドできるため顕在化し得る。
- **検証状態**: 未検証の懸念（静的解析のみ）

### AST-17 [Info] C ABI の重なり未検査・未初期化出力への `&mut [T]`

- `src/cabi/stream.rs:58-70`: 出力とハンドルの重なりを検査しない（文書で禁止済み。Sol は検査して no-op）。未初期化の可能性がある C 側バッファに `from_raw_parts_mut` で `&mut [T]` を作る（整数・浮動小数では実害は知られていないが、`MaybeUninit` で書く方が厳密）。
- **検証状態**: 静的解析で確認（契約違反入力は UB のため実行試験せず）

## 3. 既存修正の評価

Astra の phase-1 REVIEW.md（B1〜B21、U1、U2、提案 C.1〜C.7）と FIX_REPORT.md の主張を、上記の独立検証と突き合わせた。

### 正しく修正されていると確認できた問題

| Astra 指摘 | 内容 | 独立検証での確認 |
| --- | --- | --- |
| B1 Critical | `Xoshiro256Ssx2` の範囲外ストア | 2 レーンが独立 xoshiro256** 参照と一致。広幅ストア無し |
| B2 High | C ABI が safe な extern | 全 fill/free が `unsafe`。null/不整列/過大 count/不正範囲で no-op（cabi 4,982 チェック）。※AST-17 |
| B3 High | AVX2 経路の AVX-512VL 命令・CPU 検査 | 機械語検査で AVX2 入口から EVEX 0。安全なコンストラクタで CPU 検査。※AST-03 |
| B4 High | ChaCha/Salsa | RFC 8439 / RFC 7914 / OpenSSL scrypt で検証した参照と一致。※AST-14 |
| B5 High | チャンク間の系列重複 | 連続ストリーム化。C ABI 出力が Rust 系列と一致 |
| B6 High | wide SplitMix の重複ブロック | 平坦化出力がスカラー系列と一致 |
| B7 High | Philox C ABI のアドレス依存・重複 | 16 通りの 4 バイトオフセットで同一出力（`probe.exe addr_offsets`）。分割・連続呼び出しでも系列が連続（SIMD を含め分割非依存なのは 3 版で Astra のみ） |
| B8 High | Xorshift64 のゼロ状態 | 修正済み。※Xorshift32 は未修正（AST-02） |
| B9 High | PCG の偶数増分 | 全 inc が奇数（dump） |
| B10 High | xoroshiro128++ と long_jump | 公式 C と GF(2) 行列冪で一致 |
| B11 High | Philox32 の鍵更新・ラウンド数 | Random123 と一致（4x32/2x64/4x64） |
| B12 High | SFC64 の `c` 更新 | 一致 |
| B13 High | Biski64x8・xoroshiro64**x8/x16 | 全レーン一致 |
| B14 High | 初期ゼロバッファ | 最初の語から正しいブロック |
| B15 High | TwistedGFSR の 32 ビット出力 | 64 ビット語を生成。※AST-09 |
| B16 High | SSE `to_randi` の奇数レーン | 全レーン整合 |
| B17 Medium | 整数範囲のオーバーフロー・偏り | 2^32 語総当たりで厳密一様、全幅も正常。※AST-05（性能） |
| B18 Medium | 浮動小数範囲の上端・NaN | 160 ケース全て半開区間内、`[-MAX, MAX]` も正常 |
| B19 Medium | 偏ったシャッフル | χ²=116.9（df=119） |
| B20 Medium | Bst のゼロ重み選択 | Bst は修正済み。※Alias は未修正（AST-01）、Bst の総和溢れ（AST-11） |
| B21 Medium | `Readme.md` の大小文字 | `include_str!("../README.md")` と実ファイル名の一致を静的確認（Astra の packaging テストも同趣旨） |
| U1 | NT ストアの fence | NT 経路を撤去 |
| U2 | C ABI の連続性 | 連続ストリームを仕様化し実装（本レビューの分割試験で全ファミリ一致） |
| C.2 | `seed \| 1` によるシード衝突 | SplitMix32/64・Xoroshiro128Pp・wide Pcg/SplitMix で除去を確認。※wide Squares は残存（AST-10） |

### 修正が不完全な問題

- **B8**: Xorshift64 のみ修正し、Xorshift32 / Xorshift32x8 / wide Xorshift32 のゼロ状態シード（AST-02）が残る。
- **B20**: Bst のみ修正し、同じ「重み付きサンプラー」の Alias（AST-01）は未修正。Astra の phase-1 レビューが Alias の `while let` の欠陥を検出していなかった。
- **B3**: x86_64 専用 intrinsic を `target_arch` で囲んでおらず、非 x86_64 で `simd`/`full` がビルド不能（AST-03）。README の記述とも不一致。
- **C.2**: wide Squares に `seed | 1` が残る（AST-10）。
- **B15**: 上位シードビットの無視（文書化済み）と性能（AST-09）。
- **C.5（rand の 64 ビット API）**: 対応していない（AST-13）。FIX_REPORT にも未対応と明記。
- **C# バインディング**: 欠落 54 関数の追加は確認したが、生成物がコンパイルできない（AST-07）。
- **Threefry**: phase-1 レビューで扱われておらず、Random123 非互換と wide カウンタの桁上げ欠如（AST-04）が残る（Sol の phase-1 レビューは後者を検出・修正している）。

### 修正によって新たに発生した問題

- **AST-05**: 新しい `try_randi` の u128 剰余による `randi`/`index` 系の大幅な低速化。
- **AST-07**: C# バインディングがコンパイル不能に後退。
- **AST-08**: `RngV::randiv` の新たな panic 条件。
- **AST-06 の一部**: スカラー型の C ABI fill が N=1 の保留バッファを経由するため、同じく逐次化した Sol より最大 1.7 倍遅い（`xoshiro128pp` 1.33 対 0.80 ns/語。`sfc32`・`philox32` はほぼ同等）。

### 仕様変更として妥当なもの

- アルゴリズム修正による出力系列の変更、Rust から C ABI を呼ぶ際の `unsafe` 化、C ABI ハンドルの不透明化（対応する `new`/`free` でのみ管理）、未消費語を保持する連続ストリーム、Rust API の不正入力での panic と C ABI の no-op、`RngV` の `scale=0` を 2^32 幅とする規約、`WordV::to_randf`/`randfv` を範囲保証なしの低水準アフィン変換と明記、ChaCha/Salsa の `from_key_nonce` 追加。ただしバージョン番号の更新が必要（AST-15）。

### FIX_REPORT の記述について

- 性能比較（PCG32 randi 3.55×、SFC64 randi 2.75×、SFC64 randf 2.75×、C ABI SFC64x8 23.98× 等）は、範囲を `black_box` で隠した本レビューの実行時範囲ケース（randi 3〜4×、randf 2.5〜3.2×）と整合する。測定対象に SIMD 型の Rust API と大半の SIMD C ABI 型は含まれない。
- 「C#宣言…320/320 一致」は宣言の存在照合であり、生成 C# のコンパイル不能（AST-07）を検出できていない。

## 4. 性能評価

### 測定条件

- 同一マシン・同一ハーネス（`harness/common/bench.rs`）を 1.0.0・Sol・Astra の各ソースにリンク。`opt-level=3` の非 LTO（依存クレートとしての一般的な構成）と、出荷 cdylib と同じ fat LTO・codegen-units=1 の 2 構成。各ケースは 150 ms 以上の反復を 7 回計測した中央値、さらに実行順を入れ替えた 2 回の平均。実行時範囲ケースは 4 回の中央値。SIMD/wide は全レーンを消費する計測に修正済み（初版の 1 レーン消費では LLVM が未使用レーンを除去し、同一コードで最大 18 倍の見かけ上の差が出たため除外）。
- **注意: 1.0.0 は多くの項目で誤った実装、または異なる系列（C ABI はチャンク毎再シード・rayon 並列）である。「誤った高速実装」と「正しい実装」の比較になる行は備考に明記した。** C ABI の 1.0.0 は 16 論理 CPU の並列実行で、括弧内は `RAYON_NUM_THREADS=1` の値。

### 主要結果（ns/語。比 = Astra / 1.0.0）

| ケース | 1.0.0 | Sol | Astra | Astra 比 | 備考 / 分類 |
| --- | ---: | ---: | ---: | ---: | --- |
| `Xoshiro256Pp::nextu` | 0.498 | 0.500 | 0.493 | 0.99 | 変化なし |
| `Sfc64::nextu` | 0.506 | 0.681 | 0.675 | 1.33 | 1.0.0 は誤った式。必要なコスト |
| `Xoroshiro128Pp::nextu` | 0.683 | 0.783 | 0.769 | 1.13 | 1.0.0 は誤った式。必要なコスト |
| `Philox32::nextu` | 4.075 | 1.721 | 1.588 | 0.39 | 1.0.0 は 20 ラウンド・乗算鍵（誤り） |
| `ChaCha20::nextu` | 7.129 | 4.616 | 3.911 | 0.55 | 1.0.0 は 40 ラウンド（誤り） |
| `Threefish256::nextu` | 12.931 | 12.856 | 13.801 | 1.07 | コードは 1.0.0 と同一（初期 `pos` のみ変更）。LTO で 1.22×、原因不明 |
| `TwistedGFSR::nextu` | 0.783 | 3.164 | 3.117 | 3.98 | 2 語連結で約 2× は必要。残りは `#[inline]` 欠如（AST-09） |
| `Sfc32::randi(0,999)`（定数範囲） | 0.729 | 1.040 | 3.092 | 4.24 | u128 剰余（AST-05） |
| `Sfc32::randi`（実行時範囲） | 1.099 | 1.782 | 4.502 | 4.10 | 同上 |
| `Xoshiro256Pp::randi`（実行時範囲） | 1.283 | 2.451 | 3.395 | 2.65 | 同上 |
| `Sfc32::randf`（実行時範囲） | 0.910 | 1.976 | 2.260 | 2.48 | 1.0.0 は上端を返し得る。検証と補正のコスト |
| `Sfc64::shuffle(1M)`（ns/要素） | 1.129 | 2.241 | 3.626 | 3.21 | 1.0.0 は偏ったシャッフル＋AST-05 |
| `Alias32::sample(16)` | 5.613 | 6.589 | 8.526 | 1.52 | AST-05（分布自体も誤り: AST-01） |
| `simd::Sfc32x8::nextu` | 0.750 | 0.407 | 0.816 | 1.09 | 1.0.0 は AVX2 CPU で不正命令。Astra は AVX2 補助関数が非 LTO で非インライン（LTO 0.405） |
| `simd::SplitMix32x16::nextuv` | 0.904 | 1.879 | 0.803 | 0.89 | 1.0.0 は別アルゴリズム |
| `simd::Philox32x4x4::nextu` | 0.483 | 1.328 | 1.362 | 2.82 | 1.0.0 は誤った Philox。スカラー×4 |
| `simd::Biski64x8::nextu` | 3.681 | 4.003 | 0.392 | 0.11 | `#[target_feature]` 付与で 9 倍以上高速化 |
| `simd::Sfc64x8::nextu` | 6.928 | 6.848 | 0.465 | 0.07 | 同上 |
| `simd::Xoshiro128Ppx16::nextuv` | 3.997 | 4.089 | 3.988 | 1.00 | 3 版とも非インライン（スカラーの 8 倍遅い） |
| `wide::SplitMix32x16::nextu` | 0.370 | 0.773 | 0.782 | 2.11 | 1.0.0 は重複ブロック・別の混合関数。必要なコスト |
| `wide::Xoshiro128Ppx16::randi(0,999)` | 0.753 | 2.236 | 3.571 | 4.74 | 1.0.0 は偏りあり。AST-05 |
| `cabi::xoshiro128pp_next_u32s[1M]` | 0.179 (0.699) | 0.801 | 1.334 | 7.47 | 並列撤去＋N=1 保留バッファ（AST-06） |
| `cabi::mt19937_next_u32s[64M]` | 0.152 (0.802) | 0.883 | 1.006 | 6.63 | 同上 |
| `cabi::philox32_next_u32s[64M]` | 0.295 (2.970) | 2.360 | 2.446 | 8.29 | 同上 |
| `cabi::xoshiro256pp_next_u64s[1M]` | 0.088 (0.543) | 1.212 | 1.426 | 16.30 | 同上 |
| `cabi::sfc32x16_next_u32s[1M]` | 0.027 (0.080) | 0.368 | 0.356 | 13.17 | SIMD 経路の最適化不足 |
| `cabi::splitmix32x16_next_u32s[1M]` | 0.172 (0.168) | 3.078 | 0.920 | 5.34 | 1.0.0 は別アルゴリズム |
| `cabi::philox32x4x4_next_u32s[1M]` | 0.160 (0.167) | 1.303 | 1.319 | 8.26 | 1.0.0 は誤った Philox |
| `cabi::xoshiro128ppx16_next_u32s[1M]` | 4.226 (4.263) | 4.492 | 4.451 | 1.05 | SIMD 経路がスカラーより遅い |
| `cabi::pcg32x8_next_u32s[1M]` | 0.178 (0.297) | 0.550 | 0.535 | 3.01 | |
| `cabi::sfc32x16_rand_i32s(0,999)[1M]` | 0.032 (0.091) | 1.286 | 1.840 | 58.12 | 1.0.0 は偏りあり・並列。AST-05 |
| `cabi::xoshiro256pp_rand_f64s(0,1)[1M]` | 0.136 (0.802) | 1.191 | 1.563 | 11.52 | 1.0.0 は並列・半開保証なし |

LTO 構成（出荷 cdylib と同条件）の主要値: `xoshiro128pp_next_u32s[1M]` 0.178 / 1.165 / 1.277、`xoshiro256pp_next_u64s[1M]` 0.093 / 0.781 / 1.253、`sfc32x16_rand_i32s[1M]` 0.034 / 1.229 / 2.064、実行時範囲 `Sfc32::randi` 1.10 / 1.99 / 4.19（1.0.0 / Sol / Astra）。全表は `harness/out/bench-table.md`（非 LTO）、`bench-table-lto.md`（LTO）、`bench-table-lanes*.md`（全レーン SIMD/wide）、`bench-table-bb-median.txt`（実行時範囲）。生データは `out/bench-*.csv`。測定は Sol レビューと共通。

### 性能低下の分類

| 分類 | 該当 |
| --- | --- |
| 正確性・安全性確保に必要なコスト | SFC64/xoroshiro128++ の正しい式、拒否法と半開区間保証の基本コスト、TwistedGFSR の 64 ビット化（約 2× まで）、C ABI の並列撤去（系列の厳密化）、wide SplitMix32 の正しい最終化子（約 2×） |
| 最適化不足の可能性 | `try_randi` の候補語ごとの u128 剰余（AST-05）、C ABI スカラー fill の N=1 保留バッファ（AST-06）、SIMD C ABI と `Xoshiro128Ppx16` 等 `RngV` の非インライン組み込み関数、`TwistedGFSR` の `#[inline]` 欠如、AVX2 補助関数 `avx2::rol_epi32` の非インライン（非 LTO の `Sfc32x8`）、Philox32x4x4 のスカラー化、系列を保った並列 fill の未提供 |
| 原因不明 | `Threefish256` の LTO 構成のみ 1.22×（出力 XOR 以外の差は不明。非 LTO は 1.07×） |
| 測定誤差の可能性 | 1.0.0 の並列 C ABI（1M 語）の一部で 2 回の差 18〜30%、実行時範囲ケースの個別回の揺らぎ（中央値で評価） |

### Astra のベンチマーク自体の妥当性（`tools/review_bench.rs`）

- 範囲を `black_box` で隠す・9 回×3 組の中央値・実行順の入れ替え・測定条件（`metadata.json`）の保存と、手法は妥当。対象は 13 ケースで、SIMD 型の Rust API と大半の SIMD C ABI 型を含まない。1.0.0 側の Rayon を 4 スレッドに制限しており、既定（論理 CPU 数）の利用者が体感する C ABI の差（本レビューでは最大 16〜58 倍）より小さく見える点に注意。

## 5. 実行した検証

| # | コマンド / ハーネス | 結果 | 保存先 |
| --- | --- | --- | --- |
| 1 | `cargo test --all-features --locked --no-fail-fast`（スクラッチコピー） | 成功: lib 114, algorithms 8, cabi 59, packaging 1, ranges 6, simd_conversions 2, doctest 104+1。失敗 0 | `logs/cargo-test-all-features.log` |
| 2 | `python ref/check_vectors.py` | 62/62 成功 | Sol 側 `harness/out/check_vectors.log`（参照実装は同一ファイル） |
| 3 | `python ref/check_xoshiro_c.py` | 全成功 | 同上 |
| 4 | `dump.exe` + `ref/compare_dump.py astra` | 793/793 系列一致（Threefry32x4/Threefish256/wide Threefry32x4 は 1.0.0 の tweak+XOR 変種として） | `harness/out/compare-astra.log` |
| 5 | `cabi.exe`（56 ファミリ） | 4,982 チェック全成功、全ファミリ連続ストリーム | `harness/out/cabi-astra.log` |
| 6 | `probe.exe seed_pairs / xorshift32_zero / alias_small / cipher_exhaust / threefry_wrap / addr_offsets` | AST-02/04/09/10/14、AST-01 を再現。C ABI 出力はアドレス非依存 | `harness/out/probe-*-astra.log` |
| 7 | `numeric.exe randi_enum`（2^32 語総当たり） | 全範囲で理論値と完全一致 | `harness/out/numeric-randi_enum-astra.log` |
| 8 | `numeric.exe randi64_edge / randf_edge / index / shuffle / sampler / simdconv / randcore` | randf 160 ケース全て半開区間内、index(3) 総当たり一様、χ²=116.9、AST-01/08/11/13 を再現 | `harness/out/numeric-*-astra.log` |
| 9 | `ref/alias_emulate.py`（Alias32 構築の厳密 f32 エミュレーション） | 小さな整数重みで 164 通りの誤り | `harness/out/alias_emulate-astra.log` |
| 10 | `ref/threefry_kat_demo.py` | AST-04 | `harness/out/threefry_kat_demo-astra.log` |
| 11 | `build_matrix.ps1` | x86_64 は全 64 組み合わせ・doctest・doc 成功（doc 警告 9）。非 x86_64 + simd は失敗（AST-03） | `harness/out/build-matrix-astra.log`, `build-matrix-base.log` |
| 12 | `ref/evex_scan.py` | AVX2 入口から EVEX 命令 0（1.0.0 は `vprold` に到達＝陽性対照） | 本文 |
| 13 | DLL エクスポート比較、生成 `RngNative.cs` の照合と `dotnet build` | 320/320 宣言（1.0.0 と同一シンボル集合）、コンパイルは CS0246×108（AST-07） | `harness/out/csharp-build-astra.log` |
| 14 | `bench.exe`（base/Sol/Astra、非 LTO と LTO、各 2〜4 回） | §4 | `harness/out/bench-*.csv`, `bench-table*.md` |

- 参照実装・期待値は Sol レビューと同じ（`harness/ref/refimpl.py`、`harness/vectors/`、Bouncy Castle の Threefish-256 KAT、RFC 8439/7914、OpenSSL scrypt、pcg32-demo、prng.di.unimi.it の公式 C、biski64 参照式）。
- 実行できなかった検証: AVX-512 非搭載 CPU 上の実行、big-endian 実機、Miri、C# の P/Invoke 実行、統計検定スイート、Linux 実機ビルド。

## 6. 総括

- **採用上の重大な障害**: AST-01（Alias サンプラーの誤分布）。加えて AST-02（ゼロ状態シード）、AST-03（非 x86_64 で `full` がビルド不能）、AST-04（Threefry の非互換と wide の周期反復）、AST-07（C# バインディングのコンパイル不能）。
- **長所**: C ABI の連続ストリーム設計と C# 関数の網羅（コンパイル問題を除く）、シード衝突の解消、`Biski64x8`/`Sfc64x8` の `#[target_feature]` 化。
- **残存リスク**: `randi` 系の性能（AST-05）、C ABI の性能（AST-06）、big-endian の SFMT（AST-16）、ChaCha のカウンタ枯渇（AST-14）。
- **優先して修正すべき事項**: (1) AST-01、(2) AST-02、(3) AST-03、(4) AST-04、(5) AST-07、(6) AST-05（u32/u64 剰余と Lemire 法）、(7) AST-06、(8) AST-15（2.0.0 としてリリース）。
- **追加検証の推奨**: AVX2 のみの CPU（または Intel SDE）での実行、big-endian での SFMT、Miri、PractRand/TestU01、C# の P/Invoke 実行、Linux 実機 CI。

## 作業ログ（再開用）

- ハーネス: `harness/setup.ps1 -Target astra` で `_src/`・`_base/` を展開。`run-target/` で `cargo build --release --features v_astra --bin <dump|cabi|probe|numeric|bench>`（LTO は `--profile lto`）。
- 性能データは Sol レビューと共通の同一測定（`harness/out/` に同じ CSV を保存）。
