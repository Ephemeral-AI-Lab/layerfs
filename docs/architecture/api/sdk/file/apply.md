# file.apply

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [file](README.md) › apply

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Applies a list of changes (write, edit, mkdir, chmod, rm, cp, mv) on a Branch as one Commit; the first refused change stops the call and nothing is committed.

| | |
| --- | --- |
| Family | [file](README.md) |
| Library entry | [`api/crates/layerfs-api/src/file/apply.rs`](../../../../../api/crates/layerfs-api/src/file/apply.rs) |
| Implemented in | `layerfs_control::store::calls::run` → `file`, Store method `fs.apply` ([`methods/file/apply.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/file/apply.rs)); each change's snake_case fields are renamed (`changes`) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.file.apply(project, branch, changes, { message });
```

```python
# Python
layerfs.file.apply(project, branch, changes, message=None)
```

```bash
# CLI
layerfs file apply <store>/<project> <branch> [--from FILE] [--message TEXT]
# changes: JSON on stdin, or in the --from file
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>`; both records must exist | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `branch` | Name | the Branch to commit to; `main` is `ReadOnly` | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `changes` | Json | a nonempty list of change records (below); the CLI reads it as JSON from stdin or `--from` | [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs) `changes` |

| `op` | Fields | Effect | Source |
| --- | --- | --- | --- |
| `write` | `path`, `content` (text or bytes), `mode?` | replace or create a file (new: 0o644) | [`write/mutation.rs`](../../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `edit` | `path`, `old`, `new` | replace the one occurrence | [`write/edit.rs`](../../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `mkdir` | `path`, `mode?` | create it and missing parents (0o755) | [`write/mutation.rs`](../../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `chmod` | `path`, `mode` | set permission bits | same |
| `rm` | `path` | remove, recursively | [`write/recursive.rs`](../../../../../engine/crates/layerfs-store-service/src/write/recursive.rs) |
| `cp` | `from_path`, `to_path` | copy, recursively, keeping hard links | same |
| `mv` | `from_path`, `to_path` | rename | same |

Control renames `old`/`new` to `oldText`/`newText`, `from_path`/`to_path`
to `from`/`to`, and bytes `content` to `bytes`. A change that uses the
Store's own names (`from`, `to`, `oldText`, `newText`, `bytes`) is refused,
so a folder scope can check every path field
([`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs), [`calls/scope.rs`](../../../../../control/crates/layerfs-control/src/calls/scope.rs)).

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

Changes apply in order on the private Workspace, each seeing the ones
before it.

## Where it runs

```text
 caller process                       Store's machine worker / this process
 --------------                       -------------------------------------
 resolve::target; lease::claim
 store_call fs.apply        --------> branch "main"? --> ReadOnly
                                      bind a private Workspace on the Branch
                                      apply the change(s) in short overlay
                                        jobs (first refusal stops)
                                      commit_captured_via(Files): capture,
                                        build, merge a moved head, publish
                                      close the private Workspace
```

The Commit is the same code a mounted Workspace's Commit runs; only the
final publish holds the lane. The private Workspace is never listed,
mounted or shared. See the [routing table](../../../control/routing/routing-table.md),
[Engine: writes](../../../engine/store-calls/writes.md) and
[Engine: Commit](../../../engine/commit/README.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | `changes` not a list of records; a Store field name in a change; a bad path or mode; a path outside a folder scope | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs), [`calls/scope.rs`](../../../../../control/crates/layerfs-control/src/calls/scope.rs) |
| `ReadOnly` | `branch` is `main`; `main` changes only through [branch.publish](../branch/publish.md) | no | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `NoSuchStore`, `NoSuchProject`, `NoSuchBranch` | not recorded, or no such Branch | no | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Conflict` | the Branch's head moved since the call bound it and a changed path conflicts; nothing committed (`detail.branch`, `head`, `conflicts`) | no | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `HeadMoved`, `Busy`, `Capacity`, `Failed` | the Commit failed and was settled locally | no | same |
| `Unresolved`, `Unavailable` | the Store's `op`, or its worker not reached | no | [`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs) |
| any change refusal (`NoSuchPath`, `Exists`, `NoMatch`, `Binary`, …) | that change was refused; `detail.index` names it; nothing committed | no | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Unknown` | **a change or the Commit ended with an unknown outcome (the private Workspace is kept), or the worker's answer was lost** | **yes** | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs), [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

**After `Unknown`** the Commit may exist. Read [branch.get](../branch/get.md)
or [commit.list](../commit/list.md) for the Branch's head before writing
again.

## Examples

Not run; written from the call table.

```js
await layerfs.file.apply("shop/api", "agent", [
  { op: "mkdir", path: "/src/lib" },
  { op: "write", path: "/src/lib/a.ts", content: "export {};\n" },
  { op: "mv", from_path: "/old.ts", to_path: "/src/lib/old.ts" },
], { message: "restructure" });
```

```python
layerfs.file.apply("shop/api", "agent", [
    {"op": "edit", "path": "/README.md", "old": "v1", "new": "v2"},
    {"op": "chmod", "path": "/run.sh", "mode": 0o755},
])
```

```bash
echo '[{"op":"rm","path":"/dist"}]' | layerfs file apply shop/api agent --message clean
```

## Related

- The single-change calls: [file.write](write.md), [file.edit](edit.md), [file.rm](rm.md), [file.mkdir](mkdir.md), [file.mv](mv.md), [file.cp](cp.md).
- [Engine: writes](../../../engine/store-calls/writes.md), [SDK facade](../../../engine/store-calls/sdk-facade.md), [Engine: Commit](../../../engine/commit/README.md).

---

← [file.cp](cp.md) · [Up: file](README.md) · [SDK: sandbox](../sandbox/README.md) →
