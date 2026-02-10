# Troubleshooting

Common failures and recovery patterns.

## `ABI version mismatch: host X, logic Y`

Cause:

- Host and logic built against incompatible `ABI_VERSION`

Fix:

- Rebuild host and logic from matching revision
- Ensure deployed logic artifact matches running host binary

## `AppState size mismatch`

Cause:

- `AppState` layout changed in one side only

Fix:

- Rebuild both host and logic
- Avoid changing `AppState` without coordinated rollout

## `Allocator mismatch` (Windows)

Cause:

- Logic crate did not expose expected allocator tag

Fix:

- Add `flow_reload::allocator!()` in logic crate
- Rebuild logic and reload

## `hot_init failed` or `hot_init error: ...`

Cause:

- Context pointer invalid
- Init panicked
- Init returned non-`Done` status

Fix:

- Validate context lifetime and type
- Make init deterministic and panic-free
- Log and inspect init status path

## `attached async entry not available`

Cause:

- Logic entrypoint does not support attached mode
- Missing exported attached symbols

Fix:

- Use async `#[reload_direct]` logic
- Ensure host is using runtime-enabled `flow_reload` feature set

## `failed to attach runtime`

Cause:

- Runtime vtable registration failed in logic module

Fix:

- Verify host runtime feature (`tokio`, `async-std`, `smol`) and call path
- Confirm logic crate was built with compatible `flow_reload`

## Reload Not Triggered After File Edit

Cause:

- Watcher path does not include edited file
- Debounce window not elapsed
- Editing a different logic crate than host expects

Fix:

- Confirm logic manifest path resolution
- Optionally set `FLOW_RELOAD_LOGIC_MANIFEST_DIR`
- Wait for debounce period and check host logs

## `io error: Access is denied (os error 5)` on `target/*`

Cause:

- File lock from antivirus/indexer/other process
- Restricted directory permissions

Fix:

- Use a writable workspace path
- Exclude build directories from aggressive scanning
- Close processes holding files and retry build/reload

## `library load error` on reload

Cause:

- Corrupt/partial dynamic library copy
- Locked old artifact path

Fix:

- Rebuild logic fully
- Ensure copy destination is writable
- Retry `reload_if_needed()` after short backoff

