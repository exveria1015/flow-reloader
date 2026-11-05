# 導入手順

## 接続を維持するサンプル

リポジトリのルートで実行します。

```powershell
cargo run -p service_host
```

TCP クライアントで `127.0.0.1:4000` へ接続し、改行付きのテキストを送ります。
[service_logic](../examples/service_logic/src/lib.rs) の応答ラベルを変更・保存して、
**同じ接続**で次のメッセージを送ると、新世代の応答を確認できます。
`slow` と改行を送ると約5秒の処理になり、Drain 中の旧処理を観察できます。

単一接続の小さな例です。ビルドは同期処理で、入力待ち中の変更は次のメッセージ境界で反映します。
画面内で編集する場合は [Tauri GUI Lab](TAURI_GUI_LAB.md)、
停止・移行の最小例は `cargo run -p flow_reload --example restart` を使ってください。

## 新しいアプリケーションへ組み込む

ホストを長寿命の `bin`、ロジックを `cdylib` に分け、両方から `flow_reload` へ依存します。
メッセージ API ではホストからロジックへの静的リンク依存は不要です。
`examples/my_logic` に置くロジックの Cargo.toml の例:

```toml
[package]
name = "my_logic"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
flow_reload = { path = "../../flow_reload", default-features = false }
```

クレートを workspace members に追加します。ホストとロジック間のサービス ID・版・操作番号・
バイト列の形式を定義してください。ID が同じでも、データの意味が自動で検証されるわけではありません。

```rust
use flow_reload::{RequestContext, ServiceError, ServiceKey};
const SESSION: ServiceKey = ServiceKey::new(0x4c49_4e45, 1);

#[flow_reload::reload_handler(SESSION)]
async fn handle(ctx: RequestContext) -> Result<(), ServiceError> {
    ctx.call(0, ctx.input(), &mut [])?;
    Ok(())
}
```

操作0はホスト側で定義します。たとえば有限の送信キューへ入力をコピーする処理です。
`HostResource::call` は短い非ブロッキング処理とし、実際のソケット・受信・送信・フレーミングはホストに置きます。
ホストの全体例は [service_host](../examples/service_host/src/main.rs) を参照してください。

ホストで `HostResources` に資源を登録し、ビルドした DLL を
`unsafe RequestRouter::load(path, resources)` でロードします。
初期化・終了処理と今後の置換候補を含め、SDK 契約を守る信頼済みコードであることが unsafe の条件です。
`dispatch(resource.id(), input)` は入力をコピーし、所有する Future を返します。ホストが poll を進めてください。
コード世代と資源へのアクセスはハンドルで保持し、ソケットなどの Rust オブジェクト自体は DLL へ渡しません。

## 切り替え方式を選ぶ

- **Drain**: ビルド後に `unsafe prepare(path)`、続いて `activate(candidate)`。
  既存処理は旧世代、新規受付は新世代です。サービス契約と状態スキーマは一致が必要です。
- **Restart**: `start_worker` で再生成対象を登録し、チェックポイントを明示的に保存します。
  `restart(candidate, migrate)` の停止待ち中も、ホストが所有する通常タスクの poll を進めます。
  再生成するのは登録ワーカーだけで、通常リクエストは再実行しません。

詳細は [ホスト資源と受付](HOST_RESOURCES_AND_ROUTING.md)、
[Restart と移行](RESTART_AND_MIGRATION.md) を参照してください。

静的リンクと feature の選択は [API の一本化](API_CONSOLIDATION.md) を参照してください。
