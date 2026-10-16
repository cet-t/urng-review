# Opus 版の独立レビュー（Sol 担当）

## 1. 総合評価

- 対象: `urng-review_opus/`、コミット `3a9297465082c07dd2be2d2c697bdf22eef322a1`。
- 比較元: `4068051`（完全なハッシュは `baseline-revision.txt`）。担当区分は利用者指定の Sol。
- 環境: 2026-10-16、Windows 11 x86_64/MSVC、AMD Ryzen 7 9800X3D、16 logical CPUs、Rust/Cargo 1.98.1、.NET SDK 10.0.401。詳細は `environment.json`。
- 結論: scalar の主要修正と C ABI の改善を確認したが、そのままの採用には障害が残る。**High 3 件、Medium 3 件、Critical 0 件**。以下の 6 件はすべて再現済み。
- 原本の追跡ファイルは変更していない。他モデルのレビュー結果・修正報告書は参照していない。コピー、独立参照、ログをこのディレクトリに保存した。

既存テストは 196 件成功・0 件失敗、doctest は 112 件成功・2 件 ignored。別に追加した境界・参照テストは 30 件中 18 件成功・12 件失敗。原因を集約すると上記の 6 問題になる。テスト成功をもって全実装の安全性・正確性を保証してはいない。

## 2. 発見した問題

### SOL-OPUS-01 — High: wide Threefry4x32 が Random123 と異なる

- **場所:** `src/wide/threefry.rs:170,220,228,239`（対象ディレクトリ基準。以下同じ）。
- **原因・影響:** `Threefry32x4x4/x8/x16` に Threefish 風の tweak 注入、誤った最終キー添字、counter との XOR が残る。scalar だけが修正され、wide の全レーンは名前・説明の Random123 Threefry4x32-20 と一致しない。
- **再現:** `python run_checks.py probes` の `wide_threefry_*_matches_random123` 3 件。seed 1、初回 block・lane 0 から不一致。x4 の実値は `[3023759841,2343225285,1155817591,3763945978]`、期待値は `[3307560983,627444597,2532636873,236180073]`。
- **期待値の根拠:** 独立したループ実装を [Random123 公式 KAT](https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors) 3 ケースで検証し、constructor のレーン別キー配列を用いて比較した。同じ seed の scalar と直接比較したものではない。
- **修正案:** 検証済み round/key schedule を wide に適用し、tweak と counter XOR を除去。全幅・全レーンを公式ベクタと複数 counter で確認する。
- **状態:** 実証済み。既存不具合の修正漏れ。

### SOL-OPUS-02 — High: wide Threefry が 2^32 block で同じ系列へ戻る

- **場所:** `src/wide/threefry.rs:100,255`。
- **原因・影響:** 2x32/4x32 の x4/x8/x16 全幅で `c0` のみを増分し、`c1` 以降へ carry しない。64/128-bit counter の代わりに 32-bit counter を使用する。各レーンの出力量が 2x32 では 32 GiB、4x32 では 64 GiB に達すると反復する。
- **再現:** `python run_checks.py counters`。コピーにだけ `counter_probes.rs` を一時追加し、`c0=u32::MAX` という有効な境界状態を注入。2 件とも wrap 後の出力が seed 1 の先頭 block と完全一致し、`c1=[0;4]` のまま。期待値は `c1=[1;4]`。
- **限界:** 2^32 block 全体を実際に生成した実験ではない。x4 の境界状態で実証し、他の幅は共通 macro を静的確認した。runner は検証後に追記をコピーから除去する。
- **修正案:** レーンごとに overflow mask を作り、上位 counter へ carry を伝播する。
- **状態:** 実証済み。既存不具合の修正漏れ。01 の round 修正とは独立して必要。

### SOL-OPUS-03 — High: wide Xorshift32 に永久にゼロのレーンを作れる

- **場所:** `src/wide/xorshift.rs:39`（初期化）と同ファイルの `nextu`。
- **原因・影響:** scalar のゼロ状態対策が wide に反映されていない。合法な seed `0x61c88647` では最初の SplitMix32 Weyl state と出力が 0。そのまま lane 0 に設定され、XOR/shift 遷移で永続する。x4/x8/x16 全幅に影響。
- **再現:** `wide_xorshift32_*_avoids_zero_state` 3 件。lane 0 の最初の 16 出力がすべて 0。期待値は非ゼロ状態からの Xorshift 系列。
- **修正案:** 各レーンの seed 展開後にゼロを検査し、非ゼロへ置換する。入力 seed 自体が非ゼロでも発生する。
- **状態:** 実証済み。ゼロ状態修正の適用範囲が不完全。

### SOL-OPUS-04 — Medium: Alias32/64 が有効な極小重みでゼロ重みを選ぶ

