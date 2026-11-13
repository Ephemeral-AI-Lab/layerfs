# file family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `file` calls: read files at a saved version without a mount, and write a Branch one Commit per call, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/file`](../../../../api/crates/layerfs-api/src/file), [`layerfs-store/src/calls/file.rs`](../../../../control/crates/layerfs-store/src/calls/file.rs), [`methods/file`](../../../../engine/crates/layerfs-store-service/src/methods/file), [`read/`](../../../../engine/crates/layerfs-store-service/src/read), [`write/`](../../../../engine/crates/layerfs-store-service/src/write) |
| Runs by default | where the Store's calls run: the caller's process for PostgreSQL and for sqlite on a locally executing machine, else the Store machine's worker (`store.call`). No daemon serves them |
| Design | [Store calls](../../engine/store-calls/README.md), [reads](../../engine/store-calls/reads.md) |

## Calls

```text
 reads: at a Branch, main, Commit or Layer      writes: one Commit on a Branch
 (immutable snapshot, no lane)                  (never main: ReadOnly)

   read  stat  list  glob  grep                 write  edit  rm  mkdir
   revision  blame  diff                        mv  cp  apply (many in one)
                                                       |
                                    private Workspace bound to the Branch
                                    --> Commit (a moved head is merged)
                                    --> Committed | UpToDate | Conflict
```

