# urng 1.0.0 修正版レビュー — Astra 修正版（レビュアー: Opus 5.5）

> 状態: **独立レビュー部分（§1, §2, §4, §5, §6）完了。§3 は独立レビュー完了後に Astra の REVIEW.md / FIX_REPORT.md を参照して記入。**

## 1. 総合評価

| 項目 | 内容 |
| --- | --- |
| 対象モデル | Astra |
| 対象ディレクトリ | `urng-review_astra/` |
| 対象コミット | `cb07a9a`（修正本体 `3379c33`、文書 `cb07a9a`、ベース `19bc224` = urng 1.0.0 + phase 1 REVIEW.md。`src/` は urng 1.0.0 と同一） |
| 実施環境 | Windows 11 Pro 10.0.26200 / AMD Ryzen 7 9800X3D（Zen 5、AVX2・AVX-512F/VL/DQ/BW 対応）/ 64 GB / rustc 1.98.1 stable（x86_64-pc-windows-msvc）, nightly-2026-10-08（`-Zbuild-std` のクロスチェック用）/ Python 3.13.15 / gcc（Strawberry）/ LLVM 22（llvm-objdump） |
| 検証方法 | 原本を `git archive` でハーネス内へ展開してビルド（原本ディレクトリでは一切ビルドしていない）。対象コードから独立した参照実装（Python、公式ベクタで事前検証）と公式 C ソースで系列を照合 |

### 結論の要約

- **修正した部分の正確性は高い。** 独立参照実装と照合し、スカラー 47 型・SIMD 17 型の全レーン・wide 38 型の全レーンで 793/793 系列が Astra のシード方式どおりに一致。Philox（4x32/2x64/4x64）、ChaCha20（RFC 8439）、Salsa20、PCG32、SFC64、xoroshiro128++/long_jump、Biski64x8、xoroshiro64**x8/x16 などの 1.0.0 の誤りは正しく修正されている。`randi`/`index` は 2^32 語総当たりで厳密に一様、`randf` は半開区間を保証、shuffle は不偏。
- **C ABI は最も良く設計されている。** 未消費語をハンドルに保持し、スカラー・SIMD とも呼び出し分割に依存しない連続ストリーム（4,982 チェック全成功）。C# バインディングは 320/320 関数を網羅。AVX2 型から AVX-512 命令は機械語レベルで到達しない。
- **一方で、修正されずに残った 1.0.0 の欠陥が重大。** (1) `Alias32`/`Alias64` が重み `[2, 2, 3]` のような単純な入力でも誤った確率で標本化し（期待 28.6% の要素が 61.9%）、ゼロ重みの要素も選ばれ得る（AST-01, High）。(2) 3 つの u32 シードで `Xorshift32` が永久に 0 を出力（AST-02）。(3) `simd`（したがって `full`）feature が x86_64 以外でコンパイル不能（AST-03）。(4) `Threefry32x4`（スカラー・wide）は文書に反して Random123 Threefry ではなく、wide 版はカウンタが桁上げせず 2^32 ブロックで周期する（AST-04）。
- 性能面では `Rng::randi` が u128 剰余により 1.0.0 比 4.2 倍・Sol 比 3 倍遅く、shuffle・サンプラー・C ABI の範囲生成に波及（AST-05）。C ABI 一括生成も 1.0.0（並列・別系列）比で大幅に遅い（AST-06）。一方 `Biski64x8`/`Sfc64x8` に `#[target_feature]` を付けて 10 倍以上高速化した点は優れている。

### 重大な問題の件数

| 重要度 | 件数 | ID |
| --- | --- | --- |
| Critical | 0 | — |
| High | 1 | AST-01 |
| Medium | 5 | AST-02, AST-03, AST-04, AST-05, AST-06 |
| Low | 8 | AST-07 〜 AST-14 |
| Info（未検証の懸念を含む） | 2 | AST-15, AST-16 |

### 検証できた範囲 / 未検証範囲

- 検証済み: 全スカラー・SIMD・wide 生成器の出力系列（独立参照）、C ABI 全 56 ファミリ、`randi`（2^32 語総当たり）、`randf`、`index`、shuffle、サンプラー（ゼロ・極端値・不正値・ランダム重みの分布掃引）、rand_core 連携、全 64 feature 組み合わせ、非 x86_64 クロスチェック、AVX2 経路の機械語検査、性能（同一環境・同一ハーネスで base/Sol/Astra を比較）。
- 未検証: AVX2 のみの CPU での実行（機械語の静的検査で代替）、big-endian での実行時動作、Miri、C# 実行、統計検定スイート。

