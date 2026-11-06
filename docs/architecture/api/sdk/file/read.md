# file.read

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › read

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Reads lines of one text file at a Branch, `main` or a Commit, one bounded window at a time.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/read.rs`](../../../../../api/crates/layerfs-api/src/file/read.rs) |
| Implemented in | `layerfs_control::store::calls::run` → `file`, Store method `fs.read` ([`methods/file/read.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/read.rs)); `start_line`, `end_line` become `startLine`, `endLine` |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.read(project, branch, path, commit?, { start_line, end_line });
```

```python
# Python
layerfs.file.read(project, branch, path, commit=None, start_line=None, end_line=None)
```

```bash
# CLI
layerfs file read <store>/<project> <branch> <path> [<commit>] \
  [--start-line N] [--end-line N]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs) |
| `branch` | Name | a Branch or `main` (its head Layer); used when `commit` is absent | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `path` | Text | a regular text file; absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `commit` | Name, optional | a 66-hex Commit or Layer id; read at it instead of `branch` | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `start_line` | Int | 1 | a positive line number | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) `line` |
| `end_line` | Int | the end of the file | a positive line number; the window may stop earlier | same |

## Returns

```text
 { kind: "ok", snapshot, root, path,
   text, start_line, end_line | null, line_truncated, truncated,
   next_line | null, size }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `snapshot`, `root`, `path` | text | the header: snapshot label (e.g. `agent@<short id>`), its root, the path | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) `header` |
| `text` | text | the lines, joined, with their line ends | [`read/reads.rs`](../../../../../engine/crates/layerfs-store-service/src/read/reads.rs) `read` |
| `start_line`, `end_line` | int | the first and last line in `text` (`end_line` null when none) | same |
| `line_truncated` | bool | a long line was clipped | same |
| `truncated` | bool | clipped, or more lines follow | same |
| `next_line` | int or null | where the next window starts | same |
| `size` | int | the file's bytes, from Store metadata | same |

| Window bound | Value | Source |
| --- | --- | --- |
| lines per answer | 2000 (`LINES`) | [`read/window.rs`](../../../../../engine/crates/layerfs-store-service/src/read/window.rs) |
| bytes per answer | 48 KiB (`REPLY_BUDGET`) | same |
| binary check | a NUL or invalid UTF-8 in a 16 KiB window is `Binary` | same |

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target  (NoSuchStore / NoSuchProject)
 lease::claim     (Store lock; op? Busy / Unresolved)
 store_call fs.read         --------> snapshot: commit if given, else branch
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
| `NoSuchPath`, `NotFile`, `IsDirectory` | the path is missing or not a regular file | no | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `Binary` | the file is not text | no | [`read/window.rs`](../../../../../engine/crates/layerfs-store-service/src/read/window.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

To read a whole file, call again with `start_line: next_line` until `next_line` is null.

## Examples

Not run; written from the call table.

```js
const r = await layerfs.file.read("shop/api", "agent", "/src/index.ts", { start_line: 1, end_line: 80 });
console.log(r.text, r.next_line);
```

```python
r = layerfs.file.read("shop/api", "main", "/README.md")
print(r["text"])
```

```bash
layerfs file read shop/api agent /src/index.ts --start-line 1 --end-line 80
```

## Related

- Before: [file.list](list.md), [file.glob](glob.md), [file.stat](stat.md). After: [file.edit](edit.md), [file.write](write.md).
- [Engine: reads](../../../engine/store-calls/reads.md), [SDK facade](../../../engine/store-calls/sdk-facade.md).

---

← [SDK: file](README.md) · [Up: file](README.md) · [file.stat](stat.md) →
