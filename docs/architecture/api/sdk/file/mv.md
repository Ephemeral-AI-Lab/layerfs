# file.mv

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › mv

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Moves (renames) a path on a Branch as one Commit; the target must not exist.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/mv.rs`](../../../../../api/crates/layerfs-api/src/file/mv.rs) |
| Implemented in | `layerfs_control::store::calls::run` → `file`, Store method `fs.mv` ([`methods/file/mv.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/mv.rs)); `from_path`, `to_path` become `from`, `to` |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.mv(project, branch, from_path, to_path, { message });
```

```python
# Python
layerfs.file.mv(project, branch, from_path, to_path, message=None)
```

```bash
# CLI
layerfs file mv <store>/<project> <branch> <from_path> <to_path> [--message TEXT]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `branch` | Name | the Branch to commit to; `main` is `ReadOnly` | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `from_path` | Text | an existing entry; absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `to_path` | Text | not existing; not equal to or under `from_path`; absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `message` | Text | `Apply filesystem changes` | the Commit's message | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |

## Returns

```text
 Committed: { kind: "Committed", branch, head: <Commit id>,
              commit: { ...commit } }
 UpToDate:  { kind: "UpToDate", branch, head: <Commit id> | null, root }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `kind` | text | `Committed`, or `UpToDate` when the tree did not change | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `branch`, `head` | text | the Branch and its head after the call | same |
| `commit` | map | the new Commit, as an item of [commit.list](../commit/list.md#returns) | same |
| `root` | text | the unchanged tree root (`UpToDate`) | same |

One rename with `replace: false`; the entry keeps its inode serial.

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target; lease::claim
 store_call fs.mv           --------> branch "main"? --> ReadOnly
                                      bind a private Workspace on the Branch
                                      apply the change(s) in short overlay
                                        jobs (first refusal stops)
                                      commit_captured_via(Files): capture,
                                        build, merge a moved head, publish
                                      close the private Workspace
```

The Commit is the same code a mounted Workspace's Commit runs; only the
final publish holds the lane. The private Workspace is never listed,
mounted or shared. See the [routing table](../../routing/routing-table.md),
[Engine: writes](../../../engine/store-calls/writes.md) and
[Engine: Commit](../../../engine/commit/README.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | overlapping paths (`/` or `to_path` under `from_path`); a bad project, path, mode or option; a path outside a folder scope | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| `ReadOnly` | `branch` is `main`; `main` changes only through [branch.publish](../branch/publish.md) | no | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `NoSuchStore`, `NoSuchProject`, `NoSuchBranch` | not recorded, or no such Branch | no | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Conflict` | the Branch's head moved since the call bound it and a changed path conflicts; nothing committed (`detail.branch`, `head`, `conflicts`) | no | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `HeadMoved`, `Busy`, `Capacity`, `Failed` | the Commit failed and was settled locally | no | same |
| `Unresolved`, `Unavailable` | the Store's `op`, or its worker not reached | no | [`store/call.rs`](../../../../../api/crates/layerfs-control/src/store/call.rs) |
| `NoSuchPath`, `Exists` | `from_path` is missing; `to_path` exists | no | [`write/recursive.rs`](../../../../../engine/crates/layerfs-store-service/src/write/recursive.rs) |
| `Unknown` | **a change or the Commit ended with an unknown outcome (the private Workspace is kept), or the worker's answer was lost** | **yes** | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs), [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs) |

**After `Unknown`** the Commit may exist. Read [branch.get](../branch/get.md)
or [commit.list](../commit/list.md) for the Branch's head before writing
again.

## Examples

Not run; written from the call table.

```js
await layerfs.file.mv("shop/api", "agent", "/src/old.ts", "/src/new.ts");
```

```python
layerfs.file.mv("shop/api", "agent", "/docs", "/doc", message="rename")
```

```bash
layerfs file mv shop/api agent /src/old.ts /src/new.ts
```

## Related

- [file.cp](cp.md), [file.apply](apply.md).
- [Engine: writes, recursive](../../../engine/store-calls/writes.md#recursive-writes).

---

← [file.mkdir](mkdir.md) · [Up: file](README.md) · [file.cp](cp.md) →
