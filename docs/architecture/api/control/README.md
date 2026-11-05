# Control

[Architecture](../../README.md) › [API](../README.md) › Control

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The management library under the call table: the home and its records, machines and workers, Stores, projects, Sandboxes, Workspaces and backups.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Crates | `layerfs-control` |
| Source | [`api/crates/layerfs-control/src`](../../../../api/crates/layerfs-control/src) |
| Uses | [Routing and workers](../routing/README.md), [Connect: Machines](../../connect/machines/README.md), [Connect: Sandboxes](../../connect/sandboxes/README.md), [Connect: Backups](../../connect/backups/README.md), [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Project Init](../../engine/project-init/README.md) |
| Used by | [Call table](../call-table/README.md) |
| Flows | [A call, end to end](../../flows/call.md), [Project Init](../../flows/init.md), [A Workspace session](../../flows/workspace.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Home and records](home-and-records.md) | The LayerFS home and its records. |
| [Families](families.md) | How control implements each call family. |
| [Doctor](doctor.md) | Health checks over the home. |

## The picture

```text
 call table (layerfs-api)        one Run fn per call
        |
        v
 calls::dispatch  -- check args, scope, open home, built-ins --+
        |                                                      |
        v                                                      v
 +--------------------- layerfs-control ----------------------------+
 | machine/   store/   project/   sandbox/   workspace/   backup/   |
 |    |         |         |          |           |           |      |
 |    +---------+---------+----------+-----------+-----------+      |
 |                        |                                         |
 |   common/: home (records, locks, leases, migrate), names,        |
 |   op (one intent), secrets, place (where it runs), trace         |
 |   inventory.rs: relationships derived from records               |
 |   doctor/: read-only report over the same records                |
 +------------------------------------------------------------------+
        |                 |                    |
   this process     machine's worker     Sandbox daemon
   (Local)          (place::Worker)      (daemon/, direct.rs)
```

Every call first opens the home ([Home and records](home-and-records.md)),
then runs one family function ([Families](families.md)). Where the work
runs is decided by [`common/place.rs`](../../../../api/crates/layerfs-control/src/common/place.rs);
the routes are described in [Routing and workers](../routing/README.md).
The [Doctor](doctor.md) reads the same records and changes nothing.

## What it does

| Folder | What it owns | Source |
| --- | --- | --- |
| `calls/` | Table shape, argument checks, folder scope, snake_case keys, the one dispatcher | [`calls/`](../../../../api/crates/layerfs-control/src/calls) |
| `common/home/` | `layerfs.json`, record files, the home lock, entity locks, Store leases, conversions, `import-env` | [`common/home/`](../../../../api/crates/layerfs-control/src/common/home) |
| `common/` | Names, the `op` intent, outcomes, secrets, place, trace, JSON | [`common/`](../../../../api/crates/layerfs-control/src/common) |
| `inventory.rs` | Reads by name and the relationships no record stores | [`inventory.rs`](../../../../api/crates/layerfs-control/src/inventory.rs) |
| `machine/` | Machine records, `worker.json`, start/stop, the worker client and server | [`machine/`](../../../../api/crates/layerfs-control/src/machine) |
| `store/` | Store records, Store calls (`branch`, `commit`, `file`), push, pull, remove, PostgreSQL connect | [`store/`](../../../../api/crates/layerfs-control/src/store) |
| `project/` | Project records, `project.init` on each route, folder copy to a worker | [`project/`](../../../../api/crates/layerfs-control/src/project) |
| `sandbox/` | Sandbox records, create/close, host operations, direct daemon calls | [`sandbox/`](../../../../api/crates/layerfs-control/src/sandbox) |
| `workspace/` | The `workspace` calls and their serving side | [`workspace/`](../../../../api/crates/layerfs-control/src/workspace) |
| `daemon/` | The authenticated daemon channel and its pool | [`daemon/`](../../../../api/crates/layerfs-control/src/daemon) |
| `backup/` | Bucket records, Store backup links, create, restore, list | [`backup/`](../../../../api/crates/layerfs-control/src/backup) |
| `doctor/` | `layerfs doctor`, outside the call table | [`doctor/`](../../../../api/crates/layerfs-control/src/doctor) |

