# file family

[Architecture](../../../README.md) › [API](../../README.md) › [Call table](../README.md) › file family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `file` calls.

| | |
| --- | --- |
| Component | [Call table](../README.md) |
| Source | [`api/crates/layerfs-api/src/file`](../../../../../api/crates/layerfs-api/src/file), [`api/crates/layerfs-control/src/store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs), [`engine/crates/layerfs-store-service/src/methods/file`](../../../../../engine/crates/layerfs-store-service/src/methods/file) |

## Calls

Fourteen calls, in help order
([`file/mod.rs`](../../../../../api/crates/layerfs-api/src/file/mod.rs)):
seven reads at a Branch or Commit, then seven writes that each make one
Commit on a side Branch. Each runs `layerfs_control::store::calls::run`.
Every call takes `project` (Project) first; `branch` and `commit` are Name.

| Call | Arguments after `project` | Options | Returns | Store method |
| --- | --- | --- | --- | --- |
| [`read`](../../../../../api/crates/layerfs-api/src/file/read.rs) | `branch`, `path` (Text), `commit?` | `start_line`, `end_line` (Int) | header + `text`, `start_line`, `end_line`, `line_truncated`, `truncated`, `next_line`, `size` | `fs.read` |
| [`stat`](../../../../../api/crates/layerfs-api/src/file/stat.rs) | `branch`, `path`, `commit?` | — | header + `type`, `inode`, `size`, `mode` | `fs.stat` |
| [`list`](../../../../../api/crates/layerfs-api/src/file/list.rs) | `branch`, `path`, `commit?` | `PAGE` | header + `items: [{ name, path, type, inode }]`, `has_more`, `next_page_pos` | `fs.list` |
| [`glob`](../../../../../api/crates/layerfs-api/src/file/glob.rs) | `branch`, `path`, `pattern` (Text), `commit?` | `PAGE` | header + `items: [path]`, `has_more`, `next_page_pos` | `fs.glob` |
| [`grep`](../../../../../api/crates/layerfs-api/src/file/grep.rs) | `branch`, `path`, `pattern`, `commit?` | `PAGE` | header + `items: [{ path, line, text, line_truncated }]`, `skipped`, `has_more`, `next_page_pos` | `fs.grep` |
| [`blame`](../../../../../api/crates/layerfs-api/src/file/blame.rs) | `branch`, `path`, `commit?` | `from_line` (Int) | header + `lines: [{ n, text, line_truncated, commit, message, … }]`, `versions`, `comparison_coarse` | `fs.blame` |
| [`diff`](../../../../../api/crates/layerfs-api/src/file/diff.rs) | `left_branch`, `left_path`, `right_branch`, `right_path` | `left_commit`, `right_commit` | `{ kind: Equal, Text or Binary, left, right, content_equal, same_inode, hunks_omitted, hunks }` | `fs.diff_files` |
| [`write`](../../../../../api/crates/layerfs-api/src/file/write.rs) | `branch`, `path`, `content` (Content) | `message`, `mode` (Int) | write outcome | `fs.write` |
| [`edit`](../../../../../api/crates/layerfs-api/src/file/edit.rs) | `branch`, `path` | `old`, `new` (Text), `message` | write outcome | `fs.edit` |
| [`rm`](../../../../../api/crates/layerfs-api/src/file/rm.rs) | `branch`, `path` | `message` | write outcome | `fs.rm` |
| [`mkdir`](../../../../../api/crates/layerfs-api/src/file/mkdir.rs) | `branch`, `path` | `message`, `mode` | write outcome | `fs.mkdir` |
| [`mv`](../../../../../api/crates/layerfs-api/src/file/mv.rs) | `branch`, `from_path`, `to_path` (Text) | `message` | write outcome | `fs.mv` |
| [`cp`](../../../../../api/crates/layerfs-api/src/file/cp.rs) | `branch`, `from_path`, `to_path` | `message` | write outcome | `fs.cp` |
| [`apply`](../../../../../api/crates/layerfs-api/src/file/apply.rs) | `branch`, `changes` (Json) | `message` | write outcome | `fs.apply` |

- The read header is `{ kind: "ok", snapshot, root, path }`
  ([`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs)).
