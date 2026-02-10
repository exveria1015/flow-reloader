# flow-reloader

Hot-reload framework for Rust logic crates, with optional attached async runtime support and snapshot state persistence.

This workspace contains:

- `flow_reload`: host/runtime crate and public API
- `hot_macro`: proc macros (`#[reload_direct]`, `#[reload_snapshot]`, `#[reload_init]`)
- `interface`: ABI-safe shared types (`AppState`, metadata, cartridge API)
- `examples/*`: runnable host + logic samples

## Why This Project

`flow-reloader` is designed for workflows where the host process stays alive while logic is rebuilt and reloaded as a dynamic library.

Core goals:

- Keep host process state stable while swapping logic
- Detect ABI/state incompatibilities at runtime
- Support both sync and async logic entrypoints
- Allow optional state snapshots between reloads

## Features

- Dynamic library reload with file watcher integration
- Runtime metadata validation (`ABI_VERSION`, `AppState` size, allocator tag on Windows)
- Attached async bridge for `tokio`, `async-std`, and `smol` hosts
- Snapshot cartridge (`AppState::cartridge`) with pluggable serialization
- Host macro utilities (`flow_reload::main!`, `flow_reload::allocator!`)

## Quick Start

Run a sample host:

```bash
cargo run -p sync_host
```

Then edit logic code:

- `examples/sync_logic/src/lib.rs`

The host rebuilds and reloads logic automatically in debug builds.

More samples:

- `cargo run -p async_tokio_host`
- `cargo run -p async_std_host`
- `cargo run -p async_smol_host`
- `cargo run -p snapshot_host`
- `cargo run -p tauri_tokio`

## Documentation

- `docs/README.md`: documentation index
- `docs/GETTING_STARTED.md`: integrate into your own host/logic crates
- `docs/API_REFERENCE.md`: macros, runtime APIs, and feature flags
- `docs/PRODUCTION_GUIDE.md`: deployment and production checklist
- `docs/TROUBLESHOOTING.md`: common failures and recovery paths
- `docs_ja/README.md`: Japanese documentation index

## Example Layout

```text
examples/
  sync_logic/        # logic crate (cdylib + rlib)
  sync_host/         # host process loading the logic library
  async_tokio_logic/ # async logic using attached runtime helpers
  async_tokio_host/  # tokio host driving attached futures
  snapshot_logic/    # state snapshot logic with serde cartridge
  snapshot_host/     # host decoding cartridge for visibility
```

## Notes

- In debug mode, host helpers auto-build and watch source changes.
- In release mode, logic entrypoints are called directly and no auto-build/watch loop is used.
- On Windows, use `flow_reload::allocator!()` in logic crates to match host allocator expectations.
