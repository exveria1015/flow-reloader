# Documentation

Use the message API for native reload and statically linked logic. Start with
the getting-started guide, then read the contracts relevant to your host.

## Start and integrate

| Guide | Purpose |
| --- | --- |
| [Getting started](GETTING_STARTED.md) | Run the TCP example and split host/logic ownership |
| [Examples](../examples/README.md) | Select a native, static, game, database or GUI sample |
| [API reference](API_REFERENCE.md) | Resources, routing, workers, limits and build configurations |
| [Compatibility and scope](COMPATIBILITY.md) | Toolchains, platforms, ABI and application responsibilities |
| [Tauri GUI lab](TAURI_GUI_LAB.md) | Edit Rust in the English UI and compare Drain/Restart |

## Operate and verify

| Guide | Purpose |
| --- | --- |
| [Development operation](DEVELOPMENT.md) | Build, cutover, recovery and retained DLLs |
| [Troubleshooting](TROUBLESHOOTING.md) | Errors and recovery paths |
| [Testing](TESTING.md) | Test roles, reproducible commands, Miri and verification limits |
| [Contributing](../CONTRIBUTING.md) | Source layout and change/documentation conventions |
| [Changelog](../CHANGELOG.md) | User-visible changes and breaking APIs |

## Detailed contracts in Japanese

| Guide | Purpose |
| --- | --- |
| [Architecture](../docs_ja/ARCHITECTURE.md) | Ownership and update boundaries |
| [Resources and Drain](../docs_ja/HOST_RESOURCES_AND_ROUTING.md) | Service contracts and admission cutover |
| [Restart and migration](../docs_ja/RESTART_AND_MIGRATION.md) | Shutdown, staged workers and checkpoints |
| [Managed children/callbacks](../docs_ja/MANAGED_ACTIVITIES.md) | Explicit registration, join and cancellation cleanup |
| [Async services and threads](../docs_ja/ASYNC_SERVICES_AND_THREADS.md) | Bounded host I/O, FIFO and owner threads |
| [Service contracts and typed init](../docs_ja/SERVICE_CONTRACTS_AND_INIT.md) | Explicit bindings and generation-owned configuration |
| [Host resource / DB migration](../docs_ja/HOST_RESOURCE_MIGRATION.md) | Commit hooks, SQLite transactions and recovery |
| [Retention and cache cleanup](../docs_ja/RETENTION_AND_CLEANUP.md) | Native diagnostics and exited-session cleanup |
| [Ownership and lifetimes](../docs_ja/LIFETIME_REVIEW.md) | Future, Waker, data and code lifetimes |
| [DLL lifetime](../docs_ja/UNLOAD_SAFETY.md) | Retained code and unmanaged work contracts |
| [Bevy game sample](../docs_ja/BEVY_GAME.md) | Frame boundaries, worker Restart and stale result rejection |

[Project overview](../README.md) · [日本語ドキュメント](../docs_ja/README.md)
