# Testing

Run checks from the workspace root with Cargo and rustc on PATH. Native tests
compile standalone DLL fixtures; they are independent of the proc macros.

```sh
cargo fmt --all -- --check
rustfmt --check flow_reload/tests/fixtures/handler.rs
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets -- -D warnings
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets --all-features -- -D warnings
cargo test -p interface -p hot_macro -p flow_reload
cargo test --workspace --exclude tauri_tokio --all-features
cargo test -p interface -p hot_macro -p flow_reload --release --all-features
```

## Test roles

| Target | Purpose |
| --- | --- |
| `host_resources`, `request_routing` | Ownership, revocation, contract checks, input copying and Drain |
| `cache_lifecycle` | Separate native host/cleaner processes, image outcomes, leases after router drop, normal/forced exit, noninheritance, namespace contention, trash retry, corrupt/legacy/link exclusion and CLI defaults |
| `restart` | Shutdown, old generations, staged migration, interruption, retry and budgets |
| `host_commit` | Final host commit ordering/one-shot behavior, earlier failures, interruption, retry and panic |
| DB host `database` | Real SQLite schema/data rollback, BEGIN/COMMIT contention, host cache exclusion, recovery fencing and overflow |
| DB host `native_migration` (ignored) | Three real DLL variants, compatible Drain, breaking Restart, rollback/retry, stale adapters and durable reopen |
| `managed_activities` | Registered child/callback cleanup, delayed join, failures, cancellation, bounds and panic |
| `async_services` | Owned host I/O, bounds, cancellation, revoke, operation IDs and retained short-buffer results |
| `threaded_services` | FIFO, capacity, notifications, shutdown/join, separate logic owners, real TCP and cancellation |
| `native_threaded_io` (ignored) | Two actual SDK DLL generations, two logic owners, host-owned TCP I/O and Drain |
| `service_contracts` | Required service bindings, typed init validation/panic, revocation checks and generation environments |
| `native_contracts` (ignored) | Actual SDK DLLs with multiple services and typed init rejection before request execution |
| `native_routing` | Actual DLLs, persistent connection, checkpoint formats, external callback return/join, failed unsubscribe and old ABI rejection |
| Bevy `native_game` (ignored) | Real DLLs in Bevy frames, delayed worker Restart, stale generation/entity/revision results and owner thread |

The standalone native ABI fixture and generated SDK handlers exercise independent code paths.

## Tauri backend

```sh
cargo clippy -p tauri_tokio -p tauri_tokio_logic --all-targets -- -D warnings
cargo test -p tauri_tokio --bin tauri_tokio -- --ignored --test-threads=1
node --check examples/tauri_tokio/dist/main.js
```

The ignored integration test builds native logic in an isolated temporary project,
checks Drain/Restart, counter continuity, build-error recovery and DLL copying
independent of CWD. It does not edit the live sample or open a window. Use one test
thread because it temporarily changes the test process's CWD. Interactive checks
follow the [GUI guide](TAURI_GUI_LAB.md).

## Bevy game integration

```sh
cargo clippy -p bevy_game_host -p bevy_game_logic -p bevy_game_protocol --all-targets --all-features -- -D warnings
cargo test -p bevy_game_host --test native_game -- --ignored --test-threads=1
```

Requires Rust 1.97.1+. Builds two workspace DLL variants in a separate target directory.
No window or GPU is required. See the [Japanese design and verification guide](../docs_ja/BEVY_GAME.md).

## Host resource / SQLite migration

```sh
cargo clippy -p db_migration_host -p db_migration_logic --all-targets --all-features -- -D warnings
cargo test -p db_migration_host --test database
cargo test -p db_migration_host --test native_migration -- --ignored --test-threads=1
```

The example alone depends on rusqlite with bundled SQLite and needs a C compiler.
It creates new files under `target/`; it does not use an existing application DB.
See [migration policy and limits](../docs_ja/HOST_RESOURCE_MIGRATION.md).

## Retention and cache cleanup

```sh
cargo test -p flow_reload --test cache_lifecycle -- --test-threads=1
```

Runs standalone host children and the actual CLI, using isolated roots below
`target/cache-lifecycle-tests/`; it never sweeps the user's default cache.
The two ignored tests are subprocess entrypoints, invoked by the parent tests.
Windows tests real deletion denial and junctions; Unix tests symlinks and refusal
of a group/world-writable root. Both run the common lifecycle cases in CI.
These file/OS checks do not run in Miri.

## Miri

In PowerShell:

```powershell
rustup toolchain install nightly-2026-11-02 --profile minimal --component miri --component rust-src
cargo +nightly-2026-11-02 miri setup --target x86_64-unknown-linux-gnu
./scripts/check-miri.ps1 -Toolchain nightly-2026-11-02
```

The script runs both Stacked and Tree Borrows with seed 0, without disabling
borrow checks. Use `-Seed 1` for another seed or `-BorrowModel Tree` / `Stacked`
for one model. It covers task/Waker ownership, host resources, routing, Restart, host commit, managed activities, async/threaded services and service contracts.
The threaded service TCP tests are excluded under Miri; their native execution is separate.
SQLite and its C code are tested natively; Miri covers the generic commit lifecycle only.
It writes source hashes, environment details, logs and exit codes under
`target/miri-audit/seed-<seed>/`, and fails if any group fails.

Miri does not verify optimized machine code or replace native DLL/network tests. Historical findings are in
the [audit record](../docs_ja/archive/MIRI_BASELINE.md).

## Scope

This refactor requires fresh Windows/Linux CI and both Miri models; earlier
results are archived in [the initial milestone](../docs_ja/archive/INITIAL_MILESTONE.md).
Local Windows results do not establish Linux or interactive GUI behavior.

## Static and native-only configurations

```sh
cargo clippy -p flow_reload --all-targets --no-default-features --locked -- -D warnings
cargo test -p flow_reload --no-default-features --locked
cargo check -p flow_reload --no-default-features --features native --locked
cargo test -p service_host --release --no-default-features --features static-logic --locked
cargo tree -p service_host --no-default-features --features static-logic --edges normal
```

The static dependency tree must exclude libloading, notify and runtime adapters.
The default members are the core crates; examples are selected with `-p` or `--workspace`.
