# トラブルシューティング

## メッセージ API

| エラー・症状 | 確認と対処 |
| --- | --- |
| `Abi` / handler ABI mismatch | 同じ SDK で両側を再ビルド。Handler ABI は2、旧 AppState ABI は4 |
| `ContractMismatch` | 資源とハンドラーのサービス ID・版を合わせる。サービス本体の移行はアプリ側の責務 |
| `StateSchemaMismatch` | Drain は同じスキーマが必要。形式変更はチェックポイント変換を伴う Restart へ |
| `WrongRouter` / `StaleCandidate` | 対象 Router の現在世代から候補を準備し直す |
| `GenerationLimit` | 累積ロード予算が枯渇。開発プロセスを再起動。失敗したネイティブロードも消費する |
| Restart 失敗・中断後の `Paused` | 復旧方針を決めて再試行、または `resume()?`。旧ワーカーの再生成とは別 |
| Restart が Pending のまま | 外部所有の RequestTask も poll する。ロジック側で停止要求に応答し、子処理も終了させる |
| `WorkerFailed` | 状態を確認し、必要なら登録を解除・修正して再試行。通常要求を無条件で再実行しない |
| `CheckpointTooLarge` / 移行失敗 | 形式・スキーマ・サイズ上限を確認。旧チェックポイントは残る |
| サービスの `BUSY` / `BUFFER_TOO_SMALL` | 競合する借用を解放、または十分な出力領域を確保。自動リトライはない |
| `REVOKED` / 資源が見つからない | 失効、レジストリ、ResourceId の世代を確認。正しいレジストリの有効 ID を使う |

Drain で常駐ワーカーが旧コードのままなのは正常です。GUI で新しいリクエストの文言だけが変わり、
カウンターの加算幅が変わらない場合は Restart を選択して再ビルドしてください。

## ビルド・リロード

ホストが監視するパスとビルドログを確認します。Router 自体に監視機能はありません。
`service_host` はメッセージ境界または処理の poll 中に変更を扱い、GUI はバックグラウンドでビルドします。
旧サンプルホストは成果物を監視するため、対応する `target/hot-reload-build` へ手動再ビルドします。
旧 `main!` のソース自動監視は Debug のみです。

Windows のアクセス拒否では、エラーに出た**パス**を確認してください。
Cargo の出力先とユーザー一時領域に書き込み権限が必要です。DLL コピー先は現在の作業ディレクトリではなく
`flow-reloader/<PID>/` です。権限と使用中プロセス・ファイルロックを確認し、使用中 DLL のコピーは削除しません。
不完全な成果物は完全にビルドし直してから候補を準備します。

GUI の HTML／CSS／JS はホストの埋め込みアセットです。変更後はホストを再ビルド・再起動してください。
ホットリロード対象は Rust ロジック DLL です。

## 旧 API

- `AbiVersion` / `StateSize` / `AllocatorTag`: ABI v4 と AppState を両側で合わせて再ビルド。
  Windows の旧ロジックには `flow_reload::allocator!()` が必要です。
  置換候補の拒否では旧世代を維持するため、一律にプロセス再起動が必要という意味ではありません。
- `InitFailed` / `InitError`: init の戻り値・panic と、unsafe コンテキストの型・排他性・
  外部に逃げた参照を含む寿命を確認します。
- `AsyncRequiresRun`: `run_once` は同期専用。用途に応じて `run`、所有する `start`、attached 実行を使います。
- `AttachedUnavailable` / `AttachedAttachFailed`: async SDK ロジックと、ホストの runtime feature・
  `run_attached_with_*` を合わせます。所有する `start` は旧サービス表を接続しません。

KeepLoaded はタスクを停止したり不正な参照を直したりしません。
[所有と借用](LIFETIME_REVIEW.md)、[DLL の回収条件](UNLOAD_SAFETY.md) も参照してください。
