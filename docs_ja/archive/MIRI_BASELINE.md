# Miri 導入時の監査記録

本書は2026-11-02〜03の検出・修正経緯を保存する履歴です。本文の「現在」「今回」や件数は当時のものです。
途中の `CartridgeStateStorage` 案は後に削除し、現在は外側の pinned async ブロックが状態を所有します。
Release main の借用違反も共通 blocking アダプターと実行回帰テストで対処しています。
現在の契約は [寿命管理](../LIFETIME_REVIEW.md)、再実行手順と範囲は [テストと Miri](../TESTING.md) を参照してください。
記載した `target/` 以下の証拠はローカル成果物で、リポジトリには同梱していません。

実施日: 2026-11-02

## 実行環境と範囲

- ホスト: Windows / x86_64-pc-windows-msvc
- インタープリターの対象: x86_64-unknown-linux-gnu
- ツールチェーン: nightly 2026-11-02、rustc 1.101.0-nightly (3e92e1a2d 2026-11-01)
- 借用モデル: Stacked Borrows、Tree Borrows
- 通常ビルドの既定ツールチェーンは stable のまま。

`--no-default-features` で、DLL やネットワークを使わない次の対象を選んで実行する。
Miri のためにテストを無視したり、借用検証を無効化したりする設定は使わない。

| 対象 | テスト数 | 初回 Stacked | 初回 Tree | 修正後 Stacked / Tree |
| --- | ---: | --- | --- | --- |
| `--lib attached::` | 初回4、修正後5 | 成功 | 成功 | 成功 |
| `--test async_direct` | 1 | 成功 | 成功 | 成功 |
| `--test async_snapshot` | 1 | 借用違反 | 借用違反 | 成功 |

修正後は対象7件を2モデル・seed 0 / 1で実行し、延べ28件が成功した。
保存したソースの SHA-256 が最終コードと一致することも確認済み。
通常の Windows 実行では、Tauri GUI を除くワークスペーステストと Release テストが成功した（非同期関連を含む15件）。
整形チェックと、既定／全 feature の Clippy も成功した。GitHub 上での CI 実行は未確認。

## 検出した問題

非同期 `reload_snapshot` の生成コードは、次の順で自己参照を作っていた。

```rust
let ptr = state.as_mut() as *mut State;
let future = Box::pin(update(&mut *ptr));
*slot = Some(CartridgeSlot { future, state });
```

ヒープのアドレスは変わらなくても、Future が参照する状態を所有する通常の `Box` を後から移動することが、借用の有効性に影響する。
前回修正した「Future → 状態」の破棄順序だけでは不足していた。

- Stacked Borrows: 最初の poll で `trying to retag ... for Unique permission`。
- Tree Borrows: Pending のリセット後、状態の破棄で `reborrow ... is forbidden`。

同じ所有構造の最小再現でも両モデルで失敗した。対照として `Box::into_raw` 後のポインタを所有し、Future の破棄後に `Box::from_raw` で回収する構造は両モデルで成功した。
これに基づき、`hot_macro/src/snapshot.rs` の状態保持を専用の所有型 `CartridgeStateStorage` に変更した。
保存先の移動中は生ポインタとして保持し、Future の破棄後にだけ状態を読み出し、最後に一度だけ `Box` を再構成して解放する。

## 修正後に確認する経路

- Pending のリセット、ロジックの panic、正常完了での破棄順序。
- restore / snapshot の panic 後の状態解放。
- Pending → Ready と複数回 poll を経た状態保存。
- FFI Future の一度だけの破棄と、完了後に再 poll しないこと。
- Waker の clone / wake / wake_by_ref / drop。
- 元の poll スコープ終了後、別スレッドからの所有済み Waker の使用と解放。
- ランタイム接続の解除、スレッド分離、ネストした poll 後の Waker 復元。

## 再実行

PowerShell から実行する。初回のみ対象の nightly と Miri の sysroot を用意する。