| Call | Store method | Kind |
| --- | --- | --- |
| [`file.read`](#fileread) | `fs.read` | read |
| [`file.stat`](#filestat) | `fs.stat` | read |
| [`file.list`](#filelist) | `fs.list` | read |
| [`file.glob`](#fileglob) | `fs.glob` | read |
| [`file.grep`](#filegrep) | `fs.grep` | read |
| [`file.revision`](#filerevision) | `fs.revision` | saved read |
| [`file.blame`](#fileblame) | `fs.blame` | saved read |
| [`file.diff`](#filediff) | `fs.diff_files` | saved read |
| [`file.write`](#filewrite) | `fs.write` | write |
| [`file.edit`](#fileedit) | `fs.edit` | write |
| [`file.rm`](#filerm) | `fs.rm` | write |
| [`file.mkdir`](#filemkdir) | `fs.mkdir` | write |
| [`file.mv`](#filemv) | `fs.mv` | write |
| [`file.cp`](#filecp) | `fs.cp` | write |
| [`file.apply`](#fileapply) | `fs.apply` | write |

Shared rules:

| Rule | Detail | Source |
| --- | --- | --- |
| Version | a read is at `commit` when given (a 66-hex Commit or Layer id), else at `branch`; `main` is its head Layer. The id must belong to the project | [`read/snapshot.rs`](../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| Read header | `{ kind: "ok", snapshot, root, path, at: { id, kind, branch, root } }`; reuse `at.id` as `commit` to pin later reads. Reads never see a live Workspace | same |
| Paths | absolute, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) `path` |
| Reply budget | 48 KiB per answer (`REPLY_BUDGET`); `truncated`, `next_line`, `has_more` say where to go on | [`read/reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs) |
| Paging | legacy `page_size` (1..=1000, default 50) with `page_pos`; or numbered `page` ([saved queries](branch.md#saved-queries)); never both | [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| Field names | control renames table names to the Store's: `start_line`→`startLine`, `old`/`new`→`oldText`/`newText`, `from_path`/`to_path`→`from`/`to`, byte content→`bytes` | [`calls/op.rs`](../../../../control/crates/layerfs-store/src/calls/op.rs) |
| Write outcome | `Committed: { kind, branch, head, commit }` (`commit` as a `commit.list` item) or `UpToDate: { kind, branch, head \| null, root }` when the tree did not change | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| Writes | through a private Workspace bound to the Branch, then one Commit through the Store's single writer; a write to `main` is `ReadOnly` (it changes only through `branch.publish`) | same |
| Scope | every path must be inside the scope's folder; `branch`, `commit`, `left_*`, `right_*` are composed into the namespace; `file.revision` is refused (`out of scope: history`) | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |

### file.read

Lines of one text file, one bounded window at a time.

```text
JS      await layerfs.file.read(project, branch, path, commit?, { start_line, end_line })
Python  layerfs.file.read(project, branch, path, commit=None, *, start_line=None, end_line=None)
CLI     layerfs file read <project> <branch> <path> [<commit>] [--start-line <start-line>] [--end-line <end-line>]
```

`start_line` (default 1) and `end_line` (default the end) are positive line
numbers; the window may stop earlier.

```text
header + { text, start_line, end_line | null, line_truncated, truncated,
           next_line | null, size }
```

A window holds at most 2000 lines (`LINES`) and 48 KiB; `truncated` means
clipped or more lines follow. To read a whole file, call again with
`start_line: next_line` until it is null. A NUL or invalid UTF-8 in a
16 KiB window is `Binary`
([`read/window.rs`](../../../../engine/crates/layerfs-store-service/src/read/window.rs)).
Errors: `NoSuchPath`, `NotFile`, `IsDirectory`, `Binary`.

### file.stat

A path's kind, inode serial, size and mode.

```text
JS      await layerfs.file.stat(project, branch, path, commit?)
Python  layerfs.file.stat(project, branch, path, commit=None)
CLI     layerfs file stat <project> <branch> <path> [<commit>]
```

Returns the header plus `type`, `inode` (stable within the project), `size`
(from Store metadata) and `mode`. Error: `NoSuchPath`.

### file.list

One folder's entries, in name order, one page at a time.

```text
JS      await layerfs.file.list(project, branch, path, commit?, { page_size, page, page_pos })
Python  layerfs.file.list(project, branch, path, commit=None, *, page_size=None, page=None, page_pos=None)
CLI     layerfs file list <project> <branch> <path> [<commit>] [--page-size <page-size>] [--page <page>] [--page-pos <page-pos>]
```

Returns the header plus `items: [{ name, path, type, inode }]`, `has_more`
and `next_page_pos` (numbered mode: `page`, `page_size`). Entries are read
64 at a time, so a page costs its size, not the folder's; numbered replay is
admitted within 10,000 entries per request. Errors: `NoSuchPath`,
`NotDirectory`.

### file.glob

File paths under a folder that match a glob.

```text
JS      await layerfs.file.glob(project, branch, path, pattern, commit?, { page_size, page, page_pos })
Python  layerfs.file.glob(project, branch, path, pattern, commit=None, *, page_size=None, page=None, page_pos=None)
CLI     layerfs file glob <project> <branch> <path> <pattern> [<commit>] [--page-size <page-size>] [--page <page>] [--page-pos <page-pos>]
```

`pattern` uses the `/.lww` rule syntax, matched against whole paths
(`src/**/*.ts`). Returns the header plus `items: [path]` (depth first, name
order), `has_more`, `next_page_pos`. Only folders that agree with the
pattern's literal leading folders are entered (`a/b/**/*.js` enters `a/` and
`a/b/` only, [`read/walk.rs`](../../../../engine/crates/layerfs-store-service/src/read/walk.rs)).
Numbered mode admits 10,000 candidate entries per request. Errors:
`NoSuchPath`, `NotDirectory`.

### file.grep

Lines under a folder or file that contain a text.

```text
JS      await layerfs.file.grep(project, branch, path, pattern, commit?, { page_size, page, page_pos })
Python  layerfs.file.grep(project, branch, path, pattern, commit=None, *, page_size=None, page=None, page_pos=None)
CLI     layerfs file grep <project> <branch> <path> <pattern> [<commit>] [--page-size <page-size>] [--page <page>] [--page-pos <page-pos>]
```

`pattern` is a nonempty **literal** substring: the help line says "regular
expression", but no regular-expression syntax is interpreted, and a line feed
anywhere but last matches nothing
([`read/search.rs`](../../../../engine/crates/layerfs-store-service/src/read/search.rs)).

```text
header + { items: [ { path, line, text, line_truncated } ], skipped,
           has_more, next_page_pos | null }
```

`skipped` counts binary files. Files are read in batches of 64, and content
already scanned in the call is answered from its kept lines. Numbered mode
admits 10,000 candidate or matching-line visits and 16 MiB of file payload per
request; it resumes inside a file from the cached last match, but line
counting in that file still replays from byte zero, so a deep page in one
large file can be `Capacity`. Error: `NoSuchPath`.

### file.revision

Saved changes to one literal path, deletions and metadata-only changes
included; no rename or copy ancestry is inferred.

```text
JS      await layerfs.file.revision(project, branch, path, { at_commit, traversal, page, page_size, continuation })
Python  layerfs.file.revision(project, branch, path, *, at_commit=None, traversal=None, page=None, page_size=None, continuation=None)
CLI     layerfs file revision <project> <branch> <path> [--at-commit <at-commit>] [--traversal <traversal>] [--page <page>] [--page-size <page-size>] [--continuation <continuation>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `path` | Text | may be absent at the start; a path that never existed answers an exhausted empty list |
| `at_commit` option | Name | a full Commit or Layer id in the Branch's context; reuse the first answer's `at.id` |
| `traversal` option | Text | `provenance` (default: Commit first parent or base Layer; Layer source Commit or parent Layer, as blame walks) or `branch` (side-Branch first parents, or `main`'s Layers in publication order) |
| `page`, `page_size`, `continuation` options | Int, Text | [saved queries](branch.md#saved-queries) |

```text
{ kind: "ok", at, path, traversal,
  items: [ { candidate_index, ref, compared_to, kind: Added|Removed|Modified,
             before: facts | null, after: facts | null,
             changes: { content, metadata, identity, attribution: null },
             message, recorded_at, time_status } ],
  page, page_size, has_more, end_of_query, continuation,
  scan: { complete, candidates_examined, reasons } }
```

Facts are `type`, `inode`, `size`, `mode`. A side Branch's first Commit is
compared with its base Layer. A Layer's `recorded_at` is null with
`time_status: "unavailable"`. A request admits 10,000 visited records and
64 MiB of content comparison (equal-length files with different roots
reserve their combined length; equal roots and unequal lengths cost
nothing) ([`read/revision.rs`](../../../../engine/crates/layerfs-store-service/src/read/revision.rs),
[`read/revision_pages.rs`](../../../../engine/crates/layerfs-store-service/src/read/revision_pages.rs)).

### file.blame

The saved owner of each line, walking back from a Branch, `main` or a
Commit.

```text
JS      await layerfs.file.blame(project, branch, path, commit?, { from_line, line_count })
Python  layerfs.file.blame(project, branch, path, commit=None, *, from_line=None, line_count=None)
CLI     layerfs file blame <project> <branch> <path> [<commit>] [--from-line <from-line>] [--line-count <line-count>]
```

`from_line` defaults to 1; `line_count` is 1..=2000, default 2000.

```text
header + { lines: [ { n, text, line_truncated, clipped, commit, message,
                      owner: { ref, message } | null } ],
           origins, versions, comparison_coarse, coverage, next_line? }
```

```text
 tip file --> parent (Commit: parent or base Layer; Layer: source Commit,
              else parent Layer) --> ... until every line has an owner
   absent / not regular  -> remaining lines owned here, stop
   same content root     -> step back
   both texts in window  -> line LCS;  else coarse prefix/suffix
```

`owner` is a full typed Commit or Layer reference (a published line follows
its source Commit; Init can own lines as a Layer); `origins` deduplicates
the owners returned; `coverage` reports status, exact or coarse comparison
and reasons such as `history_budget` or `line_clipped`. An unresolved line
stays null and is never assigned to the tip. A request admits 10,000
ancestry visits and 64 MiB of payload; lines and origins must fit 48 KiB
together, and `next_line` names the first line not returned
([`read/blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs)).
Errors: `NoSuchPath`, `NotFile`, `Capacity` when the budget ends before the
requested line.

### file.diff

Two files compared line by line; each side is a path at a Branch, `main` or
a saved version, so a file can be compared with itself across history.

```text
JS      await layerfs.file.diff(project, left_branch, left_path, right_branch, right_path, { left_commit, right_commit, include_attribution, allow_missing, page, page_size })
Python  layerfs.file.diff(project, left_branch, left_path, right_branch, right_path, *, left_commit=None, right_commit=None, include_attribution=None, allow_missing=None, page=None, page_size=None)
CLI     layerfs file diff <project> <left-branch> <left-path> <right-branch> <right-path> [--left-commit <left-commit>] [--right-commit <right-commit>] [--include-attribution] [--allow-missing] [--page <page>] [--page-size <page-size>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `left_commit`, `right_commit` options | Name | a Commit or Layer id; replaces that side's Branch |
| `include_attribution` option | Bool | compare line owners too |
| `allow_missing` option | Bool | one absent side answers Added or Removed with a null side; both absent still refuse |
| `page`, `page_size` options | Int | windows of aligned `rows` |

```text
{ kind: "Equal" | "Text" | "Binary",
  left: { path, snapshot, inode }, right: { path, snapshot, inode },
  content_equal, same_inode, hunks_omitted, hunks: [ ... ] }
```

Without the new options this is the whole answer: `Binary` means different
and not text, or over 1 MiB a side (`MAXIMUM_TEXT_BYTES`,
[`compare/text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs)),
and hunks that do not fit 48 KiB are omitted. Any of the four new options
adds `left_ref`, `right_ref`, `before`/`after` facts, independent `changes`
(`content`, `metadata`, `identity`, nullable `attribution`),
`omission_reason` (`binary`, `text_limit`, …), `content_coverage`,
`attribution_coverage`, and `rows`:

```text
{ index, op: " " | "-" | "+", left_line, right_line, text, text_clipped,
  before_owner, after_owner, owner_changed, attribution_coverage }
```

An unchanged aligned row with different known owners has
`owner_changed: true`; unknown or coarse ownership is null, never false.
That mode admits 10,000 rows or ancestry visits, 64 MiB of provenance reads
and, separately, 64 MiB of content comparison per request; a mandatory row
page that does not fit 48 KiB is `Capacity`
([`read/file_compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/file_compare.rs)).
Errors: `NoSuchPath` (unless `allow_missing`), `IsDirectory` (`detail.path`).

### file.write

A file's whole content, created or replaced, as one Commit.

```text
JS      await layerfs.file.write(project, branch, path, content, { message, mode })
Python  layerfs.file.write(project, branch, path, content, *, message=None, mode=None)
CLI     layerfs file write <project> <branch> <path> [--message <message>] [--mode <mode>] [--from <file>] (content from stdin)
```

`content` is text or bytes (the CLI reads stdin or `--from`). `mode` is
0..=0o7777 without set-user-ID, set-group-ID or sticky bits; a new file gets
0o644, an existing one keeps its mode
([`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs)).
The file is written in pieces of at most 65,519 bytes; a piece equal to what
is there is not rewritten. Errors: `IsDirectory`, `NotDirectory` (a parent
is not a folder), `NoSuchPath`.

### file.edit

Replaces exactly one occurrence of a text in a file, as one Commit.

```text
JS      await layerfs.file.edit(project, branch, path, { old, new, message })
Python  layerfs.file.edit(project, branch, path, *, old=None, new=None, message=None)
CLI     layerfs file edit <project> <branch> <path> [--old <old>] [--new <new>] [--message <message>]
```

`old` must occur exactly once (empty or absent is `InvalidInput`); `new`
equal to `old` changes nothing. The file is scanned and its tail shifted in
32 KiB windows, so memory does not grow with the file
([`write/edit.rs`](../../../../engine/crates/layerfs-store-service/src/write/edit.rs)).
Errors: `NoSuchPath`, `NotFile`, `Binary`, `NoMatch` or `AmbiguousMatch`
(`detail.matches`).

### file.rm

Removes a path, a folder with everything in it, as one Commit.

```text
JS      await layerfs.file.rm(project, branch, path, { message })
Python  layerfs.file.rm(project, branch, path, *, message=None)
CLI     layerfs file rm <project> <branch> <path> [--message <message>]
```

`/` is refused. A folder is removed depth first, one page of entries per
level at a time
([`write/recursive.rs`](../../../../engine/crates/layerfs-store-service/src/write/recursive.rs)).
Error: `NoSuchPath`.

### file.mkdir

A folder and any missing parents, as one Commit.

```text
JS      await layerfs.file.mkdir(project, branch, path, { message, mode })
Python  layerfs.file.mkdir(project, branch, path, *, message=None, mode=None)
CLI     layerfs file mkdir <project> <branch> <path> [--message <message>] [--mode <mode>]
```

`mode` (default 0o755) applies to `path` only; missing parents get 0o755.
An existing folder keeps its mode and the call answers `UpToDate`; a file or
symlink at `path` or at a parent is `NotDirectory`
([`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) `mkdir`).

### file.mv

Renames a path, as one Commit; the entry keeps its inode serial.

```text
JS      await layerfs.file.mv(project, branch, from_path, to_path, { message })
Python  layerfs.file.mv(project, branch, from_path, to_path, *, message=None)
CLI     layerfs file mv <project> <branch> <from-path> <to-path> [--message <message>]
```

`to_path` must not exist and must not be `from_path` or under it (one rename
with `replace: false`). Errors: `NoSuchPath`, `Exists`.

### file.cp

Copies a path, folders recursively and hard links kept, as one Commit.

```text
JS      await layerfs.file.cp(project, branch, from_path, to_path, { message })
Python  layerfs.file.cp(project, branch, from_path, to_path, *, message=None)
CLI     layerfs file cp <project> <branch> <from-path> <to-path> [--message <message>]
```

Same path rules as `file.mv`. New entries get new inode serials; a file seen
twice (a hard link) becomes a link to its first copy, tracked in the private
overlay's indexed records, not in memory; bytes are copied in 32 KiB
windows. Errors: `NoSuchPath`, `Exists`.

### file.apply

A list of changes applied in order, as one Commit; the first refused change
stops the call and nothing is committed.

```text
JS      await layerfs.file.apply(project, branch, changes, { message })
Python  layerfs.file.apply(project, branch, changes, *, message=None)
CLI     layerfs file apply <project> <branch> [--message <message>] [--from <file>] (content from stdin)
```

| `op` | Fields | Effect |
| --- | --- | --- |
| `write` | `path`, `content` (text or bytes), `mode?` | create or replace a file (new: 0o644) |
| `edit` | `path`, `old`, `new` | replace the one occurrence |
| `mkdir` | `path`, `mode?` | create it and missing parents (0o755) |
| `chmod` | `path`, `mode` | set permission bits |
| `rm` | `path` | remove, recursively |
| `cp` | `from_path`, `to_path` | copy, recursively, keeping hard links |
| `mv` | `from_path`, `to_path` | rename |

`changes` is a JSON list of records (the CLI reads it from stdin or
`--from`); an empty list commits nothing and answers `UpToDate`. A change that uses the Store's own names (`from`, `to`,
`oldText`, `newText`, `bytes`) is `InvalidInput`, so a folder scope sees
every path field
([`file/apply.rs`](../../../../api/crates/layerfs-api/src/file/apply.rs)).
Each change sees the ones before it. A refused change keeps its own code
with `detail.index` naming it.

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument, change or page; a path outside a folder scope (`out of scope: path`) | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs), [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project is not recorded | [`resolve.rs`](../../../../control/crates/layerfs-store/src/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | the Branch, Commit or Layer is missing | [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Capacity` | a numbered page or saved query cannot fit its work or reply budget; for a write, the Commit had no room | [`read/query.rs`](../../../../engine/crates/layerfs-store-service/src/read/query.rs) |
| `ReadOnly` | a write to `main` | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Conflict` | a write whose Branch head moved and a changed path conflicts; nothing committed (`detail.branch`, `head`, `conflicts`) | same |
| `HeadMoved`, `Busy`, `Failed` | a write's Commit failed and was settled | same |
| `Busy`, `Unresolved`, `Unavailable` | a push, pull or remove holds the Store; the worker is not reached | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `Unknown` | a read's worker answer was lost (call again); a write's change or Commit ended unknown (the private Workspace is kept) or its answer was lost: the Commit may exist, so read `branch.get` or `commit.list` before writing again | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs), [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Related

- [branch family](branch.md) (saved queries), [commit family](commit.md), [workspace family](workspace.md) for mounted writes.
- [Store calls](../../engine/store-calls/README.md), [reads](../../engine/store-calls/reads.md).
- [Errors and events](../calls/errors-and-events.md).
