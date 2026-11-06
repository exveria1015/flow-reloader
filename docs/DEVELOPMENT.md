# Development operation

The initial scope is local development, with trusted native logic built against the
same SDK as the host. Production rollout, crash isolation and automatic rollback
of external side effects are not provided.

## Build and cut over

The host chooses its build/watch policy. `RequestRouter` does not run Cargo or
watch sources by itself. `service_host` builds synchronously; the Tauri example
runs Cargo in a background child process.

Build a complete artifact before preparing it. `unsafe prepare(path)` loads and
validates an inert candidate without changing admission; native initialization
must itself obey that contract. `activate` implements Drain. Use `restart` for
registered workers and explicit checkpoint migration.

Maintain compatible service contracts during Drain. The router checks the
handler ABI, target service and state schema; it cannot verify byte meanings or
DLL safety. Rebuild both host and logic when changing ABI versions.

## Recovery

| Failure | Behavior / host action |
| --- | --- |
| Build or candidate validation fails | Keep the current admission and tasks; report the error and repair the candidate |
| Candidate is stale or from another router | Prepare a fresh candidate for the intended router |
| Restart cannot finish shutdown | Keep polling external tasks; apply an application timeout if needed |
| Restart fails after closing admission, or its Future is dropped | Admission stays paused; checkpoints remain. Retry migration or explicitly `resume()?` after deciding recovery |
| Load budget exhausted | Stop loading and restart the development process |

`resume` does not recreate old workers or undo I/O. Ordinary requests are never
replayed automatically. A polling function that never returns cannot be safely
interrupted by a timeout on the same thread. See [Restart](../docs_ja/RESTART_AND_MIGRATION.md).

## Resource bounds and diagnostics

Record active/worker generations, admission status, in-flight roots, build/load
errors, restart progress, checkpoint sizes, budget usage and copy locations.
`in_flight()` does not count unmanaged tasks, escaped callbacks or retained images.
It does keep roots counted until their explicitly registered activities join,
including cancelled roots driven through owner-thread `poll_cancelled_requests`.

Reuse the same `GenerationBudget` when recreating routers in one development
session. Its default 32 slots count cumulative native load attempts, including
failures and abandoned candidates. It is not a process-wide memory cap. Checkpoint
limits default to 1 MiB per worker. Hosts must separately bound queues and I/O.

Native images default to KeepLoaded. Copies are placed under the user's temporary
directory at `flow-reloader/managed-v1/sessions/<SessionId>/copies/`; code and files
can accumulate. Use `retention_snapshot()` for SDK-native references and
`flow-cache inspect` for files; `flow-cache clean --apply` removes eligible exited
sessions. Legacy PID directories are listed only. See [cache operations](../docs_ja/RETENTION_AND_CLEANUP.md).
Images remain loaded by default. Constrained hosts can opt into explicit unsafe
reclamation after proving external/TLS/dependency quiescence; the cumulative attempt budget is unchanged.
see [DLL lifetime](../docs_ja/UNLOAD_SAFETY.md).

The opt-in `development` module provides build/path helpers,
not a stable build-system API. Applications can supply their own artifact builder.
