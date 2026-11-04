# file.write

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › write

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Writes one file's whole content on a Branch as one Commit, creating it or replacing its bytes.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/write.rs`](../../../../../api/crates/layerfs-api/src/file/write.rs) |
| Implemented in | `layerfs_control::store::calls::run` → `file`, Store method `fs.write` ([`methods/file/write.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/write.rs)); text `content` is sent as `content`, bytes as `bytes` |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.write(project, branch, path, content, { message, mode });
```

```python
# Python
layerfs.file.write(project, branch, path, content, message=None, mode=None)
```

```bash
# CLI
layerfs file write <store>/<project> <branch> <path> [--from FILE] \
  [--message TEXT] [--mode N]      # content from stdin or --from
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `branch` | Name | the Branch to commit to; `main` is `ReadOnly` | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `path` | Text | absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`files::path`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `content` | Content | text (JS string, Python `str`) or bytes (`Uint8Array`/`Buffer`, `bytes`); the CLI reads stdin or `--from` | [`store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `message` | Text | `Apply filesystem changes` | the Commit's message | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `mode` | Int | new file 0o644; an existing file keeps its mode | 0..=0o7777; no set-user-ID, set-group-ID or sticky bit on a file | [`write/mutation.rs`](../../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |

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

The file is written in pieces of at most 65,519 bytes; a piece equal to
what is there is not rewritten.

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target; lease::claim
 store_call fs.write        --------> branch "main"? --> ReadOnly
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
| `InvalidInput` | a bad project, path, mode or option; a path outside a folder scope | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| `ReadOnly` | `branch` is `main`; `main` changes only through [branch.publish](../branch/publish.md) | no | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `NoSuchStore`, `NoSuchProject`, `NoSuchBranch` | not recorded, or no such Branch | no | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Conflict` | the Branch's head moved since the call bound it and a changed path conflicts; nothing committed (`detail.branch`, `head`, `conflicts`) | no | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `HeadMoved`, `Busy`, `Capacity`, `Failed` | the Commit failed and was settled locally | no | same |
| `Unresolved`, `Unavailable` | the Store's `op`, or its worker not reached | no | [`store/call.rs`](../../../../../api/crates/layerfs-control/src/store/call.rs) |
| `IsDirectory`, `NotDirectory`, `NoSuchPath` | the path is a folder, or a parent is not a folder or missing | no | [`write/mutation.rs`](../../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `Unknown` | **a change or the Commit ended with an unknown outcome (the private Workspace is kept), or the worker's answer was lost** | **yes** | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs), [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs) |

**After `Unknown`** the Commit may exist. Read [branch.get](../branch/get.md)
or [commit.list](../commit/list.md) for the Branch's head before writing
again.

## Examples

Not run; written from the call table.

```js
await layerfs.file.write("shop/api", "agent", "/notes.md", "# notes\n", { message: "add notes" });
```

```python
layerfs.file.write("shop/api", "agent", "/logo.png", open("logo.png", "rb").read())
```

```bash
echo "hello" | layerfs file write shop/api agent /hello.txt --message hello
```

## Related

- Before: [branch.fork](../branch/fork.md) (writes never go to `main`). After: [file.read](read.md), [branch.publish](../branch/publish.md). Several changes in one Commit: [file.apply](apply.md).
- [Engine: writes](../../../engine/store-calls/writes.md), [SDK facade](../../../engine/store-calls/sdk-facade.md), [Engine: Commit](../../../engine/commit/README.md).

---

← [file.diff](diff.md) · [Up: file](README.md) · [file.edit](edit.md) →
