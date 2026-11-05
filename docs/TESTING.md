# Testing

Run checks from the workspace root with Cargo and rustc on PATH. Native tests
compile standalone DLL fixtures; they are independent of the proc macros.

```sh
cargo fmt --all -- --check
rustfmt --check flow_reload/tests/fixtures/handler.rs flow_reload/tests/fixtures/abi_v5.rs
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets --locked -- -D warnings
cargo clippy -p interface -p hot_macro -p flow_reload --all-targets --all-features --locked -- -D warnings
cargo test -p interface -p hot_macro -p flow_reload --locked
cargo test --workspace --exclude tauri_tokio --all-features --locked
cargo test -p interface -p hot_macro -p flow_reload --release --all-features --locked
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
| `threaded_io_host` / `native_threaded_io` (ignored) | Two actual SDK DLL generations, two logic owners, host-owned TCP I/O and Drain |
| `service_contracts` | Required service bindings, typed init validation/panic, revocation checks and generation environments |
| `contracts_host` / `native_contracts` (ignored) | Actual SDK DLLs with multiple services and typed init rejection before request execution |
| `native_routing` | Actual DLLs, persistent connection, checkpoint formats, external callback return/join, failed unsubscribe and old ABI rejection |
| Bevy `native_game` (ignored) | Real DLLs in Bevy frames, delayed worker Restart, stale generation/entity/revision results and owner thread |

The standalone native ABI fixture uses its bundled, frozen `abi_v5.rs` table
snapshot. It and the generated SDK handlers exercise independent code paths.
Repository-dependent native demos live in separate example packages.

## Package and documentation boundaries

```powershell
python scripts/check-docs.py
./scripts/check-packages.ps1
```

Use `-Offline` for the package check when dependencies are cached. The script
packages the three SDK crates, extracts their archives into an isolated workspace,
checks all targets with and without native features, builds a standalone ABI DLL
and runs a static consumer. Patches resolve the unpublished SDK dependencies to
the extracted archives, never to the source checkout. README and license texts
must be present in each archive. Results and archive hashes remain under
`target/package-checks/`. This does not publish packages or reserve crate names.

The documentation check validates repository-relative files and headings and
rejects public links to ignored local context files. CI runs both checks on
Windows and Linux.

## Tauri backend

```sh
cargo clippy -p tauri_tokio -p tauri_tokio_logic --all-targets --locked -- -D warnings
cargo test -p tauri_tokio --bin tauri_tokio --locked -- --ignored --test-threads=1
node --check examples/tauri_tokio/dist/main.js
```

The ignored integration test builds native logic in an isolated temporary project,
checks Drain/Restart, counter continuity, build-error recovery and DLL copying
independent of CWD. It does not edit the live sample or open a window. Use one test
thread because it temporarily changes the test process's CWD. Interactive checks
follow the [GUI guide](TAURI_GUI_LAB.md).

The Linux GUI job also launches the real WebView under Xvfb with
`tauri-driver 2.1.0`. The standard-library Python smoke test clicks editor
presets and save, compares Drain/Restart, sends a request, injects a build error
and repairs it while checking host/counter continuity. It restores the sample
source and uploads a screenshot and diagnostics. This tests UI behavior;
manual visual review and Windows interactive checks remain separate.

## Bevy game integration

```sh
cargo clippy -p bevy_game_host -p bevy_game_logic -p bevy_game_protocol --all-targets --all-features --locked -- -D warnings
cargo test -p bevy_game_host --test native_game --locked -- --ignored --test-threads=1
```

Requires Rust 1.97.1+. Builds two workspace DLL variants in a separate target directory.
No window or GPU is required. See the [Japanese design and verification guide](../docs_ja/BEVY_GAME.md).

## Host resource / SQLite migration

```sh
cargo clippy -p db_migration_host -p db_migration_logic --all-targets --all-features --locked -- -D warnings
cargo test -p db_migration_host --test database --locked
cargo test -p db_migration_host --test native_migration --locked -- --ignored --test-threads=1
```

The example alone depends on rusqlite with bundled SQLite and needs a C compiler.
It creates new files under `target/`; it does not use an existing application DB.
See [migration policy and limits](../docs_ja/HOST_RESOURCE_MIGRATION.md).

## Retention and cache cleanup

```sh
cargo test -p flow_reload --test cache_lifecycle --locked -- --test-threads=1
```

Runs standalone host children and the actual CLI, using isolated roots below
`target/tmp/cache-lifecycle-tests/`; it never sweeps the user's default cache.
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

Miri does not verify optimized machine code or replace native DLL/network tests.

## Scope

CI configures Windows/Linux core and native integrations, core MSRV checks,
both Miri borrow models and the Windows/Linux GUI backend. Match each result to the
source revision being evaluated; see the [workflow](../.github/workflows/ci.yml).
Local Windows results do not establish Linux or interactive GUI behavior.
See [compatibility and scope](COMPATIBILITY.md) for platform coverage and
application responsibilities.

## Static and native-only configurations

```sh
cargo clippy -p flow_reload --all-targets --no-default-features --locked -- -D warnings
cargo test -p flow_reload --no-default-features --locked
cargo check -p flow_reload --no-default-features --features native --locked
cargo test -p service_host --release --no-default-features --features static-logic --locked
cargo tree -p service_host --no-default-features --features static-logic --edges normal --locked
```

The static dependency tree must exclude libloading, notify and runtime adapters.
The default members are the core crates; examples are selected with `-p` or `--workspace`.
