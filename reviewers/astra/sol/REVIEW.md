# Sol 版の独立レビュー

## 1. 総合評価

担当は Astra（ユーザー指定）。実作業セッションは GPT-6 ベースの Codex。未確認のモデルバージョン・推論設定は記載しない。実施日: 2026-10-16～17 JST。

| 項目 | 内容 |
| --- | --- |
| 対象 | `urng-review_sol/` |
| 対象コミット | `8417b4b3c36735015065c02878b665020292199e` |
| 性能比較元 | `386eb4d4f19457dabd6e82553eafc809c7e3951b` |
| 環境 | Windows x86_64 MSVC、Rust/Cargo 1.98.1、LLVM 22.1.8、.NET SDK 10.0.401 |
| CPU | AMD Ryzen 7 9800X3D、16論理スレッド。AVX2 / AVX-512F / DQ / VL を CPUID で確認 |
| 指摘 | Critical 0、High 3、Medium 3、Low 0 |
| 既存テスト | 通常349成功 / 0失敗 / 1 ignored、native 358成功 / 0失敗 / 1 ignored（doc testを含む） |
| 独立テスト | 24件中21成功、3失敗 |
| C# compile | 失敗。未定義型による54エラー |

**Rust の主要な修正は独立検証でも広く確認できたが、採用前に残存不具合の修正が必要。** wide Squares のゼロ固定、暗号 RNG の Debug による鍵露出、生成 C# のコンパイル不能がある。SplitMix の seed 衝突と C# API 欠落も残る。大容量 C ABI は正確な逐次系列を得る代わりに、比較元より約3.5～19.4倍の所要時間となった。

原本を変更せず、選択したソース・既存テスト・設定を当ディレクトリの `work/source/` にコピーした。他レビュアーの成果物および対象モデルの修正報告書は参照していない。行番号は対象コミットの原本を指す。

独立検証は公式 KAT、暗号ブロック、xoroshiro の行列累乗、SIMD レーン、数値境界、実 DLL の出力境界・C# コンパイルを含む。全アルゴリズムの品質・周期、全 feature 組合せ、他 OS / CPU、Miri / sanitizer による UB 検証は網羅していない。

## 2. 発見した問題

### ASO-01 — High — wide Squares32 の合法 seed で lane 0 がゼロ固定

- **場所:** `src/wide/squares.rs:39,42,67`。
- **原因・影響:** seed 0x61c8864680b583eb は既に奇数なので seed | 1 を通過する。SplitMix64 の最初の出力が0となり、これを乗数鍵 k に採用するため y = counter * k と z = y + k が常に0になる。
- **再現:** `wide_squares_zero_key` で `wide::Squares32x4::new(0x61c8864680b583eb)` を32回生成する。
- **実際 / 期待:** lane 0 は32回すべて0、式から以後も0。期待は非退化な乱数系列。
- **修正案:** seed の奇数化ではなく、展開した鍵を Squares の鍵条件に従って構成する。少なくとも0を排除する。任意の非ゼロ鍵の統計品質までは今回検証していない。
- **検証状態:** **実証済み**（x4）。x8 への影響は共通 macro の静的確認。修正前から残る欠陥。

### ASO-02 — High — ChaCha / Salsa の Debug が256 bit秘密鍵を表示する

- **場所:** `src/crng/b32/chacha.rs:18,37`、`src/crng/b32/salsa.rs:18`。
- **原因・影響:** from_key_nonce で格納した鍵を含む state と出力バッファが derive(Debug) に含まれる。構造体やそのラッパーを診断ログへ出すと、鍵・keystream が露出する。明示的なログ出力が発生条件であり、通常の生成だけで外部送信されるという指摘ではない。
- **再現:** `crypto_debug_redacts_secret`。ChaCha20 / Salsa20 を鍵 [0x42;32] で作成し、format!("{rng:?}") を実行。
- **実際 / 期待:** Debug 出力の state に1111638594（0x42424242）が8ワード現れる。[実際の出力](logs/independent-final.txt)を保存。期待は鍵・内部出力バッファを表示しないこと。
- **修正案:** Debug を独自実装し、アルゴリズム名・round数など非秘密情報だけを出す。ネストされた Debug でも漏れない回帰試験を置く。
- **検証状態:** **実証済み**。derive 自体は従来から残るが、新設した鍵指定 API と「秘密用途には from_key_nonce」という文書にも適用される。

