# Opus 版の独立レビュー

## 1. 総合評価

担当は Astra（ユーザー指定）。実作業セッションは GPT-6 ベースの Codex。未確認のモデルバージョン・推論設定は記載しない。実施日: 2026-10-16～17 JST。

| 項目 | 内容 |
| --- | --- |
| 対象 | `urng-review_opus/` |
| 対象コミット | `3a9297465082c07dd2be2d2c697bdf22eef322a1` |
| 性能比較元 | `406805168dcf580c8422f7211a4a86aa72a57b4c` |
| 環境 | Windows x86_64 MSVC、Rust/Cargo 1.98.1、LLVM 22.1.8、.NET SDK 10.0.401 |
| CPU | AMD Ryzen 7 9800X3D、16論理スレッド。AVX2 / AVX-512F / DQ / VL を CPUID で確認 |
| 指摘 | Critical 0、High 7、Medium 1、Low 0 |
| 既存テスト | 通常・native とも308成功、0失敗、2 ignored（doc testを含む） |
| 独立テスト | 24件中15成功、9失敗。失敗9件は下記6原因に対応 |

**このままの全面採用は推奨しない。** scalar の主要修正と大容量出力性能には改善がある一方、wide Threefry、特殊 seed の固定ゼロ列、重み付き抽選、極端な float 範囲に実証済みの問題が残る。C ABI の未初期化メモリ契約にも静的に確認できる欠陥がある。

原本を変更せず、選択したソース・既存テスト・設定を当ディレクトリの `work/source/` にコピーした。他レビュアーの成果物および対象モデルの修正報告書は参照していない。行番号は対象コミットの原本を指す。

独立検証は公式 KAT、暗号ブロック、xoroshiro の行列累乗、SIMD レーン、数値境界、実 DLL の出力境界・C# コンパイルを含む。全アルゴリズムの品質・周期、全 feature 組合せ、他 OS / CPU、Miri / sanitizer による UB 検証は網羅していない。

## 2. 発見した問題

### AOP-01 — High — 書き込み専用出力を初期化済み slice として扱う

