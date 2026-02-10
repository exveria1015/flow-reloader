# API Reference

This is a practical reference for the public surface used by host and logic crates.

## Logic Macros

## `#[reload_direct]`

Use for direct state mutation with `&mut AppState`.

Accepted signatures:

- `fn update(state: &mut AppState)`
- `async fn update(state: &mut AppState)`

Behavior:

- Generates `hot_type`, `hot_entry`, `hot_reset`
- Async functions return `RunStatus::Pending` until complete
- Panics are caught and mapped to `RunStatus::Error`

## `#[reload_snapshot]`

Use with a custom snapshot type that implements `CartridgeState`.

Accepted signatures:

- `fn update(state: &mut MyState)`
- `async fn update(state: &mut MyState)`

Behavior:

- State is restored from `AppState::cartridge`
- State is serialized back into cartridge on completion

## `#[reload_init]`

One-time init hook for host context.

Accepted signature:

- `fn init(ctx: &mut MyContext)`

## Alias Macros

- `#[reloadable]` -> `#[hot_swap]`
- `#[reload_direct]` -> `#[hot_swap]`
- `#[reload_snapshot]` -> `#[hot_reload]`
- `#[reload_init]` -> `#[hot_init]`

## Host Macros

## `flow_reload::allocator!()`

On Windows:

- Sets `std::alloc::System` as global allocator in logic crate
- Exposes allocator tag metadata for host validation

## `flow_reload::main!(...)`

Generates a host loop with optional:

- custom state type
- context value
- runtime handle
- init block
- tick callback

Macro variants include:

- no context
- context
- no context + runtime
- context + runtime

## Core Types

## `AppState`

Shared ABI state:

- `counter: i32`
- `message: [u8; 256]`
- `cartridge_len: u32`
- `cartridge: [u8; 8192]`

Helpers:

- `set_message(&str)`
- `message_str()`
- `cartridge_bytes()`
- `write_cartridge(&[u8])`
- `load_cartridge<T: CartridgeState>() -> Result<T, String>`
- `save_cartridge<T: CartridgeState>(&T) -> Result<(), String>`

## `CartridgeState`

```rust
pub trait CartridgeState: Default {
    fn snapshot(&self, out: &mut Vec<u8>) -> Result<(), String>;
    fn restore(&mut self, input: &[u8]) -> Result<(), String>;
}
```

With `cartridge-serde` feature, serde types automatically implement it via bincode.

## `FlowRunner`

Constructors:

- `FlowRunner::new(path)`
- `FlowRunner::with_runtime(path, runtime)`
- `FlowRunner::with_context(path, &mut context)`
- `FlowRunner::with_context_and_runtime(path, &mut context, runtime)`

Execution:

- `run(&mut AppState)` (sync or async-until-done)
- `run_once(&mut AppState)`
- `run_attached_with_tokio(...)`
- `run_attached_with_async_std(...)`
- `run_attached_with_smol(...)`
- `reload_if_needed()`

## `RunnerError`

Important variants:

- `AbiVersion`
- `StateSize`
- `AllocatorTag` (Windows)
- `InitFailed` / `InitError`
- `AttachedUnavailable` / `AttachedInvalid` / `AttachedAttachFailed`

Treat `AbiVersion`, `StateSize`, and `AllocatorTag` as restart-required errors.

## Runtime Feature Flags

`flow_reload` crate features:

- `tokio`
- `async-std`
- `smol`
- `cartridge-serde`

Use only the runtime features your host needs.

## Environment Variables

- `LOGIC_LIB_PATH`: override path to loaded logic library
- `FLOW_RELOAD_LOGIC_MANIFEST_DIR`: override logic manifest directory for macro-generated debug build loops

