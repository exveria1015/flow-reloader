# Production Guide

This guide focuses on making a `flow-reloader` deployment predictable and safe in production.

## Runtime Model

Debug builds:

- Auto-build and file-watch logic source
- Intended for local development

Release builds:

- No auto-build/watch loop
- Host directly invokes exported logic entrypoints
- You control artifact rollout explicitly

## Recommended Deployment Strategy

1. Build logic artifacts in CI.
2. Version and publish artifacts with immutable names.
3. Roll out by updating host-visible library path (or replacing symlink/indirection).
4. Let host call `reload_if_needed()` in the main loop.

Avoid compiling logic inside production host processes.

## ABI Compatibility Rules

The host validates metadata before running init/entry:

- `ABI_VERSION`
- `AppState` size
- allocator tag on Windows

If these mismatch, reload fails and host should restart with compatible artifacts.

Do not change ABI fields casually:

- `AppState` layout
- metadata semantics
- exported symbol contract

## Context and Init Safety

`hot_init` is run only after metadata validation.

Rules:

- Context pointer must remain valid for runner lifetime
- Keep init idempotent and failure-aware
- Return only `RunStatus::Done` for success

## Snapshot State Strategy

If using `#[reload_snapshot]`:

- Define explicit versioned snapshot schemas
- Treat snapshot failures as deploy blockers
- Add migration logic in `restore()` when schema changes

`CartridgeState::snapshot` now returns `Result`, so serialization failures are no longer silent.

## Async Attached Runtime

When using attached mode:

- Enable only one required runtime feature (`tokio`, `async-std`, `smol`)
- Use attached helpers (`AttachedSleep`, `AttachedTcpRequest`) only under attached execution
- Handle `AttachedUnavailable` and `AttachedAttachFailed` explicitly

## Error Handling Policy

Recommended policy:

- Recoverable transient errors: retry with backoff
- Metadata mismatch (`AbiVersion`, `StateSize`, `AllocatorTag`): fail fast and restart
- Init failures: fail fast and surface deploy status

## Observability

Capture at minimum:

- reload attempts and outcomes
- metadata mismatch details
- init failures
- logic `RunStatus::Error` frequency

Add structured logs around `run()` and `reload_if_needed()`.

## Production Checklist

- [ ] Host and logic built from pinned, compatible revisions
- [ ] ABI/state compatibility tests pass
- [ ] Reload success/failure metrics exported
- [ ] Restart policy defined for non-recoverable runner errors
- [ ] Snapshot schema and migration strategy documented
- [ ] Runtime feature set minimized to what is required

