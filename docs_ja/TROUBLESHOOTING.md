# トラブルシューティング

よくある障害と復旧手順です。

## `ABI version mismatch: host X, logic Y`

原因:

- Host と Logic が異なる `ABI_VERSION` でビルドされている

対処:

- Host/Logic を同じリビジョンで再ビルドする
- 稼働中 Host に対して互換 artifact を配備する

## `AppState size mismatch`

原因:

- 片側だけ `AppState` レイアウトが変わった

対処:

- Host/Logic を両方再ビルドする
- `AppState` 変更は計画的に同時ロールアウトする

## `Allocator mismatch`（Windows）

原因:

- Logic 側が期待 allocator tag を公開していない

対処:

- Logic クレートに `flow_reload::allocator!()` を追加
- Logic を再ビルドしてリロード

## `hot_init failed` / `hot_init error: ...`

原因:

- context ポインタが不正
- init 内で panic
- init が `Done` 以外を返した

対処:

- context の型・寿命を確認
- init を決定的で panic しない実装にする
- init の status ログを確認

## `attached async entry not available`

原因:

- Logic エントリポイントが attached 非対応
- attached 用 export シンボル不足

対処:

- async な `#[reload_direct]` ロジックにする
- Host 側 `flow_reload` の runtime feature 設定を確認

## `failed to attach runtime`

原因:

- Logic モジュール側で runtime vtable アタッチに失敗

対処:

- Host の runtime feature（`tokio` / `async-std` / `smol`）と呼び出し経路を確認
- Logic 側も互換 `flow_reload` で再ビルドする

## ファイル編集してもリロードされない

原因:

- 監視対象外ファイルを編集している
- デバウンス待ち時間中
- Host が見ている logic crate と別の場所を編集している

対処:

- logic manifest 解決結果を確認
- 必要なら `FLOW_RELOAD_LOGIC_MANIFEST_DIR` を設定
- デバウンス経過後に Host ログを確認

## `io error: Access is denied (os error 5)`（`target/*`）

原因:

- ウイルス対策/インデクサ/別プロセスによるファイルロック
- 作業ディレクトリ権限不足

対処:

- 書き込み可能なワークスペースで実行
- ビルドディレクトリを過度なスキャン対象から除外
- ロックしているプロセスを停止して再試行

## リロード時の `library load error`

原因:

- 動的ライブラリコピーが不完全/破損
- 旧 artifact パスがロックされている

対処:

- Logic を完全に再ビルド
- コピー先ディレクトリ書き込み権限を確認
- 少し待って `reload_if_needed()` を再試行

