# file.glob

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › glob

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists the file paths under a folder that match a glob, at a Branch, `main` or a Commit.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/glob.rs`](../../../../../api/crates/layerfs-api/src/file/glob.rs) |
| Implemented in | `layerfs_store::calls::file_glob` → `file`, Store method `fs.glob` ([`methods/file/glob.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/glob.rs)) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.glob(project, branch, path, pattern, commit?, { page_size, page_pos });
```

```python
# Python
layerfs.file.glob(project, branch, path, pattern, commit=None, page_size=None, page_pos=None)
```

```bash
# CLI
layerfs file glob <store>/<project> <branch> <path> <pattern> [<commit>] \
  [--page-size N] [--page-pos N]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `branch` | Name | a Branch or `main` (its head Layer); used when `commit` is absent | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `path` | Text | the folder to search under (`/` for all); absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `pattern` | Text | a glob in `/.lww` rule syntax, matched against whole paths (e.g. `src/**/*.ts`) | [`methods/file/glob.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/glob.rs) |
| `commit` | Name, optional | a 66-hex Commit or Layer id; read at it instead of `branch` | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |

The call table requires `path`; the Store method treats it as optional
(`optional_path`, `/` when absent), so only the table makes it required
([`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs)).

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `page_size` | Int | 50 | 1..=1000 | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `page_pos` | Int | 0 | the last page's `next_page_pos`; earlier items are skipped by walking | same |

## Returns

```text
 { kind: "ok", snapshot, root, path,
   items: [ "<path>", ... ],  (depth-first, name order)
   has_more, next_page_pos | null }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `items` | list of text | matching paths | [`read/reads.rs`](../../../../../engine/crates/layerfs-store-service/src/read/reads.rs) `glob` |
| `has_more`, `next_page_pos` | bool, int or null | paging; a page also ends at 48 KiB | same |

Only folders that agree with the pattern's literal leading folders are
entered (`walk::leading`: `a/b/**/*.js` enters only `a/` and `a/b/`)
([`read/walk.rs`](../../../../../engine/crates/layerfs-store-service/src/read/walk.rs)).

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target  (NoSuchStore / NoSuchProject)
 lease::claim     (Store lock; op? Busy / Unresolved)
 store_call fs.glob         --------> snapshot: commit if given, else branch
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
| `InvalidInput` | a bad project, path or option; a path outside a folder scope (`out of scope: path`) | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project is not recorded | no | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | the Branch, Commit or Layer is missing | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Busy`, `Unresolved` | a push, pull or remove holds the Store | no | [`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer; an unreachable PostgreSQL server | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `NoSuchPath`, `NotDirectory` | `path` is missing or not a folder | no | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const { items } = await layerfs.file.glob("shop/api", "agent", "/", "src/**/*.ts");
```

```python
layerfs.file.glob("shop/api", "main", "/docs", "**/*.md")["items"]
```

```bash
layerfs file glob shop/api agent / 'src/**/*.ts'
```

## Related

- After: [file.read](read.md), [file.grep](grep.md).
- [Engine: reads](../../../engine/store-calls/reads.md), [SDK facade](../../../engine/store-calls/sdk-facade.md).

---

← [file.list](list.md) · [Up: file](README.md) · [file.grep](grep.md) →