## 2. 発見した問題

### AST-01 [High] `Alias32` / `Alias64` が誤った分布で標本化する（ゼロ重み要素も選ばれ得る、未修正）

- **対象**: `src/sampler/b32/alias.rs:36-74`（特に `:56` の `while let (Some(s), Some(l)) = (small.pop(), large.last().copied())`、`:39` の検証なしの総和、`:66-70`）、`src/sampler/b64/alias.rs` 同じ行
- **技術的原因**: タプルの左辺 `small.pop()` が `large` の空判定より先に評価されるため、`large` が空になった時点で `small` から取り出した要素が失われる（`prob=0`、`alias=0` のまま）。浮動小数点の丸めで最後の large 要素の残差が 1.0 をわずかに下回ると `large` が先に空になり、この経路に入る。失われた列の確率質量は常に要素 0 へ流れる。さらに重みの検証が無く（負値・NaN・∞・総和の溢れ・全ゼロ）、`f32::MAX` 同士の総和は ∞ になって全要素が `small` に入る。
- **影響範囲**: `Alias32`/`Alias64` を使うすべての重み付き抽出。乱数の重み付き選択という目的そのものが満たされない。
- **再現方法**: `harness/common/probe.rs alias_small`、`harness/common/numeric.rs sampler`、厳密な f32 エミュレーション `harness/ref/alias_emulate.py`。
- **実際の結果**:
  - `Alias32::new(&mut rng, &[2.0, 2.0, 3.0])` を 100 万回: 観測 `[0.6191, 0.2856, 0.0953]`、期待 `[0.2857, 0.2857, 0.4286]`。`Alias64` も `[0.6189, 0.286, 0.0951]`。`[1, 5, 7]` でも `Alias32` は `[0.4105, 0.0511, 0.5384]`（期待 `[0.0769, 0.3846, 0.5385]`）。
  - 重みが 1〜12 の整数の n=2..3 ベクトルのうち 164 通りで確率誤差が 0.05 超（エミュレーション）。ランダム重み 300 通り掃引で Alias32・Alias64 とも 73 通りが誤差 1% 超、最悪 0.50。
  - `[f32::MAX, 0, f32::MAX]` でゼロ重みの要素 1 を 1/3 の確率で返す。`[f32::MAX, f32::MAX]` は常に 0 を返す。`[NaN, 1]`、`[-1, 2]`、`[0, 0]`、`[∞, 1]` も panic せず任意の結果を返す。
  - 同じ入力で Sol 修正版は正しい分布（`out/probe-alias_small-sol.log` 相当の比較）。
- **期待される結果**: 各要素が重みに比例した確率で選ばれ、ゼロ重みは選ばれない。不正な重みは拒否。
- **修正方針**: 両スタックの非空を確認してから pop する（`while !small.is_empty() && !large.is_empty()`）。重みを最大値で正規化して総和の溢れを防ぎ、有限・非負・正の総和を検証する。Vose 法の終了処理（残りを prob=1）は両リストに残ったものだけに適用する。
- **検証状態**: 実証済み

### AST-02 [Medium] `Xorshift32` 系が永久ゼロ状態になるシードが存在する（未修正）

- **対象**: `src/prng/b32/xorshift.rs:27-30`（`Xorshift32::new`）、同 `:117-`（`Xorshift32x8::new`）、`src/wide/xorshift.rs:34`（`Xorshift32xN::new`）、C ABI `xorshift32_new`
- **技術的原因**: 状態を SplitMix32 の 1 出力で初期化し 0 を除外していない。Astra は `SplitMix32::new` の `| 1` を除去したうえで、Xorshift64 にだけゼロ回避を追加し 32 ビット版を漏らした。`0x61C88647 = -γ` は状態 `seed+γ = 0` を経由し、残り 2 つは SplitMix32 最終化子の零点。
- **影響範囲**: 該当シードの `Xorshift32` は 0 しか出力しない（`randi` は常に最小値、`nextf` は常に 0.0）。SIMD/wide 版は該当レーンが 0 固定。
- **再現方法**: `probe.exe xorshift32_zero`（全 2^32 シードを総当たり）。
- **実際の結果**: `Xorshift32::new(0x61C88647)`, `(0xF7AA06C3)`, `(0xD7BDEF1B)` → `[0, 0, 0, ...]`。C ABI `xorshift32_new(同)` も 0 のみ、`wide::Xorshift32x4::new(同)` のレーン 0 が 0。ログ: `harness/out/probe-xorshift32_zero-astra.log`。Sol 修正版は該当シードなし。
- **修正方針**: `if state == 0 { 1 } else { state }`（Xorshift64 と同じ）を Xorshift32 / Xorshift32x8 / wide の各レーンに適用する。
- **検証状態**: 実証済み

