# DLL のアンロードと管理外タスク

## 既定の動作

メッセージ API の RequestRouter は KeepLoaded 固定で、下記の WhenIdle 操作は旧 FlowRunner のみが公開する。

`FlowRunner` の既定は `UnloadPolicy::KeepLoaded`。
リロードは新しい DLL への切り替えを行うが、旧 DLL のコードはプロセス終了まで保持する。
Runner を破棄しても、旧 DLL を自動でアンロードしない。
開けた候補 DLL は、シンボル検証・ABI 検証・初期化で失敗した場合も既定では保持する。
初期化が失敗する前に管理外の処理を開始している可能性を、ホスト側では排除できないため。

これは「DLL 内のコードが解放された後で呼ばれる」問題への対策であり、タスクを止めたり、借用先のデータの寿命を延ばしたりする保証ではない。
`with_context` 系は現在 unsafe であり、ポインタの寿命はその利用者の契約となる。
独自に spawn した処理は継続するため、外部 I/O や副作用も継続し得る。

保持にはコストがある。リロードごとにコード・静的データ・コピー DLL の使用量が増える。
コピー先はユーザーの一時領域の `flow-reloader/managed-v1/sessions/<SessionId>/copies/`。
ファイルはプロセス終了後も残る場合がある。[観測と掃除](RETENTION_AND_CLEANUP.md) は終了済みの
新形式セッションを検査・削除する。DLL のアンロード条件を緩めず、旧 PID 形式は表示だけにする。

## 追跡する参照

`run`／`start`／`run_attached_with_*` は、その実行で使う DLL 世代への参照を Future とホスト Waker に渡す。
`HostWakerVTable::clone` で作られた Waker も同じ参照を保持する。
Future の完了後や Runner の破棄後に Waker が残っていても、その世代への参照は残る。

旧世代はホストの待機リストに移す。Waker の最後の参照が他スレッドで破棄されても、その場では DLL を閉じない。
ライブラリを開いたスレッドが終了した場合も、別スレッドに自動回収を引き継がせない。

低水準の `AttachedFutureTask::new`／`Execution<S>::from_ffi` は、この DLL 世代の追跡を自動で付けない。
同 API を直接使う呼び出し元は、従来どおりコード・状態・コールバックの寿命を保証する必要がある。

## 明示的にアンロードする場合

`unsafe { runner.set_unload_policy(UnloadPolicy::WhenIdle) }` で、現在と今後の世代を回収候補にできる。
既に KeepLoaded として退役した世代には遡って適用しない。
この設定自体や `reload_if_needed`、Runner の Drop は DLL を閉じない。

回収は `unsafe { collect_retired_libraries() }` で明示的に行う。
対象は、同じスレッドで作られた WhenIdle 世代で、追跡中の参照がなくなったものだけ。
戻り値はホストのライブラリ参照を解放した数であり、OS が別の参照を持っている場合の物理的なアンロードまでは表さない。

この unsafe 呼び出しの前に、呼び出し元は以下の条件を成立させる必要がある。

- 管理外のタスク・スレッドは停止を確認し、必要な join を完了する。
- 外部コールバックを解除し、実行中の呼び出しが DLL から戻っていることを同期して確認する。
- 後から実行される TLS・値のデストラクタや、DLL の関数ポインタが残らないようにする。
- そのスレッドで作った回収候補すべてを確認する。初期化に失敗した候補も含む。

参照カウントがゼロになっただけでは不十分。例えば DLL 側の Waker 解放関数は、ホストの最後の参照を解放した後も、呼び出し元へ戻る命令を実行する。
そこで別スレッドが DLL を閉じる競合を防ぐため、参照数の確認と「DLL 側の呼び出しが戻ったこと」の確認を区別する。
検証できないライブラリには KeepLoaded を使う。

ABI v4 では生成マクロの TLS Future slot を削除した。ただしランタイム接続用の TLS や依存ライブラリの終了処理までなくなったとはみなさない。
WhenIdle の実アンロード検証には TLS を使わない専用フィクスチャを用いており、任意の生成マクロ付き DLL を無条件にアンロードできるという保証ではない。

## spawn の区別

| 処理 | コードの所在と扱い |
| --- | --- |
| `spawn_detached_sleep` / `spawn_detached_tcp_request` | ホストの vtable 経由で値をコピーし、ホスト側の固定処理を実行する。ロジックの任意のクロージャを受け取らない。 |
| ロジック自身の `tokio::spawn` / `smol::spawn` / `std::thread::spawn` 等 | ホストが自動で発見・停止することはできない。KeepLoaded でコードを保持するか、呼び出し元が終了を確認する必要がある。 |
| DLL の poll/drop 関数を持つ Future をホストが独自に spawn | Runner を経由しなければ世代参照は自動では付かない。同じ管理外処理として扱う。 |

JoinHandle を捨てることは停止確認にならない。例えば Tokio の JoinHandle は Drop でタスクを detach し、タスクは継続する。[Tokio の公式仕様](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html)

## 実 DLL による検証

Windows で、テスト用 cdylib を rustc で生成して確認した。

1. DLL 内で開始した管理外スレッドが、リロードと Runner の Drop 後も実行できること。
2. DLL から返した Future を Runner 外の Tokio LocalSet に spawn し、リロード・Runner の Drop 後に poll/drop を完了できること。
3. 初期化が管理外スレッドを開始してから失敗した場合も、コードを保持すること。
4. WhenIdle の DLL がホスト Waker を保持している間は回収されないこと。
5. 外部スレッドで Waker を使用・解放した後、join と所有スレッドからの明示的な回収でアンロードできること。

ファイルの削除だけでなく、Windows の `DllMain(DLL_PROCESS_DETACH)` からホストのカウンタを更新し、終了処理が呼ばれたことも観測する。
Linux 用には `.fini_array` の同等プローブを用意し、既存の CI 対象に含めている。Linux の実行結果はローカルでは未確認。
Miri は実 DLL のロードを使わず、Waker の参照がスコープ・スレッドを越えて保持・解放される部分を引き続き確認する。

再実行:

```powershell
cargo test -p flow_reload --all-features runner::tests::
cargo test -p flow_reload --release --all-features
./scripts/check-miri.ps1 -Toolchain nightly-2026-11-02
```

検証の対象・限界と CI の範囲は [テスト](TESTING.md) を参照。

管理外タスクの強制停止や、追跡付きの汎用 spawn API は提供しない。
メッセージ API では [明示登録した処理](MANAGED_ACTIVITIES.md) の停止・join・解除を
ルートの終了待ちに含める。未登録の処理の終了確認や、WhenIdle の自動回収を代替するものではない。
