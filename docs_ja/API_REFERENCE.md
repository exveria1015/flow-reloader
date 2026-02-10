# API Reference（日本語）

Host/Logic 双方で使う公開インターフェースの実用リファレンスです。

## Logic マクロ

## `#[reload_direct]`

`&mut AppState` を直接更新する用途です。

許可されるシグネチャ:

- `fn update(state: &mut AppState)`
- `async fn update(state: &mut AppState)`

挙動:

- `hot_type`, `hot_entry`, `hot_reset` を生成
- async 関数は完了まで `RunStatus::Pending` を返す
- panic は捕捉され `RunStatus::Error` に変換

## `#[reload_snapshot]`

`CartridgeState` を実装した独自状態型で使います。

許可されるシグネチャ:

- `fn update(state: &mut MyState)`
- `async fn update(state: &mut MyState)`

挙動:

- `AppState::cartridge` から状態を復元
- 完了時に状態を cartridge へ再保存

## `#[reload_init]`

Host context の初期化フックです。

許可されるシグネチャ:

- `fn init(ctx: &mut MyContext)`

## エイリアスマクロ

- `#[reloadable]` -> `#[hot_swap]`
- `#[reload_direct]` -> `#[hot_swap]`
- `#[reload_snapshot]` -> `#[hot_reload]`
- `#[reload_init]` -> `#[hot_init]`

## Host マクロ

## `flow_reload::allocator!()`

Windows で:

- Logic 側の global allocator を `std::alloc::System` に設定
- Host 検証用の allocator tag を公開

## `flow_reload::main!(...)`

次を任意指定できる Host ループを生成します。

- 状態型
- context 値
- runtime ハンドル
- 初期化ブロック
- tick コールバック

バリエーション:

- context なし
- context あり
- context なし + runtime
- context あり + runtime

## 主要型

## `AppState`

共有 ABI 状態:

- `counter: i32`
- `message: [u8; 256]`
- `cartridge_len: u32`
- `cartridge: [u8; 8192]`

主なヘルパー:

- `set_message(&str)`
- `message_str()`
- `cartridge_bytes()`
- `write_cartridge(&[u8])`
- `load_cartridge<T: CartridgeState>() -> Result<T, String>`
- `save_cartridge<T: CartridgeState>(&T) -> Result<(), String>`

## `CartridgeState`

```rust
pub trait CartridgeState: Default {
    fn snapshot(&self, out: &mut Vec<u8>) -> Result<(), String>;
    fn restore(&mut self, input: &[u8]) -> Result<(), String>;
}
```

`cartridge-serde` feature 有効時は、serde 型に bincode ベース実装が自動付与されます。

## `FlowRunner`

コンストラクタ:

- `FlowRunner::new(path)`
- `FlowRunner::with_runtime(path, runtime)`
- `FlowRunner::with_context(path, &mut context)`
- `FlowRunner::with_context_and_runtime(path, &mut context, runtime)`

実行:

- `run(&mut AppState)`（sync または async 完了まで）
- `run_once(&mut AppState)`
- `run_attached_with_tokio(...)`
- `run_attached_with_async_std(...)`
- `run_attached_with_smol(...)`
- `reload_if_needed()`

## `RunnerError`

重要な variant:

- `AbiVersion`
- `StateSize`
- `AllocatorTag`（Windows）
- `InitFailed` / `InitError`
- `AttachedUnavailable` / `AttachedInvalid` / `AttachedAttachFailed`

`AbiVersion` / `StateSize` / `AllocatorTag` は再起動前提の致命エラーとして扱うのが推奨です。

## Runtime Feature Flags

`flow_reload` の feature:

- `tokio`
- `async-std`
- `smol`
- `cartridge-serde`

Host に必要な feature のみ有効化してください。

## 環境変数

- `LOGIC_LIB_PATH`: 読み込む logic ライブラリパスを上書き
- `FLOW_RELOAD_LOGIC_MANIFEST_DIR`: マクロ生成の debug ビルドループで logic manifest 位置を上書き