### ASO-03 — Medium — SplitMix の隣接 seed が同じ系列になる

- **場所:** `src/prng/b32/splitmix.rs:30`、`src/prng/b64/splitmix.rs:23`。
- **原因・影響:** state = seed | 1 により seed の下位1 bitを捨てる。seed 2k と 2k+1 は同じ state であり、32 / 64 bit 入力が31 / 63 bit分になる。SplitMix を seed expander に使う他の生成器にも同じ衝突が波及し得る。連番で worker を seed する用途では隣同士が同じ系列になる。
- **再現:** `neighboring_seeds_distinct` で SplitMix32 / SplitMix64 をそれぞれ seed 0 と1で作り、先頭32 wordを比較。
- **実際 / 期待:** 両型とも全32 wordが一致。初期 state が等しいため以後も同じ。期待は入力 seed を不必要に潰さないこと。任意の seed 間の統計的独立性を保証すべき、という主張ではない。
- **修正案:** SplitMix の state は seed をそのまま保持する。加算ステップの奇数性と、初期 state の奇数化を区別する。Xorshift 等のゼロ状態は展開後に個別に対処する。偶数 seed の系列変更は互換性資料に明記。
- **検証状態:** **実証済み**。比較元から残る問題。

### ASO-04 — High — 生成 C# が未定義の SFMT 型を参照してコンパイル不能

- **場所:** `build.rs:93,102,135`、`src/prng/b32/mersenne.rs:352`、`src/cabi/prng/b32/mersenne.rs:199` 以降。
- **原因・影響:** C ABI 関数を macro から明示的な宣言へ変更したため csbindgen が読み取るようになった一方、9種類の SFMT 本体は macro 内のままで型宣言が生成されない。生成ファイル全体がコンパイルできず、他の RNG の C# 利用も阻害する。
- **再現:** release build 後に `python run.py csharp-prepare`、`python run.py csharp-modern dotnet build ..\csharp\binding_check.csproj --ignore-failed-sources --verbosity quiet`。
- **実際 / 期待:** .NET SDK 10.0.401 で CS0246 が54件。未定義型は Sfmt607 / 1279 / 2281 / 4253 / 11213 / 44497 / 86243 / 132049 / 216091（各6宣言）。期待は生成物がコンパイルできること。
- **修正案:** 生成器へ明示的な opaque 型を与える、または型・関数を同じ方法で展開する。実際の生成物を C# compiler に通す smoke test を追加する。
- **検証状態:** **実証済み**。[csharp-modern.log](logs/csharp-modern.log)。関数側だけの明示化による新たな統合不具合。Rust の cargo test 成功では検出できない。

### ASO-05 — Medium — 54個の C export に C# 宣言がない

- **場所:** `build.rs:90` からの input_extern_file 列。
- **原因・影響:** Sfc32 系、Biski64 系、32 bit出力の Xoroshiro64Ss 系の C ABI ファイルが入力から漏れる。ASO-04 を直しても、これらの API は生成 C# から利用できない。
- **再現:** `ffi_probe.py` で DLL の llvm-readobj --coff-exports と RngNative.cs の EntryPoint を比較。
- **実際 / 期待:** DLL 336 export 対 C# 282、欠落54（Biski 12、Sfc32 24、Xoroshiro64Ss 18）。存在しない export 参照は0。期待は公開 C ABI の完全な生成。
- **修正案:** 入力一覧を補完し、export と EntryPoint の集合を CI で照合する。プラットフォーム・feature 別の意図した差も明示する。
- **検証状態:** **実証済み**。[全欠落一覧](logs/ffi.json)。比較元から残る問題で、ASO-04 の型欠落とは別原因。

### ASO-06 — Medium — 大容量 C ABI の逐次化に大きな性能回帰

