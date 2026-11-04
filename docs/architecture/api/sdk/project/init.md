# project.init

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [project](README.md) › init

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`project.init` imports a folder on the caller's host as a new project of an
existing Store: the project's first Layer and its hidden first Branch.

| | |
| --- | --- |
| Family | [project](README.md) |
| Library entry | [`api/crates/layerfs-api/src/project/init.rs`](../../../../../api/crates/layerfs-api/src/project/init.rs) |
| Implemented in | `layerfs_control::project::family::run` → `init` ([`project/family.rs`](../../../../../api/crates/layerfs-control/src/project/family.rs)), then `store::identity::add_project` ([`store/identity.rs`](../../../../../api/crates/layerfs-control/src/store/identity.rs)) or `project::postgres_init::add_project` ([`project/postgres_init.rs`](../../../../../api/crates/layerfs-control/src/project/postgres_init.rs)) |
| Runs in | where the Store's calls run: the caller's process, or the Store's machine worker (sqlite on a non-local machine). No Sandbox daemon. |

## Signature

```js
// JavaScript
await layerfs.project.init(project, folder, { timeout, on_progress });
await client.project.init(project, folder, { timeout, on_progress }); // client = layerfs.client({ home })
```

```python
# Python
layerfs.project.init(project, folder, timeout=None, on_progress=None)
await layerfs.aio.project.init(project, folder)  # coroutine form
```

```bash
# CLI
layerfs project init <project> <folder> [--timeout <timeout>] [--progress]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<name>`; both parts 1–63 of `a-z 0-9 _ . -`, starting and ending with a letter or digit, not a Windows device name. The Store must have a record in the home (`store.create` or `store.connect`). | [`calls/check.rs`](../../../../../api/crates/layerfs-control/src/calls/check.rs), [`common/names.rs`](../../../../../api/crates/layerfs-control/src/common/names.rs) |
| `folder` | Text | an absolute path on the caller's host; read whole, with no Git-ignore filtering | [`project/family.rs`](../../../../../api/crates/layerfs-control/src/project/family.rs) `init` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `timeout` | Int | 3,600,000 (`INIT_TIMEOUT_MS`, one hour) | Init's deadline in ms, sent as `timeoutMs`; must be positive, else `InvalidInput`. Checked where the add runs (on the worker route, after the copy is sent). | [`project/family.rs`](../../../../../api/crates/layerfs-control/src/project/family.rs), [`project/init.rs`](../../../../../api/crates/layerfs-control/src/project/init.rs) `deadline` |
| `on_progress` | Function | none | receives `{ phase, done, total }`; the CLI's `--progress` prints the same as JSON lines on stderr | [`calls/table.rs`](../../../../../api/crates/layerfs-control/src/calls/table.rs) `PROGRESS`, [`store/family.rs`](../../../../../api/crates/layerfs-control/src/store/family.rs) `progress` |

Progress phases, in order: `check`, `send` (worker route only; `done` and
`total` in bytes), `install`, `record`. The other phases carry `null` counts.

## Returns