### AST-03 [Medium] `simd`（および `full`）feature が x86_64 以外でコンパイル不能（未修正）

- **対象**: `src/lib.rs:9-12`（`#[cfg(feature = "simd")] mod rngv;` に `target_arch` 条件なし）、`src/rngv.rs:1`（`use std::arch::x86_64::*;`）、`src/cbrng/b32/philox.rs:2`、`src/prng/**` の `#[cfg(feature = "simd")] pub mod simd`、`src/prng/b32/xorshift.rs:118`（`is_x86_feature_detected!`）
- **影響範囲**: Apple Silicon・ARM Linux・32 ビット x86・big-endian で `features = ["full"]` や `simd` を有効にした依存クレートがビルドできない。README は「`simd` targets x86_64」と記すが、feature を有効化すると x86_64 以外では警告ではなくハードエラーになる。
- **再現方法**: `cargo +nightly check --lib -Zbuild-std=core,std --target aarch64-unknown-linux-gnu --features simd`（`harness/build_matrix.ps1`）。
- **実際の結果**: aarch64 31 エラー、i686 22 エラー、powerpc64 31 エラー（`--all-features` も同じ）。1.0.0 も同様に失敗。Sol 修正版は全ターゲットで成功。ログ: `harness/out/build-matrix-astra.log`。
- **修正方針**: SIMD 関連のモジュール・再エクスポート・テストを `#[cfg(all(feature = "simd", target_arch = "x86_64"))]` で囲む（Sol の方式）。
- **検証状態**: 実証済み

### AST-04 [Medium] `Threefry32x4`（スカラー・wide）が Random123 Threefry ではなく、wide 版はカウンタが桁上げしない

- **対象**: `src/cbrng/b32/threefry.rs:45-47`（tweak を SplitMix32 から生成）、`:129-132`（出力をカウンタと XOR）、`src/wide/threefry.rs:176-177`, `:240-243`（同）、`src/wide/threefry.rs:100`, `:255`（`c0 += 1` のみで `c1..c3` へ桁上げしない）、`src/cbrng/b64/threefish.rs:137-140`（Threefish256 の出力 XOR）
- **技術的原因**: 1.0.0 の tweak 付き・カウンタ XOR 付きの変種をそのまま残している（Random123 の Threefry は tweak 無しで出力の XOR も無い）。wide 版の 2x32/4x32 は最下位カウンタ語だけを加算する。
- **影響範囲**: (1) 型の文書「A Threefry4x32 random number generator (Random123 family)」と実装が一致せず、Random123 の KAT や他ライブラリの Threefry 系列と照合・再現できない。(2) `wide::Threefry32x2x{4,8,16}` / `wide::Threefry32x4x{4,8,16}` は各レーンで 2^32 ブロック（4x32 は 2^34 語、2x32 は 2^33 語）後に同じ系列を繰り返す。(3) `Threefish256` も `E_k,t(ctr) XOR ctr` を返し Threefish-256 の暗号文そのものではない（KAT 照合不可）。
- **再現方法**: `python ref/threefry_kat_demo.py out/dump-astra.txt`（`out/threefry_kat_demo-astra.log`）、`ref/compare_dump.py`（Astra の出力は tweak+XOR 変種としてのみ一致）、wide のカウンタ周期は `probe.exe threefry_wrap`（§5 参照）。
- **実際の結果**: `Threefry32x4::new(0)` の最初のブロック `[adac674d, 893a65e2, 90317864, aa2cae8b]` は、同じ鍵での Random123 threefry4x32-20(ctr=0) `[bce7d479, 9f84e6fa, 8f889494, d5aa49f0]` と一致せず、1.0.0 の tweak+XOR 変種と一致。`Threefish256::new(0)` のブロック 1 は `E(1) XOR 1` と一致。
- **期待される結果**: Random123 Threefry4x32-20（tweak=0、XOR 無し）。wide 版は 128 ビット（4x32）/64 ビット（2x32）カウンタとして桁上げ。
- **修正方針**: tweak を 0、出力 XOR を削除（Sol の方式）。wide のカウンタに桁上げ（`simd_eq(0)` マスクで伝播）を追加。Threefish256 は XOR の有無を仕様として文書化するか削除。
- **検証状態**: 実証済み（Threefry/Threefish の非互換）。wide カウンタ周期は §5 の `threefry_wrap` の結果を参照。

