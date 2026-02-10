# Production Guide（日本語）

このガイドは、`flow-reloader` を本番で安定運用するための実務ポイントをまとめたものです。

## 実行モデル

Debug ビルド:

- Logic ソースを自動ビルド・自動監視
- ローカル開発用途

Release ビルド:

- 自動ビルド/監視ループなし
- Host が export されたエントリポイントを直接呼ぶ
- アーティファクト配布タイミングを明示的に管理

## 推奨デプロイ戦略

1. CI で logic アーティファクトをビルドする
2. 不変なバージョン名で保管・配布する
3. Host が見るライブラリパス（またはシンボリックリンク）を切り替える
4. メインループで `reload_if_needed()` を呼ぶ

本番 Host プロセス内でのコンパイルは避けてください。

## ABI 互換ルール

Host は実行前に次を検証します。

- `ABI_VERSION`
- `AppState` サイズ
- Windows の allocator tag

不一致ならリロード失敗になり、互換アーティファクトで再起動が必要です。

次は慎重に変更してください。

- `AppState` レイアウト
- メタデータ契約
- エクスポートシンボル仕様

## Context / Init の安全性

`hot_init` はメタデータ検証後にのみ実行されます。

運用ルール:

- context ポインタは runner 生存中ずっと有効にする
- init は冪等・失敗可能を前提に設計する
- 成功時は必ず `RunStatus::Done` を返す

## Snapshot 状態の方針

`#[reload_snapshot]` を使う場合:

- スナップショット schema に明示バージョンを持たせる
- 直列化失敗をデプロイブロッカーとして扱う
- schema 変更時は `restore()` に移行ロジックを入れる

`CartridgeState::snapshot` は `Result` を返すため、直列化失敗はサイレントに無視されません。

## Async Attached Runtime

attached モード運用時:

- 必要な runtime feature（`tokio` / `async-std` / `smol`）だけ有効化
- `AttachedSleep` / `AttachedTcpRequest` は attached 実行コンテキストでのみ使用
- `AttachedUnavailable` / `AttachedAttachFailed` を明示ハンドリング

## エラーハンドリング方針

推奨方針:

- 一時的エラー: バックオフして再試行
- メタデータ不一致（`AbiVersion`, `StateSize`, `AllocatorTag`）: 即 fail fast + 再起動
- init 失敗: 即 fail fast + 配備状態を通知

## 可観測性

最低限、次を記録してください。

- リロード試行回数と成否
- メタデータ不一致の詳細
- init 失敗
- `RunStatus::Error` の発生頻度

`run()` と `reload_if_needed()` の前後に構造化ログを入れるのが有効です。

## 本番チェックリスト

- [ ] Host と Logic を互換リビジョンでビルドしている
- [ ] ABI/State 互換テストが通っている
- [ ] リロード成功/失敗メトリクスを収集している
- [ ] 致命 `RunnerError` の再起動方針がある
- [ ] Snapshot schema と移行方針を文書化している
- [ ] Runtime feature を最小化している

