# Troubleshooting

## Message API

| Error or symptom | Check / recovery |
| --- | --- |
| `Abi` / handler ABI mismatch | Rebuild host and logic against the same SDK. Handler ABI is 2; the older AppState ABI is 4 |
| `ContractMismatch` | Match the resource service ID/version with the handler contract; incompatible service migration is application-owned |
| `StateSchemaMismatch` | Drain requires identical schemas. Use Restart with an explicit checkpoint conversion for a new schema |
| `WrongRouter` / `StaleCandidate` | Prepare again using the intended router's current generation |
| `GenerationLimit` | Shared load-attempt budget is exhausted; restart the development process. Failed native loads count too |
| `Paused` after failed/interrupted Restart | Decide recovery, retry Restart, or call `resume()?`. Resuming does not recreate old workers |
| Restart stays Pending | Poll external RequestTasks as well as Restart; handlers must cooperate with cancellation and finish their children |
| `WorkerFailed` | Inspect worker status; remove or repair its registration before retrying. Do not blindly replay requests |
| `CheckpointTooLarge` / migration failure | Check encoding, schema and `max_checkpoint_bytes`. Previous checkpoints remain available |
| `BUSY` / `BUFFER_TOO_SMALL` service errors | Release conflicting host borrows or provide enough output space. Calls are not retried automatically |
| `REVOKED` / resource unavailable | Check revocation, registry and ResourceId generation; use a live ID from the correct registry |

A successful Drain keeps existing workers on old code. In the Tauri lab, a new
request can show new text while the counter keeps its old increment. Use Restart
and rebuild to update the registered counter worker.

## Build and reload

Check the host's source path and build log first. The router itself has no watcher.
`service_host` processes edits at message boundaries or during request polling;
the GUI uses background builds. Compatibility hosts watch the built artifact,
so rebuild logic into their expected `target/hot-reload-build` directory.
Only the compatibility `main!` Debug loop automatically watches sources.

On Windows access-denied errors, inspect the **reported path**: Cargo output must
be writable, and copied DLLs use the user's temporary `flow-reloader/<PID>/`
directory rather than the current working directory. Check permissions and active
process/file locks; do not delete live DLL copies. A partial or stale artifact
should be rebuilt fully before preparing another candidate.

GUI HTML/CSS/JS are embedded host assets. Rebuild and restart the host after
changing them; only the sample's Rust logic DLL is hot-reloaded.

## Compatibility API

- `AbiVersion`, `StateSize`, `AllocatorTag`: rebuild both sides with matching ABI v4
  and AppState layout. Windows logic uses `flow_reload::allocator!()`. A rejected
  replacement leaves the previous generation available; this is not automatically
  a process-restart requirement.
- `InitFailed` / `InitError`: inspect init status, unwind handling and the unsafe
  context contract, including concrete type, exclusive access and escaped lifetimes.
- `AsyncRequiresRun`: `run_once` accepts synchronous logic only. Use `run`, owned
  `start`, or a scoped attached execution as appropriate.
- `AttachedUnavailable` / `AttachedAttachFailed`: use an async SDK handler and the
  matching host runtime feature and `run_attached_with_*` method. Owned `start`
  does not attach the older borrowed runtime service table.

KeepLoaded does not cancel tasks or repair invalid references. See
[ownership](../docs_ja/LIFETIME_REVIEW.md) and [unloading](../docs_ja/UNLOAD_SAFETY.md).
