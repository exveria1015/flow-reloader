# API reference

## Message API

The recommended API separates host resources, request admission and generation-bound
execution. It uses **Handler ABI 5**, Services ABI 2 and task-control ABI 2,
independently of the compatibility AppState ABI v4.

### Resources and logic

`retention_snapshot()` reports this SDK instance's native handles, copied file
lengths, lifecycle/outcome and bounded deletion diagnostics, without retaining new
library leases. These are not RAM measurements or budget usage. `inspect_cache`
and `clean_cache(&CacheOptions)` inspect the dedicated copy root; clean is a dry
run unless `apply` is true. The `flow-cache` bin exposes the same operations.
See [session/cleanup contracts](../docs_ja/RETENTION_AND_CLEANUP.md).

| API | Contract |
| --- | --- |
| `HostResource::call(operation, input, output)` | Short synchronous operation returning bytes written or ServiceError; never retain borrowed buffers |
| `HostResources::register(key, value)` | Register a host-owned resource under a nonzero service ID/version; returns typed `Resource<T>` |
| `Resource<T>::id()` / `try_borrow*()` | Opaque ID for logic, typed access only inside the host |
| `HostResources::revoke(id)` | Invalidate future calls; old IDs never identify a replacement slot generation |
| `HostResources::services()` | Owning `Services` client retaining the registry |
| `Services::call(...)` | Validate registry, ID generation, service version and output length |

Dropping `Resource<T>` does not revoke its registration. After revocation, an
existing typed owner can still keep the object alive; DLL access returns REVOKED.
Clients retain the registry even after the original registry/router handle drops.
Avoid ownership cycles, for example a registered object retaining its own registry.
These are thread-bound native handles, not a security boundary against malicious code.

```rust
use flow_reload::{RequestContext, ServiceError, ServiceKey};
const SERVICE: ServiceKey = ServiceKey::new(1, 1);
const STATE: ServiceKey = ServiceKey::new(2, 1);

#[flow_reload::reload_handler(SERVICE, state = STATE)]
async fn worker(ctx: RequestContext) -> Result<(), ServiceError> {
    let saved = ctx.checkpoint().ok_or(ServiceError::INVALID_ARGUMENT)?;
    ctx.cancelled().await;
    ctx.save_checkpoint(STATE, &saved.bytes)?;
    Ok(())
}
```

`reload_handler(SERVICE)` also works without worker state. The macro requires an
`async fn(RequestContext) -> Result<(), ServiceError>` and generates `hot_handler_api`
and owned opaque Future handles. Factory calls copy input/seed and retain host
services/control before returning. The body starts at first poll; an unpolled
factory/Future must not start I/O or mutate shared application state.

`RequestContext` exposes `input`, `generation`, `resource`, `service`, `services`,
`call`, `stop_requested`, `cancelled`, `checkpoint`, `save_checkpoint`,
`spawn_child` and `register_activity`.
`checkpoint()` reads the initial seed; saves copy committed bytes into the host.
`reload_handler` also accepts `requires = &[ServiceRequirement::new(binding, key), ...]`
and `init = Config` (`Config: InitPayload`). Use `HandlerEnvironment::bind` and
`with_initial(InitData::from_value(&config)?)`, passed to `load_with_environment`
or `from_api_with_environment` with RouterOptions. Candidate variants are
`prepare_with_environment` / `prepare_api_with_environment`; ordinary prepare reuses
the current environment. Required service IDs/versions and pure init decoding are
checked before request factories, then resource liveness is checked again at admission/cutover.
`ctx.initial::<Config>()`, `binding`, `call_service` and `call_service_async` access
the copied generation configuration and resolved targets. Limits: 32 additional
services and 1 MiB init bytes. This is distinct from request input and checkpoints.
See [complete contracts](../docs_ja/SERVICE_CONTRACTS_AND_INIT.md).
`call_async(operation, input, max_output_bytes)` returns an owner-thread `ServiceCall`.
It copies input and registers host I/O cleanup, so cancellation waits for I/O quiescence.
Implement `HostResource::start_async` with an owned `HostServiceFuture` and a cooperative
`CancellationToken`. `HostResources::with_async_limits` bounds operations and buffers.
`ThreadService::spawn` supplies a FIFO host worker, a Send + Sync `ServiceClient`
for separate local registries, and an owner-thread `ServiceWorker` to shut down/join.
SDK handles themselves remain !Send. See [full async/thread contracts](../docs_ja/ASYNC_SERVICES_AND_THREADS.md).
The handler's ServiceError or caught unwind becomes `RunStatus::Error`; RequestTask
currently does not expose the original detailed error. Destructor panics and
panic=abort are not recoverable by this boundary.

