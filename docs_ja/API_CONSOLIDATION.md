# API の一本化

2026-11-06 に、利用先がない段階で実行経路をメッセージ API に統一しました。
互換アダプターとその専用サンプルを削除し、ネイティブロードと静的リンクの双方で
`reload_handler`・`HostResources`・`RequestRouter` を使います。

## 削除・変更した API

| 整理前 | 現在の方法 |
| --- | --- |
| `FlowRunner`、固定 `AppState`、`Execution<S>` | `RequestRouter` と `RequestTask`。継続する状態はホスト資源へ |
| direct／snapshot／init の旧マクロと別名、`main!` | `reload_handler` と明示的なホスト実装 |
| cartridge、`cartridge-serde` | 版付き `Checkpoint`、`InitPayload`、アプリケーションの codec |
| attached ランタイム、専用 sleep／TCP、runtime feature | `HostResource::start_async`、`ServiceCall`、`ThreadService` |
| 生の初期化 context、allocator tag と `allocator!` | `HandlerEnvironment`、サービス割り当て、所有する初期化データ |
| `AttachedFuture`／`AttachedFutureVTable`／`AttachedFutureTask` | `TaskHandle`／`TaskVTable`／`FfiFuture` |
| `RunnerError` | `native` 有効時の `LoadError`。受付全体の失敗は `RouteError` |
| `UnloadPolicy`、`WhenIdle`、明示的アンロード | 開けた画像をプロセス終了まで保持。観測と終了後のファイル掃除を提供 |
| `internal` のビルド・パス・監視関数 | `dev-tools` feature の `development` モジュール |

削除した API の別名や互換 feature は残していません。
Handler ABI 5、Services ABI 2、タスク制御 ABI 2 のバイナリレイアウトは維持しています。
ただし Rust 側の API は破壊的変更なので、ホストとロジックを同じ SDK でビルドし直してください。

## ビルド構成

| 構成 | 指定 | 含まれる機能 |
| --- | --- | --- |
| 開発ホスト | 既定 feature | DLL ロード・保持・掃除、ソース監視・ビルド補助 |
| 成果物を外部でビルドするホスト | `default-features = false, features = ["native"]` | DLL ロード・保持・掃除 |
| ロジック crate・静的ホスト | `default-features = false` | サービス・タスク・ルーティング・生成マクロ |

Debug／Release とロード方式は独立しています。静的ホストの実行例は次のとおりです。

```sh
cargo run -p service_host --release --no-default-features --features static-logic
```

生成した `service_logic::handler_api()` を `unsafe RequestRouter::from_api` へ渡します。
静的構成には DLL ローダー・ファイル監視・キャッシュ操作の OS 依存を含めません。

## リポジトリの管理

`cargo build` と `cargo test` の既定対象はコア3 crate にしました。
GUI・Bevy・DB サンプルはワークスペースに残し、必要なときに `-p` で選びます。
Cargo.lock を管理し、CI は `--locked` で依存解決を固定します。
コアは Rust 1.95、Bevy サンプルは Rust 1.97.1 をマニフェストに記載しています。
最小バージョンと OS ごとの動作は CI で個別に確認します。

所有・停止・Drain／Restart・外部副作用の契約は維持しています。
現在の検証手順と未確認範囲は [テスト](TESTING.md) と [残る課題](REMAINING_WORK.md) を参照してください。

## 整理後のローカル検証

2026-11-06 に Windows、Rust 1.99.1、ローカルの nightly Miri で確認しました。

| 検査 | 結果 |
| --- | --- |
| 既定・feature なし・native 単独の Clippy | 警告をエラーとして成功 |
| GUI を除く全ワークスペースの Clippy | 成功 |
| GUI を除く全ワークスペースのテスト | TCP 3件・junction 1件を除いて成功 |
| コアの Release テスト | 同じ環境制限の4件を除いて成功 |
| 静的 service_host の Release と依存ツリー | 成功。DLL ローダー・監視・ランタイムの依存なし |
| 実 DLL のサービス契約・SQLite 移行・Bevy 統合 | それぞれ成功 |
| Miri Stacked／Tree、seed 0 | 両モデルとも全9グループ成功。追加した panic／null host 試験も両モデルで成功 |
| rustdoc、GUI JavaScript の構文 | 成功 |

TCP と junction はこの実行環境のアクセス制限で実行できませんでした。
通信を伴う ignored 統合試験も、制限のない環境で確認する必要があります。
Tauri 本体の Clippy は、未キャッシュの `futures-sink 0.3.34` を DNS 制限で取得できず中断しました。
フロントエンド構文の成功を Tauri 全体の検証結果には含めません。
Rust 1.95 のコア検査、今回の変更に対する Windows／Linux CI と GUI の実操作も未確認です。
