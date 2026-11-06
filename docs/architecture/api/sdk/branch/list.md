# branch.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [branch](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists a project's Branches, `main` first, one page at a time.

| | |
| --- | --- |
| Family | [branch](README.md) |
| Library entry | [`api/crates/layerfs-api/src/branch/list.rs`](../../../../../api/crates/layerfs-api/src/branch/list.rs) |
| Implemented in | `layerfs_control::store::calls::run` → Store method `project.list_branches` ([`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs), [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs)) |
| Runs in | the caller's process for PostgreSQL and for sqlite on `native`; else the Store's machine worker (`store.call`) |

## Signature

```js
// JavaScript
await layerfs.branch.list(project, { page_size, page_pos });
await client.branch.list(project, { page_size, page_pos });
```

```python
# Python
layerfs.branch.list(project, page_size=None, page_pos=None)
```

```bash
# CLI
layerfs branch list <store>/<project> [--page-size N] [--page-pos N]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `page_size` | Int | 50 | 1..=1000; sent as `pageSize` | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `page_pos` | Int | 0 | the `next_page_pos` of the last page; sent as `pagePos` | same |

## Returns

```text
 { items: [
     { name: "main", id: null, base: <head Layer>, head: null },
     { name, id, base, head | null, forked_from | null },
     ...                                   (byte-name order)
   ],
   has_more, next_page_pos | null }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name` | text | the Branch name | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) `item`, `main_item` |
| `id` | text or null | the Branch id (hex); null for `main` | same |
| `base` | text | the Layer it is based on; for `main`, the project's head Layer | same |
| `head` | text or null | its newest Commit; null for none and for `main` | same |
| `forked_from` | text or null | the Commit or Layer it was forked from | same |
| `has_more`, `next_page_pos` | bool, int or null | paging | same |

A page also ends early at the 48 KiB reply budget. Keys arrive snake_case
([`calls/keys.rs`](../../../../../control/crates/layerfs-control/src/calls/keys.rs)).

## Where it runs

```text
 caller process                                   Store's machine worker
 --------------                                   ----------------------
 resolve::target    --> NoSuchStore / NoSuchProject
 lease::claim (Store lock; op? --> Busy / Unresolved)
 PostgreSQL, or sqlite on native:
   open::run project.list_branches   (here)
 sqlite on another machine:
   store.call { op, args, store_record } ----->  open::run on its file
```

A read: it never takes the publication lane. A stopped machine is
`Unavailable`; nothing is started. See the
[routing table](../../../control/routing/routing-table.md).

| Folder-scoped client | Behaviour | Source |
| --- | --- | --- |
| scope with `branch: <ns>` | only Branches named `<ns>.<x>`, shown as `<x>`; read 1000 at a time and paged here | [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs) `branches_in` |
| scope without `branch` | `InvalidInput` "out of scope: branch" | [`calls/scope.rs`](../../../../../control/crates/layerfs-control/src/calls/scope.rs) |

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad project; `page_size` outside 1..=1000; out of scope | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project is not recorded | no | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `Busy`, `Unresolved` | a push, pull or remove holds the Store | no | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) |
| `Unavailable` | the machine's worker is not recorded or does not answer; an unreachable PostgreSQL server | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Failed` | a scoped list made no progress | no | [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
let page = await layerfs.branch.list("shop/api");
while (page.has_more) page = await layerfs.branch.list("shop/api", { page_pos: page.next_page_pos });
```

```python
for b in layerfs.branch.list("shop/api", page_size=200)["items"]:
    print(b["name"], b["head"])
```

```bash
layerfs branch list shop/api
```

## Related

- After: [branch.get](get.md), [branch.fork](fork.md),
  [commit.list](../commit/list.md).
- [Engine: Store calls, branches](../../../engine/store-calls/branches.md),
  [Core: History model](../../../core/history-model/README.md).

---

← [SDK: branch](README.md) · [Up: branch](README.md) · [branch.get](get.md) →
