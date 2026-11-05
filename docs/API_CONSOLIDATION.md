# API consolidation

The pre-release API now uses `reload_handler`, `HostResources`, `RequestRouter`
and `RequestTask` for both native reload and static linking. Compatibility aliases
and samples have been removed while there are no downstream users.

| Previous API | Replacement |
| --- | --- |
| FlowRunner, AppState, Execution, direct/snapshot/init/main macros | Message handlers, host resources and owned RequestTasks |
| Cartridge snapshots and serde feature | Versioned Checkpoint/InitPayload and application codecs |
| Attached runtimes and runtime features | Async host services, ServiceCall and ThreadService |
| Raw init context and allocator metadata | HandlerEnvironment and owned initialization data |
| AttachedFuture / AttachedFutureVTable / AttachedFutureTask | TaskHandle / TaskVTable / FfiFuture |
| RunnerError | LoadError with native enabled; RouteError for routing |
| UnloadPolicy / WhenIdle / explicit collection | Process-lifetime image retention and exited-session file cleanup |
| Hidden internal build/watch helpers | Opt-in development module |

Handler ABI 5, Services ABI 2 and task-control ABI 2 retain their binary layouts.
Rust APIs have breaking changes; rebuild host and logic against the same SDK.

`flow_reload` defaults to `native` and `dev-tools`. Use `default-features = false`
for logic crates and static hosts, or enable only `native` when builds/watching
are owned by another tool. `dev-tools` enables `native` plus Cargo/watch helpers.

```sh
cargo run -p service_host --release --no-default-features --features static-logic
```

The generated `handler_api()` returns the table for `unsafe RequestRouter::from_api`.
The static dependency graph excludes the native loader, file watcher and cache OS
dependencies. Features choose the mode independently of Debug/Release.

Plain Cargo build/test selects the three core crates. GUI, game and DB examples
remain explicit workspace targets. Cargo.lock is committed; CI uses `--locked`.
The declared core MSRV is 1.95 (Bevy: 1.97.1); CI checks it separately.
See [testing](TESTING.md) and [remaining verification (日本語)](../docs_ja/REMAINING_WORK.md).

Local Windows checks passed core/static/native-only lints, non-GUI workspace lints,
workspace and core Release tests excluding three TCP cases and one junction case,
static Release integration, native contract/SQLite/Bevy integrations, rustdoc,
frontend syntax, and all nine Miri groups in both borrow models (seed 0).
The excluded tests encountered sandbox access restrictions. Tauri compilation is
blocked by DNS failure fetching uncached futures-sink 0.3.34. Fresh cross-platform
CI, minimum-version checks and interactive GUI verification remain outstanding.
