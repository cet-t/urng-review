# Astra 版の独立レビュー（Sol 担当）

## 1. 総合評価

- 対象: `urng-review_astra/`、コミット `cb07a9a0c76bb20b31cf1e29ac7fc0064cf10e90`。
- 比較元: `19bc224`（完全なハッシュは `baseline-revision.txt`）。担当区分は利用者指定の Sol。
- 環境: 2026-10-16、Windows 11 x86_64/MSVC、AMD Ryzen 7 9800X3D、16 logical CPUs、Rust/Cargo 1.98.1、.NET SDK 10.0.401。詳細は `environment.json`。
- 結論: 数値境界と C ABI の系列保持に改善があるが、そのままの採用は推奨しない。**High 5 件、Medium 4 件、Critical 0 件**。以下の 9 件はすべて再現済み。
- 原本の追跡ファイルは変更していない。他モデルのレビュー結果・修正報告書は参照していない。専用コピーで検証した。

既存テストは 190 件成功・0 件失敗、doctest は 105 件成功。追加テストは実行できた 29 件中 14 件成功・15 件失敗。別の rand_core compile canary はビルド失敗し、生成 C# も 54 エラーでコンパイル失敗。失敗を原因別に集約した結果が下記である。

## 2. 発見した問題

### SOL-ASTRA-01 — High: scalar/wide Threefry4x32 が Random123 と異なる