### Admission and execution

| API | Behavior |
| --- | --- |
| `unsafe RequestRouter::load(path, resources)` | Load trusted SDK-native code |
| `unsafe load_with_options(path, resources, options)` | Load with a shared budget and checkpoint limit |
| `unsafe from_api` / `from_api_with_options` | Static/custom callbacks; caller guarantees code and ABI lifetimes |
| `dispatch(resource_id, input)` | Copy input, return an owned `RequestTask` implementing Future<Output = RunStatus> |
| `unsafe prepare(path)` / `prepare_api(api)` | Validate an inert candidate; admission remains unchanged |
| `activate(candidate)` | Drain cutover; service and state schema must match |
| `pause()` / `resume() -> Result` | Close/open admission, without queuing or replay |
| `in_flight()` | Unfinished managed roots, including registered workers and old Drain generations |
| `generation()` / `is_accepting()` / `options()` | Current admission generation, state and limits |

All native loads require trusted initialization/finalization and callback contracts,
including future replacements. Metadata validation cannot verify native code safety.
Wrong-router and stale candidates are rejected. Pause remains in effect across Drain.

RequestTask owns its Future, generation and control. It can outlive the router;
the host must drive it on its owner thread. `generation`, `resource` and `status`
provide observations. `cancel(self)` and Drop destroy the root Future synchronously;
registered activities move into owner-thread cleanup and remain counted until joined.
Hosts drive `poll_cancelled_requests(cx)` after cancellation, even if the router was
dropped; `poll_workers` and Restart also drive that queue. Independent unregistered
tasks and side effects remain the application's responsibility. External
Wakers retain code references; KeepLoaded remains the native default.

### Managed children and callbacks

`spawn_child(|stop| async move { ... })` registers a `'static` local Future with
cooperative `ChildStop::cancelled()` / `is_requested()`. Factories run after successful
registration, at first poll. The host polls children; this does not spawn OS/runtime tasks.
`register_activity(adapter)` accepts an `unsafe impl ManagedActivity` owning a join
handle or callback registration. `request_stop` errors are latched and retried;
`poll_join` may return **any Ready result only after quiescence**, including completed
join/unregistration and return of in-flight callbacks. Unknown shutdown remains Pending.
Rejected adapters must be safe to drop before their first poll.

All registered work must join before effective request completion, including normal
return, errors, cancellation and old Drain generations. `activity_count` / `activity_error`
and `worker_activity_count` / `worker_activity_error` expose pending work/failures.
The limit is 64 simultaneous registrations per request (`BUSY`); stopping or closing
requests reject registration (`REVOKED`). Unsafe adapters that panic are retained
without assuming quiescence. Unfinished cleanup is retained on abandonment/thread exit.

After activities join, any activity failure blocks Restart with `ActivityFailed`.
`acknowledge_activity_failure(generation)` clears the latch only once that generation
has no unfinished roots. Acknowledgement does not resume admission, recreate failed
workers or roll back effects. See [full contracts and verification](../docs_ja/MANAGED_ACTIVITIES.md).

### Registered workers and Restart

- `start_worker(WorkerId, resource, input, Checkpoint)`: register explicitly restartable work.
- `poll_workers(cx)`, `worker_status(id)`, `worker_checkpoint(id)`: drive and observe workers.
- `stop_worker(id)`: request cooperative stop; `remove_worker(id)`: cancel and unregister.
- `restart(candidate, migrate) -> Result<Restart<'_>, RouteError>`: close admission,
  stop all tracked roots including old Drain generations, wait, stage checkpoint
  conversion and all factories, then commit. Host executors keep polling external
  RequestTasks while the Restart Future drives registered workers.
- `restart_with_commit(candidate, migrate, commit)`: the same shutdown/staging,
  followed by a single synchronous `FnOnce() -> Result<(), ServiceError>` before
  generation installation. Earlier errors/interruption never invoke the hook.
  Hook errors/unwinds become `RouteError::HostCommit` and leave admission paused.
  Applications own exclusion, transactions and recovery; the SDK does not undo effects.
  Do not invalidate staged bindings. See [SQLite/host-cache example](../docs_ja/HOST_RESOURCE_MIGRATION.md).

Only registered workers are recreated. Ordinary requests are not replayed. Failed
or dropped Restart keeps admission paused and previous checkpoints intact;
`resume()?` does not recreate workers. The service contract must still match;
Restart can change the checkpoint schema. See [full Restart contracts](../docs_ja/RESTART_AND_MIGRATION.md).