```text
 { project: "<store>/<name>",   store, name,
   kind:    "sqlite" | "postgresql",       (the Store's kind)
   machine: "<machine>",                   ("native" when none recorded)
   created: <ms since epoch> | null,
   layer:   "<hex>",                       (the project's first Layer)
   root:    "<hex>" }                      (its filesystem root)
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `project`, `store`, `name` | text | the project as named, and its two parts |
| `kind`, `machine` | text | from the Store's record (`ProjectRecord::to_sdk`) |
| `created` | int or null | when the home recorded the project |
| `layer` | hex | the first Branch's base Layer: the Layer Init published |
| `root` | hex | that Branch's effective root |

The summary is built in [`project/record.rs`](../../../../../api/crates/layerfs-control/src/project/record.rs);
`layer` and `root` come from the add's answer.

## Where it runs

```text
 caller's process                               machine worker
 ----------------                               --------------
 check args, Store record, record.unblocked()
 progress "check"
   |
   +-- PostgreSQL Store: Exists check, lease, progress "install"
   |     postgres_init::add_project  (live add into the schema)
   |
   +-- sqlite, Executor::Local: lease, progress "install"
   |     identity::add_project
   |       kernel_guard::check (Store's filesystem)
   |       make_entry, ProjectApi::add_project (live add)
   |
   +-- sqlite, Executor::Worker:
         Exists check, send::measure, send::room
         lease::claim
         project.stage x N  ---------------->  write staged parts
         progress "install"
         project.init  --------------------->  in_worker: own lease,
                                               identity::add_project
         <-------------- { stack, layer, root } staged copy removed
 progress "record", write project record, answer
```

| Store kind | Platform of the Store's machine | Runs in |
| --- | --- | --- |
| PostgreSQL | any | caller's process (needs a build with the `postgresql` feature) |
| sqlite | executes locally (`native` on Linux or macOS) | caller's process; the kernel guard admits the Store's directory only on Linux ext4, xfs or btrfs, or macOS apfs or hfs |
| sqlite | any other machine, and `native` on Windows | that machine's worker, on a copy of the folder |

Init is a live add, not a seal: `ProjectApi::add_project` opens the
existing Store through its admission and writer lane, refuses a taken
LayerStack name, imports the folder, then forks the first Branch, named
`main` in the catalog, from the new Layer
([`store/live.rs`](../../../../../engine/crates/layerfs-project/src/store/live.rs)).
Store calls hide that Branch: they treat `main` as the line of Layers (see
[branch family](../branch/README.md)). See the
[routing table](../../routing/routing-table.md) and the
[Project Init flow](../../../flows/init.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad `project`, relative `folder`, non-positive `timeout`; a source entry Init refuses (device, FIFO, socket, set-user-ID or sticky bits); worker route: the folder is missing or not a directory | no | [`project/family.rs`](../../../../../api/crates/layerfs-control/src/project/family.rs), [`project/send.rs`](../../../../../api/crates/layerfs-control/src/project/send.rs), [`store/identity.rs`](../../../../../api/crates/layerfs-control/src/store/identity.rs) `add_failure` |
| `NotFound` | no Store record of that name | no | `project/family.rs` |
| `Busy`, `Unresolved` | the Store's record holds another operation's intent; `Busy` also when another add's working rows remain in the Store | no | [`common/op.rs`](../../../../../api/crates/layerfs-control/src/common/op.rs), `add_failure` |
| `Exists` | the project's record, entry or LayerStack name exists | no | `project/family.rs`, `store/identity.rs` |
| `Capacity` | worker route: the machine's free disk is below the copy plus headroom | no | `project/send.rs` `room` |
| `Unavailable` | worker route: no worker recorded, or not reached; nothing sent | no | [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) |
| `Unsupported` | the kernel guard refuses the Store's filesystem; the Store has no acquisition tables; PostgreSQL in a build without `postgresql` | no | `store/identity.rs`, [`kernel_guard.rs`](../../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| `NoStore` | no Store file at the Store's path | no | `store/identity.rs` |
| `Failed` | deadline passed, read error, or any engine failure, **including an Init whose outcome is unknown** | **may be** | `store/identity.rs` `add_failure` |
| `Unknown` | **worker route: a stage or the `project.init` answer was lost** | **yes** | [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs) |

**An uncertain Init in the add reaches the caller as `Failed`, not
`Unknown`:** `add_failure` maps only `Exists`, `NoAcquisition`, `Busy` and a
refused source entry to their own codes; every other `InitError` becomes
`Failed`, and the entry this call made is kept. That entry makes
`project.list` show the project with no `created`, while `project.get`
answers `NoSuchProject` (no record). A later add into the same Store is
`Busy` while the uncertain add's working rows remain
([seal and install: uncertain Init](../../../engine/project-init/seal-and-install.md#uncertain-init)).
After **`Unknown`** the home gains no record and no entry; the message
names the staged copy the worker removes when its Init ends. Nothing is
resent. `project.list` reads home records only, so it cannot tell whether
the Store holds the project; a second `project.init` of the same name is
refused `Exists` (or `Busy`) if the first add got that far, and changes
nothing. See [uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Examples

Not run; written from the call table.

```js
const p = await layerfs.project.init("shop/api", "/src/api", { timeout: 600000, on_progress: (e) => console.log(e.phase) });
```

```python
p = layerfs.project.init("shop/api", "/src/api", timeout=600000, on_progress=print)
```

```bash
layerfs project init shop/api /src/api --timeout 600000 --progress
```

## Related

- Before: `store.create` or `store.connect` ([store family](../store/README.md)).
- After: [`project.get`](get.md), [`branch.fork`](../branch/fork.md) from `main`, then `sandbox.create` and `workspace.mount`.
- [Project Init flow](../../../flows/init.md); [Engine: Project Init](../../../engine/project-init/README.md), [live open](../../../engine/project-init/seal-and-install.md#live-open).
- [Control: families](../../control/families.md#project); [Routing: workers](../../routing/workers.md).

---

← [SDK: project](README.md) · [Up: project](README.md) · [project.list](list.md) →