```powershell
rustup toolchain install nightly-2026-11-02 --profile minimal --component miri --component rust-src
cargo +nightly-2026-11-02 miri setup --target x86_64-unknown-linux-gnu
./scripts/check-miri.ps1 -Toolchain nightly-2026-11-02
./scripts/check-miri.ps1 -Toolchain nightly-2026-11-02 -Seed 1
```

今回のローカル実行では、同一リビジョンの `nightly` を指定した。
スクリプトは失敗しても残りの対象を実行し、一つでも失敗したら終了コード1を返す。
ログ、実際のコンパイラバージョン、対象ソースと Cargo.lock の SHA-256、各実行の終了コードを `target/miri-audit/seed-<seed>/` に保存する。
CI は日付固定の nightly と両借用モデルで同じスクリプトを実行し、ログを artifact として保存する。

今回の修正前の証拠は `target/miri-audit/baseline-seed-0/` に保存した。
初回の詳細診断は `baseline-default.log` / `baseline-tree.log`、マクロ展開は `async_snapshot.expanded.rs`、最小再現の結果は `repro-results.json` に保存した。
これらの `target/` 以下のファイルはローカルの診断成果物であり、Git の管理対象外。

## この確認が保証しないもの

この時点の Miri 対象は生成されたエントリポイント等であり、Release 版 `main!` のループ実行を含んでいない。
その後、そのループで別の借用違反を検出した。[Release main の検出記録](#release-main-の追加検出記録) を参照。

実際の DLL のロード／アンロード、独自 spawn タスクの終了、DLL を越えた Waker の残存は、今回の Miri 対象に含めていない。
これらは通常の実DLLテストと、世代管理・キャンセルの API 設計で扱う。
その後追加した実 DLL の検証と、既定の保持方針は [アンロードの確認記録](../UNLOAD_SAFETY.md) を参照。
Miri は実行した経路の検証であり、安全性全体の証明にはならない。FFI・ネットワーク等にも対応の制限がある。[Miri の公式説明](https://github.com/rust-lang/miri#readme)

## Release main の追加検出記録

以下は修正前の監査記録です。現在の main! は該当する実装を使いません。


`flow_reload/src/macros.rs` の Release 分岐では、`hot_entry(&mut state)` を呼び、結果が Pending でも `tick(&state)` を実行する。
次の poll でも `&mut state` を作り直すため、ライブラリに保存された Future の借用と両立しない経路がある。
`FlowRunner::run_until_done` には対策を入れてあるが、Release の直接呼び出しループには反映されていない。

再現は、状態を変更 → 一度 Pending → 同じ状態を変更して Ready となる非同期ロジックを、実際の `flow_reload::main!` で動かすもの。
Miri の `--release` で Release 分岐を選択し、両借用モデルで2回目の状態更新に対する借用違反を検出した。
Miri は最適化を実行しないため、これは Release 分岐の意味論の検証であり、最適化後の機械語を検証したものではない。

ローカルの再現プログラムとログ:

- `target/remaining-audit/release-main/`
- `target/remaining-audit/release-main-stacked.log`
- `target/remaining-audit/release-main-tree.log`

実行したコマンド:

```powershell
$env:MIRIFLAGS = '-Zmiri-seed=0'
cargo +nightly miri run --release --manifest-path target/remaining-audit/release-main/Cargo.toml --target x86_64-unknown-linux-gnu
$env:MIRIFLAGS = '-Zmiri-tree-borrows -Zmiri-seed=0'
cargo +nightly miri run --release --manifest-path target/remaining-audit/release-main/Cargo.toml --target x86_64-unknown-linux-gnu
```

再現はコンテキストなしの形式で実施した。コンテキストありの Release 分岐にも同じ呼び出し構造があり、修正・回帰テストの対象に含める。
既存の `main_macro` テストは各形式のコンパイル確認であり、生成された main のループを実行していない。
修正ではまず現在の「完了まで呼び出し中に借用する」方式を Release でも成立させる。新しい非同期実行 API の導入とは切り分ける。