- **場所:** `src/sampler/b32/alias.rs:44,50`、`src/sampler/b64/alias.rs:44,50`。
- **原因・影響:** 重み検証は通るが `scale=n/total` が infinity になる。`0*infinity` の NaN は large 側へ分類され、ゼロ重みの probability が 1 になる。
- **再現:** `Alias32::new(...,&[f32::from_bits(1),0.0])` と u64/f64 の同等入力。常に最大 word を返す合法な `Rng` で `sample()` が index 1 を返す。期待値は唯一正の重みを持つ index 0 のみ。`alias*_small_valid_weight_never_selects_zero` 2 件が失敗。
- **修正案:** 最大重み等で先に正規化して中間値を有限に保つか、`(w/total)*n` 等の計算順序を検討する。zero/subnormal の組合せを回帰テストに含める。
- **状態:** 実証済み。入力検証と通常のゼロ重み境界の修正は有効だが、数値安定性が不足。

### SOL-OPUS-05 — Medium: 有限端点の randf が NaN を返す

- **場所:** `src/rng.rs:120`。
- **原因・影響:** `to_f01()*(max-min)+min` の区間幅が overflow する。`[-f32::MAX,f32::MAX)` と f64 の同等区間で word 0 は `0*infinity` から NaN となり、追加された上端 clamp も修正できない。scalar、同じ Word を使う wide、および該当 C ABI 経路に波及する。
- **再現:** `0u32.to_randf(-f32::MAX,f32::MAX)` と u64/f64。実値 NaN、期待値は有限の区間内値（word 0 なら下端）。`float*_finite_extreme_endpoints_remain_in_range` 2 件が失敗。
- **修正案:** 幅の有限性を検査し、overflow 時は `(1-t)*min+t*max` 等で補間する。端点の丸めも引き続き確認する。
- **状態:** 実証済み。上端丸めの修正は有効だが、有限な巨大区間への対応が不完全。

### SOL-OPUS-06 — Medium: C# バインディングから 108 の公開関数が欠落

- **場所:** `build.rs:92` 以降の `.input_extern_file(...)` 一覧。
- **原因・影響:** SFC32、xoroshiro64ss、Biski64 系等の C ABI 入力ファイルが列挙されていない。macro 由来の SFMT variant 関数も取り込まれない。生成 C# はコンパイルできるが公開機能全体を使用できない。
- **再現:** release/full DLL の exports と生成 C# の `EntryPoint` を照合。DLL 322 関数に対し C# 214、欠落 108、存在しない import は 0。例: `sfc32_new`, `xoroshiro64ss_new`, `biski64_new`。全件は `csharp-inventory.json`。
- **期待値・修正案:** 全公開関数の入力を網羅し、macro 展開後の宣言を生成器へ与える。必要な opaque 型、C# コンパイル、DLL/import 集合一致まで確認する。
- **状態:** 実証済み。比較元でも 108 件欠落しており、新規導入ではない。

## 3. 既存修正の評価

独立検証で確認できた改善:

