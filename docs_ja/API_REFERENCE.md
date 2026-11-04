# API リファレンス

## メッセージ API

推奨する API は資源の所有・処理の受付・世代付き実行を分離します。
**Handler ABI 2**、Services ABI 1、タスク制御 ABI 1 を使い、旧 AppState ABI v4 とは独立しています。

### 資源とロジック

| API | 契約 |
| --- | --- |
| `HostResource::call(operation, input, output)` | 短い同期操作。書き込んだ長さまたは ServiceError を返し、借用バッファを保持しない |
| `HostResources::register(key, value)` | 非ゼロのサービス ID・版で登録し、型付き Resource<T> を返す |
| `Resource<T>::id()` / `try_borrow*()` | DLL には ID、ホスト内部だけ型付きアクセス |
| `HostResources::revoke(id)` | 以降のアクセスを失効。古い ID は再利用後のスロットを指さない |
| `HostResources::services()` | レジストリを保持する所有 Services を作る |
| `Services::call(...)` | レジストリ、ID 世代、サービス版、出力長を検査 |

Resource<T> の Drop だけでは登録を消しません。失効後も型付き所有者は本体を保持できますが、DLL 操作は REVOKED になります。
元のレジストリや Router のハンドルを捨てても、Services は所有参照を維持します。
同じレジストリを資源から保持する循環参照は revoke 等で解消してください。
所有スレッド上の信頼済みコード間のハンドルであり、悪意のある DLL の権限隔離ではありません。

```rust
use flow_reload::{RequestContext, ServiceError, ServiceKey};
const SERVICE: ServiceKey = ServiceKey::new(1, 1);
const STATE: ServiceKey = ServiceKey::new(2, 1);

#[flow_reload::reload_handler(SERVICE, state = STATE)]
async fn worker(ctx: RequestContext) -> Result<(), ServiceError> {
    let saved = ctx.checkpoint().ok_or(ServiceError::INVALID_ARGUMENT)?;
    ctx.cancelled().await;
    ctx.save_checkpoint(STATE, &saved.bytes)?;
    Ok(())
}
```

状態を使わない `reload_handler(SERVICE)` 形式もあります。
`async fn(RequestContext) -> Result<(), ServiceError>` から hot_handler_api と実行ごとの opaque Future を生成します。
factory 内で入力・seed をコピーし、サービスと制御表を retain します。
本体の開始は最初の poll です。未 poll の生成・破棄で I/O や共有状態の変更を始めません。

RequestContext は `input`、`generation`、`resource`、`service`、`services`、`call`、
`stop_requested`、`cancelled`、`checkpoint`、`save_checkpoint` を公開します。
checkpoint() は初期 seed、save_checkpoint はホストへの確定データのコピーです。
ServiceError と捕捉した unwind は RunStatus::Error へ変換し、元の詳細エラーは現在の RequestTask から取得できません。
デストラクタの panic や panic=abort からの復旧は対象外です。

### 受付と実行

| API | 挙動 |
| --- | --- |
| `unsafe RequestRouter::load(path, resources)` | 信頼済み SDK ネイティブコードをロード |
| `unsafe load_with_options(path, resources, options)` | 共有予算とチェックポイント上限を指定 |
| `unsafe from_api` / `from_api_with_options` | 静的・独自 API。コードと ABI の寿命は呼び出し元が保証 |
| `dispatch(resource_id, input)` | 入力をコピーし、Future<Output = RunStatus> の所有 RequestTask を返す |
| `unsafe prepare(path)` / `prepare_api(api)` | 受付を変えず、不活性な候補を準備 |
| `activate(candidate)` | Drain。サービスと状態スキーマの一致が必要 |
| `pause()` / `resume() -> Result` | 受付を停止・再開。キューイングと再実行はしない |
| `in_flight()` | 登録ワーカーと旧 Drain 世代を含む管理対象ルートの未完了数 |
| `generation()` / `is_accepting()` / `options()` | 受付世代・状態・上限を確認 |

ネイティブロードは今後の置換候補も含め、初期化・終了・コールバックの契約を守ることが unsafe の条件です。
メタデータ検査はネイティブコードの安全性を検証しません。別 Router・古い基準世代の候補は拒否し、Drain 後も pause は維持します。

RequestTask は Future・世代・制御を所有し、Router より長く生存できます。
ホストが所有スレッド上で poll します。generation・resource・status で状態を確認できます。
`cancel(self)` と Drop は当該 Future を同期的に破棄します。副作用の取り消しや独立した子処理の停止ではありません。
外部 Waker はコード参照を保持し、ネイティブの既定は KeepLoaded です。

### 登録ワーカーと Restart

- `start_worker(WorkerId, resource, input, Checkpoint)`: 再生成する処理を明示登録。
- `poll_workers(cx)`、`worker_status(id)`、`worker_checkpoint(id)`: 駆動と観測。
- `stop_worker(id)`: 協調停止の要求。`remove_worker(id)`: キャンセルと登録解除。
- `restart(candidate, migrate) -> Result<Restart<'_>, RouteError>`: 受付停止、旧 Drain 世代を含む全管理ルートの停止待ち、
  全チェックポイントの移行と factory 準備後に切り替え。Restart が登録ワーカーを駆動し、外部所有の RequestTask はホストが並行して進める。

