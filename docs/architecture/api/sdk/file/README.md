# file family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › file

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `file` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/file`](../../../../../api/crates/layerfs-api/src/file), [`store/calls.rs`](../../../../../control/crates/layerfs-control/src/store/calls.rs), [`engine/crates/layerfs-store-service/src/methods/file`](../../../../../engine/crates/layerfs-store-service/src/methods/file) |

## Calls

```text
 reads: at a Branch, main or a Commit        writes: one Commit on a Branch
 (immutable snapshot, no lane)               (never main: ReadOnly)

   stat  list  glob  grep                      write  edit  rm  mkdir
     \     |     |    /                        mv  cp       \    |
      read     blame    diff                         apply (many in one)
                                                        |
                                    private Workspace on the Branch
                                    --> Commit (merge a moved head)
                                    --> Committed | UpToDate | Conflict
```

Fourteen calls, in help order
([`file/mod.rs`](../../../../../api/crates/layerfs-api/src/file/mod.rs)):
seven reads at a Branch or Commit, then seven writes that each make one
Commit on the named Branch, through a private Workspace bound to it. Each
runs `layerfs_control::store::calls::run`.
Every call takes `project` (Project) first; `branch` and `commit` are Name.

| Call | Arguments after `project` | Options | Returns | Store method |
| --- | --- | --- | --- | --- |
| [`read`](read.md) | `branch`, `path` (Text), `commit?` | `start_line`, `end_line` (Int) | header + `text`, `start_line`, `end_line`, `line_truncated`, `truncated`, `next_line`, `size` | `fs.read` |
| [`stat`](stat.md) | `branch`, `path`, `commit?` | — | header + `type`, `inode`, `size`, `mode` | `fs.stat` |
| [`list`](list.md) | `branch`, `path`, `commit?` | `PAGE` | header + `items: [{ name, path, type, inode }]`, `has_more`, `next_page_pos` | `fs.list` |
| [`glob`](glob.md) | `branch`, `path`, `pattern` (Text), `commit?` | `PAGE` | header + `items: [path]`, `has_more`, `next_page_pos` | `fs.glob` |
| [`grep`](grep.md) | `branch`, `path`, `pattern`, `commit?` | `PAGE` | header + `items: [{ path, line, text, line_truncated }]`, `skipped`, `has_more`, `next_page_pos` | `fs.grep` |
| [`blame`](blame.md) | `branch`, `path`, `commit?` | `from_line` (Int) | header + `lines: [{ n, text, line_truncated, commit, message, … }]`, `versions`, `comparison_coarse` | `fs.blame` |
| [`diff`](diff.md) | `left_branch`, `left_path`, `right_branch`, `right_path` | `left_commit`, `right_commit` | `{ kind: Equal, Text or Binary, left, right, content_equal, same_inode, hunks_omitted, hunks }` | `fs.diff_files` |
| [`write`](write.md) | `branch`, `path`, `content` (Content) | `message`, `mode` (Int) | write outcome | `fs.write` |
| [`edit`](edit.md) | `branch`, `path` | `old`, `new` (Text), `message` | write outcome | `fs.edit` |
| [`rm`](rm.md) | `branch`, `path` | `message` | write outcome | `fs.rm` |
| [`mkdir`](mkdir.md) | `branch`, `path` | `message`, `mode` | write outcome | `fs.mkdir` |
| [`mv`](mv.md) | `branch`, `from_path`, `to_path` (Text) | `message` | write outcome | `fs.mv` |
| [`cp`](cp.md) | `branch`, `from_path`, `to_path` | `message` | write outcome | `fs.cp` |
| [`apply`](apply.md) | `branch`, `changes` (Json) | `message` | write outcome | `fs.apply` |

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
([`store/calls.rs`](../../../../../control/crates/layerfs-control/src/store/calls.rs),
[`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs)).
A read reply is bounded by `REPLY_BUDGET` (48 KiB); `truncated`,
`next_line` and `has_more` say where to go on
([`read/reads.rs`](../../../../../engine/crates/layerfs-store-service/src/read/reads.rs)).

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument, change or page; a path outside a folder scope (`out of scope: path`) | [`calls/scope.rs`](../../../../../control/crates/layerfs-control/src/calls/scope.rs), [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchStore`, `NoSuchProject`, `NoSuchBranch`, `NoSuchCommit` | a missing Store, project, Branch or Commit | [`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchPath`, `NotFile`, `NotDirectory`, `IsDirectory`, `Exists` | a path of the wrong kind or missing | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs), [`write/mutation.rs`](../../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `Binary` | `read` or `edit` of a file that is not text | [`read/window.rs`](../../../../../engine/crates/layerfs-store-service/src/read/window.rs), [`write/edit.rs`](../../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `NoMatch`, `AmbiguousMatch` | `edit` finds the old text 0 or more than 1 times (`detail.matches`) | [`write/edit.rs`](../../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `ReadOnly` | a write to `main`, which changes only through `branch.publish` | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Conflict` | the head moved and paths conflict; nothing was committed | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Busy`, `Unresolved`, `Unavailable` | the Store's `op`, or its worker not reached | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs), [`common/place.rs`](../../../../../control/crates/layerfs-control/src/common/place.rs) |
| `Unknown` | **uncertain:** a worker's answer was lost, or the Store reported an unknown outcome | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |

After an `Unknown` write the Commit may exist. Read the Branch head with
`branch.get` or `commit.list` before writing again. See
[error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

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
([`store/call.rs`](../../../../../control/crates/layerfs-control/src/store/call.rs),
[`common/place.rs`](../../../../../control/crates/layerfs-control/src/common/place.rs)):
a sqlite Store runs locally when its machine's platform executes locally
on this host (`native`, except on Windows). Writes go through the Store's
one writer; no Workspace daemon serves them. See the
[routing table](../../../control/routing/routing-table.md).

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

- [Table](../../call-table/table.md) and [Errors](../../call-table/errors.md).
- [branch family](../branch/README.md) and [commit family](../commit/README.md).
- [Routing table](../../../control/routing/routing-table.md).
- [Engine: Store calls](../../../engine/store-calls/README.md): [reads](../../../engine/store-calls/reads.md), [writes](../../../engine/store-calls/writes.md), [SDK facade](../../../engine/store-calls/sdk-facade.md).

---

← [commit.diff](../commit/diff.md) · [Up: SDK](../README.md) · [file.read](read.md) →