- scalar Threefry4x32 は公式 KAT で検証した参照と 3 seeds × 20 blocks 一致。
- Xoshiro256++ は著者の遷移式と 3 seeds × 1000 outputs 一致。xoroshiro128++/** の通常遷移、`jump` と `long_jump` も著者の多項式を独立適用した結果に一致。
- ChaCha8/20、Salsa8/20 は独立 block 実装に一致。ChaCha 参照は RFC 8439 公式ベクタでも検証。`Debug` は synthetic secret key を表示しない。
- full-width signed integer 変換、代表的な 64-bit generator の rand_core trait、SplitMix64/Threefry C ABI の奇数長・複数呼び出しをまたぐ継続が成功。
- 既存テストにより SIMD 参照一致、重複 block 回帰、通常の sampler 境界、shuffle 等も通過。ただし独立した網羅検証と同一視しない。

今回確認した 6 件を新規導入とは判定しない。C ABI の handle 操作や raw SIMD trait を Rust から unsafe にしたこと、exact `Rng::randi` が rejection で追加 word を消費することは合理的な仕様変更。

一方、wide と一部 C ABI の整数範囲変換は文書上も rejection のない multiply-high であり、scalar の exact-uniform `randi` と分布・消費量が異なる。一部 bulk API は sequential stream ではなく reseed した substream を返す。この差を承認せず互換と扱うことはできない。

## 4. 性能評価

同一 Rust の `-O -C panic=abort` で `bench.rs` をコンパイルし、比較元/修正版の release/full rlib へリンク。前後を順番に実行し、他の benchmark は同時実行していない。seed 1、各 5 回の中央値、ns/word。scalar は各 2^24 words、bulk は warm-up 後の反復。CPU 固定なし。

| 項目 | word 数/回 | 比較元 | 修正版 | 時間比 after/before | 解釈 |
|---|---:|---:|---:|---:|---|
| scalar SplitMix64 | 16,777,216 | 0.766 | 0.770 | 1.005 | 測定誤差の可能性 |
| scalar Xoshiro256++ | 16,777,216 | 0.496 | 0.497 | 1.002 | 測定誤差の可能性 |
| scalar Xorshift64 | 16,777,216 | 1.147 | 1.149 | 1.002 | 測定誤差の可能性 |
| scalar Threefry32x4 | 16,777,216 | 2.254 | 2.128 | 0.944 | 比較元は非標準、修正版は標準。出力が異なる |
| C ABI SplitMix64 | 4,194,304 | 0.308 | 0.313 | 1.016 | 小差。測定誤差の可能性 |
| C ABI Xorshift64 | 4,194,304 | 1.149 | 1.150 | 1.001 | 測定誤差の可能性 |
| C ABI Threefry32x4 | 4,194,304 | 0.215 | 0.206 | 0.958 | 比較元は誤った関数・tail 管理。意味の異なる比較 |

測定対象では重大な退行を確認しなかった。小 buffer を含む全 13 条件の raw 5 試行と min/max は `benchmark-before.csv`, `benchmark-after.csv`, `benchmark-comparison.csv`。高速な誤実装との差を正確性のコストと断定せず、未測定の SIMD/他 generator へ一般化しない。

## 5. 実行した検証

このディレクトリから `python prepare.py` で `work/` を用意する。原本の `src/tests/examples` と build 入力のみをコピーし、修正報告書は読まない。測定に使用した依存解決は `Cargo.review.lock` に保存し、再現時にコピーへ適用する。

| コマンド / ログ | 結果 |
|---|---|
| 初回コピーで `cargo test --offline --lib --tests --features full` / `logs/existing-full.log` | 196 成功、0 失敗 |
| `python run_checks.py probes` / `logs/probes.log` | 10 成功、10 失敗 |
| `python run_checks.py counters` / `logs/counter-probes.log` | 0 成功、2 失敗（境界状態注入） |
| `python run_checks.py ciphers` / `logs/cipher-probes.log` | 7 成功、0 失敗 |
| `python run_checks.py rand` / `logs/rand64-probe.log` | 1 成功、0 失敗 |
| `python run_checks.py features` | default、単独 6 features、cabi+simd、full の 9 構成すべて成功 |
| `python run_checks.py docs` / `logs/doctests.log` | 112 成功、0 失敗、2 ignored |
| `cargo check --offline --all-targets --features full` / `logs/alltargets.log` | 成功（後で追加した rand canary は別途上記で成功） |
| `python run_checks.py build` / `logs/build-release.log` | release/full 成功 |
| `python audit_csharp.py` / `logs/csharp-build.log` | .NET コンパイル成功、0 warnings/errors、import 欠落 108 |
| `python benchmark.py prepare`、`python benchmark.py measure` | 比較元ビルドと前後 13 条件 × 5 試行に成功 |

既存テストだけの再実行は `python run_checks.py existing`。失敗テストを修正・無効化せず、各ログの `EXIT` と summary JSON に保存した。配布する runner も失敗時は非ゼロで終了する。初回の PowerShell ログは UTF-8 へ変換した。C# 検証は tool launch 制約により承認済みの sandbox 外実行を使用し、生成物は引き続きこのディレクトリ以下へ保存した。

参照元: [Random123](https://github.com/DEShawResearch/random123/blob/main/include/Random123/threefry.h)、[RFC 8439 §2.3.2](https://www.rfc-editor.org/rfc/rfc8439.html#section-2.3.2)、[Bernstein の Salsa20 core](https://cr.yp.to/salsa20.html)、[Xoshiro256++ 著者実装](https://prng.di.unimi.it/xoshiro256plusplus.c)、[Xoroshiro128++ 著者実装](https://prng.di.unimi.it/xoroshiro128plusplus.c)。再現コードは `probes.rs`, `counter_probes.rs`, `cipher_probes.rs`, `cipher_reference.rs`, `rand64_probe.rs`。対象の SHA-256 は `reviewed-source-sha256.json`。

未実施: Linux/ARM/32-bit/Big Endian、MSRV 1.92、全 feature 組合せ、AVX2/AVX-512 がない実機、Miri/ASan/TSan、C# 実 P/Invoke、PractRand/TestU01、全 PRNG の長期周期・相関。host は AVX-512 を備えるため低機能 CPU の dispatch 実証には別環境が必要。網羅的なメモリ安全性の証明はしていない。

## 6. 総括

採用前には wide Threefry の round/carry と wide Xorshift のゼロレーンを優先して修正すべきである。続いて Alias の極小重み、巨大 float 区間、C# API 欠落を解消する必要がある。scalar の主要修正と測定対象の性能は良好だが、wide/C# 全体へ一般化できない。未検証範囲を明示し、「再現しなかった」を「問題なし」とは判定していない。