- **場所:** `src/cabi/cbrng/b32/threefry.rs:38`、`src/cabi/prng/b32/splitmix.rs:38`、同様の scalar fill。
- **原因・影響:** 並列 chunk 生成を廃し、全 word を逐次 nextu で生成する。counter / jump で系列位置を直接指定できる生成器も同じ処理で、大容量一括生成で CPU 並列性とブロック単位の処理を失う。
- **再現:** 第4・5節の native release benchmark。初期化済み32 MiB出力を事前確保し、warm-up 後7回測定。
- **実際 / 期待:** 比較元比で SplitMix32 が3.72倍、Threefry4 が19.41倍、Sfc32 が3.54倍の時間。期待は正確な系列・安全な書込みを維持しつつ、counter / jump が利用可能なものでは大容量性能も維持すること。
- **重要な制約:** 比較元には誤ったブロック関数・系列契約が含まれるため、旧版へ戻す提案ではない。README が逐次化を明記している点は妥当。すべての速度差が回避可能とまでは断定しない。
- **修正案:** Threefry / SplitMix 等から、バッファ位置・部分ブロック・最終 state を厳密に管理したブロック生成や並列区間分割を導入する。MaybeUninit による出力契約を維持する。jump のない生成器は別途費用対効果を判断する。
- **検証状態:** **実測済みの性能回帰**、原因はソースとも整合。改善率自体は未実装・未測定。詳細・正しい列同士の比較は第4節。

## 3. 既存修正の評価

修正報告書ではなく、対象のソース、比較元との差分、独立検証から評価した。

| 分類 | 評価と証拠 |
| --- | --- |
| 確認できた修正 | scalar Threefry4 の公式3 KAT、Threefry2 / Philox32 / Philox64 のゼロ KAT、scalar carry が成功。wide Threefry4 も独立実装に100 counter × 4 word × 4 laneで一致し、wide x2 / x4 の低位 counter 周回後は初期系列と異なる。 |
| 確認できた修正 | ChaCha20 は RFC 8439 ベクタに一致。Salsa20 は独立ラウンド実装と32 bit carry を跨ぐ4ブロックで一致。新しい from_key_nonce によって鍵・nonce・counter を直接指定できる。 |
| 確認できた修正 | scalar Xorshift32 の3特殊 seed、Xorshift64 の特殊 seed、wide Xorshift32 のゼロ lane 対策が成功。 |
| 確認できた修正 | SIMD Sfc64x8 の8レーンと Philox32x4x4 の4ブロックを独立計算に各1,000回照合して成功。xoroshiro128++ / ** の jump / long_jump は遷移行列の 2^64 / 2^96 乗に一致。 |
| 確認できた修正 | 極小 Alias32 / 64 の [0,min-subnormal] は [0,10000]、極端な有限 float 区間も境界条件を満たす。scalar 整数境界・分布 smoke test、通常 float 半開区間、不正重み拒否、Bst32 のゼロ重み排除も成功。 |
| 確認できた修正 | C ABI の SplitMix32 / Threefry4 が混合長の連続呼出しでも scalar と一致。56 raw fill API の境界・alignment 試験が成功。出力は MaybeUninit 経由の書込みになっており、書込み前に初期化済み T を要求する問題をソース上で解消している。 |
| 不完全・残存 | seed の奇数化は展開後の Squares 鍵0を防がず、SplitMix では入力1 bitを潰す（ASO-01,03）。鍵指定 API の追加に Debug の秘匿が伴っていない（ASO-02）。C# の入力漏れが残る（ASO-05）。 |
| 新たな問題 | C ABI 関数の明示化で未定義 SFMT 型参照が C# に出現（ASO-04）。大容量 C ABI の速度低下は逐次化による性能回帰（ASO-06）。 |
| 妥当な仕様変更 | unsafe な FFI 契約、null / alignment / 長さ / overlap の入口検査、不正区間・不正重みの扱い、暗号 counter 枯渇の明示、誤った系列の修正、C ABI の scalar 逐次系列への統一。SIMD の部分ブロック消費も文書に明記されている。 |

C ABI の検査は allocation の実長や provenance、解放済み pointer、同時使用を証明できない。その限界を公開契約に書く方針は妥当。内部に残る未使用の旧 helper を、現在の公開 C ABI 経路の問題と混同していない。安全性全体を Miri で証明したという評価ではない。

## 4. 性能評価

[測定コード](astra_bench.rs)、[修正前ログ](logs/baseline-bench-native.log)、[修正後ログ](logs/bench-native.log)、[中央値・最小値・最大値](logs/summary.json)を保存した。同一ホスト、release / all-features、RUSTFLAGS=-C target-cpu=native、opt-level=3 / fat LTO / codegen-units=1。1回 warm-up 後7回測定し、中央値を比較。ベンチプロセスは順番に実行した。

