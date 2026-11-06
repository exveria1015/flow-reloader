# Changelog

User-visible changes are recorded here. The project is pre-release; the entries
below describe unreleased work and do not announce an RC or stable release.

## Unreleased

### Breaking API changes

- Use `reload_handler`, `HostResources`, `RequestRouter` and `RequestTask` for
  both native reload and static linking. The earlier runner, direct/snapshot
  macros, attached runtimes and compatibility aliases have been removed.
- Use versioned `Checkpoint` / `InitPayload` values and application codecs for
  saved state and initial configuration.
- Low-level owned tasks use `TaskHandle`, `TaskVTable` and `FfiFuture`.
  Native loading errors use `LoadError`; routing errors use `RouteError`.
- Native images default to KeepLoaded. The old unload-policy API has been removed;
  explicit `unsafe reclaim_native_image` requires host proof of external/TLS/dependency
  quiescence. Retained images and exited-session files have separate cleanup APIs.
- Cargo/build/watch helpers are in the feature-gated `development` module.

### Capabilities

- Message-boundary Drain and cooperative worker Restart with versioned
  checkpoints, staged factories and a final synchronous host commit hook.
- Explicit child-task and callback registration, cancellation cleanup and
  join tracking before request completion and Restart.
- Async host services, bounded threaded I/O, required service bindings and
  typed per-generation initialization data.
- Native retention diagnostics, managed cache sessions and the `flow-cache`
  inspection / exited-session cleanup CLI.
- Explicit owner-thread DLL reclamation, host call-return guards, retention reasons,
  retired-generation release and separate close/copy-removal outcomes. Handler ABI 5
  and cumulative attempt-budget semantics are unchanged.
- Examples for TCP, static logic, threaded I/O, service contracts, SQLite
  resource migration, a headless Bevy game and a Tauri GUI.
- Registered `GenerationLocal<T>` values with owner-thread destruction and
  cleanup-failure retention, plus resident `HostNotification` handles for host I/O.
  Delayed host notifications detach after registered request cleanup and can
  outlive a reclaimed DLL. Individual dependency shutdown stays application-owned.
  An optional task-control tail adds generation-services ABI 1 without changing
  Handler ABI 5 or the task-control ABI 2 base layout.

### Build and compatibility

- Default features are `native` and `dev-tools`. Logic crates and static hosts
  can use `default-features = false`; `native` alone omits watch/build helpers.
- Plain Cargo build/test selects the three core crates. Examples remain
  explicit workspace targets. The workspace lockfile is committed.
- Native contracts and threaded I/O demos are separate `contracts_host` and
  `threaded_io_host` workspace packages. Shared contract types live in
  `contracts_protocol`; packaged core examples and ABI fixtures do not read
  sibling crate sources.
- SDK packages include README and license texts. Package verification extracts
  all three archives, checks every target with native/static features and runs
  an independent ABI DLL check plus a statically linked consumer.
- Declared minimum Rust versions are 1.95 for the core and 1.97.1 for the Bevy
  example. Handler ABI 5, Services ABI 2 and task-control ABI 2 are retained.
  Rebuild host and logic together against the same SDK.

See [compatibility and scope](docs/COMPATIBILITY.md) / [互換性と対象範囲](docs_ja/COMPATIBILITY.md)
and [testing](docs/TESTING.md) for contracts, checks and verification limits.
