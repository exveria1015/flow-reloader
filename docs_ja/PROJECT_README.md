# flow-reloader

Rust のロジッククレートをホットリロードするためのフレームワークです。アタッチ型の非同期ランタイム連携と、スナップショット状態の永続化をサポートします。

このワークスペースは次で構成されます。

- `flow_reload`: ホスト/ランタイム側の公開 API
- `hot_macro`: プロシージャルマクロ（`#[reload_direct]`, `#[reload_snapshot]`, `#[reload_init]`）
- `interface`: ABI 共有型（`AppState`、メタデータ、cartridge API）
- `examples/*`: 実行可能な host + logic サンプル

## このプロジェクトの目的

`flow-reloader` は、ホストプロセスを停止せずにロジックのみを再ビルド/再読み込みするワークフロー向けに設計されています。

主な目標:

- ロジック差し替え中もホスト状態を維持する
- ABI/状態不整合を実行時に検出する
- 同期/非同期の両エントリポイントを扱う
- リロード間で任意の状態スナップショットを保持する

## 特徴

- 動的ライブラリのリロードとファイル監視
- メタデータ検証（`ABI_VERSION`、`AppState` サイズ、Windows の allocator tag）
- `tokio` / `async-std` / `smol` 向けの attached async ブリッジ
- `AppState::cartridge` を使ったスナップショット保存
- ホスト補助マクロ（`flow_reload::main!`, `flow_reload::allocator!`）

## クイックスタート

サンプルホストを起動:

```bash
cargo run -p sync_host
```

次にロジックを編集:

- `examples/sync_logic/src/lib.rs`

デバッグビルドでは、ホストがロジックを自動ビルド・自動リロードします。

他のサンプル:

- `cargo run -p async_tokio_host`
- `cargo run -p async_std_host`
- `cargo run -p async_smol_host`
- `cargo run -p snapshot_host`
- `cargo run -p tauri_tokio`

## ドキュメント

- `docs_ja/README.md`: 日本語ドキュメントの索引
- `docs_ja/GETTING_STARTED.md`: 導入手順
- `docs_ja/API_REFERENCE.md`: マクロ/API/feature 仕様
- `docs_ja/PRODUCTION_GUIDE.md`: 本番運用チェックリスト
- `docs_ja/TROUBLESHOOTING.md`: よくある障害と対処

## サンプル構成

```text
examples/
  sync_logic/        # ロジッククレート（cdylib + rlib）
  sync_host/         # ロジックを読み込むホストプロセス
  async_tokio_logic/ # attached ランタイムを使う async ロジック
  async_tokio_host/  # tokio で attached future を駆動
  snapshot_logic/    # serde cartridge を使うスナップショットロジック
  snapshot_host/     # cartridge を decode して状態を表示
```

## 補足

- Debug では自動ビルド/監視ループが有効です。
- Release では直接エントリポイントを呼び、監視/自動ビルドは行いません。
- Windows では allocator 整合のため、logic 側に `flow_reload::allocator!()` を入れてください。

