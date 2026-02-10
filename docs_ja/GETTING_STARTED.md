# Getting Started（導入手順）

このガイドでは、`flow-reloader` を自分のプロジェクトへ組み込む手順を説明します。

## 1. クレートを 2 つ用意する

基本構成は次です。

- Host クレート: 常駐プロセス（`bin`）
- Logic クレート: ホットスワップされる動的ライブラリ（`cdylib`）

Logic 側 `Cargo.toml`:

```toml
[package]
name = "my_logic"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
flow_reload = { path = "../flow_reload" }
```

Host 側 `Cargo.toml`:

```toml
[dependencies]
flow_reload = { path = "../flow_reload" }
my_logic = { path = "../my_logic" }
```

## 2. Logic エントリポイントを書く

Logic クレートの `src/lib.rs`:

```rust
use flow_reload::{allocator, reload_direct, reload_init, AppState};

allocator!();

static mut HOST_CONTEXT: u64 = 0;

#[reload_init]
pub fn init(ctx: &mut u64) {
    unsafe { HOST_CONTEXT = *ctx; }
}

#[reload_direct]
pub fn update(state: &mut AppState) {
    let ctx = unsafe { HOST_CONTEXT };
    state.counter += 1;
    state.set_message(&format!("counter={} ctx={}", state.counter, ctx));
}
```

## 3. Host からビルドして実行する

Host クレートの `src/main.rs`:

```rust
use flow_reload::{FlowRunner, RunStatus};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let logic_path = logic_library_path()?;
    let mut state = flow_reload::AppState::default();
    let mut context = 123_u64;
    let mut runner = FlowRunner::with_context(&logic_path, &mut context)?;

    loop {
        let status = runner.run(&mut state)?;
        if status == RunStatus::Error {
            eprintln!("logic returned error");
        }

        if runner.reload_if_needed()? {
            println!("reloaded");
        }

        std::thread::sleep(Duration::from_millis(16));
    }
}

fn logic_library_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    if let Ok(path) = std::env::var("LOGIC_LIB_PATH") {
        return Ok(path.into());
    }

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let logic_manifest_dir = manifest_dir.join("..").join("my_logic");
    let target_dir = flow_reload::internal::default_target_dir(&logic_manifest_dir);
    let status = flow_reload::internal::build_logic("my_logic", &logic_manifest_dir, &target_dir)?;
    if !status.success() {
        return Err("logic build failed".into());
    }
    Ok(flow_reload::internal::library_path_in(&target_dir, "my_logic"))
}
```

## 4. 必要なら `flow_reload::main!` を使う

Host ループがフレームワークの想定に合う場合はマクロ化できます。

```rust
flow_reload::main!(my_logic);
```

Context + カスタム runtime 付き:

```rust
flow_reload::main!(my_logic, flow_reload::AppState, 123_u64, runtime: my_runtime);
```

Debug 時の挙動:

- Logic クレートを自動ビルド
- `src/`, `Cargo.toml`, `build.rs` を監視
- 変更後にデバウンスしてリロード

## 5. Attached Async Runtime（任意）

Host 側で feature を有効にし、対応メソッドを呼びます。

- `tokio`: `run_attached_with_tokio`
- `async-std`: `run_attached_with_async_std`
- `smol`: `run_attached_with_smol`

Logic 側で使える helper:

- `AttachedSleep`
- `AttachedTcpRequest`
- `spawn_detached_sleep`
- `spawn_detached_tcp_request`

## 環境変数

- `LOGIC_LIB_PATH`: 読み込む動的ライブラリを固定パス指定
- `FLOW_RELOAD_LOGIC_MANIFEST_DIR`: マクロベース Host ループの logic crate 位置を上書き