The home is a folder of JSON records (layout 3): `layerfs.json`,
`machine/`, `store/`, `sandbox/`, `backup/`, `log/` and `trace/`. Its
layout is drawn in [Home and records](home-and-records.md#home-layout).
Only host processes read or write it; a worker gets what it needs in each
request and keeps its own root, which is not a home.

The `machine` feature turns on the machine adapters. The guest worker is
built without it, so every machine call there is `Unsupported`
([`machine/mod.rs`](../../../../api/crates/layerfs-control/src/machine/mod.rs),
[`Cargo.toml`](../../../../api/crates/layerfs-control/Cargo.toml)). The
`postgresql` and `tls` features add PostgreSQL Stores and `https://`
buckets.

## Invariants

| Rule | Enforced by |
| --- | --- |
| A call's arguments are checked against the table before any effect | [`calls/check.rs`](../../../../api/crates/layerfs-control/src/calls/check.rs) |
| A folder scope checks call arguments, not processes: `workspace.exec` is a `cd` prefix, not confinement; Commit and Layer ids and a Workspace's Branch are not confined ([open gaps](../call-table/table.md#scopes)) | [`calls/scope.rs`](../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| Every call opens (creates or converts) its home first | [`calls/dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs), [`common/home/migrate/open.rs`](../../../../api/crates/layerfs-control/src/common/home/migrate/open.rs) |
| LayerFS reads `LAYERFS_*` settings only and names no plugin; `HOME` and `XDG_CACHE_HOME` are read only as fallback folders | [`calls/dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs) (`LAYERFS_HOME`, else `~/.layerfs`), [`common/trace.rs`](../../../../api/crates/layerfs-control/src/common/trace.rs) (`LAYERFS_TRACE`), [`sandbox/deploy.rs`](../../../../api/crates/layerfs-control/src/sandbox/deploy.rs) (bundle cache) |
| Records never hold a PostgreSQL password: the URL is stored without it and the password as `env:NAME` or `file:/path`, resolved per call | [`store/connect.rs`](../../../../api/crates/layerfs-control/src/store/connect.rs) (`refuse_password`), [`common/secrets.rs`](../../../../api/crates/layerfs-control/src/common/secrets.rs) |
| Bucket records hold a key reference, never a secret | [`backup/record.rs`](../../../../api/crates/layerfs-control/src/backup/record.rs) |
| A record is replaced whole by one rename; files 0600, folders 0700 | [`common/home/records.rs`](../../../../api/crates/layerfs-control/src/common/home/records.rs) |
| At most one `op` intent per entity; a live owner refuses others `Busy`, a gone owner's uncertain intent `Unresolved` | [`common/op.rs`](../../../../api/crates/layerfs-control/src/common/op.rs) |
| One attempt per call; a lost worker answer is `Unknown`, never resent | [`common/place.rs`](../../../../api/crates/layerfs-control/src/common/place.rs), [`machine/worker/client.rs`](../../../../api/crates/layerfs-control/src/machine/worker/client.rs) |
| Nothing but `machine.start` starts a machine (#535 D11) | [`common/place.rs`](../../../../api/crates/layerfs-control/src/common/place.rs), [`machine/endpoint.rs`](../../../../api/crates/layerfs-control/src/machine/endpoint.rs) |
| Locks are taken home, start, store, sandbox, never the reverse; none is held across a worker call or a transfer | [`common/home/entity_lock.rs`](../../../../api/crates/layerfs-control/src/common/home/entity_lock.rs) |
| A guest never reads the home; a worker's root is never converted and has no id of its own | [`common/home/guest.rs`](../../../../api/crates/layerfs-control/src/common/home/guest.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Home layout this build writes | 3 (`LAYOUT_VERSION`); layout 2 and 1.1.0 homes converted once on first open | [`layout.rs`](../../../../api/crates/layerfs-control/src/common/home/layout.rs) |
| Home of a newer layout | `Unsupported` | [`layout.rs`](../../../../api/crates/layerfs-control/src/common/home/layout.rs) |
| Unreadable `layerfs.json`, or layout below 2 | `Failed` | [`layout.rs`](../../../../api/crates/layerfs-control/src/common/home/layout.rs) |
| Layout 2 home with anything live | `InUse`, `detail.related` | [`migrate/v2.rs`](../../../../api/crates/layerfs-control/src/common/home/migrate/v2.rs) |
| Relative `LAYERFS_HOME` | `InvalidInput` | [`calls/dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs) |
| Home lock, Store lock, Sandbox lock wait | 120 s, then `Unavailable` (a readiness wait, not a retry) | [`lock.rs`](../../../../api/crates/layerfs-control/src/common/home/lock.rs), [`entity_lock.rs`](../../../../api/crates/layerfs-control/src/common/home/entity_lock.rs) |
| Machine `start.lock` wait | 600 s | [`machine/endpoint.rs`](../../../../api/crates/layerfs-control/src/machine/endpoint.rs) |
| Removing, stopping or changing a machine that holds Stores or Sandboxes | `InUse`, `detail.related` | [`inventory.rs`](../../../../api/crates/layerfs-control/src/inventory.rs) |
| Push, pull, remove or disconnect a Store with a live lease or a recorded Sandbox | `InUse` | [`common/home/lease.rs`](../../../../api/crates/layerfs-control/src/common/home/lease.rs), [`store/family.rs`](../../../../api/crates/layerfs-control/src/store/family.rs) |
| An operation runs on the entity | `Busy`; after a gone owner's uncertain outcome, `Unresolved` | [`common/op.rs`](../../../../api/crates/layerfs-control/src/common/op.rs) |
| An open Store unused | closed after 60 s (`IDLE`) by a reaper thread | [`store/open.rs`](../../../../api/crates/layerfs-control/src/store/open.rs) |

The full code list is in [Errors](../call-table/errors.md#error-kinds).

## Demonstration

Commands are written from source and were not run while writing this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`home_layout.rs`](../../../../api/crates/layerfs-control/tests/home_layout.rs) | `layerfs-control` | A missing home is `Empty` and `detect` writes nothing; `ensure` writes layout 3 and a 32-hex id; a 1.1.0 home is `Old` and not written by `ensure`; a newer or broken layout is refused. |
| [`typed_records.rs`](../../../../api/crates/layerfs-control/tests/typed_records.rs) | `layerfs-control` | Each typed record's `to_sdk` keeps its result keys byte for byte, shows no secrets, reads past `"v": 1`; Store and Sandbox states (busy, unresolved, down, idle, in use) are derived from the record and its intent. |
| [`sandbox_operations.rs`](../../../../api/crates/layerfs-control/tests/sandbox_operations.rs) | `layerfs-control` | `SandboxApi::create_provider_file` against a scripted Docker Engine peer (no real Docker): a matching project keeps one Hello and does not reinstall; a mismatched one is refused before any deployment or control effect. |

```text
cargo +1.85.1 test --locked -p layerfs-control --test home_layout
cargo +1.85.1 test --locked -p layerfs-control --test typed_records
cargo +1.85.1 test --locked -p layerfs-control --test sandbox_operations
```

All three run on a Unix host with no Docker, root or environment
variable. `sandbox_operations.rs` is `#![cfg(unix)]` and includes the
Engine peer from
[`layerfs-sandbox/tests/support/engine.rs`](../../../../connect/crates/layerfs-sandbox/tests/support/engine.rs).
Part of `typed_records.rs` is `#[cfg(feature = "machine")]`; add
`--features machine` to run it. Related tests in the same crate:
[`op_intent.rs`](../../../../api/crates/layerfs-control/tests/op_intent.rs)
(the intent and entity lock) and
[`doctor.rs`](../../../../api/crates/layerfs-control/tests/doctor.rs).

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`docs/versioned/2.0.0/home.md`](../../../versioned/2.0.0/home.md): the user's view of the home; its layout, locks, leases and logs are covered in [Home and records](home-and-records.md).

---

← [API](../README.md) · [Up: API](../README.md) · [Control: Home and records](home-and-records.md) →