scalar / SIMD / wide は4,194,304出力 word、C ABI は8,388,608 u32（32 MiB）。出力・checksum に black_box を使用。C ABI は初期化済みバッファを事前確保して再利用する。scalar randi の上下限は動的入力、wide は定数入力。比は **修正後時間 / 修正前時間** で、大きいほど遅い。

| 対象 | 修正前 ns/word | 修正後 ns/word | 時間比 |
| --- | ---: | ---: | ---: |
| scalar SplitMix64 | 0.0679 | 0.0682 | 1.00x |
| scalar Threefry4 | 2.2713 | 2.1541 | 0.95x |
| wide SplitMix32x8 | 0.0735 | 0.1028 | 1.40x |
| wide Sfc32x8 | 0.1467 | 0.1466 | 1.00x |
| wide Sfc32x8 randi(0,9) | 0.1649 | 0.2994 | 1.82x |
| SIMD Sfc64x8 | 0.1432 | 0.1481 | 1.03x |
| scalar Sfc32 randi(0,9) | 0.9865 | 1.5830 | 1.60x |
| C ABI SplitMix32、32 MiB | 0.1516 | 0.5633 | 3.72x |
| C ABI Threefry4、32 MiB | 0.1498 | 2.9072 | 19.41x |
| C ABI Sfc32、32 MiB | 0.1784 | 0.6307 | 3.54x |

- **正確性確保のコスト:** scalar / wide randi は旧版の偏りを除く rejection を含む。旧版と同じ計算量を要求すべきではない。SIMD Sfc64 も誤った状態更新を修正しており、旧版の速度は正しいアルゴリズムの速度ではない。
- **正確性と最適化不足の両方:** C ABI の逐次化で系列契約が単純・正確になったが、counter / jump を使う分割まで失ったことは必須ではない。特に Threefry4 の約19倍は最適化の優先候補。jump のない Sfc32 は同じ理由で単純に並列化できない。
- **追加の比較材料:** 本レビュアーが同じ手法で測定した Opus 修正版の C ABI Threefry4 は0.150752 ns/wordで、Sol は約19.28倍の時間。両修正版の scalar ブロックは公式 KAT、C ABI は混合長の逐次系列試験に通過している（[Opus測定ログ](../opus/logs/bench-native.log)、[系列試験](../opus/logs/independent-final.txt)）。ただし32 MiB全列の逐次期待値照合までは行っていない。Opus に別途ある未初期化出力の問題は、この初期化済みバッファ測定では発生条件に入らない。この比較は安全な最適化の実装・保証を代替するものではない。
- **最適化不足の可能性:** scalar randi は各 draw で剰余 threshold を計算している。必要時だけ計算する fast path を持つ Opus 修正版は同一測定で1.116157 ns/word。Sol の1.583004との差は、rejection の意味を維持する最適化候補。wide SplitMix32 は scalar と整合させる64 bit中間演算を含む処理変更があり、約40%増をレーン重複修正だけの不可避コストとは断定しない。
- **旧版との比較制約:** 旧 Threefry4 は誤ったブロック関数、旧 wide SplitMix はレーン間の系列重複、旧 C ABI は再 seed / 分割方法による異なる列を含む。旧 C ABI の全32 MiB列を独立参照に照合していない。
- **測定誤差 / 原因不明:** 約1～5%差は CPU 周波数・スケジューリングの影響を含み得る。最短ケースは約0.3 msで、CPU affinity / 周波数固定はしていない。32 MiB は DRAM 帯域だけを測る条件でもない。細かな差の命令単位の原因・最適化後の改善率は未確定。

## 5. 実行した検証

### 再実行方法

リポジトリルートで PowerShell を使う。Python、MSVC Rust toolchain、キャッシュ済み Cargo 依存、.NET 10、PATH 上の llvm-readobj が必要。prepare は原本を読むだけで、書込み先は当ディレクトリ以下。以下は初回構築順序であり、保存済みログを残すには実行ラベルを変える。

```powershell
$runner = 'reviewers/astra/sol/run.py'
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
python reviewers/astra/sol/ffi_probe.py
python $runner csharp-prepare
python $runner csharp-modern dotnet build ..\csharp\binding_check.csproj --ignore-failed-sources --verbosity quiet
python $runner package-list cargo package --offline --allow-dirty --list
python reviewers/astra/sol/summarize.py
Remove-Item Env:RUSTFLAGS
```