- **場所:** `src/_internal.rs:220,224` の `out_slice`、公開契約 `src/cabi/mod.rs:10`。
- **原因・影響:** 出力は「count 要素を書き込める」領域とだけ規定されるが、書込み前に `from_raw_parts_mut::<T>` で `&mut [T]` を作る。C の malloc や Rust の MaybeUninit による領域は、初期化済み T という事前条件を満たさない。整数の全ビットパターンが有効でも、未初期化値は有効にならない。[Rust の関数契約](https://doc.rust-lang.org/core/slice/fn.from_raw_parts_mut.html)が根拠。
- **再現経路:** `let mut out = [MaybeUninit::<u32>::uninit(); 16];` を確保し、`splitmix32_next_u32s(handle, out.as_mut_ptr().cast(), 16)` に渡す。
- **実際 / 期待:** 呼出側は書込み契約を満たすが、内部で初期化前に `&mut [u32]` ができる。期待は未初期化領域を受け入れて初期化すること。上記は静的な呼出例で、Miri 検出結果ではない。
- **修正案:** `&mut [MaybeUninit<T>]` または raw pointer のまま扱い、write / raw copy で初期化する。並列 chunk 化する場合もその型を維持する。
- **検証状態:** **静的解析で確認**。初期化済み出力の境界テスト成功は、この問題を否定しない。

### AOP-02 — High — wide Threefry の counter が32 bitで周回する

- **場所:** `src/wide/threefry.rs:100,255`。
- **原因・影響:** refill で c0 だけを増やし、c1 以降へ carry しない。Threefry32x2x{4,8,16} と Threefry32x4x{4,8,16} の各レーンが **2^32 blocks ごとに同じ系列を繰り返す**。
- **再現:** `wide_threefry2_carry` / `wide_threefry4_carry`。コピーだけに test 専用 setter を追加し、初期ブロックを記録後、下位 counter を u32::MAX にして境界を横断する。
- **実際 / 期待:** x2 の先頭ベクトルは初期・周回後とも `[295270083,3937102892,376655324,3337748263]`。x4 も完全一致。期待は上位 counter が増えた別ブロック。
- **修正案:** 各レーンの carry mask を上位ワードへ伝播させ、連続する上位ワードの最大値境界もテストする。
- **検証状態:** **実証済み**（x4 レーン幅の2種）。x8 / x16 幅は共通 macro の静的確認。

### AOP-03 — High — wide Threefry4 が Random123 と一致しない

- **場所:** `src/wide/threefry.rs:176,177,220,240`。
- **原因・影響:** seed から追加した tweak を subkey に加え、出力を counter と XOR する。Random123 Threefry4x32 にない処理で、scalar の修正が wide の x4 / x8 / x16 レーン版に反映されていない。
- **再現:** `random123_known_answers` で独立 tf4 を公式3ベクタに照合後、`wide_threefry4_reference` で seed=19 の展開鍵を使って比較。鍵展開には対象の SplitMix32 を使うが、ブロック期待値は独立実装。
- **実際 / 期待:** counter=0、word=0、lane=0 は706044619、期待値2899152195。carry 以前の最初のブロックから相違。
- **修正案:** 公式の20ラウンド、鍵注入、出力規則に統一し、全 lane・複数 counter を参照実装に照合する。
- **検証状態:** **実証済み**。[Random123 公式 KAT](https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors)に基づく。AOP-02 とは独立した欠陥。

### AOP-04 — High — Alias32 / Alias64 がゼロ重みを約50%選ぶ

- **場所:** `src/sampler/b32/alias.rs:44,50`、`src/sampler/b64/alias.rs:44,50`。
- **原因・影響:** 正で有限の total でも n / total が infinity になり、重み0との積が NaN となって alias table が壊れる。合法な極小重みの分布に影響する。
- **再現:** `alias32_subnormal_zero_weight` / `alias64_subnormal_zero_weight`。重み `[0.0, f32::from_bits(1)]` または f64 相当で10,000回 sample。
- **実際 / 期待:** 選択回数は32 bit版 `[5064,4936]`、64 bit版 `[4965,5035]`。期待は厳密に `[0,10000]`。
- **修正案:** 最大重みで正規化してから総和・確率を作るなど、途中計算の overflow を防ぐ。weights 更新経路も検証する。
- **検証状態:** **実証済み**。入力の有限性・総和の正値性を調べるだけでは不十分。

### AOP-05 — High — 有限の広い float 区間で NaN または定数列になる

- **場所:** `src/rng.rs:120` の to_randf。Rng::randf、同変換を使う wide / C ABI に波及。
- **原因:** max - min が infinity になる。unit 値0は 0 * infinity で NaN、それ以外は infinity を max.next_down() にクランプする。
- **再現:** `extreme_float32` / `extreme_float64` で `0.to_randf(-MAX, MAX)`。DLL の公開 rand_f32s / rand_f64s も同区間で1,024個ずつ生成。
- **実際 / 期待:** Rust の両型で NaN。DLL の51関数で定数化または区間違反を検出（[関数別結果](logs/ffi.json)）。期待は有限な半開区間内の値と乱数の変動。
- **修正案:** 幅が有限の場合と overflow する場合を分け、後者に overflow を避ける凸結合などを使う。端点クランプはその後に行い、不正区間の契約も明記する。
- **検証状態:** **実証済み**。通常の [100,101) や [0,1) の端点テストは成功した。

### AOP-06 — High — wide Xorshift32 の合法 seed で lane 0 がゼロ固定

- **場所:** `src/wide/xorshift.rs:32,34`。
- **原因・影響:** SplitMix32 の出力を各レーンに無検査で採用する。seed 0x61c88647 の初回出力が0となり、Xorshift のゼロ吸収状態に入る。scalar のゼロ回避が wide にない。
- **再現:** `wide_xorshift_zero_lane` で `wide::Xorshift32x4::new(0x61c88647)` の lane 0 を32回観測。
- **実際 / 期待:** 32回すべて0、遷移式から以後も0。期待は非ゼロ状態の PRNG 系列。
- **修正案:** 各レーンの状態が0なら規定の非ゼロ値へ置換する。seed 自体の非ゼロ化では防げない。
- **検証状態:** **実証済み**（x4）。x8 / x16 は共通 macro の静的確認。

### AOP-07 — High — wide Squares32 の鍵0により lane 0 がゼロ固定

- **場所:** `src/wide/squares.rs:39,42,67`。
- **原因・影響:** SplitMix64::new(0x61c8864680b583eb) の最初の値は0。そのまま乗数鍵 k に使うため y = counter * k と z = y + k が常に0となる。
- **再現:** `wide_squares_zero_key` で `wide::Squares32x4::new(0x61c8864680b583eb)` を32回生成。
- **実際 / 期待:** lane 0 がすべて0、式から以後も0。期待は非退化な系列。
- **修正案:** 少なくとも0を排除し、Squares の鍵条件に沿った鍵生成にする。任意の非ゼロ値で統計品質まで保証されるとは本検証から言えない。
- **検証状態:** **実証済み**（x4）。x8 は共通 macro の静的確認。

### AOP-08 — Medium — 生成 C# に108個の C export がない

- **場所:** `build.rs:90` からの入力列、`src/cabi/prng/b32/mersenne.rs` の macro 生成 API。
- **原因・影響:** Sfc32 系、Biski64 系、32 bit出力の Xoroshiro64Ss 系の C ABI ファイルが生成器の入力から漏れる。SFMT の9変種は macro 内の関数が見えない。生成 C# はコンパイルできるが対応機能を使えない。
- **再現:** `ffi_probe.py` で DLL の llvm-readobj --coff-exports と RngNative.cs の EntryPoint を比較。
- **実際 / 期待:** DLL 322 export 対 C# 214、欠落108（Biski 12、Sfc32 24、Xoroshiro64Ss 18、SFMT 54）。存在しない export 参照は0。期待は公開 C ABI の完全な生成。
- **修正案:** 入力を補完し、macro の関数と opaque 型を生成器から可視にする。C# コンパイルと export 集合照合を CI に追加。
- **検証状態:** **実証済み**。[完全な差分一覧](logs/ffi.json)。

## 3. 既存修正の評価

修正報告書ではなく、対象のソース、比較元との差分、独立検証から評価した。

| 分類 | 評価と証拠 |
| --- | --- |
| 確認できた修正 | scalar Threefry4 の3 KAT、Threefry2 / Philox32 / Philox64 のゼロ KAT、および scalar counter carry が成功。 |
| 確認できた修正 | ChaCha20 は RFC 8439 のブロック値と一致。Opus の64 bit counter / 64 bit nonce 配置へ同じ初期 state を構成した比較であり、IETF の96 bit nonce API を備えるとの主張ではない。Salsa20 は独立ラウンド実装と32 bit carry を跨ぐ4ブロックで一致。Debug は鍵を表示しない。 |
| 確認できた修正 | scalar Xorshift32 の3特殊 seed と Xorshift64 の特殊 seed で固定0を回避。SplitMix32 / 64 の seed 0 と1は異なる。 |
| 確認できた修正 | SIMD Sfc64x8 の8レーン、Philox32x4x4 の4ブロックを独立計算に各1,000回照合して成功。xoroshiro128++ / ** の jump / long_jump は遷移行列の 2^64 / 2^96 乗に一致。 |
| 確認できた修正 | scalar 整数 rejection の境界・分布 smoke test、通常 float 半開区間、不正重み拒否、Bst32 のゼロ重み排除が成功。 |
| 確認できた修正 | C ABI の SplitMix32 / Threefry4 の混合長連続呼出しが scalar と一致。56 raw fill API の canary・alignment・guard page 検証が成功。 |
| 不完全 | scalar の Threefry / Xorshift 修正が wide に及んでいない（AOP-02,03,06）。float の上端丸め対策は幅 overflow を防がず、重み検証も途中計算の overflow を防がない（AOP-04,05）。FFI の unsafe 化・null 対処と初期化契約は別問題（AOP-01）。 |
| 残存 | Squares の退化鍵と C# API 欠落（AOP-07,08）。 |
| 新たな問題 | 本調査で、修正によって初めて生じたと断定できる別の不具合は確認しなかった。上記の多くは従来の処理の修正漏れ。 |
| 妥当な仕様変更 | 不正 sampler 入力の拒否、FFI の unsafe 契約明示、誤った乱数列の修正に伴う出力変更。counter/jump 系と再 seed する substream 系を C ABI 文書で区別する方針は明確だが、後者を scalar と同じ列と扱ってはならない。 |

wide / SIMD / 一部 C ABI の1 word整数変換には、仕様として偏りのある multiply-shift が残る。scalar の rejection と同じ一様性を必要とする利用者には互換ではない。文書化された制約として扱い、今回の不具合件数には重複加算しない。

## 4. 性能評価

[測定コード](astra_bench.rs)、[修正前ログ](logs/baseline-bench-native.log)、[修正後ログ](logs/bench-native.log)、[中央値・最小値・最大値](logs/summary.json)を保存した。同一ホスト、release / all-features、RUSTFLAGS=-C target-cpu=native、opt-level=3 / fat LTO / codegen-units=1。1回 warm-up 後7回測定して中央値を比較。ベンチプロセスは順番に実行した。

scalar / SIMD / wide は4,194,304出力 word、C ABI は8,388,608 u32（32 MiB）。出力・checksum に black_box を使用。C ABI は初期化済みバッファを事前確保して再利用する。scalar randi の上下限は動的入力、wide は定数入力。比は **修正後時間 / 修正前時間** で、大きいほど遅い。

| 対象 | 修正前 ns/word | 修正後 ns/word | 時間比 |
| --- | ---: | ---: | ---: |
| scalar SplitMix64 | 0.0679 | 0.0681 | 1.00x |
| scalar Threefry4 | 2.2724 | 2.1769 | 0.96x |
| wide SplitMix32x8 | 0.0730 | 0.0734 | 1.01x |
| wide Sfc32x8 | 0.1468 | 0.1468 | 1.00x |
| wide Sfc32x8 randi(0,9) | 0.1665 | 0.1653 | 0.99x |
| SIMD Sfc64x8 | 0.1432 | 0.1481 | 1.03x |
| scalar Sfc32 randi(0,9) | 0.9864 | 1.1162 | 1.13x |
| C ABI SplitMix32、32 MiB | 0.1494 | 0.1504 | 1.01x |
| C ABI Threefry4、32 MiB | 0.1511 | 0.1508 | 1.00x |
| C ABI Sfc32、32 MiB | 0.1663 | 0.1577 | 0.95x |

- **正確性確保のコスト:** scalar randi の約13%増は rejection の導入を含む。旧 multiply-shift は厳密一様でない。SIMD Sfc64 の約3%増も誤った状態更新の修正を含むため、旧実装への復帰を改善とはみなさない。
- **最適化不足:** 測定対象では新たな大幅退行を確認しなかった。wide randi が速いことは scalar と同じ一様性を意味しない。
- **比較上の制約:** 旧 Threefry4 は誤ったブロック関数、旧 wide SplitMix はレーン間の系列重複がある。C ABI Sfc32 は substream の seed 方法が変わっている。旧 C ABI の全32 MiB列を独立参照に照合していないため、旧版の速度を正しい系列の速度とみなさない。
- **測定誤差 / 原因不明:** 約1～5%差は CPU 周波数・スケジューリングの影響を含み得る。最短ケースは約0.3 msで、CPU affinity / 周波数固定はしていない。32 MiB は DRAM 帯域だけを測る条件でもない。小幅差の命令単位の原因は確定していない。

## 5. 実行した検証

### 再実行方法

リポジトリルートで PowerShell を使う。Python、MSVC Rust toolchain、キャッシュ済み Cargo 依存、.NET 10、PATH 上の llvm-readobj が必要。prepare は原本を読むだけで、書込み先は当ディレクトリ以下。以下は初回構築順序であり、保存済みログを残すには実行ラベルを変える。

```powershell
$runner = 'reviewers/astra/opus/run.py'
python $runner prepare
python $runner existing-full cargo test --offline --all-features
python $runner matrix
python $runner install
python $runner independent-final cargo test --offline --all-features --lib astra_review -- --nocapture --test-threads=1
$env:RUSTFLAGS = '-C target-cpu=native'
python $runner existing-native cargo test --offline --all-features -- --skip astra_review
python $runner benchmark-install
python $runner benchmark-build-final cargo build --offline --release --all-features --example astra_bench --lib
python $runner bench-native ..\..\target\release\examples\astra_bench.exe
python $runner baseline-prepare
python $runner baseline-build cargo build --offline --release --all-features --example astra_bench
python $runner baseline-bench-native ..\..\target\baseline\release\examples\astra_bench.exe
python reviewers/astra/opus/ffi_probe.py
python $runner csharp-prepare
python $runner csharp-modern dotnet build ..\csharp\binding_check.csproj --ignore-failed-sources --verbosity quiet
python $runner package-list cargo package --offline --allow-dirty --list
python reviewers/astra/opus/summarize.py
Remove-Item Env:RUSTFLAGS
```

work/ と target/ は再生成可能なため Git 除外。独立テストは全 feature 前提。install は test module と adapter を追加し、Salsa の counter setter と wide Threefry の低位 counter setter をコピーにだけ cfg(test) で追加する。製品の生成処理や期待値は変更していない。

### 結果と証拠

| 検証 | 結果 | 証拠 |
| --- | --- | --- |
| 既存 all-features | 308成功 / 0失敗 / 2 ignored | [existing-full.log](logs/existing-full.log) |
| 既存 native | 308成功 / 0失敗 / 2 ignored | [existing-native.log](logs/existing-native.log) |
| 独立24テスト | 15成功 / 9失敗 | [astra_review.rs](astra_review.rs)、[adapter.rs](adapter.rs)、[independent-final.log](logs/independent-final.txt) |
| feature check | 9構成すべて成功 | logs/check-*.log。no-default-featuresで無し、rand、cabi、simd、wide、sampler、seedgen、cabi+simd、full |
| DLL raw fill | 56 API、canary / alignment 1,018ケース成功、guard page 392ケース成功 | [ffi_probe.py](ffi_probe.py)、[ffi.json](logs/ffi.json) |
| DLL float極端範囲 | 51 APIで退化・区間違反 | 同上 |
| C# .NET 10 compile | 成功、警告0 / エラー0 | [csharp-modern.log](logs/csharp-modern.log) |
| C# export照合 | 欠落108、存在しないexport参照0 | [ffi.json](logs/ffi.json) |
| package一覧 | 成功 | [package-list.log](logs/package-list.log)。instrument済みコピーの一覧のみ。配布packageのビルド検証ではない |

DLL テストの count は0,1,3,7,16,17,257,8193,131075。64 byte整列と要素1個分ずらしたアドレスを比較した。SplitMix32 / Pcg32 / Threefry4 / Sfc32 / Sfc32x16 には6,291,457 word（24 MiB超）も追加。保護ページ直前の長さ1,3,15,16,17,63,65も使用した。出力は初期化済みであり、AOP-01 の検証とは異なる。

参照は [Random123 KAT](https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors)、[RFC 8439 §2.3.2](https://www.rfc-editor.org/rfc/rfc8439.html#section-2.3.2)、[Salsa20 作者資料](https://cr.yp.to/salsa20.html)。Salsa は独立ラウンド実装との比較で、別ライブラリとの相互運用試験ではない。xoroshiro は遷移行列を GF(2) 上で二乗して jump を検証した。整数分布は300,000標本の剰余3クラスが [100046,99840,100114]。有限標本の成功を数学的証明とは扱わない。

初期 adapter が存在しない Salsa setter を呼んだコンパイル失敗は [harness-initial-compile.log](logs/harness-initial-compile.log) に保存し、コピーへの test setter 追加で修正した。製品の不具合には数えない。古い .NET Framework csc の nuint 未対応による失敗は logs/csharp.log に残し、判定には現行 .NET SDK の結果を使った。初期・拡張段階の独立テストログも保存した。invalid weights テスト内の捕捉した panic は期待どおりである。

初回ベンチビルドの失敗も [benchmark-build.log](logs/benchmark-build.log) に残した。Sfc64x8 コンストラクタを呼ぶハーネスに CPU 検査と unsafe block を追加し、最終ビルドは成功している。これは公開 API の unsafe 要件へのハーネスの適応であり、製品の不具合に数えない。

[環境情報](logs/environment.txt)と[export一覧](logs/export_inventory.json)も保存した。independent-final.txt は CPUID 出力の NUL を可視の `\0` に置換し、行末空白を除去した閲覧用コピー。元の independent-final.log も保持している。

**未検証範囲:** AVX2 / AVX-512 非対応 CPU、ARM・32 bit・Linux/macOS、MSRV 1.92、Miri / sanitizer / データ競合検出、TestU01 / PractRand、全 PRNG の独立 KAT、全 jump の証明、全 SIMD の上位 counter carry、shuffle の独立分布試験、全重み更新パターン。インストール済み Rust target は Windows x86_64のみで、これらを網羅していない。既存 ignored テストは残した。

## 6. 総括

最優先は C ABI の初期化契約と、wide Threefry のブロック関数・carry。その後、wide のゼロ状態 / ゼロ鍵、Alias と float 範囲を修正すべきである。C# export の完全性はコンパイル成功とは別に保証する必要がある。

大容量出力の速度は維持されているが、誤った数値・乱数列の残存は採用の障害となる。失敗テストを回帰テスト化し、未初期化 FFI 出力を Miri 等で検証したうえで、異なる CPU と長期統計試験へ広げることを推奨する。確認済みの境界テスト成功だけでライブラリ全体の安全性を保証することはできない。
