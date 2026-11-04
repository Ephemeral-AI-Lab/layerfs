# file.grep

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › grep

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Finds the lines under a folder that contain a literal text, at a Branch, `main` or a Commit.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/grep.rs`](../../../../../api/crates/layerfs-api/src/file/grep.rs) |
| Implemented in | `layerfs_control::store::calls::run` → `file`, Store method `fs.grep` ([`methods/file/grep.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/grep.rs)) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.grep(project, branch, path, pattern, commit?, { page_size, page_pos });
```

```python
# Python
layerfs.file.grep(project, branch, path, pattern, commit=None, page_size=None, page_pos=None)
```

```bash
# CLI
layerfs file grep <store>/<project> <branch> <path> <pattern> [<commit>] \
  [--page-size N] [--page-pos N]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `branch` | Name | a Branch or `main` (its head Layer); used when `commit` is absent | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `path` | Text | the folder (or file) to search under; absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `pattern` | Text | a nonempty **literal** text; a line feed anywhere but last matches nothing | [`methods/file/grep.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/grep.rs), [`read/search.rs`](../../../../../engine/crates/layerfs-store-service/src/read/search.rs) |
| `commit` | Name, optional | a 66-hex Commit or Layer id; read at it instead of `branch` | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |

The call table's help line says "matching a regular expression", but the
Store method searches for the pattern as a literal substring; no regular
expression syntax is interpreted. As with [file.glob](glob.md), the table
requires `path` while the Store treats it as optional (`/`).

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `page_size` | Int | 50 | 1..=1000 | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `page_pos` | Int | 0 | the last page's `next_page_pos`; earlier items are skipped by walking | same |

## Returns

```text
 { kind: "ok", snapshot, root, path,
   items: [ { path, line, text, line_truncated } ],
   skipped, has_more, next_page_pos | null }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `path`, `line` | text, int | the file and the 1-based line number | [`read/reads.rs`](../../../../../engine/crates/layerfs-store-service/src/read/reads.rs) `grep` |
| `text`, `line_truncated` | text, bool | the line, clipped when long | same |
| `skipped` | int | binary files skipped | same |
| `has_more`, `next_page_pos` | bool, int or null | paging; a page also ends at 48 KiB | same |

Files are read in batches of 64 (at most 4 MiB of roots each), and a
content already scanned in the call is answered from its kept lines.

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target  (NoSuchStore / NoSuchProject)
 lease::claim     (Store lock; op? Busy / Unresolved)
 store_call fs.grep         --------> snapshot: commit if given, else branch
                                      ("main" = head Layer); then reads by
                                      immutable root only
```

A read on the Store's read-only session: it never takes the publication
lane and never sees a live Workspace. PostgreSQL Stores and sqlite Stores
on `native` run in this process; other sqlite Stores in their machine's
worker (`store.call`), with nothing started. See the
[routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | an empty pattern; a bad project, path or option; a path outside a folder scope (`out of scope: path`) | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project is not recorded | no | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | the Branch, Commit or Layer is missing | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Busy`, `Unresolved` | a push, pull or remove holds the Store | no | [`store/call.rs`](../../../../../api/crates/layerfs-control/src/store/call.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer; an unreachable PostgreSQL server | no | [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) |
| `NoSuchPath` | `path` is missing | no | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const { items } = await layerfs.file.grep("shop/api", "agent", "/src", "TODO");
```

```python
for m in layerfs.file.grep("shop/api", "main", "/", "import fs")["items"]:
    print(f'{m["path"]}:{m["line"]}: {m["text"]}')
```

```bash
layerfs file grep shop/api agent /src TODO
```

## Related

- Before: [file.glob](glob.md). After: [file.read](read.md) at the line, [file.edit](edit.md).
- [Engine: reads](../../../engine/store-calls/reads.md), [SDK facade](../../../engine/store-calls/sdk-facade.md).

---

← [file.glob](glob.md) · [Up: file](README.md) · [file.blame](blame.md) →
