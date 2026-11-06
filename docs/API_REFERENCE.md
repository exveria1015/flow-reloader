# API reference

## Message API

The recommended API separates host resources, request admission and generation-bound
execution. It uses **Handler ABI 5**, Services ABI 2 and task-control ABI 2.

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

## Explicit native reclamation

KeepLoaded is the default. `unsafe reclaim_native_image(ArtifactId)` is a low-level
host operation for constrained images whose external execution, TLS, OS registrations
and dependency finalizers the caller can certify as safe to unload. Handler ABI 5 is
unchanged; no plugin shutdown hook or dedicated generation thread is generated.

Obtain value-only IDs from `RequestRouter::artifact_id()` or `PreparedHandler::artifact_id()`.
After cutover, finish/drop old tasks and candidates, drive registered cancellation cleanup,
and call `release_retired_generations()` to release the router's completed old generations.
Activity failures must be acknowledged before releasing latched generations.
Join external execution and establish the TLS/dependency contract before calling reclaim
on the loading thread. Escaped Waker wake/drop/clone code must have returned completely:
the last host lease disappearing, a delay or another poll is insufficient.

The SDK rechecks retirement, code leases, returned host-initiated calls and owner thread,
then closes outside its locks. Rejected/unvalidated candidates remain retained.
`ReclaimResult::Retained { reasons }` reports blockers; `Released { copy_removal_error }`
separates native-reference release from copy deletion. `CloseFailed` preserves an
uncertain diagnostic/copy and never retries the consumed handle. Unknown/released IDs
return `NotFound`. Drop never closes images and successful reclaim never refunds the
cumulative `GenerationBudget`.

Image records report `code_leases`, `active_calls`, `retention_reasons` and `close_error`.
`ExternalQuiescenceUnconfirmed` always appears in snapshots because only the caller
can supply that proof. `released_native_images` and `close_failure_count` count SDK
outcomes; OS module residency and RAM usage are separate observations.
See the [full safety contract](../docs_ja/UNLOAD_SAFETY.md).

## Registered generation locals and resident notifications

`GenerationLocal<T>::new(fn() -> T)` defines a static key. `with(&ctx, f)` and
`with_mut(&ctx, f)` lazily access an owner-thread value shared only within one
router generation. References cannot escape the closure. Limits: 64 keys per
generation; conflicting borrows or recursive initialization return BUSY. Init/access
panics return PANICKED without rolling back mutations or side effects.
Values are destroyed in reverse initialization order, while code is leased, when
the last generation owner is released. Completed task handles still own a generation.
Destructor failures retain code and are never retried; native records expose
`registered_locals`, `local_cleanup_failures`, `RegisteredLocals` and `LocalCleanupFailed`.

`RequestContext::host_notification()` returns an owner-thread `HostNotification`.
Its `as_raw()` is a borrowed C descriptor: foreign retention requires a vtable clone
and exactly one drop/consuming wake per owned handle. The resident host owns the
callbacks/data. Notifications detach from the executor after the root AND all
registered cleanup finish; delayed host wake/clone/drop do not enter or pin the DLL.
SDK `call_async`/`call_service_async` use this path automatically on supporting hosts.
Executor Wakers must belong to resident host code independently of reloaded generations.

Handler ABI 5 and task-control ABI 2 retain their base layouts. An optional,
size-checked task-control tail carries generation-services ABI 1. Older/unknown
extensions return UNSUPPORTED_OPERATION for new APIs; async services on old hosts
keep the legacy retained-Waker path. C ABI handles remain the cross-library boundary.
Arbitrary thread_local!, DLL Wakers directly captured by external runtimes, and
dependency-specific shutdown remain application/adapter responsibilities. Reclamation
stays unsafe and KeepLoaded stays the default. See [scope and lifecycle](../docs_ja/MANAGED_LOCALS_AND_NOTIFICATIONS.md).

## Build configurations

Defaults enable `native` (DLL loading, retention and cache cleanup) and `dev-tools`
(source watching and Cargo helpers in `flow_reload::development`). Logic crates and
statically linked hosts should use `default-features = false`. `native` alone does
not include a file watcher. `flow-cache` requires `native`.

| Configuration | Dependency options | Included features |
| --- | --- | --- |
| Development host | Defaults | Native loading, retention, cache cleanup and build/watch helpers |
| Host with an external builder | `default-features = false, features = ["native"]` | Native loading, retention and cache cleanup |
| Logic crate or static host | `default-features = false` | Resources, tasks, routing and handler generation |

Features select the loading mode independently of Debug/Release.

`reload_handler` also generates `handler_api() -> HandlerApi` for static linking.
Pass it to `unsafe RequestRouter::from_api`; the callback lifetime contract still applies.
The static service example uses the same handler and host resource contracts:

```sh
cargo run -p service_host --release --no-default-features --features static-logic --locked
```

## Low-level task ABI

`TaskHandle` / `TaskVTable` describe an owned foreign Future, while `FfiFuture`
polls and destroys that handle exactly once. Low-level `unsafe FfiFuture::new`
requires the caller to retain code and borrowed data; use `RequestTask` for routed work.
`LoadError` reports I/O and library/symbol errors through `RouteError::Load` when
`native` is enabled. Handler ABI 5, Services ABI 2 and task-control ABI 2 define
the current boundaries. Build host and logic against the same SDK.

See [compatibility and scope](COMPATIBILITY.md) for the runtime contract and
[changelog](../CHANGELOG.md) for breaking changes.