再生成は登録ワーカーだけです。失敗・中断後は旧チェックポイントを維持し、受付を閉じたままにします。
resume()? はワーカーの再生成ではありません。Restart は状態スキーマを変更できますが、サービス契約は一致が必要です。
詳細は [Restart の契約](RESTART_AND_MIGRATION.md) を参照してください。

Checkpoint は ServiceKey スキーマと所有 Vec<u8> です。
RouterOptions の既定は共有 GenerationBudget の累積ロード32枠、チェックポイント1 MiB。
ネイティブ枠はロード前に予約し、ファイル不存在・検査失敗・破棄候補も返却しません。
Router 再作成時も同じ予算を共有でき、used()・limit() で確認します。プロセス全体のメモリ上限ではありません。

## 互換 API: AppState ABI v4

### ロジックとホストのマクロ

| マクロ | 入力・挙動 |
| --- | --- |
| `reload_direct` / `hot_swap` / `reloadable` | fn または async fn(&mut AppState)。直接更新 |
| `reload_snapshot` / `hot_reload` | fn または async fn(&mut MyState)。MyState: CartridgeState |
| `reload_init` / `hot_init` | fn(&mut MyContext)。ロードした各世代の初期化 |
| `allocator!()` | Windows の System allocator と、旧経路の allocator tag |
| `main!` | AppState ループ。明示的な型別名、runtime、init ブロック、tick を指定可能 |

async は独立した hot_make_future ハンドルを使い、async hot_entry は Error を返します。
hot_reset は Pending ハンドルを消しません。snapshot は外側の pinned async ブロックに状態を所有させ、
restore → inner Future の await → snapshot を行います。捕捉できる実行・復元・保存の unwind は Error ですが、全デストラクタを含む保証ではありません。

main! は Debug でソースを監視・ビルドし、Release は静的 hot_run を呼びます。
共通 blocking アダプターを使い、Future を破棄してから tick へ状態を渡します。
生 context 形式は削除しました。任意の型の状態は低水準の Execution<S> の対象で、固定 AppState Runner の型拡張ではありません。

### 状態と実行

AppState は counter: i32、message: [u8; 256]、cartridge_len: u32、cartridge: [u8; 8192] です。
set_message、message_str、cartridge_bytes、write_cartridge、load_cartridge、save_cartridge を使えます。

```rust
pub trait CartridgeState: Default {
    fn snapshot(&self, out: &mut Vec<u8>) -> Result<(), String>;
    fn restore(&mut self, input: &[u8]) -> Result<(), String>;
}
```

cartridge-serde feature は serde/bincode の実装を提供します。

FlowRunner のネイティブロードは new・with_runtime・with_context・with_context_and_runtime と raw 形式を含めすべて unsafe です。
置換候補も SDK 契約を守る必要があります。context はさらに具体型・レイアウト・排他性・有効期間を呼び出し元が保証し、
初期化失敗・外部タスク・コールバック・終了処理まで対象にします。Rust の型・寿命による証明は保持しません。新規実装はサービスを使ってください。

- start(AppState) は独立した Execution<AppState> を返し、リロード・Runner 破棄後も使えます。
  Execution<S: 'static> は安定した状態・Future・世代参照を所有し、所有スレッドで扱います。
  完了後に state()、Future 破棄後に cancel(self) で状態を回収します。unsafe from_ffi はコード・所有権の拡張境界です。
- run／run_until_done は Future 破棄まで状態を借用し、ホスト unwind でも破棄順を守ります。
  run_once は同期専用で、async は AsyncRequiresRun になります。
- run_attached_with_tokio／async_std／smol は実行スコープ内だけ借用サービス表を接続します。
  Future → 接続解除 → 表の順で破棄します。所有する start はこの旧サービス表を接続しません。
- reload_if_needed() は成果物の置換を検出します。KeepLoaded では開いた失敗候補と Runner 破棄後もコードを保持します。

unsafe set_unload_policy(WhenIdle) と collect_retired_libraries() は旧経路だけの操作です。
参照数ゼロだけでは足りず、DLL 呼び出し・コールバック・管理外タスク・遅延デストラクタの終了確認が必要です。
[回収契約](UNLOAD_SAFETY.md) を参照してください。

RunnerError は AbiVersion・StateSize・Windows の AllocatorTag、初期化失敗、attached 失敗等を区別します。
置換候補の拒否だけで、正常な旧世代の再起動が必要になるわけではありません。

### feature とサンプルの環境変数

feature は tokio、async-std、smol、cartridge-serde です。ホストに必要なものを有効にします。
旧サンプルは LOGIC_LIB_PATH、main! の Debug ループは FLOW_RELOAD_LOGIC_MANIFEST_DIR を利用します。
RequestRouter 共通の設定変数ではありません。[サンプル](../examples/README.md) と [テスト](TESTING.md) も参照してください。
