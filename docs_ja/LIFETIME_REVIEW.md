# 非同期実行の所有・借用・破棄順序

現行の Handler ABI 2 と、互換用 AppState ABI v4 の寿命契約を区別します。

## メッセージ実行

`RequestTask` は Future、コード世代、ホスト側の停止制御を保持します。
DLL の RequestContext は入力・初期チェックポイントを所有し、サービスとタスク制御の参照を retain／release します。
Router や元のレジストリハンドルを破棄しても、実行に必要な参照は残ります。
RequestTask の完了・cancel・Drop では外国側 Future を破棄してから未完了数を減らします。
Restart は管理対象ルートの Future 破棄まで待ってからチェックポイントを移行します。

これらは所有スレッドで扱います。独自 spawn・コールバックとその借用先は自動管理されません。
停止要求と子処理の終了、コード回収は別の契約です。詳細は [Restart](RESTART_AND_MIGRATION.md) を参照。

## コードの責務

- `flow_reload/src/execution.rs`: 型付きのホスト状態を所有する `Execution<S>`。Future → 状態 → 世代参照の破棄順序。
- `flow_reload/src/runner/blocking.rs`: 固定 AppState を借用する旧 API と、静的リンク実行の共通アダプター。
- `flow_reload/src/attached/future.rs`: FFI Future の所有・poll・破棄。
- `flow_reload/src/attached/waker.rs`: Waker の変換と、poll 中のホスト Waker の一時設定。
- `flow_reload/src/attached/host.rs`: ランタイム別の旧 sleep／TCP コールバック。
- `flow_reload/src/attached/mod.rs`: 旧サービスヘルパーとランタイム接続スコープ。
- `flow_reload/src/runner/generation.rs`: DLL 世代の保持・退役・明示的な回収。
- `hot_macro/src/exports.rs`: 共通 FFI エントリポイント、実行ごとの opaque Future ハンドルの生成。
- `hot_macro/src/direct.rs`／`snapshot.rs`: direct／snapshot の処理本体。

## 所有する実行

`FlowRunner::start(AppState)` は Runner を借用しない実行ハンドルを返す。
Future、安定したヒープ上のホスト状態、DLL 世代参照を一体で保持し、Runner のリロード・破棄後も poll できる。
ホスト状態を Box から raw owner に変換してから Future に貸し、借用後の Box の移動による retag を避ける。
`Execution<S>` の S は static なホスト所有型であり、低水準 factory は Future 破棄後に S を移動できる契約を守る。

未完了の状態は参照できない。完了・エラーでは Future の破棄後に `state()` で参照できる。
`cancel(self)` は Future を同期的に破棄して状態を返す。実行ハンドルの Drop でも同じ破棄順序を守る。
実行は非 Send とし、作成スレッドで poll・破棄する。ホスト Waker の別スレッドからの wake は別の契約として扱う。

これは一つの Future の所有とキャンセルであり、子タスクの停止・join やチェックポイントからの再生成ではない。

## 借用する旧 API と Release main

`run`／`run_until_done` は呼び出し中に AppState を排他的に借用し、内部で一つの外国側 Future ハンドルを保持する。
Pending の間に状態を再借用せず、ホスト待機処理が panic した場合もハンドルを破棄してから戻る。
`run_once` は同期専用で、非同期の場合は実行前に `AsyncRequiresRun` を返す。
Release `main!` も同じ実行処理を使い、Pending の状態を tick へ渡さない。

## Attached 実行

各実行で現在のランタイムハンドルからコールバック表を作成する。
破棄順序は Future → ランタイム接続解除 → コールバック表・ハンドル → ライブラリ。
接続情報はスレッド単位で保持し、同じスレッドで poll する。
poll 中の Waker 設定はネストや panic の後も元に戻す。
所有する `start` 経路へ、この借用するサービス表をそのまま持ち出すことはしない。

## スナップショット状態

外側の pinned async ブロックがロジック状態を所有する。
restore → inner Future の await → snapshot を Rust の async に生成させる。
手書きの自己参照コンテナと TLS slot は削除した。
Default・restore・実行・snapshot の panic は、unwind が有効なビルドではエラーへ変換する。
デストラクタや FFI 自体の panic、panic=abort からの回復は対象外。

## コンテキストと外部参照

全 `with_context*` は unsafe。型・レイアウト・有効期間・排他的アクセスを呼び出し元が保証する。
安全な main マクロの context 形式は削除した。新規の継続的ホストアクセスには版付きサービスを使う。
旧コンテキストは DLL 内の具体型・レイアウトの一致を自動検査しない。初期化失敗や外部へ逃げたアクセスの寿命も呼び出し元が保証する。
外部に残る Waker は世代参照で追跡するが、任意の spawn を自動追跡・停止しない。
物理回収には DLL 呼び出しから戻ったことや TLS 等の確認も必要。

現在の検証範囲は [テストと Miri](TESTING.md)、修正前の検出証拠は [監査記録](archive/MIRI_BASELINE.md)、
回収契約は [アンロードの確認記録](UNLOAD_SAFETY.md) を参照。
