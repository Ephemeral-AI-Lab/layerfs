# file.diff

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › diff

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Compares two files line by line; each side is a path at a Branch, `main` or a Commit, so a file can be compared with itself across history.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/diff.rs`](../../../../../api/crates/layerfs-api/src/file/diff.rs) |
| Implemented in | `layerfs_control::store::calls::run` → `file`, Store method `fs.diff_files` ([`methods/file/diff_files.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/diff_files.rs)); each side becomes `{ commit, path }`, `*_commit` winning over `*_branch` |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.diff(project, left_branch, left_path, right_branch, right_path,
                        { left_commit, right_commit });
```

```python
# Python
layerfs.file.diff(project, left_branch, left_path, right_branch, right_path,
                  left_commit=None, right_commit=None)
```

```bash
# CLI
layerfs file diff <store>/<project> <left_branch> <left_path> \
  <right_branch> <right_path> [--left-commit ID] [--right-commit ID]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs) |
| `left_branch`, `right_branch` | Name | a Branch or `main` per side | [`store/calls.rs`](../../../../../control/crates/layerfs-control/src/store/calls.rs) |
| `left_path` | Text | absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `right_path` | Text | absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `left_commit` | Name | none | a Commit or Layer id; replaces `left_branch` | [`store/calls.rs`](../../../../../control/crates/layerfs-control/src/store/calls.rs) |
| `right_commit` | Name | none | the same for the right side | same |

## Returns

```text
 { kind: "Equal" | "Text" | "Binary",
   left:  { path, snapshot, inode },
   right: { path, snapshot, inode },
   content_equal, same_inode, hunks_omitted, hunks: [ ... ] }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `kind` | text | `Equal` (same bytes), `Text` (line hunks), `Binary` (differs, not text, or over 1 MiB) | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `content_equal`, `same_inode` | bool | same bytes; same serial | same |
| `hunks`, `hunks_omitted` | list, bool | line hunks within 48 KiB, else omitted | same |

Whole-text comparison is bounded at 1 MiB per side (`MAXIMUM_TEXT_BYTES`).

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target  (NoSuchStore / NoSuchProject)
 lease::claim     (Store lock; op? Busy / Unresolved)
 store_call fs.diff_files   --------> snapshot: commit if given, else branch
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
| `NoSuchPath`, `IsDirectory` | a side's path is missing or is a folder (`detail.path`) | no | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`machine/worker/client.rs`](../../../../../control/crates/layerfs-control/src/machine/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const d = await layerfs.file.diff("shop/api", "main", "/src/app.ts", "agent", "/src/app.ts");
```

```python
layerfs.file.diff("shop/api", "agent", "/a.txt", "agent", "/a.txt", left_commit=older_id)
```

```bash
layerfs file diff shop/api main /src/app.ts agent /src/app.ts
```

## Related

- Whole trees: [branch.diff](../branch/diff.md), [commit.diff](../commit/diff.md).
- [Engine: reads, compare](../../../engine/store-calls/reads.md#compare).

---

← [file.blame](blame.md) · [Up: file](README.md) · [file.write](write.md) →
