# 保持量の観測とコピーの掃除

ホスト内の DLL 参照の観測、プロセス寿命のセッション管理、明示的なコピー掃除を提供します。
ネイティブ画像はプロセス終了まで保持し、ロード予算と Handler ABI 5 の契約を維持します。

## ホストで観測する

```rust,ignore
let snapshot = flow_reload::retention_snapshot();
println!("native_images={} recorded_copy_bytes={}",
    snapshot.tracked_native_images, snapshot.recorded_copy_bytes);
for image in snapshot.images {
    println!("{:?} {:?} {:?}: {}", image.id, image.state,
        image.outcome, image.copy.display());
}
println!("attempts={}/{}", router.options().generations.used(),
    router.options().generations.limit());
```

`retention_snapshot()` はこのホストの SDK 実体が保持するネイティブ参照を数えます。
現用、準備中、退役を区別し、検証拒否・候補破棄も追跡します。同じ Router 世代から prepare した複数候補にも
別々の `ArtifactId` を付けます。静的 API はネイティブ画像数には入りません。
スナップショットは値だけを所有し、画像・Future・Waker の寿命を延ばしません。

`recorded_copy_bytes` は保持する画像のコピー作成時のファイル長の合計です。
実メモリ使用量、OS のモジュール数、現在のディスク使用量とは別です。
`GenerationBudget::used()` も累積試行数であり、掃除や画像解放で減りません。
コピー失敗・ネイティブ open 失敗は累積件数を持ち、コピー削除エラーは直近32件と累積件数を返します。

## CLI でコピーを調べる

リポジトリのルートで実行します。

```powershell
cargo run -p flow_reload --bin flow-cache -- inspect
cargo run -p flow_reload --bin flow-cache -- clean
cargo run -p flow_reload --bin flow-cache -- clean --apply
```

`inspect` と `--apply` のない `clean` は読み取りだけです。欠けた管理ファイルを作成せず、削除もしません。
apply は表示済みの計画を信用せず、その時点で再検査します。
外部 CLI は実行中の他プロセスのメモリ量や保持画像数を取得しません。

既定のルートは `<temp>/flow-reloader/` です。ホストを起動する前に `FLOW_RELOAD_CACHE_ROOT` で
専用のローカル絶対パスを指定できます。SDK 実体は最初のネイティブロード時にルートを確定します。
CLI／API では `--root`／`CacheOptions::root` で同じルートを指定してください。
相対パス、`..`、リンク・junction 等を経由するルートは拒否します。

```powershell
cargo run -p flow_reload --bin flow-cache -- inspect --root C:\scratch\flow-cache
cargo run -p flow_reload --bin flow-cache -- clean --apply --max-sessions 100 --max-entries 10000
```

既定上限はセッション1024件、ディレクトリ項目65536件です。未知・旧形式の走査も項目上限に含めます。
打ち切りや読み取れない項目は `partial=true` として表示し、完全な合計とは扱いません。
件数・bytes は対象コピーのファイル長です。OS が報告する空き容量の増加を保証する値ではありません。

| 表示 | 意味 |
| --- | --- |
| InUse | 現 SDK のセッション、または lease が取得できない。他プロセスの生存を推測して削除しない |
| Eligible | 新形式・構造・権限の検査と lease 取得に成功した候補 |
| Trash | 利用終了を確認して削除専用領域へ移動済み。削除を再試行できる |
| Legacy | PID だけを使う旧形式。容量とパスの表示だけで、apply でも削除しない |
| Unknown | 不明・不完全な管理情報、未知ファイル、リンク、権限等の問題。削除しない |
| Deleted | 当該セッションの掃除が完了 |
| PartialFailure | trash の処理失敗。削除できた件数・bytes とエラーを報告し、次の apply で再試行 |

CLI の終了コードは0が完全な走査・操作、1が引数・ルート・ロック等のエラー、2が部分集計・部分失敗です。
正常な InUse／Legacy のスキップはエラーではありません。

