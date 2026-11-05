# flow-reloader

Hot-reload trusted Rust logic while keeping the host process, connections and UI alive.
The current target is reliable **development-time** updates for servers, games and desktop applications.

The recommended API is message-oriented: the host owns resources and admission;
logic runs in generation-bound tasks. Choose **Drain** to finish existing work on old
code, or **Restart** to stop registered workers cooperatively and recreate them from
versioned checkpoints. Arbitrary Future stacks and external I/O effects are not migrated.

## Try it

Run commands from the workspace root with Rust 1.95+ (Bevy: 1.97.1+) and Cargo installed:

```sh
cargo run -p service_host
cargo run -p flow_reload --example restart
cargo run -p flow_reload --example threaded_io
cargo run -p flow_reload --example contracts
cargo run -p db_migration_host
cargo run -p bevy_game_host -- --verify
cargo run -p tauri_tokio
```

Run one example at a time:

- `service_host`: send newline-terminated text to `127.0.0.1:4000`, edit
  `examples/service_logic/src/lib.rs`, then send another message on the same connection.
  Transport stays in the host; new requests use the new generation.
- `restart`: a small static example of cooperative shutdown, checkpoint migration and load budgets.
- `threaded_io`: two logic owner threads, two actual DLL generations and TCP I/O
  on a bounded host worker; exits after verification. [Contracts](docs_ja/ASYNC_SERVICES_AND_THREADS.md).
- `contracts`: native DLLs declaring multiple required services and typed initial
  configuration; rejects missing contracts and malformed data before request execution.
  [Contracts and scope](docs_ja/SERVICE_CONTRACTS_AND_INIT.md).
- `db_migration_host`: real DLLs, compatible Drain and a breaking SQLite/host-cache
  migration after worker shutdown; verifies rollback, retry and durable reopen.
  Requires a C compiler for bundled SQLite. [Policy and recovery](docs_ja/HOST_RESOURCE_MIGRATION.md).
- `bevy_game_host --verify`: a headless Bevy 0.20.1 game with real DLLs, frame-boundary
  cutover, multi-frame worker Restart and stale job rejection (Rust 1.97.1+).
  Without arguments, edit its logic's `STEP` to build in the background and Restart.
- `tauri_tokio`: a Tauri GUI with an in-window Rust editor, Drain/Restart controls,
  counter and build diagnostics. Requires the platform's Tauri build prerequisites;
  no npm or Tauri CLI is needed. The package name is historical; its logic uses the message API.

See [all examples](examples/README.md), [English documentation](docs/README.md),
or [日本語ドキュメント](docs_ja/README.md).

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
- Native images use **KeepLoaded**, including abandoned or invalid loaded
  candidates. Code and temporary copies can accumulate. This preserves code lifetime,
  not borrowed data lifetime or task termination.
- `RouterOptions` defaults to 32 cumulative load attempts per shared
  `GenerationBudget` and 1 MiB per worker checkpoint. Failed native loads consume
  budget too; completion does not refund it. Restart the development process when exhausted.
- DLL copies live under `flow-reloader/managed-v1/sessions/<SessionId>/copies/`
  in the user's temporary directory. `retention_snapshot()` reports SDK-held images;
  `cargo run -p flow_reload --bin flow-cache -- inspect` reports copy files.
  `clean --apply` removes eligible exited sessions; legacy PID directories are listed only.
  See [operations and limits](docs_ja/RETENTION_AND_CLEANUP.md).

## Build modes

The public API is `reload_handler` + `HostResources` + `RequestRouter` in both modes.
The generated `handler_api()` table also supports statically linked logic:

```sh
cargo run -p service_host --release --no-default-features --features static-logic
```

`flow_reload` defaults to `native` and `dev-tools`. Use `default-features = false`
in logic crates and static hosts to remove the DLL loader, watcher and cache OS
dependencies. Build mode is selected explicitly through features, independently
of Debug/Release. [API consolidation](docs/API_CONSOLIDATION.md) records the
removed pre-release compatibility API and its replacements.

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
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets --all-features -- -D warnings
cargo test -p flow_reload --no-default-features --locked
cargo test -p service_host --no-default-features --features static-logic --locked
cargo test --workspace --exclude tauri_tokio --all-features --locked
cargo test -p interface -p hot_macro -p flow_reload --release --all-features
```

See [testing and scope](docs/TESTING.md) for native DLL fixtures, the GUI backend
test, Miri setup and CI coverage. Miri, native DLL tests and interactive GUI checks
cover different behavior; none establishes safety for arbitrary plugins.

## License

Licensed under either the [MIT License](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.

SPDX identifier: `MIT OR Apache-2.0`.