- The write outcome is `{ kind: "Committed", branch, head, commit }` or
  `{ kind: "UpToDate", branch, head, root }`
  ([`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs)).

## Arguments and options

| Table name | Store field | Note |
| --- | --- | --- |
| `branch`, `commit` | `branch`, `commit` | a read is at the Commit when given, else the Branch |
| `start_line`, `end_line`, `from_line` | `startLine`, `endLine`, `fromLine` | positive line numbers |
| `page_size`, `page_pos` | `pageSize`, `pagePos` | 1..=1000, default 50 |
| `content` text / bytes | `content` / `bytes` | bytes travel as `bytes` |
| `old`, `new` | `oldText`, `newText` | `edit` replaces exactly one occurrence |
| `from_path`, `to_path` | `from`, `to` | `mv`, `cp`, and inside `changes` |
| `left_*`, `right_*` | `left`, `right` = `{ commit, path }` | `left_commit` wins over `left_branch` |

A `changes` item is `{ op, path, from_path, to_path, content, old, new,
mode }` with `op` one of `write`, `edit`, `mkdir`, `chmod`, `rm`, `cp`,
`mv`. The Store's own names (`from`, `to`, `oldText`, `newText`, `bytes`)
are refused in a change, so the folder scope checks every path field
([`store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs),
[`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs)).
A read reply is bounded by `REPLY_BUDGET` (48 KiB); `truncated`,
`next_line` and `has_more` say where to go on
([`read/reads.rs`](../../../../../engine/crates/layerfs-store-service/src/read/reads.rs)).

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument, change or page; a path outside a folder scope (`out of scope: path`) | [`calls/scope.rs`](../../../../../api/crates/layerfs-control/src/calls/scope.rs), [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchStore`, `NoSuchProject`, `NoSuchBranch`, `NoSuchCommit` | a missing Store, project, Branch or Commit | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchPath`, `NotFile`, `NotDirectory`, `IsDirectory`, `Exists` | a path of the wrong kind or missing | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs), [`write/mutation.rs`](../../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `Binary` | `read` or `edit` of a file that is not text | [`read/window.rs`](../../../../../engine/crates/layerfs-store-service/src/read/window.rs), [`write/edit.rs`](../../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `NoMatch`, `AmbiguousMatch` | `edit` finds the old text 0 or more than 1 times (`detail.matches`) | [`write/edit.rs`](../../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `ReadOnly` | a write to `main`, which changes only through `branch.publish` | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Conflict` | the head moved and paths conflict; nothing was committed | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Busy`, `Unresolved`, `Unavailable` | the Store's `op`, or its worker not reached | [`common/op.rs`](../../../../../api/crates/layerfs-control/src/common/op.rs), [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) |
| `Unknown` | **uncertain:** a worker's answer was lost, or the Store reported an unknown outcome | [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |

After an `Unknown` write the Commit may exist. Read the Branch head with
`branch.get` or `commit.list` before writing again. See
[error kinds](../errors.md#error-kinds) and
[uncertain outcomes](../errors.md#uncertain-outcomes).

## Where it runs

```text
 file.<call> --> store::calls::file: table names -> Store fields
     |
     v
 store_call: lease claim, then by place::plan
     +-- caller's process (PostgreSQL; sqlite on a local machine)
     |      open::run(fs.<method>)
     +-- machine's worker (sqlite elsewhere)
            "store.call" { op: fs.<method>, args } -> open::run there
```

The route is the same for every Store call
([`store/call.rs`](../../../../../api/crates/layerfs-control/src/store/call.rs),
[`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs)):
a sqlite Store runs locally when its machine's platform executes locally
on this host (`native`, except on Windows). Writes go through the Store's
one writer; no Workspace daemon serves them. See the
[routing table](../../routing/routing-table.md).

## Example

Not run; written from the call table.

```js
await layerfs.file.write("shop/api", "agent", "/README.md", "hello\n", { message: "docs" });
const { text } = await layerfs.file.read("shop/api", "agent", "/README.md", { start_line: 1 });
```

```python
layerfs.file.write("shop/api", "agent", "/README.md", "hello\n", message="docs")
text = layerfs.file.read("shop/api", "agent", "/README.md", start_line=1)["text"]
```

```console
$ echo hello | layerfs file write shop/api agent /README.md -m docs
$ layerfs file read shop/api agent /README.md --start-line 1
```

## Related

- [Table](../table.md) and [Errors](../errors.md).
- [branch family](branch.md) and [commit family](commit.md).
- [Routing table](../../routing/routing-table.md).
- [Engine: Store calls](../../../engine/store-calls/README.md): [reads](../../../engine/store-calls/reads.md), [writes](../../../engine/store-calls/writes.md), [SDK facade](../../../engine/store-calls/sdk-facade.md).

---

← [Call table: commit family](commit.md) · [Up: Call table](../README.md) · [Call table: sandbox family](sandbox.md) →
