# Testing

Run checks from the workspace root with Cargo and rustc on PATH. Native tests
compile standalone DLL fixtures; they are independent of the proc macros.

```sh
cargo fmt --all -- --check
rustfmt --check flow_reload/tests/fixtures/logic.rs
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
| `restart` | Shutdown, old generations, staged migration, interruption, retry and budgets |
| `native_routing` | Actual DLLs, one persistent TCP connection and checkpoint format changes |
| `async_direct`, `async_snapshot`, `execution` | Generated code, borrowing and completion/panic/cancellation drop order |
| `runner::tests` | Compatibility native lifecycle, unmanaged work and explicit fixture unloading |
| `main_macro` | Compile all nine public forms in both profiles; zero runtime tests is intentional |
| `release_main` | Execute the actual Release macro loop and check state access after Future destruction |

Keep compile fixtures, executed macro regressions and native ABI fixtures:
they detect different failures. Runtime sample similarity also does not make
Tokio, async-std and smol adapters equivalent coverage.

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

## Miri

In PowerShell:

```powershell
rustup toolchain install nightly-2026-11-02 --profile minimal --component miri --component rust-src
cargo +nightly-2026-11-02 miri setup --target x86_64-unknown-linux-gnu
./scripts/check-miri.ps1 -Toolchain nightly-2026-11-02
```

The script runs both Stacked and Tree Borrows with seed 0, without disabling
borrow checks. Use `-Seed 1` for another seed or `-BorrowModel Tree` / `Stacked`
for one model. It covers attached execution, direct/snapshot macros, owned
execution, host resources, routing, Restart and the Release main branch.
It writes source hashes, environment details, logs and exit codes under
`target/miri-audit/seed-<seed>/`, and fails if any group fails.

The Release flag selects the macro branch; Miri does not verify optimized machine
code. It does not replace native DLL/network tests. Historical findings are in
the [audit record](../docs_ja/archive/MIRI_BASELINE.md).

## Scope

Local records cover Windows native DLLs, both Miri models, the Tauri backend and
interactive Windows GUI behavior. [CI](../.github/workflows/ci.yml) is configured
for Windows/Linux core tests, Linux Miri and Windows GUI backend checks. Remote
CI success and Linux native/GUI execution are not established by those local
records. These checks do not prove safety for arbitrary plugins or automatic
quiescence of unmanaged tasks and callbacks.