- **場所:** `src/cbrng/b32/threefry.rs:85,123`、`src/wide/threefry.rs:220,239`（対象ディレクトリ基準）。
- **原因・影響:** scalar `Threefry32x4` と wide `Threefry32x4x{4,8,16}` に Threefish 風の tweak 注入、誤った最終キー添字、counter XOR が残る。名前・説明の Random123 Threefry4x32-20 と異なる関数であり、C ABI もその出力を使用する。
- **再現:** `python run_checks.py probes` の scalar/wide 計 4 テスト。scalar seed 0、block 0 は実値 `[2913757005,2302305762,2419161188,2855054987]`、期待値 `[3169309817,2676287226,2408092820,3584707056]`。wide も全幅で初回から不一致。
- **根拠:** constructor の key 配列を再現し、独立した Threefry ループ実装と比較した。その参照実装自体は [Random123 公式 KAT](https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors) の zero/all-ones/pi 3 ケースに一致。
- **修正案:** [公式 schedule](https://github.com/DEShawResearch/random123/blob/main/include/Random123/threefry.h) に合わせ、tweak/counter XOR を除去し、最終注入を group 5 のキー添字に直す。scalar/wide/C ABI を検証する。
- **状態:** 実証済み。比較元から残存。2x32 の round が誤っているという指摘ではない。

### SOL-ASTRA-02 — High: wide Threefry が上位 counter へ carry しない

- **場所:** `src/wide/threefry.rs:100,255`。
- **原因・影響:** 2x32/4x32 の x4/x8/x16 全幅で `c0` しか増分せず、2^32 block で先頭へ戻る。1 レーン当たりの反復までの出力量は 2x32 が 32 GiB、4x32 が 64 GiB。
- **再現:** `python run_checks.py counters`。コピーに `counter_probes.rs` を一時追記し、`c0=u32::MAX` の有効な状態を注入。wrap 後の出力が初期 block と完全一致し、上位 counter は `[0;4]`。期待値は `[1;4]`。
- **限界:** 2^32 block 全体を実生成したものではない。x4 の境界状態で再現し、他の幅は同じ macro を静的確認。runner は検証後に追記をコピーから除去する。
- **修正案:** 各レーンの carry を上位 word へ伝播する。
- **状態:** 実証済み。既存不具合の修正漏れ。01 と独立した原因。

### SOL-ASTRA-03 — High: wide Xorshift32 に永久にゼロのレーンがある

- **場所:** `src/wide/xorshift.rs:39`。
- **原因・影響:** seed 展開後の状態 0 を拒否しない。合法な seed `0x61c88647` で最初の SplitMix32 Weyl state と出力が 0 となり、lane 0 の XOR/shift 遷移が永続的に 0 を返す。x4/x8/x16 全幅に影響。
- **再現:** `wide_xorshift32_*_avoids_zero_state` 3 件。lane 0 の 16 回の出力がすべて 0。期待値は非ゼロ状態からの系列。
- **修正案:** scalar の seed 0 対策に加え、各レーンの展開結果も非ゼロにする。
- **状態:** 実証済み。scalar だけのゼロ状態修正では不十分。

### SOL-ASTRA-04 — High: 生成 C# が未定義の SFMT 型でコンパイル不能

- **場所:** `build.rs:96,107,143`、`src/prng/b32/mersenne.rs:352` の型生成 macro、`src/cabi/prng/b32/mersenne.rs:229` 以降の追加された literal exports。
- **原因・影響:** variant 関数が literal 宣言になって生成器へ取り込まれる一方、対応 struct は macro 内にあり csbindgen が展開しない。C# に `Sfmt607`, `Sfmt1279`, `Sfmt2281`, `Sfmt4253`, `Sfmt11213`, `Sfmt44497`, `Sfmt86243`, `Sfmt132049`, `Sfmt216091` の型宣言が存在せず、生成ファイル全体をコンパイルできない。
- **再現:** release/full build 後、`RngNative.cs` を unsafe 有効の最小 net10.0 プロジェクトへコピーし、package source を空にして `dotnet build`。9 型 × 6 関数の **CS0246 54 エラー**。詳細は `logs/csharp-build.log`。
- **実値と期待値:** DLL 320 exports / C# 320 imports の集合一致は成功するがコンパイル失敗。期待値は必要な opaque 型が宣言されコンパイルできること。シンボル数だけでは発見できない。
- **修正案:** 展開済みの型宣言を入力するか必要な opaque 型を明示生成し、生成コードのコンパイルまで CI に含める。
- **状態:** 実証済みの**新規退行**。比較元 C# は同じ .NET SDK/設定で成功した（`logs/csharp-build-before.log`）。

### SOL-ASTRA-05 — Medium: Alias32/64 が極小の有効重みでゼロ重みを選ぶ

- **場所:** `src/sampler/b32/alias.rs:40,46`、`src/sampler/b64/alias.rs:40,46`。
- **原因・影響:** `n/total` が overflow し、ゼロ重みから `0*infinity` の NaN が生じる。large 側へ分類され、その probability が 1 になる。
- **再現:** `Alias32::new(...,&[f32::from_bits(1),0.0])` と 64-bit 同等入力。常に最大 word を返す合法な `Rng` に対し index 1 が返る。期待値は唯一正の重みを持つ index 0 のみ。`alias*_small_valid_weight_never_selects_zero` 2 件が失敗。
- **修正案:** 有限の中間値で正規化し、subnormal とゼロを含む回帰ケースを追加する。
- **状態:** 実証済み。比較元から残存。

### SOL-ASTRA-06 — Medium: Alias32/64 に不正重みの検証がない

- **場所:** `src/sampler/b32/alias.rs:37`、`src/sampler/b64/alias.rs:37`。
- **原因・影響:** Bst には検証が追加されたが Alias は未変更。負値、NaN、infinity、全ゼロを分布として構築できる。例として `[-1.0,2.0]` は負の probability を保持する。空配列も constructor が成功し、sample 時までエラーを遅延する。
- **再現:** `alias32_rejects_invalid_weights` と `alias64_rejects_invalid_weights` は最初の `[-1,2]` が拒否されず失敗。NaN 等も検証がないことを静的確認した。テストは最初の失敗で止まるため、他の不正値をすべて個別実証したとはしていない。
- **期待値・修正案:** 非空・有限・非負・正の有限 total を constructor/`weights()` で検証し、Bst と共通化する。05 の有効な極小重みの対策も別途必要。
- **状態:** 負値の受理は実証済み、他の不正値は静的確認。sampler 修正の適用漏れ。

### SOL-ASTRA-07 — High: ChaCha/Salsa の Debug が secret key を表示する

- **場所:** `src/crng/b32/chacha.rs:15`、`src/crng/b32/salsa.rs:15` の `#[derive(Debug,...)]`。
- **原因・影響:** 新しい `from_key_nonce` に 256-bit key を渡すと、Debug が入力 state `x` 全体を出力する。debug ログで秘密鍵と将来の出力状態が漏れる。
- **再現:** `cipher_probes.rs` の独立した 2 テスト。synthetic key `[0xa7;32]` を設定し Debug を取得すると、key word `2812782503` が 8 個とも表示される。実際の秘密鍵を実験やログに使用していない。
- **期待値・修正案:** ラウンド数等の非秘密情報だけを示すカスタム Debug にし、key/state/buffer を表示しない。8/20 rounds 双方に適用する。
- **状態:** 実証済み。derive は以前からあるが、full-key API 追加に伴う秘密情報の取扱いが不十分。

### SOL-ASTRA-08 — Medium: rand feature で 64-bit generator を rand_core RNG として使えない

- **場所:** `src/rand/b64/mod.rs:3`、`src/_internal.rs:671`。
- **原因・影響:** 64-bit 型に `SeedableRng` だけを実装し `TryRng` を実装していない。seed を作れても rand の抽象 RNG API へ渡せない。共通 seed macro は `[u8;4]` と native endian を用い、64-bit seed と portable seed の扱いも不完全。
- **再現:** `python run_checks.py rand` の compile canary。`SplitMix64`, `Xoshiro256Pp`, `Sfc64`, `Philox4x64` に `rand_core::TryRng` bound を要求すると全 4 型が **E0277**。`logs/rand64-probe.log`。ライブラリ自身の feature check だけでは発見できない。
- **修正案:** native word 幅に応じた `TryRng`/seed 実装を追加し、代表的 consumer code をコンパイルする。
- **状態:** trait 不足は実証済み、seed の幅/endian は静的確認。比較元から残存。

### SOL-ASTRA-09 — Medium: counter-based C ABI の逐次化による性能退行

- **場所:** `src/cabi/prng/b64/splitmix.rs:29`、`src/cabi/cbrng/b32/threefry.rs:29`、`src/cabi/stream.rs:72`。
- **原因・影響:** 全出力を buffered stream の 1 word ずつのループに統一し、独立 block 計算、batch vectorization、Rayon、large buffer の NT store を失った。pending buffer が不要な SplitMix64 も同じ経路を通る。
- **再現:** 下記 benchmark。4,194,304 words/回の SplitMix64 は **2.74 倍**、Threefry32x4 は **16.49 倍**の所要時間。小 buffer の Threefry も約 2.05 倍。5 試行の中央値で raw range は明確に分離。
- **期待値・修正案:** valid input の SplitMix64 は比較元も同じ sequential stream を保持する。安全性検査を残し counter offset で分割でき、逐次化はその正確性に必須ではない。buffered head/tail を処理し body を独立 block 生成する経路を検討する。
- **状態:** 実測済みの新規退行。SplitMix64 は最適化不足と分類。Threefry は前後とも 01 の非標準関数で tail 管理も変わるため、「正しい実装が 16 倍遅い」という主張ではない。

## 3. 既存修正の評価

正しく確認できた改善:

- 有限の `[-MAX,MAX)` を含む float 変換が NaN/除外上端を返さず、full-width signed integer 変換も overflow しなかった。
- Xoshiro256++ は著者の遷移と 3 seeds × 1000 outputs 一致。xoroshiro128++/** の通常遷移、jump/long_jump も著者の多項式の独立適用と一致。
- ChaCha8/20、Salsa8/20 の block は独立参照と一致。ChaCha 参照は RFC 8439 公式 KAT にも一致。終端で停止する既存テストも通過。ただし Debug 漏えいは残る。
- C ABI SplitMix64/Threefry は odd tail と複数呼び出しの継続が一致。後者は非標準の scalar との stream 一致であり、01 を否定しない。
- 既存テストは C ABI の null/zero length/alignment/call partition、SIMD conversion、rejection sampling、shuffle/Bst 境界等で成功。独立した網羅検証とは扱わない。

不完全な修正は 01～03、05～08。明確に確認した新規退行は 04 と 09。07 は full-key API を追加した際に以前からある Debug の危険を処理していないもの。

unsafe な Rust FFI 操作、opaque handle/pending words、無効範囲の no-op、rejection による追加 word 消費は合理的な仕様変更。handle を外部で `Box<R>` と見なして解放するコードは移行が必要で、matching `*_free` を使う契約を守る必要がある。正確な stream 保持とすべての bulk 処理の逐次化は同義ではない。

## 4. 性能評価

同一 Rust の `-O -C panic=abort` で `bench.rs` をコンパイルし、前後の release/full rlib へリンク。前後を順番に測定し、別 benchmark は並列実行していない。seed 1、warm-up 後の各 5 回の中央値、ns/word。scalar は各 2^24 words。CPU 固定なし。

| 項目 | word 数/回 | 比較元 | 修正版 | 時間比 after/before | 分類 |
|---|---:|---:|---:|---:|---|
| scalar SplitMix64 | 16,777,216 | 0.760 | 0.760 | 1.000 | 測定誤差の可能性 |
| scalar Xoshiro256++ | 16,777,216 | 0.497 | 0.496 | 0.999 | 測定誤差の可能性 |
| scalar Xorshift64 | 16,777,216 | 1.148 | 1.148 | 1.000 | 測定誤差の可能性 |
| scalar Threefry32x4 | 16,777,216 | 2.248 | 2.283 | 1.016 | 小差。両方非標準 |
| C ABI SplitMix64 | 256 | 0.629 | 0.796 | 1.266 | 最適化不足の可能性 |
| C ABI SplitMix64 | 65,536 | 0.618 | 0.916 | 1.481 | 最適化不足。修正版は試行間のばらつきあり |
| C ABI SplitMix64 | 4,194,304 | 0.311 | 0.853 | 2.738 | 最適化不足。前後で stream は同じ |
| C ABI Xorshift64 | 4,194,304 | 1.149 | 1.156 | 1.006 | 測定誤差の可能性 |
| C ABI Threefry32x4 | 256 | 1.722 | 3.529 | 2.050 | batching 喪失、tail の意味も変化 |
| C ABI Threefry32x4 | 65,536 | 1.700 | 3.522 | 2.072 | 同上 |
| C ABI Threefry32x4 | 4,194,304 | 0.214 | 3.533 | 16.489 | batching/並列化喪失。両方非標準 |

SplitMix64 large の raw range は before 0.306–0.314、after 0.773–0.979 ns/word。Threefry large は before 0.206–0.215、after 3.495–3.574。大差を小さな測定誤差だけでは説明できない。

事前の安全性検査と pending 管理には必要なコストがあるが、測定からそのコストと最適化不足を厳密に分解してはいない。Threefry の誤った高速実装や異なる tail 管理を、同一仕様の正しい実装として比較していない。全 13 条件の raw 値、min/max、metadata は `benchmark-*.csv`, `benchmark-metadata.json`。他の generator/SIMD へ比率を一般化しない。

## 5. 実行した検証

このディレクトリから `python prepare.py` でコピーを作る。`src/tests/examples` と build 入力のみを読み、他モデルの報告書はコピーしない。測定に使用した依存解決は `Cargo.review.lock` に保存し、再現時にコピーへ適用する。

| コマンド / ログ | 結果 |
|---|---|
| 初回コピーで `cargo test --offline --lib --tests --features full` / `logs/existing-full.log` | 190 成功、0 失敗 |
| `python run_checks.py probes` / `logs/probes.log` | 9 成功、11 失敗 |
| `python run_checks.py counters` / `logs/counter-probes.log` | 0 成功、2 失敗（境界状態注入） |
| `python run_checks.py ciphers` / `logs/cipher-probes.log` | 5 成功、2 失敗（Debug） |
| `python run_checks.py rand` / `logs/rand64-probe.log` | compile canary が E0277、テスト未実行 |
| `python run_checks.py features` | default、単独 6 features、cabi+simd、full の 9 構成すべて成功 |
| `python run_checks.py docs` / `logs/doctests.log` | 通常 104 + compile_fail 1 成功、0 失敗 |
| `cargo check --offline --all-targets --features full` / `logs/alltargets.log` | 成功（rand compile canary 追加前） |
| `python run_checks.py build` / `logs/build-release.log` | release/full 成功 |
| `python audit_csharp.py` / `logs/csharp-build.log` | C# 54 エラー。DLL/import は 320/320 |
| `python audit_csharp.py --baseline` / `logs/csharp-build-before.log` | 比較元 C# は成功 |
| `python benchmark.py prepare`、`python benchmark.py measure` | 比較元ビルド、前後 13 条件 × 5 試行に成功 |

既存テストだけの再実行は `python run_checks.py existing`。失敗テストを修正・無効化せず、各ログの `EXIT` と summary JSON に保存した。配布する runner も失敗時は非ゼロで終了する。初回 PowerShell ログは UTF-8 へ変換済み。C# 検証は launch 制約により承認済みの sandbox 外実行を使用し、生成物は引き続きこのディレクトリ以下へ保存した。

独立参照: [Random123 KAT](https://github.com/DEShawResearch/random123/blob/main/tests/kat_vectors)、[RFC 8439 §2.3.2](https://www.rfc-editor.org/rfc/rfc8439.html#section-2.3.2)、[Bernstein の Salsa20 core](https://cr.yp.to/salsa20.html)、[Xoshiro256++ 著者実装](https://prng.di.unimi.it/xoshiro256plusplus.c)、[Xoroshiro128++ 著者実装](https://prng.di.unimi.it/xoroshiro128plusplus.c)。再現コードは `probes.rs`, `counter_probes.rs`, `cipher_probes.rs`, `cipher_reference.rs`, `rand64_probe.rs`。対象 hash は `reviewed-source-sha256.json`。

未実施: Linux/ARM/32-bit/Big Endian、MSRV 1.92、全 feature 組合せ、AVX2/AVX-512 がない実機、Miri/ASan/TSan、C# 実 P/Invoke、PractRand/TestU01、全 generator の長期周期・相関。host の AVX-512 環境で成功しても低機能 CPU の dispatch まで実証したことにはならない。未発見の UB がないという保証はしていない。

## 6. 総括

優先すべき採用障害は Threefry の非標準実装、wide の短周期・ゼロレーン、コンパイル不能な C#、full-key cipher の Debug 漏えい。Alias の有効/不正重みと rand_core 統合も残る。C ABI の連続性と数値変換には改善がある一方、counter-based generator の並列化を失う必要はなく、実測した退行を別途改善すべきである。未検証範囲を明示し、既存テスト成功だけを採用の根拠としていない。