`Checkpoint` contains a `ServiceKey` schema and owned `Vec<u8>` bytes. The default
`RouterOptions` uses a shared `GenerationBudget` of 32 cumulative load attempts and
1 MiB per checkpoint. Native reservations happen before loading and are never
refunded, including missing files, failed validation and abandoned candidates.
Share budget clones across router recreation to retain the count. `used()` and
`limit()` expose it; this is not a process-wide memory cap.

## Compatibility API: AppState ABI v4

### Logic and host macros

| Macro | Accepted input / behavior |
| --- | --- |
| `reload_direct` / `hot_swap` / `reloadable` | `fn` or `async fn(&mut AppState)`; direct mutation |
| `reload_snapshot` / `hot_reload` | `fn` or `async fn(&mut MyState)` where MyState: CartridgeState |
| `reload_init` / `hot_init` | `fn(&mut MyContext)`; initialization for each loaded generation |
| `allocator!()` | Windows System allocator and allocator-tag metadata for this older path |
| `main!` | AppState loop with optional explicit state alias, runtime, init block and tick |

Async logic uses independent `hot_make_future` handles; async `hot_entry` returns
Error. `hot_reset` does not cancel pending handles. Snapshot state lives in an
outer pinned async block: restore, await the inner logic, then snapshot. Catchable
body/restore/snapshot unwinds become errors; this does not cover all destructors.

`main!` watches/builds local source in Debug and invokes static SDK `hot_run` in
Release. Both use the blocking task adapter and destroy the foreign Future before
tick borrows state. Raw-context macro forms were removed. Arbitrary custom state
belongs in the lower-level `Execution<S>` API, not the fixed AppState runner.

### State and execution

`AppState` contains `counter: i32`, `message: [u8; 256]`, `cartridge_len: u32` and
`cartridge: [u8; 8192]`. Helpers include `set_message`, `message_str`,
`cartridge_bytes`, `write_cartridge`, `load_cartridge` and `save_cartridge`.

```rust
pub trait CartridgeState: Default {
    fn snapshot(&self, out: &mut Vec<u8>) -> Result<(), String>;
    fn restore(&mut self, input: &[u8]) -> Result<(), String>;
}
```

The `cartridge-serde` feature supplies the serde/bincode implementation.

All FlowRunner native-loading constructors are unsafe: `new`, `with_runtime`,
`with_context`, `with_context_and_runtime`, and raw variants. Every replacement
must follow the SDK contracts. Context APIs additionally require matching concrete
types/layouts, exclusive access and validity through all accesses, including
failed initialization, escaped tasks, callbacks and cleanup. They retain no Rust
lifetime/type proof. New integrations should use host services.

- `start(AppState)` returns independent `Execution<AppState>`, usable after reload
  or runner destruction. `Execution<S: 'static>` owns stable state, Future and code
  lease, is thread-bound, exposes `state()` after completion, and returns state
  from `cancel(self)` after destroying the Future. `unsafe from_ffi` is an extension
  boundary with caller-owned code/handle contracts.
- `run` / `run_until_done` borrow AppState until the foreign Future is destroyed,
  including on host unwinding. `run_once` is sync-only (`AsyncRequiresRun` otherwise).
- `run_attached_with_tokio`, `run_attached_with_async_std`, `run_attached_with_smol`
  attach a borrowed callback table for one execution on the owner thread. Future
  destruction precedes detachment and table destruction. Owned `start` does not
  attach that table.
- `reload_if_needed()` detects replacement artifacts. KeepLoaded retains opened
  images, including failed candidates and after runner drop.

Unsafe opt-in `set_unload_policy(WhenIdle)` and `collect_retired_libraries()`
apply only to the compatibility path. Zero tracked leases is insufficient:
all DLL calls, callbacks, unmanaged tasks and deferred destructors must have ended.
See [unload contracts](../docs_ja/UNLOAD_SAFETY.md).

`RunnerError` distinguishes metadata errors (`AbiVersion`, `StateSize`, Windows
`AllocatorTag`), init failures and attached execution failures. A rejected candidate
does not itself require restarting a healthy old generation.

### Features and sample environment

Features are `tokio`, `async-std`, `smol` and `cartridge-serde`; enable what the host needs.
Compatibility samples accept `LOGIC_LIB_PATH`. The `main!` Debug loop also accepts
`FLOW_RELOAD_LOGIC_MANIFEST_DIR`. These are not RequestRouter configuration variables.
See [examples](../examples/README.md) and [testing](TESTING.md).
