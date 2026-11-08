# file family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › file

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `file` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/file`](../../../../../api/crates/layerfs-api/src/file), [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs), [`engine/crates/layerfs-store-service/src/methods/file`](../../../../../engine/crates/layerfs-store-service/src/methods/file) |

## Calls

```text
 reads: at a Branch, main, Commit or Layer        writes: one Commit on a Branch
 (immutable snapshot, no lane)               (never main: ReadOnly)

   stat  list  glob  grep                      write  edit  rm  mkdir
     \     |     |    /                        mv  cp       \    |
      read   revision   blame   diff                         apply (many in one)
                                                        |
                                    private Workspace on the Branch
                                    --> Commit (merge a moved head)
                                    --> Committed | UpToDate | Conflict
```

Fifteen calls, in help order
([`file/mod.rs`](../../../../../api/crates/layerfs-api/src/file/mod.rs)):
eight saved reads, then seven writes that each make one
Commit on the named Branch, through a private Workspace bound to it. Each
runs its own function of `layerfs_store::calls` (`file_<call>`).
Every call takes `project` (Project) first; `branch` and `commit` are Name.

| Call | Arguments after `project` | Options | Returns | Store method |
| --- | --- | --- | --- | --- |
| [`read`](read.md) | `branch`, `path` (Text), `commit?` | `start_line`, `end_line` (Int) | header + `text`, `start_line`, `end_line`, `line_truncated`, `truncated`, `next_line`, `size` | `fs.read` |
| [`stat`](stat.md) | `branch`, `path`, `commit?` | — | header + `type`, `inode`, `size`, `mode` | `fs.stat` |
| [`list`](list.md) | `branch`, `path`, `commit?` | `PAGE` | header + `items: [{ name, path, type, inode }]`, `has_more`, `next_page_pos` | `fs.list` |
| [`glob`](glob.md) | `branch`, `path`, `pattern` (Text), `commit?` | `PAGE` | header + `items: [path]`, `has_more`, `next_page_pos` | `fs.glob` |
| [`grep`](grep.md) | `branch`, `path`, `pattern`, `commit?` | `PAGE` | header + `items: [{ path, line, text, line_truncated }]`, `skipped`, `has_more`, `next_page_pos` | `fs.grep` |
| [`revision`](revision.md) | `branch`, `path` | `at_commit`, `traversal`, `page`, `page_size` | pinned `at`, changed-path `items`, `has_more`, `scan` | `fs.revision` |
| [`blame`](blame.md) | `branch`, `path`, `commit?` | `from_line`, `line_count` (Int) | header + typed line `owner`, legacy `commit`, `coverage`, `versions`, `comparison_coarse` | `fs.blame` |
| [`diff`](diff.md) | `left_branch`, `left_path`, `right_branch`, `right_path` | `left_commit`, `right_commit`, `include_attribution`, `allow_missing`, `page`, `page_size` | legacy diff fields + pinned `left_ref`/`right_ref`, `before`/`after`, independent `changes`, aligned `rows`, coverage | `fs.diff_files` |
| [`write`](write.md) | `branch`, `path`, `content` (Content) | `message`, `mode` (Int) | write outcome | `fs.write` |
| [`edit`](edit.md) | `branch`, `path` | `old`, `new` (Text), `message` | write outcome | `fs.edit` |
| [`rm`](rm.md) | `branch`, `path` | `message` | write outcome | `fs.rm` |
| [`mkdir`](mkdir.md) | `branch`, `path` | `message`, `mode` | write outcome | `fs.mkdir` |
| [`mv`](mv.md) | `branch`, `from_path`, `to_path` (Text) | `message` | write outcome | `fs.mv` |
| [`cp`](cp.md) | `branch`, `from_path`, `to_path` | `message` | write outcome | `fs.cp` |
| [`apply`](apply.md) | `branch`, `changes` (Json) | `message` | write outcome | `fs.apply` |

- Saved reads retain the header `{ kind: "ok", snapshot, root, path }` and add
  typed `at: { id, kind, branch, root }`
  ([`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs)).
- The write outcome is `{ kind: "Committed", branch, head, commit }` or
  `{ kind: "UpToDate", branch, head, root }`
  ([`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs)).

## Arguments and options

| Table name | Store field | Note |
| --- | --- | --- |
| `branch`, `commit` | `branch`, `commit` | a read is at the Commit when given, else the Branch |
| `start_line`, `end_line`, `from_line`, `line_count` | camel-case counterparts | positive line windows |
| `page_size`, `page_pos` | `pageSize`, `pagePos` | legacy list/glob/grep paging; page size 1..=1000, default 50 |
| `page`, `page_size` | `page`, `pageSize` | one-based matching-result pages; do not combine with legacy offsets |
| `content` text / bytes | `content` / `bytes` | bytes travel as `bytes` |
| `old`, `new` | `oldText`, `newText` | `edit` replaces exactly one occurrence |
| `from_path`, `to_path` | `from`, `to` | `mv`, `cp`, and inside `changes` |
| `left_*`, `right_*` | `left`, `right` = `{ commit, path }` | `left_commit` wins over `left_branch` |

Numbered queries are bounded matching-result pages: a query that cannot complete its
requested page refuses `Capacity` without skipping unreturned records. Reuse the full
resolved version for later pages. See [saved queries](../saved-queries.md).
`file.revision` is refused under folder scopes because records expose whole-commit metadata.
Existing path-scoped file reads, blame and diff retain their scope restrictions.

`file.diff` keeps missing-side refusals by default; `allow_missing` admits one absent side
and returns an explicit null. Attribution means Commit/Layer ownership, not human authorship.
Unknown or coarse ownership stays explicit; publishing does not itself reassign source owners.

A `changes` item is `{ op, path, from_path, to_path, content, old, new,
mode }` with `op` one of `write`, `edit`, `mkdir`, `chmod`, `rm`, `cp`,
`mv`. The Store's own names (`from`, `to`, `oldText`, `newText`, `bytes`)
are refused in a change, so the folder scope checks every path field
([`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs),
[`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs)).
A read reply is bounded by `REPLY_BUDGET` (48 KiB); `truncated`,
`next_line` and `has_more` say where to go on
([`read/reads.rs`](../../../../../engine/crates/layerfs-store-service/src/read/reads.rs)).

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument, change or page; a path outside a folder scope (`out of scope: path`) | [`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs), [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchStore`, `NoSuchProject`, `NoSuchBranch`, `NoSuchCommit` | a missing Store, project, Branch or Commit | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchPath`, `NotFile`, `NotDirectory`, `IsDirectory`, `Exists` | a path of the wrong kind or missing | [`read/snapshot.rs`](../../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs), [`write/mutation.rs`](../../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `Capacity` | a numbered page, history scan, aligned row or provenance query cannot fit the declared work/reply budget | [`read/query.rs`](../../../../../engine/crates/layerfs-store-service/src/read/query.rs) |
| `Binary` | `read` or `edit` of a file that is not text | [`read/window.rs`](../../../../../engine/crates/layerfs-store-service/src/read/window.rs), [`write/edit.rs`](../../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `NoMatch`, `AmbiguousMatch` | `edit` finds the old text 0 or more than 1 times (`detail.matches`) | [`write/edit.rs`](../../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `ReadOnly` | a write to `main`, which changes only through `branch.publish` | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Conflict` | the head moved and paths conflict; nothing was committed | [`write/files.rs`](../../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Busy`, `Unresolved`, `Unavailable` | the Store's `op`, or its worker not reached | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs), [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
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
([`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs),
[`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs)):
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
