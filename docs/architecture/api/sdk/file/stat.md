# file.stat

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › stat

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Shows one path's kind, inode serial, size and mode at a Branch, `main` or a Commit.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/stat.rs`](../../../../../api/crates/layerfs-api/src/file/stat.rs) |
| Implemented in | `layerfs_store::calls::file::stat`, Store method `fs.stat` ([`methods/file/stat.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/stat.rs)) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.stat(project, branch, path, commit?);
```

```python
# Python
layerfs.file.stat(project, branch, path, commit=None)
```

```bash
# CLI
layerfs file stat <store>/<project> <branch> <path> [<commit>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `branch` | Name | a Branch or `main` (its head Layer); used when `commit` is absent | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `path` | Text | any kind of entry; absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `commit` | Name, optional | a 66-hex Commit or Layer id; read at it instead of `branch` | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |

## Options

None.

## Returns

```text
 { kind: "ok", snapshot, root, path, type, inode, size, mode }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `type` | text | the entry kind (file, directory, symlink, …) | [`read/reads.rs`](../../../../../engine/crates/layerfs-store-service/src/read/reads.rs) `stat` |
| `inode` | int | its stable serial in the project | same |
| `size` | int | bytes, from Store metadata | same |
| `mode` | int | permission bits and kind, as stored | same |

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target  (NoSuchStore / NoSuchProject)
 lease::claim     (Store lock; op? Busy / Unresolved)
 store_call fs.stat         --------> snapshot: commit if given, else branch
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
| `NoSuchPath`, `NotDirectory` | the path or a parent is missing, or a parent is not a folder | no | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const s = await layerfs.file.stat("shop/api", "agent", "/package.json");
```

```python
print(layerfs.file.stat("shop/api", "main", "/src")["type"])
```

```bash
layerfs file stat shop/api agent /package.json
```

## Related

- After: [file.read](read.md) for a file, [file.list](list.md) for a folder.
- [Engine: reads](../../../engine/store-calls/reads.md), [SDK facade](../../../engine/store-calls/sdk-facade.md).

---

← [file.read](read.md) · [Up: file](README.md) · [file.list](list.md) →


## Immutable saved references

Responses include `at: { id, kind: "Commit" | "Layer", branch, root }` with full
identities. Reuse `at.id` as the existing positional `commit` argument to pin later
reads to the same saved tree. Historical IDs are validated against the selected
project; an ID in another project cannot bypass that boundary. Reads do not inspect
live Workspace overlays. See [saved queries](../saved-queries.md).