### AST-05 [Medium] `Rng::randi` が候補語ごとに u128 剰余を計算し 1.0.0 比 4.2 倍・Sol 比 3 倍遅い

- **対象**: `src/rng.rs:87-95`（`try_randi` 内の `(1u128 << $bits) % range` を毎回計算）、`:141-151`（`randi` のループ）、`:182-196`（`array_randi`）、`src/rngv.rs:57-` 
- **技術的原因**: 32 ビット語でも 128 ビット剰余（`__umodti3` 呼び出し）を候補語ごとに行う。閾値は範囲だけで決まり、呼び出しごとに 1 回（さらに Lemire 法なら大半の呼び出しで 0 回）で済む。
- **影響（非 LTO, ns/語）**: `Sfc32::randi(0,999)` 3.09（1.0.0: 0.73、Sol: 1.04）、`Xoshiro256Pp::randi(0,999)` 2.71（0.66 / 0.70）、`shuffle(1M)` 3.63 ns/要素（1.13 / 2.24）、`Alias32::sample` 8.53（5.61 / 6.59）、`wide::Xoshiro128Ppx16::randi` 3.30（0.49 / 2.18）、C ABI `sfc32x16_rand_i32s` 1.84（Sol 1.29）。
- **修正方針**: `$bits == 32` では u64、64 ビットでは u64 の `wrapping_neg() % range` を使い、閾値を呼び出しごとに 1 回だけ計算する。さらに低位語が `range` 以上なら剰余を省略する Lemire 法を使う。
- **分類**: 正確性確保（拒否法）の必要コストを大きく超える最適化不足。
- **検証状態**: 実証済み（性能測定。正確性は 2^32 語総当たりで確認済み）

### AST-06 [Medium] C ABI 一括生成の大幅な性能低下（スカラー fill は Sol より遅い）

- **対象**: `src/cabi/stream.rs:30-75`（`Stream` の `[W; 1]` 保留バッファを 1 語ごとに経由）、全 fill 関数
- **技術的原因**: 1.0.0 の rayon 並列・チャンク毎再シードを撤去した（系列を厳密にするための判断）うえ、スカラー生成器も N=1 の保留バッファと位置判定を 1 語ごとに通す。SIMD 型は `RngV::nextuv`（非 `target_feature`）経由で組み込み関数が非インライン。
- **影響（1 M 語、非 LTO, ns/語）**: `xoshiro128pp_next_u32s` 1.33（1.0.0 並列 0.18 / 単一スレッド 0.70、Sol 0.80）、`xoshiro256pp_next_u64s` 1.43（Sol 1.21）、`mt19937` 1.08（Sol 0.87）、`sfc32x16` 0.36（1.0.0 単一スレッド 0.080）、`xoshiro128ppx16` 4.45（同じ Astra のスカラー版より遅い）。
- **修正方針**: スカラー型は保留バッファを経由しない直接ループにする（スカラーは保留語が生じない）。SIMD 型は `#[target_feature]` 付き内部関数でブロック単位にコピーし、保留語だけを先頭・末尾で処理する。系列を保った並列化はカウンタ型・jump 可能型で提供可能。
- **検証状態**: 実証済み（性能測定）

### AST-07 [Low] `RngV::randiv` が全幅（scale=0）や i32 を超える範囲で panic し、`to_randi`（折り返し）と不整合

- **対象**: `src/rngv.rs:57-81`（`i32::try_from(max).expect("integer lane range exceeds i32")`）、`:90`（`randfv` は上限保証なし）
- **再現**: `numeric.exe simdconv`：`randiv(scale=0, min=0)`、`randiv(scale=10, min=i32::MAX-2)`、`randiv(scale=u32::MAX, min=0)` が PANIC。同じ引数の `to_randi` は値を返す。`randfv(scale=1, min=2^24)` は上限 `min+scale` と等しい値を返す（文書上「範囲保証なし」と明記）。1.0.0 の `randiv` は panic しなかった。
- **修正方針**: 折り返し（`to_randi` と同じ）に統一するか、panic 条件を trait 文書に明記する。
- **検証状態**: 実証済み

