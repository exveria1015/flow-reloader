# flow-reloader

Hot-reload trusted Rust logic while keeping the host process, connections and UI alive.
Built for **local development** of servers, games and desktop applications.

The host owns resources and admission; message handlers run in generation-bound
tasks. Choose **Drain** to finish existing work on old code, or **Restart** to stop
registered workers cooperatively and recreate them from versioned checkpoints.

[English docs](docs/README.md) · [日本語ドキュメント](docs_ja/README.md) ·
[Examples](examples/README.md) · [Changelog](CHANGELOG.md)

**Pre-release:** the Rust API may change. Host and logic must use the same SDK.
See [compatibility and scope](docs/COMPATIBILITY.md) for compiler requirements,
platform coverage and application responsibilities.

## Quick start

From the workspace root with Rust 1.95+ and Cargo installed:

```sh
cargo run -p service_host --locked
```

Connect a TCP client to `127.0.0.1:4000` and send newline-terminated text. Edit
`examples/service_logic/src/lib.rs`, save, then send another message on the same
connection. The host keeps the connection; new requests use the rebuilt logic.
See [getting started](docs/GETTING_STARTED.md) / [導入手順](docs_ja/GETTING_STARTED.md)
for the host/logic split.

## Examples

Run one example at a time. Commands use the workspace's committed lockfile.

| Example | Command | Demonstrates |
| --- | --- | --- |
| TCP service | `cargo run -p service_host --locked` | Host-owned connections and Drain at message boundaries |
| Restart | `cargo run -p flow_reload --example restart --locked` | Cooperative shutdown, checkpoint migration and load budgets |
| Managed children | `cargo run -p flow_reload --example managed_children --locked` | Registered child cleanup before cutover |
| Threaded I/O | `cargo run -p threaded_io_host --locked` | Two DLL generations, two logic owner threads and bounded host TCP I/O |
| Service contracts | `cargo run -p contracts_host --locked` | Required services and typed initial configuration |
| SQLite migration | `cargo run -p db_migration_host --locked` | Resource migration, rollback, retry and durable reopen; needs a C compiler |
| Bevy game | `cargo run -p bevy_game_host --locked -- --verify` | Headless frame cutover, worker Restart and stale result rejection; Rust 1.97.1+ |
| Tauri GUI | `cargo run -p tauri_tokio --locked` | In-window Rust editor, Drain/Restart and build diagnostics; needs platform Tauri prerequisites |

The [example guide](examples/README.md) explains prerequisites, source files and
interactive modes. The GUI uses handwritten HTML/CSS/JavaScript; npm and the Tauri
CLI are not required.

## Contracts and limits

- Native-loading APIs are `unsafe`: all loaded code, including initialization,
  finalization and callbacks, must follow the SDK contracts. Metadata validation
  does not isolate untrusted native code.
- Host resources use service IDs/versions and copied byte buffers, rather than
  sharing Rust objects across DLL ownership boundaries. Synchronous operations are short;
  `call_async` waits on owned host I/O and joins cancellation before request completion.
- Tasks and resource registries are bound to their owner thread. Hosts drive polling;
  `spawn_child` and `register_activity` join explicitly registered work before request
  completion. Independent spawn tasks and callbacks are not automatically discovered.
  Dropped requests with pending cleanup need owner-thread `poll_cancelled_requests`
  (also driven by worker polling and Restart). See [managed activities](docs_ja/MANAGED_ACTIVITIES.md).
- `restart_with_commit` stages all workers before a synchronous application commit
  hook. Applications own resource exclusion, SQL rollback and durable recovery;
  database commit and router installation are not one crash-atomic operation.
- The message path uses **Handler ABI 5**, Services ABI 2 and task-control ABI 2.
  Rebuild host and logic together after ABI changes.
- Native images default to **KeepLoaded**, including abandoned or invalid loaded
  candidates. Code and temporary copies can accumulate. This preserves code lifetime,
  not borrowed data lifetime or task termination.
  Constrained hosts can explicitly use `unsafe reclaim_native_image` after proving
  external execution, Waker return and TLS/dependency quiescence; see [DLL lifetime](docs_ja/UNLOAD_SAFETY.md).
- `RouterOptions` defaults to 32 cumulative load attempts per shared
  `GenerationBudget` and 1 MiB per worker checkpoint. Failed native loads consume
  budget too; completion does not refund it. Restart the development process when exhausted.
- DLL copies live under `flow-reloader/managed-v1/sessions/<SessionId>/copies/`
  in the user's temporary directory. `retention_snapshot()` reports SDK-held images;
  `cargo run -p flow_reload --bin flow-cache --locked -- inspect` reports copy files.
  `clean --apply` removes eligible exited sessions; legacy PID directories are listed only.
  See [operations and limits](docs_ja/RETENTION_AND_CLEANUP.md).

## Build modes

The public API is `reload_handler` + `HostResources` + `RequestRouter` in both modes.
The generated `handler_api()` table also supports statically linked logic:

```sh
cargo run -p service_host --release --no-default-features --features static-logic --locked
```

`flow_reload` defaults to `native` and `dev-tools`. Use `default-features = false`
in logic crates and static hosts to remove the DLL loader, watcher and cache OS
dependencies. Build mode is selected explicitly through features, independently
of Debug/Release. See [build configurations](docs/API_REFERENCE.md#build-configurations)
for the feature matrix and [changelog](CHANGELOG.md) for breaking API changes.

## Workspace

| Crate | Role |
| --- | --- |
| `flow_reload` | Host resources, owned tasks, routing and optional native loading |
| `hot_macro` | `reload_handler` for native and statically linked logic |
| `interface` | Versioned C ABI tables and opaque task/service handles |
| `examples` | Runnable native hosts and logic crates |

## Verify changes

Plain `cargo build` / `cargo test` select the three core crates. The workspace lockfile
is committed; use `--locked` for repeatable dependency resolution. GUI/game samples
remain explicit workspace targets.

```sh
cargo fmt --all -- --check
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets --all-features --locked -- -D warnings
cargo test -p flow_reload --no-default-features --locked
cargo test -p service_host --no-default-features --features static-logic --locked
cargo test --workspace --exclude tauri_tokio --all-features --locked
cargo test -p interface -p hot_macro -p flow_reload --release --all-features --locked
```

See [testing and scope](docs/TESTING.md) for native DLL fixtures, the GUI backend
test, Miri setup and CI coverage. Miri, native DLL tests and interactive GUI checks
cover different behavior; none establishes safety for arbitrary plugins.

See [contributing](CONTRIBUTING.md) for source layout, documentation conventions
and the checks to run before proposing a change.

## License

Licensed under either the [MIT License](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.

SPDX identifier: `MIT OR Apache-2.0`.
