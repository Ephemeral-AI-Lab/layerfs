# project.remove

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [project](README.md) › remove

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`project.remove` drops one project's LayerStack (its Layers, Branches and
Commits) from its Store in one write, then the project's home entry; the
Store's other projects stay.

| | |
| --- | --- |
| Family | [project](README.md) |
| Library entry | [`project/remove.rs`](../../../../../api/crates/layerfs-api/src/project/remove.rs) |
| Implemented in | `layerfs_control::project::remove` → `remove` ([`project/family.rs`](../../../../../control/crates/layerfs-store/src/project/family.rs)), then `store::identity::remove_project` ([`identity.rs`](../../../../../control/crates/layerfs-store/src/identity.rs)) |
| Runs in | the caller's process, or the worker op `project.remove` on the Store's machine (sqlite on a non-local machine). Never a Sandbox daemon. |

## Signature

```js
// JavaScript
await layerfs.project.remove(project);
await client.project.remove(project);
```

```python
# Python
layerfs.project.remove(project)
```

```bash
# CLI
layerfs project remove <project>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<name>`; Store and project records must exist; no Sandbox of the project may be recorded | [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs), [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_remove_project` |

## Options

None.

## Returns

```text
 { stack: "<hex>", packs_kept: true, project: "<store>/<name>" }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `stack` | hex | the removed LayerStack's id |
| `packs_kept` | bool | always `true`: the project's packs and objects stay in the Store; reclaiming them is not built (A8, #411) |
| `project` | text | the project as named |

## Where it runs

```text
 caller's process                                  machine worker
 ----------------                                  --------------
 resolve::target, record.unblocked()
 can_remove_project (no Sandbox recorded)
 PostgreSQL Store --> Unsupported
 place::plan
   +-- Executor::Local: lease::hold
   |     remove_project: open Store, find stack by name,
   |       history().remove_layerstack(stack)  (one write)
   +-- Executor::Worker: lease::hold
         project.remove ------------------------>  remove_project on
                                                    the worker's Store
         <------------------------------ { stack, packsKept }
 remove store/<store>/project/<name>/ from the home
```

| Store kind | Machine | Runs in |
| --- | --- | --- |
| PostgreSQL | any | refused `Unsupported` ("not built"); nothing changes |
| sqlite | executes locally (`native` on Linux or macOS) | caller's process, under its lease |
| sqlite | any other machine, and `native` on Windows | that machine's worker; the caller holds the lease meanwhile |

The Store-side work is in
[`identity.rs`](../../../../../control/crates/layerfs-store/src/identity.rs)
`remove_project`; the worker op is in
[`worker_server.rs`](../../../../../control/crates/layerfs-control/src/worker_server.rs).
The lookup and the removal both go through the Store's open service, so
other daemons and SDK processes keep reading meanwhile. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | `project` is not `<store>/<name>` with valid names | no | [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs) |
| `NoSuchStore`, `NoSuchProject` | a record is missing; `NoSuchProject` also when the Store holds no LayerStack of that name | no | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs), `store/identity.rs` |
| `InUse` | a Sandbox of the project is recorded; `detail.related` lists each `{ kind: "sandbox", name }` | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) |
| `Busy`, `Unresolved` | the Store's record holds another operation's intent | no | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Unsupported` | a PostgreSQL Store; or the kernel guard refuses the Store's filesystem when the Store is opened | no | `project/family.rs`, [`service.rs`](../../../../../control/crates/layerfs-store/src/service.rs) |
| `Unavailable` | worker route: no worker recorded, or not reached | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Failed` | **the Store write failed, its outcome included**: `remove_layerstack` errors are all `Failed` | **may be** | `store/identity.rs` `remove_project` |
| `Unknown` | **worker route: the answer was lost** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

After **`Failed` or `Unknown`** the caller's home entry is kept (it is
removed only after success), so `project.get` still answers. Read the
project with [`branch.list`](../branch/list.md) before acting again:
`NoSuchProject` from the Store means the stack is gone. LayerFS does not
resend. See [uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Examples

Not run; written from the call table.

```js
await layerfs.project.remove("shop/api");
```

```python
layerfs.project.remove("shop/api")
```

```bash
layerfs project remove shop/api
```

## Related

- Before: close the project's Sandboxes (`sandbox.close`); [`project.get`](get.md).
- After: [`project.list`](list.md); `store.remove` to drop the whole Store.
- [Store: history publication, removal](../../../engine/store/history-publication.md#removal); [Control: families](../../../control/management/families.md#project).

---

← [project.export](export.md) · [Up: project](README.md) · [SDK: branch](../branch/README.md) →
