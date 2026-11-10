# project family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › project

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `project` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/project`](../../../../../api/crates/layerfs-api/src/project), [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) |

## Calls

```text
 store.create | store.connect          (a Store must exist first)
        |
        v
 init <store>/<project> <folder> --> main's first Layer
        |
        +--> list [store]   get <store>/<project>   (home records only)
        |
        +--> export <store>/<project> <destination> [--branch <name>]
        |
        +--> branch.*, commit.*, file.*, sandbox.create ...
        |
        v
 remove <store>/<project>   (no Sandbox of it may run)
```

Five calls, in help order
([`project/mod.rs`](../../../../../api/crates/layerfs-api/src/project/mod.rs)).
Every entry runs its own function of `layerfs_store::project::family`;
`project.remove` runs `layerfs_control::project::remove`. Result keys are
snake_case after the dispatcher's
[`keys::snake`](../../../../../api/crates/layerfs-api/src/calls/keys.rs).

| Call | Arguments | Options | Returns | Where it runs |
| --- | --- | --- | --- | --- |
| [`init`](init.md) | `project` (Project), `folder` (Text) | `timeout` (Int), `on_progress` (Function) | the project summary plus `layer` and `root` of the new project | where the Store's calls run |
| [`list`](list.md) | `store?` (Name) | `page_size`, `page_pos` (Int) | `{ items: [summary], next_page_pos }` | caller's process, home records only |
| [`get`](get.md) | `project` (Project) | — | the project summary | caller's process, home records only |
| [`export`](export.md) | `project` (Project), `destination` (Text) | `branch` (Name, default `main`), `on_progress` (Function) | pinned snapshot identity and local output counts | reader at the Store, writer on the caller’s host |
| [`remove`](remove.md) | `project` (Project) | — | `{ stack, packs_kept: true, project }` | caller's process, or `project.remove` in the Store's worker |

The summary is `ProjectRecord::to_sdk`
([`project/record.rs`](../../../../../control/crates/layerfs-store/src/project/record.rs)):
`{ project: "<store>/<name>", store, name, kind, machine, created }`.
`kind` and `machine` come from the Store's record; `machine` defaults to
`native`.

## Arguments and options

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`, checked by `names::project` before dispatch | [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs) |
| `folder` | Text | an absolute path on the caller's host | [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) |
| `timeout` | Int | Init time limit in ms, sent as `timeoutMs`; default `INIT_TIMEOUT_MS` = 3,600,000 (one hour) | [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) |
| `on_progress` | Function | the shared `PROGRESS` option; the front end strips it, so it never reaches the library | [`calls/table.rs`](../../../../../api/crates/layerfs-api/src/calls/table.rs) |
| `page_size`, `page_pos` | Int | the shared `PAGE` set; `list` clamps `page_size` to 1..=1000 (default 50) and `page_pos` to 0 or more | [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) |

`init` reports `{ progress: { phase, done, total } }` with the phases
`check`, `send` (bytes, only when the folder is copied to a worker),
`install` and `record`. [`export`](export.md) reports `prepare`,
`export`, `finalize`, with monotonic local output counters and `total: null`;
it accepts no `commit` selector.

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | no `/` in `project`, a bad project name, a relative `folder` | [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) |
| `NotFound` | `init` or `list` names a Store with no record | [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) |
| `NoSuchStore`, `NoSuchProject` | `get` or `remove` of a missing Store or project | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `Exists` | `init` of a project the home already records | [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) |
| `InUse` | `remove` while a Sandbox of the project runs (`detail.related`) | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) |
| `Busy`, `Unresolved` | the Store's record holds another operation's `op` | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Capacity` | the worker's machine has no room for the folder copy | [`project/send.rs`](../../../../../control/crates/layerfs-store/src/project/send.rs) |
| `Unsupported` | `remove` on a PostgreSQL Store ("not built") | [`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs) |
| `Unavailable` | the Store's machine records no worker, or the worker does not answer | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Unknown` | **uncertain:** the worker's answer to `project.init` or `project.remove` was lost | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

After an `Unknown` `init` the home writes no project record, so
`project list` does not show the project, though the Store may hold it.
The message names the staged copy the worker removes when its Init ends.
See [error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Where it runs

```text
 caller's process                                 machine's worker
 ----------------                                 ----------------
 list, get ........ home records only
 init  --+-- PostgreSQL Store: postgres_init here
         +-- sqlite, Executor::Local: identity::add_project here
         +-- sqlite, Executor::Worker:
               measure folder, room check
               send copy  (project.stage) ------->  write staged parts
               hold lease (project.init)  ------->  add the copy, drop it
               write project record  <-----------  { stack, layer, root }
 remove -+-- PostgreSQL Store: Unsupported
         +-- Executor::Local: identity::remove_project here
         +-- Executor::Worker: project.remove --->  drop the LayerStack
```

`place::plan` picks the route from the Store's record
([`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs)).
A PostgreSQL Store runs in the caller's process. A sqlite Store runs in the
caller's process when its machine's platform executes locally on this host
(`native`, except on Windows), else in that machine's worker. The Init folder is
always read on the caller's host. Export's destination is also always local
to the caller; SQLite workers stream the pinned tree back to that host.
The worker operations are in
[`worker_server.rs`](../../../../../control/crates/layerfs-control/src/worker_server.rs).
See the [routing table](../../../control/routing/routing-table.md).

## Example

Not run; written from the call table.

```js
await layerfs.project.init("shop/api", "/src/api", { timeout: 600000, on_progress: console.log });
const page = await layerfs.project.list("shop", { page_size: 20 });
```

```python
layerfs.project.init("shop/api", "/src/api", timeout=600000, on_progress=print)
page = layerfs.project.list("shop", page_size=20)
```

```console
$ layerfs project init shop/api /src/api --timeout 600000 --progress
$ layerfs project list shop --page-size 20
```

## Related

- [Table](../../call-table/table.md): entries, kinds and checks.
- [Control: Families](../../../control/management/families.md): the `project` implementation.
- [Routing table](../../../control/routing/routing-table.md) and [Workers](../../../control/routing/workers.md).
- [Engine: Project Init](../../../engine/project-init/README.md) and the [Project Init flow](../../../flows/init.md).

---

← [API: SDK](../README.md) · [Up: SDK](../README.md) · [project.init](init.md) →

- [stats](stats.md): namespace counts of one pinned saved tree.
