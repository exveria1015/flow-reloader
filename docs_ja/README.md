# flow-reloader ドキュメント

ホストを動かしたまま Rust ロジックを差し替える開発用のホットリローダーです。
主な利用先はサーバー・常駐処理、ゲーム、デスクトップの順に想定しています。
新規実装は `HostResources` / `RequestRouter` / `reload_handler` を入口にしてください。

既存処理を旧コードで続ける **Drain** と、登録ワーカーを協調停止して
チェックポイントから再生成する **Restart** を選べます。
任意の Future の途中状態や外部 I/O の副作用を自動で移行するものではありません。

## 使い始める

- [導入手順](GETTING_STARTED.md): ホストとロジックの分離、最初の実行。
- [サンプル一覧](../examples/README.md): 接続維持、Restart、GUI と旧 API の例。
- [Tauri GUI Lab](TAURI_GUI_LAB.md): 画面内で Rust を編集し、切り替えと失敗復旧を試す。
- [API リファレンス](API_REFERENCE.md): 現行 API、ABI、旧 API の区別。

## 設計と運用

- [設計と要件](ARCHITECTURE.md): 所有、切り替え境界、対象範囲と未提供の性質。
- [ホスト資源と受付切り替え](HOST_RESOURCES_AND_ROUTING.md): サービス契約と Drain。
- [Restart と状態移行](RESTART_AND_MIGRATION.md): 停止待ち、保存、移行、失敗時の挙動、予算。
- [開発時の運用](DEVELOPMENT.md): ビルド、復旧、DLL コピーの扱い。
- [トラブルシューティング](TROUBLESHOOTING.md): エラー別の確認点。
- [残る課題](REMAINING_WORK.md): 現在の未実装・未検証事項と完了条件。

## 寿命と検証

- [所有・借用・破棄順序](LIFETIME_REVIEW.md): 実行と状態・世代の関係。
- [DLL と管理外タスク](UNLOAD_SAFETY.md): KeepLoaded、明示的回収、spawn の契約。
- [テストと Miri](TESTING.md): 再実行手順、テストの役割、確認できない範囲。
- [Miri 導入時の監査記録](archive/MIRI_BASELINE.md): 修正前の借用違反の証拠。現在の利用手順とは区別する。

[English documentation](../docs/README.md)

## ライセンス

[MIT License](../LICENSE-MIT) または [Apache License 2.0](../LICENSE-APACHE) のいずれかを選択できます。
SPDX 表記は `MIT OR Apache-2.0` です。
