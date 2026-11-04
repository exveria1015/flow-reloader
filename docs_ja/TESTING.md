# テストと Miri

## テストの役割

| 対象 | 確認するもの |
| --- | --- |
| `host_resources` | 所有、失効、資源 ID・版、借用競合、出力容量、panic 境界 |
| `request_routing` | 入力コピー、Drain、候補拒否、受付停止・再開、Router より長い実行寿命 |
| `restart` | 協調停止、旧 Drain 世代の終了待ち、移行の段階適用、失敗・中断・再試行、上限 |
| `native_routing` | 実 DLL と同じ TCP 接続、異なる DLL 間のチェックポイント形式変更 |
| `async_direct` / `async_snapshot` / `execution` | 生成コードと所有状態の借用、同時 Pending、完了・panic・キャンセル・破棄順序 |
| `runner::tests` | 旧 API の実 DLL 寿命、失敗候補、管理外タスク、限定した明示的アンロード |
| `main_macro` | 公開9形式のコンパイル確認。テスト件数0でも Debug／Release の展開を検査する |
| `release_main` | 実際の Release `main!` を実行し、Pending 中に状態を tick へ公開しないこと |
| Tauri `engine` の ignored テスト | 独立プロジェクトの実 DLL ビルド、Drain／Restart、値の継続、構文エラー復帰、作業ディレクトリから独立したコピー |

`fixtures/logic.rs` と `fixtures/handler.rs` は proc macro に依存しない独立した ABI 実装です。
マクロ生成側とローダー側が同じ誤りを共有しても検出できるように残しています。
Tokio／async-std／smol のサンプルも、それぞれ別のホストアダプターを使います。
似たコードであることだけを理由に、これらの検証対象を削除しません。

## 通常の確認

リポジトリのルートで実行します。実 DLL テストが `rustc` を起動するため、Cargo と rustc を PATH に入れてください。

```powershell
cargo fmt --all -- --check
rustfmt --check flow_reload/tests/fixtures/logic.rs
rustfmt --check flow_reload/tests/fixtures/handler.rs
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets -- -D warnings
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets --all-features -- -D warnings
cargo test -p interface -p hot_macro -p flow_reload
cargo test --workspace --exclude tauri_tokio --all-features
cargo test -p interface -p hot_macro -p flow_reload --release --all-features
```

GUI のバックエンドとフロントエンド構文は別に確認します。テストは通常の GUI ソースを編集せず、ウィンドウも開きません。

```powershell
cargo clippy -p tauri_tokio -p tauri_tokio_logic --all-targets -- -D warnings
cargo test -p tauri_tokio --bin tauri_tokio -- --ignored --test-threads=1
node --check examples/tauri_tokio/dist/main.js
```

ignored 統合テストは一時的にテストプロセスの作業ディレクトリを変えるため、単一テストスレッドで実行します。
実際の GUI の見た目・操作は [GUI Lab](TAURI_GUI_LAB.md) の手順で確認してください。

## Miri

固定ツールチェーンと Linux インタープリターターゲットを使います。通常ビルドの既定ツールチェーンは変更しません。

```powershell
rustup toolchain install nightly-2026-11-02 --profile minimal --component miri --component rust-src
cargo +nightly-2026-11-02 miri setup --target x86_64-unknown-linux-gnu
./scripts/check-miri.ps1 -Toolchain nightly-2026-11-02
```

スクリプトは Stacked／Tree Borrows の両方を seed 0 で実行します。
`-Seed 1` で別 seed、`-BorrowModel Stacked` または `Tree` で単独実行できます。
対象は attached、direct、snapshot、execution、resources、routing、restart、Release main です。
Release 指定はマクロの分岐を選ぶためで、最適化した機械語の検証ではありません。

ログ、コンパイラ情報、対象ソースと Cargo.lock の SHA-256、終了コードを `target/miri-audit/seed-<seed>/` に保存します。
失敗しても残りのグループを実行し、失敗があれば非ゼロで終了します。
DLL・ネットワークを使うテストを Miri が代替するわけではありません。

## 検証範囲

Windows の実 DLL、両借用モデルの Miri、Tauri バックエンドと実ウィンドウにはローカルの確認記録があります。
[CI 設定](../.github/workflows/ci.yml) は Windows／Linux のコア、Linux インタープリター、Windows GUI バックエンドを対象にしています。
リモート CI の成功と Linux 実 DLL・Linux GUI の実行結果は、このローカル確認からは主張しません。
各コマンドの現在の結果は実際の出力で確認してください。古い時点のテスト件数を現在の完了条件にはしません。

Miri は選んだ経路、ネイティブテストは専用フィクスチャ、GUI 確認はサンプルの挙動を検証します。
任意の DLL・依存ライブラリの安全性、管理外タスクの終了、自動アンロード全般を保証するものではありません。
過去の検出証拠は [監査記録](archive/MIRI_BASELINE.md)、現在の所有契約は [寿命管理](LIFETIME_REVIEW.md) に分けています。