### AST-08 [Low] `TwistedGFSR::new(u64)` がシード上位 32 ビットを無視し、`nextu` が `#[inline]` を失って 4 倍遅い

- **対象**: `src/prng/b64/twisted_gfsr.rs:31`（`SplitMix32::new(seed as u32)`、文書化済み）、`:56`/`:61`（`#[inline]` 無し）
- **再現**: `TwistedGFSR::new(5) == TwistedGFSR::new(5 | 0xDEADBEEF << 32)`（`out/probe-seed_pairs-astra.log`）。`nextu` 0.78 → 3.12 ns/語（2 語連結で約 2 倍は必要、残りは非インライン）。
- **検証状態**: 実証済み

### AST-09 [Low] `wide::Squares32x{4,8}` に `seed | 1` が残存（シード 2k と 2k+1 が同一系列）

- **対象**: `src/wide/squares.rs:39`
- **再現**: `wide::Squares32x8::new(2) == new(3)`（他の生成器では `| 1` を除去済みで不整合）。
- **検証状態**: 実証済み

### AST-10 [Low] `Bst32`/`Bst64` が有限の重みでも総和が溢れると panic

- **対象**: `src/sampler/b32/bst.rs:41`（`assert!(sum.is_finite() && sum > 0.0, "invalid total weight")`）
- **再現**: `Bst32::new(&mut rng, &[f32::MAX, f32::MAX])` が panic（`numeric.exe sampler`）。Sol は最大値で正規化して受理する。
- **検証状態**: 実証済み

### AST-11 [Low] README の誤記が残存

- `README.md:89-90`: wide の例が存在しないトレイト `WRng` を使用（実際は `RngW`）、`rust,norun` は無効な指定。
- `README.md:238`: `void* mt19937_new(uint32_t seed, size_t warm);`（実際は `seed` のみ）。
- `README.md:149`: `SeedGen` のモジュールを `urng::seedgen` と記載（非公開モジュール。実際は `urng::SeedGen`）。
- 32 ビット表の `Philox32x4`（存在しない型名、実際は `Philox32`）、SFC32/SFC64 の周期表記。
- **検証状態**: 静的解析で確認

### AST-12 [Low] rand_core 連携が不完全・慣例と不一致（未修正）

- 64 ビット生成器（`Xoshiro256Pp` 等 16 型）は `SeedableRng` のみで `TryRng` 未実装（`src/rand/b64/mod.rs`）→ rand エコシステムの RNG として使えない（Sol は実装済み）。
- `try_next_u64` が `(第1語 << 32) | 第2語`（`src/_internal.rs:641-644`）で、rand_core の `utils::next_u64_via_u32`（第 1 語が下位）および自身の `try_fill_bytes`（LE）と不一致（`numeric.exe randcore`）。
- `SeedableRng::from_seed` が `u32::from_ne_bytes`（`src/_internal.rs:678`）でエンディアン依存。
- **検証状態**: 実証済み（語順・未実装）、静的解析（エンディアン）

### AST-13 [Low] ChaCha8/20 の 32 ビットブロックカウンタ枯渇で panic

- **対象**: `src/crng/b32/chacha.rs:98-101`
- **再現**: `ChaCha20::from_key_nonce(key, nonce, u32::MAX)` の 1 ブロック後に panic（`probe.exe cipher_exhaust`）。`ChaCha8::new(seed)` は 2^32 ブロック（256 GiB）で panic し、長時間の RNG 利用で到達し得る。RNG 用途には 64 ビットカウンタ（DJB 版 / rand_chacha）が望ましい。
- **検証状態**: 実証済み（`from_key_nonce` 経由）

### AST-14 [Low] バージョンが 1.0.0 のまま（互換性を破る変更を含む）

- 出力系列（シード展開・`randi`/`index`/shuffle・PCG 増分）、C ABI の Rust 側シグネチャ（安全→`unsafe`）、新たな panic 条件を含むが `version = "1.0.0"`。README に系列変更の注記あり。
- **検証状態**: 静的解析で確認

### AST-15 [Info] SFMT の 128 ビットシフトがネイティブエンディアン依存（big-endian で仕様と不一致の懸念）

- `src/prng/b32/mersenne.rs:32-39`、`src/prng/b64/mersenne.rs:32-39`。1.0.0 由来・未修正。`simd` 無しなら big-endian でもビルド可能なため顕在化し得る。
- **検証状態**: 未検証の懸念（静的解析のみ）

### AST-16 [Info] C ABI の重なり未検査・未初期化出力への `&mut [T]`

