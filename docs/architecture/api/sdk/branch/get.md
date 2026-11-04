# branch.get

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [branch](README.md) › get

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Shows one Branch (or `main`): its base Layer and its head Commit.

| | |
| --- | --- |
| Family | [branch](README.md) |
| Library entry | [`api/crates/layerfs-api/src/branch/get.rs`](../../../../../api/crates/layerfs-api/src/branch/get.rs) |
| Implemented in | `layerfs_control::store::calls::run` → Store method `project.list_branches` with `name` ([`store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs)) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.branch.get(project, branch);
```

```python
# Python
layerfs.branch.get(project, branch)
```

```bash
# CLI
layerfs branch get <store>/<project> <branch>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>` | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `branch` | Name | a Branch name, or `main`; sent as `name` | [`store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs) |

## Options

None.

## Returns

One item of [branch.list](list.md#returns):

```text
 Branch:  { name, id, base, head | null, forked_from | null }
 main:    { name: "main", id: null, base: <head Layer>, head: null }
```

The Store looks the name up in its name index, never by listing every
Branch; control then answers the item whose `name` (or `id`) matches
([`store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs) `first`).

## Where it runs

```text
 caller process                                   Store's machine worker
 resolve::target; lease::claim
 PostgreSQL / sqlite on native: open::run here
 sqlite elsewhere: store.call ---------------->   project.list_branches {name}
 first(page, "NoSuchBranch") over the answer
```

A read; no lane. See the [routing table](../../routing/routing-table.md).
Under a folder scope `branch` is composed as `<ns>.<branch>`.

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad project or name; out of scope | no | [`calls/check.rs`](../../../../../api/crates/layerfs-control/src/calls/check.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project is not recorded | no | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `NoSuchBranch` | no Branch of that name | no | [`store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs) |
| `Busy`, `Unresolved` | a push, pull or remove holds the Store | no | [`store/record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) |
| `Unavailable` | the worker or server does not answer | no | [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs) |

`branch.get` is the read to make after an uncertain
[fork](fork.md), [merge](merge.md) or [publish](publish.md).

## Examples

Not run; written from the call table.

```js
const b = await layerfs.branch.get("shop/api", "agent");
```

```python
head = layerfs.branch.get("shop/api", "agent")["head"]
```

```bash
layerfs branch get shop/api main
```

## Related

- Before: [branch.list](list.md). After: [commit.list](../commit/list.md),
  [workspace.mount](../workspace/mount.md).
- [Engine: Store calls, branches](../../../engine/store-calls/branches.md#lookup).

---

← [branch.list](list.md) · [Up: branch](README.md) · [branch.fork](fork.md) →
