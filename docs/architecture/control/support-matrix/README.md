# Support matrix

[Architecture](../../README.md) › [Control](../README.md) › Support matrix

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

One table decides every platform × host × Store-kind combination, and `SUPPORT.json` is generated from it.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-platform` |
| Source | [`control/crates/layerfs-platform/src/support.rs`](../../../../control/crates/layerfs-platform/src/support.rs), [`control/SUPPORT.json`](../../../../control/SUPPORT.json), [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) (the profiles it reads) |
| Uses | — |
| Used by | [Machines](../machines/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Table](table.md) | The support rows and what each field means. |
| [Generated JSON](generated-json.md) | SUPPORT.json and the test that keeps it in step. |

## The picture

```text
 platforms/<p>/profile.rs        support.rs (the one table)
 +--------------------+      +------------------------------------+
 | PROFILE            |      | host_refusal(platform, host)       |
 |  how, hosts        |----->| postgresql_refusal(platform, host) |
 |  backend(host)     |      | sandbox_allowed(p, host, store)    |
 |  kernel(host)      |      | support(p, host)   -> SupportRow   |
 |  executor(host)    |      | platforms(host)    -> Vec<row>     |
 |  options, descr.   |      | support_json()     -> String       |
 +--------------------+      +-----------------+------------------+
                                               |
        +-------------------+------------------+-----------------+
        |                   |                                    |
        v                   v                                    v
 control/SUPPORT.json  machine.platforms,             machine.create and
 (3 hosts x 7 rows;    machine.list/get `supports`    sandbox.create refuse
  tests compare it)    (layerfs-control)              in the row's words
```

The profiles hold the per-platform facts; `support.rs` turns them into one
row per platform and host and adds the refusal reasons. Everything else
reads the rows.

## What it does

| Question | Answered by | Values | Source |
| --- | --- | --- | --- |
| Is the platform on this host? | `host_refusal` | `None`, or the fixed refusal text | [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| Which Sandbox backend runs there? | `SupportRow::backend` | `docker`, `native`, `namespaces` or none | [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| Which kernel route do its daemons serve? | `SupportRow::kernel` | `fuse`, `nfs` or none | same |
| Where do a sqlite Store's calls run? | `SupportRow::sqlite` | `in_process`, `worker` or none | same |
| Where do a PostgreSQL Store's calls run? | `PostgresqlSupport::store_calls` | always `in_process` (#533 G2) | same |
| May a Sandbox of a Store kind run there? | `sandbox_allowed` | `Ok(())` or `Err(reason)` | same |
| What does one machine support? | `SupportRow::supports` | `{ sqlite, postgresql_sandbox, reason }` | same |

`machine.platforms` returns `platforms(HostOs::current())` with three live
fields added from `probe`: `ready`, `missing` and `version`
([`platforms.rs`](../../../../control/crates/layerfs-machine/src/platforms.rs)).
`machine.list` and `machine.get` give each machine its row's `supports`.
`machine create` and `sandbox create` call `sandbox_allowed` and refuse
`Unsupported` with its reason
([`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs),
[`sandbox/create.rs`](../../../../control/crates/layerfs-control/src/sandbox/create.rs)).

## Invariants

| Rule | Enforced by |
| --- | --- |
| Every refusal of a platform, host and Store kind comes from this one table. | `sandbox_allowed` is the only check the control calls make; [`tests/support.rs`](../../../../control/crates/layerfs-platform/tests/support.rs) `every_refusal_is_the_rows_reason` |
| `SUPPORT.json` is never edited by hand. | `support_json_is_the_rendered_table` compares the committed file byte for byte with `support_json()` |
| A row follows its profile: backend, kernel and sqlite site only on a host in `hosts`. | `support()`; [`conformance.rs`](../../../../control/crates/layerfs-platform/tests/conformance.rs) `each_profile_is_its_support_rows_on_every_host` |
| A backend serves exactly one kernel route, and can mount it. | `conformance.rs`: `backend.capabilities().mount_driver` matches the row's `kernel` |
| A refused sqlite Sandbox gives the same reason as a refused PostgreSQL one. | `conformance.rs` `each_refusal_comes_from_the_table_and_postgresql_needs_sqlite` |
| PostgreSQL Store calls always run in the caller's process. | `support()` sets `store_calls: CallSite::InProcess`; checked in both tests |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| A host other than macOS, Linux or Windows | "LayerFS names no host other than macOS, Linux and Windows" | `OTHER_HOST` in [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| `native` on Windows | "no Sandbox backend on Windows (O5)"; sqlite calls still route to a `worker` that is not designed | `NO_BACKEND`; [`native/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/native/profile.rs) |
| `docker` on Windows | "docker through WSL2 is not designed yet" | `host_refusal` |
| `apple-container`, `lima`, `firecracker` off macOS | "… runs on macOS hosts only" (firecracker adds ", in a nested lima machine") | `host_refusal` |
| A combination enabled without live evidence | none today: the G7 live receipts enabled PostgreSQL Sandboxes on the VM platforms ([`g7-live`](../../../../core/docs/issues/535/receipts/g7-live/RECEIPT.md)) | #535 G7, D13 |
| The crate builds only on Unix | `#![cfg(unix)]`: the Windows rows are table data, not a built or tested host | [`lib.rs`](../../../../control/crates/layerfs-platform/src/lib.rs) |

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`support.rs`](../../../../control/crates/layerfs-platform/tests/support.rs) | `layerfs-platform` | The committed `SUPPORT.json` is the rendered table; every refusal is the row's reason; PostgreSQL Sandboxes run on every platform with a backend; rows carry the profile. |
| [`conformance.rs`](../../../../control/crates/layerfs-platform/tests/conformance.rs) | `layerfs-platform` | Over `Platform::ALL` and every host: profile against rows, refusals against the table, `SUPPORT.json` read back row by row. |

`support.rs` builds rows for every named host (and `HostOs::Other`) from
`support.rs` alone; it starts nothing and needs no tool. With
`LAYERFS_BLESS_SUPPORT=1` its first test writes the file instead of
comparing it (see [Generated JSON](generated-json.md)).

```text
cargo +1.85.1 test --locked -p layerfs-platform --test support
cargo +1.85.1 test --locked -p layerfs-platform --test conformance
```

Both files are `#![cfg(unix)]`: they run on macOS and Linux, with no Docker,
VM, ssh host or root. The commands were not run while writing this page.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`control/README.md`](../../../../control/README.md): its `SUPPORT.json`
  section is covered in [Generated JSON](generated-json.md) and
  [Table](table.md); the README stays the layer guide.

---

← [Control](../README.md) · [Up: Control](../README.md) · [Support matrix: Table](table.md) →