- `src/cabi/stream.rs:58-70`: 出力とハンドルの重なりを検査しない（文書で禁止済み、Sol は検査して no-op）。未初期化かもしれない C 側バッファに `from_raw_parts_mut` で `&mut [T]` を作る（整数・浮動小数では実害は知られていないが、`MaybeUninit` で書く方が厳密）。
- **検証状態**: 静的解析で確認（契約違反入力は UB のため実行試験していない）

## 3. 既存修正の評価

（独立レビュー完了後に記入）

## 4. 性能評価

（Sol レビューと同じ測定。§4 の表は LTO 結果とともに確定版を記入予定）

## 5. 実行した検証

| # | コマンド / ハーネス | 結果 | 保存先 |
| --- | --- | --- | --- |
| 1 | `cargo test --all-features --locked --no-fail-fast`（スクラッチコピー） | 成功: lib 114, algorithms 8, cabi 59, packaging 1, ranges 6, simd_conversions 2, doctest 104+1。失敗 0 | `logs/cargo-test-all-features.log` |
| 2 | `python ref/check_vectors.py` | 62/62 成功 | Sol 側 `harness/out/check_vectors.log`（同一ファイル） |
| 3 | `python ref/check_xoshiro_c.py` | 全成功 | 同上 |
| 4 | `dump.exe` + `ref/compare_dump.py astra` | 793/793 系列一致（Threefry32x4/Threefish256/wide Threefry32x4 は 1.0.0 の tweak+XOR 変種として） | `harness/out/compare-astra.log` |
| 5 | `cabi.exe`（56 ファミリ） | 4,982 チェック全成功、全ファミリ連続ストリーム | `harness/out/cabi-astra.log` |
| 6 | `probe.exe seed_pairs / xorshift32_zero / alias_small / cipher_exhaust` | AST-02/08/09/13、AST-01 を再現 | `harness/out/probe-*-astra.log` |
| 7 | `numeric.exe randi_enum`（2^32 語総当たり） | 全範囲で理論値と完全一致 | `harness/out/numeric-randi_enum-astra.log` |
| 8 | `numeric.exe randi64_edge / randf_edge / index / shuffle / sampler / simdconv / randcore` | randf 160 ケース全て半開区間内、index(3) 総当たり一様、shuffle χ²=116.9、AST-01/07/10/12 を再現 | `harness/out/numeric-*-astra.log` |
| 9 | `ref/alias_emulate.py`（Alias32 構築の厳密 f32 エミュレーション） | 小さな整数重みで 164 通りの誤り | `harness/out/alias_emulate-astra.log` |
| 10 | `ref/threefry_kat_demo.py` | AST-04 | `harness/out/threefry_kat_demo-astra.log` |
| 11 | `build_matrix.ps1` | x86_64 は全 64 組み合わせ成功。非 x86_64 + simd は失敗（AST-03） | `harness/out/build-matrix-astra.log`, `build-matrix-base.log` |
| 12 | `ref/evex_scan.py` | AVX2 入口から EVEX 命令 0（1.0.0 は `vprold` に到達） | 本文 |
| 13 | エクスポート表・C# バインディング照合 | 320/320（1.0.0 と同一シンボル集合） | 本文 |
| 14 | `bench.exe`（base/Sol/Astra × 2 回） | §4 | Sol 側 `harness/out/bench-*.csv`（同一測定） |

## 6. 総括

- **採用上の重大な障害**: AST-01（Alias サンプラーの誤分布）。加えて AST-02（ゼロ状態シード）、AST-03（非 x86_64 で `full` がビルド不能）、AST-04（Threefry の非互換・wide の周期）。
- **残存リスク**: `randi` 系の性能（AST-05）、C ABI の性能（AST-06）、big-endian の SFMT（AST-15）。
- **優先して修正すべき事項**: (1) AST-01、(2) AST-02、(3) AST-03、(4) AST-04、(5) AST-05。
- **追加検証の推奨**: AVX2 のみの CPU（または Intel SDE）での実行、big-endian での SFMT、Miri、PractRand/TestU01、C# の P/Invoke 実行試験。

## 作業ログ（再開用）

- ハーネス: `harness/setup.ps1 -Target astra` で `_src/`・`_base/` を展開。`run-target/` で `cargo build --release --features v_astra --bin <dump|cabi|probe|numeric|bench>`。
- 未完了: LTO 構成での C ABI 性能、wide Threefry の周期試験（`probe.exe threefry_wrap`）、§3、§4 確定版。
