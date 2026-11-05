# flow-reloader ドキュメント

ホストを動かしたまま Rust ロジックを差し替える開発用のホットリローダーです。
サーバー・常駐処理、ゲーム、デスクトップへの組み込みを想定しています。
入口は `HostResources` / `RequestRouter` / `reload_handler` を入口にしてください。

既存処理を旧コードで続ける **Drain** と、登録ワーカーを協調停止して
チェックポイントから再生成する **Restart** を選べます。
対応環境とアプリケーションの責任範囲は [互換性と対象範囲](COMPATIBILITY.md) にまとめています。

## 使い始める

- [導入手順](GETTING_STARTED.md): ホストとロジックの分離、最初の実行。
- [サンプル一覧](../examples/README.md): 接続維持、Restart、GUI と静的リンクの例。
- [Tauri GUI Lab](TAURI_GUI_LAB.md): 画面内で Rust を編集し、切り替えと失敗復旧を試す。
- [Bevy ゲームへの最小適用](BEVY_GAME.md): 実 DLL、フレーム境界、旧結果の破棄、複数フレームの Restart。
- [API リファレンス](API_REFERENCE.md): 現行 API、ABI、ビルド構成。
- [互換性と対象範囲](COMPATIBILITY.md): 必要環境、対応範囲、アプリケーションの責任。

## 設計と運用

- [設計と要件](ARCHITECTURE.md): 所有、切り替え境界、対象範囲と未提供の性質。
- [ホスト資源と受付切り替え](HOST_RESOURCES_AND_ROUTING.md): サービス契約と Drain。
- [Restart と状態移行](RESTART_AND_MIGRATION.md): 停止待ち、保存、移行、失敗時の挙動、予算。
- [ホスト資源・DB の移行](HOST_RESOURCE_MIGRATION.md): 最終確定フック、SQLite とキャッシュの排他、失敗の遮断と復旧。
- [子タスク・外部コールバック](MANAGED_ACTIVITIES.md): 明示登録、停止・join、Drop 後の終了待ちと失敗。
- [非同期サービス・複数スレッド](ASYNC_SERVICES_AND_THREADS.md): 実 I/O、待機・キャンセル、有界 FIFO、所有と通知。
- [複数サービス・型付き初期化](SERVICE_CONTRACTS_AND_INIT.md): 必要サービスの照合、世代ごとの設定、形式不一致の拒否。
- [開発時の運用](DEVELOPMENT.md): ビルド、復旧、DLL コピーの扱い。
- [保持量の観測と掃除](RETENTION_AND_CLEANUP.md): DLL 参照の観測、セッション管理、終了後のコピー掃除。
- [トラブルシューティング](TROUBLESHOOTING.md): エラー別の確認点。

## 寿命と検証

- [所有・借用・破棄順序](LIFETIME_REVIEW.md): 実行と状態・世代の関係。
- [DLL と管理外タスク](UNLOAD_SAFETY.md): コード保持と登録処理・管理外処理の契約。
- [テストと Miri](TESTING.md): 再実行手順、テストの役割、確認できない範囲。
- [変更履歴](../CHANGELOG.md): 利用者向けの変更点と破壊的変更。
- [開発への参加](../CONTRIBUTING.md): ソース配置、確認方法、文書の更新方針（英語）。

[English documentation](../docs/README.md)

## ライセンス

[MIT License](../LICENSE-MIT) または [Apache License 2.0](../LICENSE-APACHE) のいずれかを選択できます。
SPDX 表記は `MIT OR Apache-2.0` です。