work/ と target/ は再生成可能なため Git 除外。独立テストは全 feature 前提。install は test module と adapter を追加し、wide Threefry の低位 counter setter をコピーにだけ cfg(test) で追加する。製品の生成処理や期待値は変更していない。

### 結果と証拠

| 検証 | 結果 | 証拠 |
| --- | --- | --- |
| 既存 all-features | 349成功 / 0失敗 / 1 ignored | [existing-full.log](logs/existing-full.log) |
| 既存 native | 358成功 / 0失敗 / 1 ignored | [existing-native.log](logs/existing-native.log) |
| 独立24テスト | 21成功 / 3失敗 | [astra_review.rs](astra_review.rs)、[adapter.rs](adapter.rs)、[independent-final.log](logs/independent-final.txt) |
| feature check | 9構成すべて成功 | logs/check-*.log。no-default-featuresで無し、rand、cabi、simd、wide、sampler、seedgen、cabi+simd、full |
| DLL raw fill | 56 API、canary / alignment 1,018ケース成功、guard page 392ケース成功 | [ffi_probe.py](ffi_probe.py)、[ffi.json](logs/ffi.json) |
| DLL float極端範囲 | 検査した56 APIで退化・区間違反なし | 同上 |
| C# .NET 10 compile | 失敗、CS0246 が54件 | [csharp-modern.log](logs/csharp-modern.log) |
| C# export照合 | 欠落54、存在しないexport参照0 | [ffi.json](logs/ffi.json) |
| package一覧 | 成功 | [package-list.log](logs/package-list.log)。instrument済みコピーの一覧のみ。配布packageのビルド検証ではない |

DLL テストの count は0,1,3,7,16,17,257,8193,131075。64 byte整列と要素1個分ずらしたアドレスを比較した。SplitMix32 / Pcg32 / Threefry4 / Sfc32 / Sfc32x16 には6,291,457 word（24 MiB超）も追加。保護ページ直前の長さ1,3,15,16,17,63,65も使用した。出力は初期化済みであり、未初期化メモリの動的 UB 検証ではない。

参照は [Random123 KAT](https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors)、[RFC 8439 §2.3.2](https://www.rfc-editor.org/rfc/rfc8439.html#section-2.3.2)、[Salsa20 作者資料](https://cr.yp.to/salsa20.html)。Salsa は独立ラウンド実装との比較で、別ライブラリとの相互運用試験ではない。xoroshiro は遷移行列を GF(2) 上で二乗して jump を検証した。整数分布は300,000標本の剰余3クラスが [100046,99840,100114]。有限標本の成功を数学的証明とは扱わない。

古い .NET Framework csc の nuint 未対応による失敗は logs/csharp.log に残し、判定には .NET SDK 10 の結果を使った。初期・拡張段階の独立テストログも保存した。invalid weights テスト内の捕捉した panic は期待どおりである。Rust build で残る warning はログに保存しており、warning-free という評価ではない。

[環境情報](logs/environment.txt)と[export一覧](logs/export_inventory.json)も保存した。independent-final.txt は CPUID 出力の NUL を可視の `\0` に置換し、行末空白を除去した閲覧用コピー。元の independent-final.log も保持している。

**未検証範囲:** AVX2 / AVX-512 非対応 CPU、ARM・32 bit・Linux/macOS、MSRV 1.92、Miri / sanitizer / データ競合検出、TestU01 / PractRand、全 PRNG の独立 KAT、全 jump の証明、全 SIMD の上位 counter carry、shuffle の独立分布試験、全重み更新パターン。インストール済み Rust target は Windows x86_64のみで、これらを網羅していない。既存 ignored テストは残した。

## 6. 総括

優先順は、Squares のゼロ鍵排除、暗号 RNG の Debug 秘匿、C# の未定義型修正。その後、seed の情報喪失と C# API 欠落を直す。これらは Rust の既存テスト成功だけでは見逃される。

安全な出力型と逐次系列契約を維持して、counter / jump 系の大容量生成を最適化する価値がある。旧版の誤った系列を復活させないことが条件となる。失敗テストと C# compile / export 照合を回帰検証へ追加し、Miri 等、非 SIMD CPU、他 OS、長期統計試験へ広げることを推奨する。
