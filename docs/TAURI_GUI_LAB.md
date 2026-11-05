# Tauri GUI lab

An English-language UI for editing trusted local Rust logic while the host window
stays open. The package name `tauri_tokio` is historical; the example uses the
message API for both request handling and worker Restart.

## Start

From the workspace root:

```sh
cargo run -p tauri_tokio
```

Install the platform's Tauri build prerequisites first. Windows also needs
WebView2. npm and the Tauri CLI are not required. The first logic build runs in a
background Cargo process. Closing the window stops and joins the engine thread.

## Edit and reload

1. Change `LABEL`, `ACCENT` or `STEP` in the editor. The **v1 · +1** and **v2 · +5**
   presets provide a repeatable comparison.
2. Select **Save & reload** or press Ctrl+S. This writes
   `examples/tauri_tokio_logic/src/lib.rs`; presets alone only change the editor buffer.
3. The host builds the saved source, validates the candidate, then applies the
   selected update policy. Check the host PID, counter, worker generation, new
   request generation, checkpoint, load budget and activity log.

**Initial source** restores the source captured at app startup into the editor.
**Reload file** reads the current file. External editor saves trigger builds too,
but do not overwrite the in-window editing buffer. **Rebuild** builds the saved
file without saving the current editor buffer.

HTML/CSS/JavaScript in `examples/tauri_tokio/dist/` are handwritten host assets.
Rebuild and restart the host after editing them. Rust logic is the hot-reloaded
part; it changes the display through host resource operations.

## Drain and Restart

| Policy | Existing counter worker | New requests |
| --- | --- | --- |
| Drain | Continues on its original code and increment | Use the new generation |
| Restart | Stops cooperatively, saves its counter, then restarts on new code | Use the new generation after cutover |

Selecting a policy applies it to the next successful build. To compare policies,
save v1 first, then select Drain and save v2. The worker should still show v1;
**Send now** should return a v2 reply. Select Restart and rebuild to update the worker.

The checkpoint is an eight-byte u64 counter, retained across reloads in the same
host process, not persisted across app exits. This example does not change its
schema. See the [Restart contract](../docs_ja/RESTART_AND_MIGRATION.md) for migration.

Select **5-second request**, then rebuild while it runs. Drain finishes it with
its original code. Restart requests cooperative stop, allowing the request to
finish early; the reply is marked **completed after a stop request**. Ordinary
requests are not replayed. The host owns the UI; TCP connection continuity is
covered separately by `service_host` and native routing tests.

## Failure recovery

Select **Build error**, then save. **Build details** shows the compiler error while
the old worker and requests continue. A v1/v2 preset removes the sample's injected
error; save to recover. Compile failures do not consume a load slot.

Candidate validation and shutdown failures are separate from compiler errors.
An interrupted Restart leaves admission paused; decide recovery before using
**Resume requests**. Resuming does not recreate a stopped worker or undo I/O.
Logic that never returns from poll cannot be safely interrupted on its own thread.

The default load budget is 32 cumulative attempts. Restart the development process
when exhausted. DLL copies are placed under the user's temporary directory at
`flow-reloader/<PID>/`; remove retained copies only after that process exits.

## Verify

```sh
cargo test -p tauri_tokio --bin tauri_tokio -- --ignored --test-threads=1
cargo clippy -p tauri_tokio -p tauri_tokio_logic --all-targets -- -D warnings
node --check examples/tauri_tokio/dist/main.js
```

The backend test builds isolated native DLLs without opening a window or editing
the live sample source. Interactive layout and input checks are separate. See
[testing and limits](TESTING.md) and the [Japanese guide](../docs_ja/TAURI_GUI_LAB.md).
