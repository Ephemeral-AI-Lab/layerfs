# Control

> **Status:** Current layer guide for LayerFS 2.0.0.

The control layer is the home's records, the management calls, and
everything that reaches a machine or the network on behalf of the engine.
Layers are `core < engine < control < api`; nothing in `engine/` or `core/`
depends on `control/`. The design is in the
[control architecture guide](../docs/architecture/control/README.md); the
binding code rules are in [AGENTS.md](AGENTS.md).

| Crate | What it holds | Guide |
| --- | --- | --- |
| `layerfs-home` | the home: layout, record files, locks, the `op` intent, names, secrets, traces, the call log | [Home](../docs/architecture/control/home/README.md) |
| `layerfs-machine` | machine records and calls, the sealed bundle, the worker protocol and client | [Machines](../docs/architecture/control/machines/README.md) |
| `layerfs-platform` | driver: *where* Sandboxes run, one adapter per platform (`native`, `docker`, `apple_container`, `lima`), the support table | [Platforms](../docs/architecture/control/machines/platforms.md) |
| `layerfs-store` | Store and project records, placement, leases, Store calls, sqlite transfer, PostgreSQL, backups | [Stores and projects](../docs/architecture/control/stores-and-projects/README.md), [Backups](../docs/architecture/control/backups/README.md) |
| `layerfs-bucket` | driver: *how* a Store is backed up to a bucket (`local_folder`, `minio`, `r2`) | [Buckets](../docs/architecture/control/backups/buckets.md) |
| `layerfs-sandbox` | Sandbox records and lifecycle, Workspace calls, exec, the daemon channel | [Sandboxes](../docs/architecture/control/sandboxes/README.md) |
| `layerfs-sandbox-backend` | driver: *how* a Sandbox runs on a kernel (`host_process`, stored as `native`; `namespaces`), and `layerfs-sandbox-helper` | [Backends](../docs/architecture/control/sandboxes/backends.md) |
| `layerfs-control` | what spans entities: the worker server, inventory, doctor, home migration, and the machine, Store, project and backup calls with their checks | [Doctor](../docs/architecture/control/doctor/README.md), [Migration](../docs/architecture/control/home/migration.md) |

`SUPPORT.json` is generated from `layerfs-platform/src/support.rs`; never
edit it by hand. After changing the table, write it and review the diff:

```
LAYERFS_BLESS_SUPPORT=1 cargo +1.85.1 test --locked -p layerfs-platform --test support
```
