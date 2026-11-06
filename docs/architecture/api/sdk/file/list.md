# file.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists one folder's entries at a Branch, `main` or a Commit, one page at a time.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/list.rs`](../../../../../api/crates/layerfs-api/src/file/list.rs) |
| Implemented in | `layerfs_control::store::calls::run` → `file`, Store method `fs.list` ([`methods/file/list.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/list.rs)); `page_size`, `page_pos` become `pageSize`, `pagePos` |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.list(project, branch, path, commit?, { page_size, page_pos });
```

```python
# Python
layerfs.file.list(project, branch, path, commit=None, page_size=None, page_pos=None)
```

```bash
# CLI
layerfs file list <store>/<project> <branch> <path> [<commit>] \
  [--page-size N] [--page-pos N]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs) |
| `branch` | Name | a Branch or `main` (its head Layer); used when `commit` is absent | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `path` | Text | a folder; `/` is the project root; absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `commit` | Name, optional | a 66-hex Commit or Layer id; read at it instead of `branch` | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `page_size` | Int | 50 | 1..=1000 | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `page_pos` | Int | 0 | the last page's `next_page_pos`; earlier items are skipped by walking | same |

## Returns

```text
 { kind: "ok", snapshot, root, path,
   items: [ { name, path, type, inode } ],  (name order)
   has_more, next_page_pos | null }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name`, `path` | text | the entry's name and absolute path | [`read/reads.rs`](../../../../../engine/crates/layerfs-store-service/src/read/reads.rs) `list` |
| `type`, `inode` | text, int | its kind and serial | same |
| `has_more`, `next_page_pos` | bool, int or null | paging; a page also ends at 48 KiB | same |

Entries are read from the folder 64 at a time, so a page costs its size,
not the folder's.

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target  (NoSuchStore / NoSuchProject)
 lease::claim     (Store lock; op? Busy / Unresolved)
 store_call fs.list         --------> snapshot: commit if given, else branch
                                      ("main" = head Layer); then reads by
                                      immutable root only
```

A read on the Store's read-only session: it never takes the publication
lane and never sees a live Workspace. PostgreSQL Stores and sqlite Stores
on `native` run in this process; other sqlite Stores in their machine's
worker (`store.call`), with nothing started. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad project, path or option; a path outside a folder scope (`out of scope: path`) | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs), [`calls/scope.rs`](../../../../../control/crates/layerfs-control/src/calls/scope.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project is not recorded | no | [`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | the Branch, Commit or Layer is missing | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Busy`, `Unresolved` | a push, pull or remove holds the Store | no | [`store/call.rs`](../../../../../control/crates/layerfs-control/src/store/call.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer; an unreachable PostgreSQL server | no | [`common/place.rs`](../../../../../control/crates/layerfs-control/src/common/place.rs) |
| `NoSuchPath`, `NotDirectory` | the path is missing or not a folder | no | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const { items } = await layerfs.file.list("shop/api", "agent", "/src");
```

```python
for e in layerfs.file.list("shop/api", "main", "/", page_size=200)["items"]:
    print(e["type"], e["path"])
```

```bash
layerfs file list shop/api agent /src --page-size 200
```

## Related

- After: [file.read](read.md), [file.stat](stat.md); a whole tree with [file.glob](glob.md).
- [Engine: reads](../../../engine/store-calls/reads.md), [SDK facade](../../../engine/store-calls/sdk-facade.md).

---

← [file.stat](stat.md) · [Up: file](README.md) · [file.glob](glob.md) →
