# flow-reloader

Hot-reload trusted Rust logic while keeping the host process, connections and UI alive.
The current target is reliable **development-time** updates for servers, games and desktop applications.

The recommended API is message-oriented: the host owns resources and admission;
logic runs in generation-bound tasks. Choose **Drain** to finish existing work on old
code, or **Restart** to stop registered workers cooperatively and recreate them from
versioned checkpoints. Arbitrary Future stacks and external I/O effects are not migrated.

## Try it

Run commands from the workspace root with Rust/Cargo installed:

```sh
cargo run -p service_host
cargo run -p flow_reload --example restart
cargo run -p tauri_tokio
```

Run one example at a time:

- `service_host`: send newline-terminated text to `127.0.0.1:4000`, edit
  `examples/service_logic/src/lib.rs`, then send another message on the same connection.
  Transport stays in the host; new requests use the new generation.
- `restart`: a small static example of cooperative shutdown, checkpoint migration and load budgets.
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
  sharing Rust objects across DLL ownership boundaries. Operations are short and synchronous.
- Tasks and resource registries are bound to their owner thread. Hosts drive polling;
  independent spawn tasks and callbacks are not automatically tracked or joined.
- The message path uses **Handler ABI 2**, Services ABI 1 and task-control ABI 1.
  Rebuild host and logic together after ABI changes.
- Native images default to **KeepLoaded**, including abandoned or invalid loaded
  candidates. Code and temporary copies can accumulate. This preserves code lifetime,
  not borrowed data lifetime or task termination.
- `RouterOptions` defaults to 32 cumulative load attempts per shared
  `GenerationBudget` and 1 MiB per worker checkpoint. Failed native loads consume
  budget too; completion does not refund it. Restart the development process when exhausted.
- DLL copies live under the user's temporary directory at `flow-reloader/<PID>/`.
  They may remain after exit; remove them only after the corresponding process has ended.

The older `FlowRunner` / `AppState` / attached-runtime API remains available with
**ABI v4**. Its sync, snapshot and Tokio/async-std/smol examples are compatibility
examples, not the starting point for the message API. Windows allocator-tag checks
and `flow_reload::allocator!()` belong to that older path. See the
[API reference](docs/API_REFERENCE.md) and [DLL lifetime contracts](docs_ja/UNLOAD_SAFETY.md).

## Workspace

| Crate | Role |
| --- | --- |
| `flow_reload` | Host resources, task ownership, routing and compatibility adapters |
| `hot_macro` | `reload_handler` and the older direct/snapshot/init proc macros |
| `interface` | Versioned C ABI tables, opaque handles and compatibility state |
| `examples` | Runnable native hosts and logic crates |

## Verify changes

```sh
cargo fmt --all -- --check
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets --all-features -- -D warnings
cargo test --workspace --exclude tauri_tokio --all-features
cargo test -p interface -p hot_macro -p flow_reload --release --all-features
```

See [testing and scope](docs/TESTING.md) for native DLL fixtures, the GUI backend
test, Miri setup and CI coverage. Miri, native DLL tests and interactive GUI checks
cover different behavior; none establishes safety for arbitrary plugins.
