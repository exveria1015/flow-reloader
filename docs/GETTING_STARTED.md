# Getting started

## Run an existing example

From the workspace root:

```sh
cargo run -p service_host
```

Connect to `127.0.0.1:4000` with a TCP client and send newline-terminated messages.
Edit the response label in [service_logic](../examples/service_logic/src/lib.rs),
save it, then send another message on the **same connection**. The host rebuilds
logic and switches new admissions after a successful load. `slow\n` takes about
five seconds, allowing an old request to remain active during a Drain cutover.

This is a single-connection example. Builds are synchronous, and edits while
waiting for input are processed at the next message boundary. Use the
[Tauri GUI lab](TAURI_GUI_LAB.md) for an editor and background builds,
or `cargo run -p flow_reload --example restart` for a minimal migration example.

## Split a new application

Create a long-lived host binary and a logic `cdylib`. Both depend on the local
`flow_reload` crate. The message host does not need a static dependency on the
logic crate. For example, a logic crate at `examples/my_logic` can use:

```toml
[package]
name = "my_logic"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
flow_reload = { path = "../../flow_reload" }
```

Add the crate to workspace members. Define service IDs, versions, operations and
byte encodings shared by host and logic. Increment the contract version when its
meaning changes; matching IDs alone do not validate an encoding.

The logic handler receives owned input and an opaque resource ID:

```rust
use flow_reload::{RequestContext, ServiceError, ServiceKey};
const SESSION: ServiceKey = ServiceKey::new(0x4c49_4e45, 1);

#[flow_reload::reload_handler(SESSION)]
async fn handle(ctx: RequestContext) -> Result<(), ServiceError> {
    ctx.call(0, ctx.input(), &mut [])?;
    Ok(())
}
```

On the host, implement `HostResource::call` with short, nonblocking operations,
register it in `HostResources`, and keep actual sockets, framing and I/O in the
host. Operation 0 above must be defined by that implementation, for example as
placing bytes in a bounded outgoing queue. Use
[service_host](../examples/service_host/src/main.rs) as the complete host example.

Load the built artifact with `unsafe RequestRouter::load(path, resources)`.
The unsafe boundary requires trusted SDK-compatible native code, including all
future replacements and module initialization. `dispatch(resource.id(), input)`
returns a thread-bound Future; the host must poll it. Input is copied before
`dispatch` returns. The resource registry and code generation stay alive through
owned handles; the socket itself never crosses the DLL boundary.

## Select an update policy

- **Drain**: build, `unsafe prepare(path)`, then `activate(candidate)`. Existing
  tasks keep their generation. New requests use the new handler. Service and
  checkpoint schema contracts must match.
- **Restart**: register long-lived work using `start_worker`, publish explicit
  checkpoints, and use `restart(candidate, migrate)`. The host continues polling
  externally owned tasks while Restart waits for shutdown. Only registered workers
  are recreated; ordinary requests are not replayed.

See the detailed [routing](../docs_ja/HOST_RESOURCES_AND_ROUTING.md) and
[Restart](../docs_ja/RESTART_AND_MIGRATION.md) contracts before integrating workers.

## Compatibility API

`FlowRunner`, `AppState`, `main!` and attached runtime helpers remain available.
They use ABI v4, separately from Handler ABI 2. Start with their
[compatibility samples](../examples/README.md), not a raw-context integration.
All `with_context*` constructors are unsafe and require matching types, exclusive
access and lifetime through every escaped access and destructor. The safe
context forms of `main!` were removed.
