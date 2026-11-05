# Documentation

Start with the message API. The older `FlowRunner` API is documented separately
within the reference and sample inventory.

- [Getting started](GETTING_STARTED.md): run the examples and split host/logic ownership.
- [Tauri GUI lab](TAURI_GUI_LAB.md): edit Rust in the English UI and compare Drain/Restart.
- [Bevy game sample (日本語)](../docs_ja/BEVY_GAME.md): real DLLs, frame boundaries, worker Restart and stale results.
- [Host resource/DB migration (日本語)](../docs_ja/HOST_RESOURCE_MIGRATION.md): final commit hook, SQLite/host-cache transactions, failure fencing and recovery.
- [Retention and cache cleanup (日本語)](../docs_ja/RETENTION_AND_CLEANUP.md): value-only native diagnostics, process-lifetime leases and explicit exited-session cleanup.
- [API reference](API_REFERENCE.md): message routing, workers, limits and compatibility APIs.
- [Managed children/callbacks (日本語)](../docs_ja/MANAGED_ACTIVITIES.md): explicit registration, quiescence and cancellation cleanup.
- [Async services and threads (日本語)](../docs_ja/ASYNC_SERVICES_AND_THREADS.md): bounded host I/O, cancellation, FIFO and multiple owner threads.
- [Multi-service contracts and typed init (日本語)](../docs_ja/SERVICE_CONTRACTS_AND_INIT.md): explicit bindings, schema/codec validation and generation-owned configuration.
- [Development operation](DEVELOPMENT.md): build, cutover, recovery and retained DLLs.
- [Troubleshooting](TROUBLESHOOTING.md): current errors and recovery paths.
- [Testing](TESTING.md): test roles, commands, Miri and verification limits.
- [Examples](../examples/README.md): recommended examples and compatibility samples.
- [日本語ドキュメント](../docs_ja/README.md): architecture, detailed routing,
  Restart, GUI and safety contracts.