## セッションと排他

新しいコピーは `managed-v1/sessions/<SessionId>/copies/<ArtifactId>.<ext>` に置きます。
プロセス／SDK 実体ごとに一つのセッションを使い、
一意な ID と新規作成で上書きを避けます。PID や時計の値だけでは削除を許可しません。

ホストはセッションの `lease.lock` をプロセス終了まで排他保持します。
最後の Router の Drop、全タスクの終了、prepare の失敗でも閉じません。
子プロセスへハンドルを継承しません。SDK 自体の途中アンロードや、コピーを別プロセスのロード元として渡す使い方は対象外です。

掃除は `namespace.lock` と対象の lease を非ブロッキングで取得します。
取得できない場合は見送り、成功時だけ `trash/<SessionId>` に移して既知の通常ファイルを個別に削除します。
掃除中断後の trash はロード元に戻しません。meta は最後まで残し、残った空ディレクトリだけを最後に削除します。
再帰的な一括削除、元のビルド成果物、プロジェクトの `target/`、DB、Cargo キャッシュの掃除は行いません。

namespace の排他はセッション作成・掃除を直列化します。競合時は WouldBlock／CLI のエラーとなり、
ホストは必要に応じて後でロードを再試行します。既存セッションでの通常ロード・実行にはこのロックを使いません。
自動アンロードや自動再起動、ロード予算の再利用も行いません。

## 権限と対象環境

同じプロトコルを守るホストが使う、同一利用者の専用ローカル領域を対象とします。
新規ディレクトリは Unix では0700、新規ファイルは0600で作成します。
Windows では新規ディレクトリだけに、利用者・SYSTEM・Administrators を許す継承可能な保護 DACL を設定します。
既存ディレクトリの権限は変更せず、所有者と他主体の書き込み権限を検査して、不適格なら拒否します。
Windows の [DACL 設定 API](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setnamedsecurityinfow) を使用しています。

権限を確認できない場合、未知の ACL、ロック非対応、I/O エラーを終了済みと読み替えません。
共有 `/tmp` のルートが別ユーザー所有等の場合も拒否するため、必要なら自分の専用ルートを明示します。
管理外の同時書き換え、攻撃者の管理するルート、ネットワーク FS のロック動作は対象外です。
この仕組みは任意の利用者による直接の DLL ロードを検出するものではありません。

## 検証

```powershell
cargo test -p flow_reload --test cache_lifecycle -- --test-threads=1
```

2026-11-06 に Windows で親ホスト・子ホスト・掃除 CLI を別プロセスにして確認しました。

- 実 DLL の現用・同一世代の複数候補・拒否・破棄・退役、静的 API と試行予算の区別。
- 全 Router の破棄後も生存中は削除せず、正常終了・強制終了後だけ適格になること。
- 子プロセスが残っていても、終了した親ホストの lease を継承していないこと。
- namespace の競合、二つの掃除 CLI の同時 apply、表示後の lease 取得、壊れた／過大な meta、lease 欠落、未知ファイル、旧形式、走査上限。
- junction とリンク先の sentinel を削除しないこと、trash の途中状態、実際のファイル削除失敗と再試行。
- CLI の既定が読み取りであることと、明示 apply の削除。

純粋な Rust の所有・実行経路は Miri でも回帰確認しますが、ファイルロック・DACL・実 DLL の代替検証にはしません。
整理前の CI 結果は [初期版の検証記録](archive/INITIAL_MILESTONE.md) に残しています。
今回の整理後の結果と区別してください。
ignored の2関数は親から起動する子プロセスの入口です。
親テスト間のセットアップは内部で直列化し、確認対象のプロセス競合は各試験内で明示的に作っています。

[当初の設計記録](archive/RETENTION_AND_CLEANUP_DESIGN.md) と [検証全体](TESTING.md) も参照してください。
