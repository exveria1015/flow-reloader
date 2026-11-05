# hot_macro

The `reload_handler` procedural macro for flow-reloader's message ABI.
Applications normally use the re-export from `flow_reload`.

The macro generates native entry points and a `handler_api()` table for static
linking. Handler bodies run when their owned task is polled, after the host has
validated service and initialization contracts.

Build the host and logic against the same SDK version. See the
[repository](https://github.com/exveria1015/flow-reloader) for usage and safety contracts.

Licensed under MIT OR Apache-2.0. Both license texts are included in this package.
