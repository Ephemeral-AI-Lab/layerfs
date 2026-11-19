# file family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `file` calls: read a Branch's files at its head or at a Commit without a mount, and change a Branch one Commit per call, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/file`](../../../../api/crates/layerfs-api/src/file) (one file per call), [`layerfs-store/src/calls/file`](../../../../control/crates/layerfs-store/src/calls/file), [`methods/file`](../../../../engine/crates/layerfs-store-service/src/methods/file), [`read/`](../../../../engine/crates/layerfs-store-service/src/read), [`write/`](../../../../engine/crates/layerfs-store-service/src/write) |
| Runs by default | where the Store's calls run: the caller's process for PostgreSQL and for sqlite on a locally executing machine, else the Store machine's worker (`store.call`). No daemon serves them |
| Design | [Store calls](../../engine/store-calls/README.md), [reads](../../engine/store-calls/reads.md) |

## Calls

```text
 reads: at a Branch's head, or --at C          writes: one Commit on a Branch
 (an immutable snapshot; no lane)              (never main)

   read  stat  list  glob  grep                write  edit  rm  mkdir
   history  blame                              mv  cp  apply (many in one)
                                                       |
                                    private Workspace bound to the Branch head
                                    --> one Commit (a moved head is merged)
                                    --> commit | unchanged | Conflict
```

| Call | Kind | Answer | MCP |
| --- | --- | --- | --- |
| [`file.read`](#fileread) | read, line window | `text`, or `binary` and `size` | `work` |
| [`file.stat`](#filestat) | read | `type`, `size`, `mode` | `admin` |
| [`file.list`](#filelist) | read, paged | `{path, type}` rows | `work` |
| [`file.glob`](#fileglob) | read, paged | paths | `work` |
| [`file.grep`](#filegrep) | read, paged | `{path, line, text}` rows | `work` |
| [`file.history`](#filehistory) | read, paged | `{commit, change}` rows | `admin` |
| [`file.blame`](#fileblame) | read, line window | `ranges`, `commits` | `admin` |
| [`file.write`](#filewrite) | write | `commit` | `work` |
| [`file.edit`](#fileedit) | write | `commit` | `work` |
| [`file.rm`](#filerm) | write | `commit` | `work` |
| [`file.mkdir`](#filemkdir) | write | `commit` | `admin` |
| [`file.mv`](#filemv) | write | `commit` | `work` |
| [`file.cp`](#filecp) | write | `commit` | `admin` |
| [`file.apply`](#fileapply) | write | `commit` | `admin` |

Each call file builds one request and calls
[`layerfs_store::calls::file`](../../../../control/crates/layerfs-store/src/calls/file)
(`read.rs` for the reads, `write.rs` for the writes), which sends one
`StoreOp::File` operation on the project's Store and maps its answer to the
call's long form. Shared rules:

| Rule | Detail | Source |
| --- | --- | --- |
| Version | a read reads the Branch's head, or `--at C`: a full Commit id or a prefix of 8 or more characters (a prefix more than one Commit starts with is `InvalidInput` with `matches`; a Branch name or a shorter prefix is `InvalidInput`, never read as a Branch). Every long form names the Commit read as `commit` (its full id), its `root` and the `path`. Reads never see a live Workspace | [`read/snapshot.rs`](../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| Paths | absolute, starting with `/`, at most 4096 bytes, no NUL, no trailing `/`, no empty, `.` or `..` component | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| Pages | `list`, `glob`, `grep` and `history` answer `items` and `next_cursor`; `limit` is 1 to 1000 (100 by default), and a page also ends early at the reply budget, with a cursor. The cursor carries the call's filters and the Commit its first page read, so a later page reads the same tree while the Branch moves on. A cursor given with other filters or another pin, or to another call, is `InvalidInput`; one whose Commit is gone, `NotFound` | [`read.rs`](../../../../control/crates/layerfs-store/src/calls/file/read.rs), [`cursor.rs`](../../../../control/crates/layerfs-home/src/cursor.rs) |
| Line windows | `read` and `blame` take `--line` (1-based, default 1) and `--limit` (1 to 2000, default 2000) and answer `next_line` while lines are left | [`read/window.rs`](../../../../engine/crates/layerfs-store-service/src/read/window.rs) |
| Writes | through a private Workspace bound to the Branch's head, then one Commit through the Store's single writer. A write to `main` is `InvalidInput` (raised `ReadOnly`) with a `fix` that forks a Branch; `main` changes only through `branch.publish` | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| Write answer | `commit`, the new Commit's full id; `unchanged` when nothing changed (`commit` is then the head); `merged` when the Branch moved during the call and the change was merged; `overwritten` (the first 20, then `overwritten_count`) for the paths `.lww` rules took. The long form adds `outcome` (`committed`, `merged` or `unchanged`), `branch`, `parent`, `root`, `message` and `created_at` | [`write.rs`](../../../../control/crates/layerfs-store/src/calls/file/write.rs) |
| Head guard | `--expected C`: the head the call's private Workspace bound to must be `C` (an id or prefix), else `Conflict` naming the head as `commit` | same |
| Message | `-m` in the shell; without one the Commit message is `Apply filesystem changes` | same |
| Mode | octal text, four digits `0000` to `7777` (`"0644"`), as `stat` answers it. A new file is `0644` and an existing one keeps its mode; a folder `mkdir` makes gets `--mode`, missing parents `0755` | [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| Scope | every path must be the scope's folder or under it; Branch names are composed into the scope's namespace; `file.history` is refused (`out of scope: history`) | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |

### file.read

Read a text file, or a window of its lines.

```text
CLI     layerfs file read <project> <branch> <path> [--at C] [--line 1] [--limit 2000]
Node    await layerfs.file.read(project, branch, path, { at, line, limit })
Python  layerfs.file.read(project, branch, path, *, at=None, line=None, limit=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path |
| `at` option | name | read as of this Commit: a full id, or a prefix of 8 or more characters |
| `line` option | int | the first line, 1-based; default `1` |
| `limit` option | int | the most lines, 1 to 2000; default `2000` |

| Answer | Keys |
| --- | --- |
| short | each when set: `text`, `next_line`, `line_truncated`, `binary`, `size` |
| long | `text, next_line, line_truncated, binary, size, commit, root, path, start_line, end_line` |
| call | a line window: `line` and `limit`; writes one call-log line; MCP `work` |

`text` is the window's lines (an empty file is `""`); `next_line` is the
line to read next while lines are left; `line_truncated` says a long line
was cut. A file that is not text answers `binary: true` and its `size`
instead of `text` (the short form shows `size` only then; the long form
always). `start_line` and `end_line` (long form) are the window read.

Run on macOS native with a debug build:

```console
$ layerfs file read shop/api agent /README.md
{"text":"# x\n"}
$ layerfs file read shop/api agent /docs/a.txt --line 2 --limit 1 --detail
{"text":"two\n","next_line":null,"line_truncated":false,"binary":false,"size":8,"commit":"123bef1a0b054d1a99e54867b7d9aa77affb40ac0e6ebb9c2d55ca158db3bc19bd","root":"746473234fcd23a82857b1b05761ef2a9781d2076ad84d008d11e14562f2d2f2","path":"/docs/a.txt","start_line":2,"end_line":2}
```

### file.stat

A path's type, size and mode.

```text
CLI     layerfs file stat <project> <branch> <path> [--at C]
Node    await layerfs.file.stat(project, branch, path, { at })
Python  layerfs.file.stat(project, branch, path, *, at=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path |
| `at` option | name | read as of this Commit: a full id, or a prefix of 8 or more characters |

| Answer | Keys |
| --- | --- |
| short | `type`, `size`, `mode` |
| long | `type, size, mode, commit, root, path, inode, modified_at` |
| call | writes one call-log line; MCP `admin` |

`type` is `file`, `dir` or `symlink`; `mode` is octal text (`"0644"`). The
long form adds `inode` (the entry's stable serial) and `modified_at`.

```console
$ layerfs file stat shop/api agent /docs/a.txt
{"type":"file","size":8,"mode":"0644"}
```

### file.list

A folder's entries.

```text
CLI     layerfs file list <project> <branch> <path> [--at C] [--limit 100] [--cursor C]
Node    await layerfs.file.list(project, branch, path, { at, limit, cursor })
Python  layerfs.file.list(project, branch, path, *, at=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path of a folder |
| `at` option | name | read as of this Commit: a full id, or a prefix of 8 or more characters |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the pinned Commit and the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor, commit, root, path, skipped_hidden` |
| `items[]` short | `path`, `type` |
| `items[]` long | `path, type, inode` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `work` |

A row is `{path, type}` (the listing reads no size or mode); the long row
adds `inode`. The long answer adds `skipped_hidden`, the entries
`project hide-path` hides.

```console
$ layerfs file list shop/api agent /
{"items":[{"path":"/README.md","type":"file"},{"path":"/docs","type":"dir"}]}
```

### file.glob

Paths under a folder that match a glob.

```text
CLI     layerfs file glob <project> <branch> <pattern> [path] [--exclude G] [--at C] [--limit 100] [--cursor C]
Node    await layerfs.file.glob(project, branch, pattern, path?, { exclude, at, limit, cursor })
Python  layerfs.file.glob(project, branch, pattern, path=None, *, exclude=None, at=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `pattern` | text | a glob |
| `path` | text | the folder searched; optional, default `/` |
| `exclude` option | text | a glob of paths to leave out, such as node_modules |
| `at` option | name | read as of this Commit: a full id, or a prefix of 8 or more characters |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the pinned Commit and the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor, commit, root, path, skipped_hidden` |
| `items[]` short | each a `path` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `work` |

A row is a path. `--exclude` takes a glob in the `.lww` rules syntax that
matches a path or one of its folders, so `--exclude node_modules` leaves out
every `node_modules` folder and the walk does not enter it.

```console
$ layerfs file glob shop/api agent **/*.txt
{"items":["/docs/a.txt"]}
```

### file.grep

Lines under a folder that contain a literal text.

```text
CLI     layerfs file grep <project> <branch> <pattern> [path] [--include G] [--exclude G] [--ignore-case] [--at C] [--limit 100] [--cursor C]
Node    await layerfs.file.grep(project, branch, pattern, path?, { include, exclude, ignore_case, at, limit, cursor })
Python  layerfs.file.grep(project, branch, pattern, path=None, *, include=None, exclude=None, ignore_case=None, at=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `pattern` | text | the literal text searched for |
| `path` | text | the folder searched; optional, default `/` |
| `include` option | text | search only paths that match this glob, such as *.ts |
| `exclude` option | text | a glob of paths to leave out, such as node_modules |
| `ignore_case` option | bool | match letters of either case |
| `at` option | name | read as of this Commit: a full id, or a prefix of 8 or more characters |
| `limit` option | int | the most matching lines in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the pinned Commit and the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor, commit, root, path, skipped_binary` |
| `items[]` short | `path`, `line`, `text`; when set: `line_truncated` |
| `items[]` long | `path, line, text, line_truncated` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `work` |

Searches literal text, line by line; `--ignore-case` folds ASCII letters. A
row is `{path, line, text}` (the line without its end), with
`line_truncated` when a long line was cut. `--include` and `--exclude` take
globs as `glob --exclude` does. The long answer adds `skipped_binary`, the
files that are not text.

```console
$ layerfs file grep shop/api agent two
{"items":[{"path":"/docs/a.txt","line":2,"text":"two"}]}
```

### file.history

The Commits that changed one path, newest first.

```text
CLI     layerfs file history <project> <branch> <path> [--at C] [--traversal provenance] [--limit 100] [--cursor C]
Node    await layerfs.file.history(project, branch, path, { at, traversal, limit, cursor })
Python  layerfs.file.history(project, branch, path, *, at=None, traversal=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path |
| `at` option | name | start at this Commit instead of the Branch's head: a full id, or a prefix of 8 or more characters |
| `traversal` option | text | `provenance` follows the path into the Branches it came from; `branch` stays on this Branch's own Commits; default `provenance` |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the pinned Commit and the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor, commit, root, path` |
| `items[]` short | `commit`, `change`; when set: `message` |
| `items[]` long | `commit, change, message, parent, created_at, type, size, mode, content, metadata, identity` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `admin` |

The Commits that changed one path, newest first. A row is `{commit,
change}` with its `message`; `change` is `added`, `modified` or `removed`.
The long row adds `parent`, `created_at`, the path's `type`, `size` and
`mode` after the Commit, and which of `content`, `metadata` and `identity`
changed. A publication on `main` has no `message` or `created_at`.
`--traversal provenance` (the default) follows a path through merges into
the Branches it came from; `--traversal branch` stays on the Branch's own
Commits ([`read/revision.rs`](../../../../engine/crates/layerfs-store-service/src/read/revision.rs)).
It was `file.revision`, removed with no alias.

```console
$ layerfs file history shop/api agent /docs/a.txt
{"items":[{"commit":"125470c85d202c48b9d6e8f7c9632e4981412d22596bb3cfd36c2eebe3e0ee3f00","change":"modified","message":"edit a"},{"commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c","change":"added"}]}
```

### file.blame

The Commit that last changed each line of a text file.

```text
CLI     layerfs file blame <project> <branch> <path> [--at C] [--line 1] [--limit 2000]
Node    await layerfs.file.blame(project, branch, path, { at, line, limit })
Python  layerfs.file.blame(project, branch, path, *, at=None, line=None, limit=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path |
| `at` option | name | read as of this Commit: a full id, or a prefix of 8 or more characters |
| `line` option | int | the first line, 1-based; default `1` |
| `limit` option | int | the most lines, 1 to 2000; default `2000` |

| Answer | Keys |
| --- | --- |
| short | `ranges`, `commits`; when set: `next_line`, `partial` |
| long | `ranges, commits, next_line, partial, commit, root, path` |
| `ranges[]` short | `start`, `end`; when set: `commit` |
| `ranges[]` long | `start, end, commit` |
| `commits[]` short | `commit`; when set: `message` |
| `commits[]` long | `commit, message, created_at` |
| call | a line window: `line` and `limit`; writes one call-log line; MCP `admin` |

`ranges` are runs of lines with one owner (`{start, end, commit}`; a range
without `commit` has none found); `commits` lists each owner once
(`{commit, message}`; the long form adds `created_at`). `partial` says the
history budget ran out or a line was cut. Blame runs at immutable roots,
never at a live Workspace
([`read/blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs)).

```console
$ layerfs file blame shop/api agent /docs/a.txt
{"ranges":[{"start":1,"end":1,"commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c"},{"start":2,"end":2,"commit":"125470c85d202c48b9d6e8f7c9632e4981412d22596bb3cfd36c2eebe3e0ee3f00"}],"commits":[{"commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c"},{"commit":"125470c85d202c48b9d6e8f7c9632e4981412d22596bb3cfd36c2eebe3e0ee3f00","message":"edit a"}]}
```

### file.write

Write a file; one Commit.

```text
CLI     layerfs file write <project> <branch> <path> [--mode M] [--expected C] [-m msg] [--from <file>] (content from stdin)
Node    await layerfs.file.write(project, branch, path, content, { mode, expected, message })
Python  layerfs.file.write(project, branch, path, content, *, mode=None, expected=None, message=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path |
| `content` | content | the file's bytes, text or bytes; the shell reads stdin or `--from <file>` |
| `mode` option | text | permission bits in octal, such as 0644, spelled as `file stat` answers them; a new file is 0644 and an existing one keeps its mode |
| `expected` option | name | refuse `Conflict` unless the Branch's head is this Commit |
| `message` option | text | the Commit message; `-m` in the shell; default `Apply filesystem changes` |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `merged`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, merged, overwritten, overwritten_count, outcome, branch, parent, root, message, created_at` |
| call | writes one call-log line; MCP `work` |

Creates or replaces a file's whole content. In the shell the content comes
from stdin, or from `--from <file>`; a fourth positional is a wrong command
line (exit 2). The file is written in pieces of at most 65,519 bytes, and a
piece equal to what is there is not rewritten. A parent that is not a folder
or a folder at `path` is `InvalidInput`.

Run on macOS native with a debug build (content on stdin):

```console
$ layerfs file write shop/api agent /README.md -m 'Update the title'
{"commit":"12c19c069d2e56f16561e40ba8d85223b7ed0440c94027f0e6407eb9e185d2acf7"}
$ layerfs file write shop/api agent /README.md --detail
{"commit":"123bef1a0b054d1a99e54867b7d9aa77affb40ac0e6ebb9c2d55ca158db3bc19bd","unchanged":false,"merged":false,"overwritten":[],"overwritten_count":null,"outcome":"committed","branch":"agent","parent":"12c19c069d2e56f16561e40ba8d85223b7ed0440c94027f0e6407eb9e185d2acf7","root":"746473234fcd23a82857b1b05761ef2a9781d2076ad84d008d11e14562f2d2f2","message":"Apply filesystem changes","created_at":"2026-11-18T22:04:15Z"}
```

### file.edit

Replace a text in a file; one Commit.

```text
CLI     layerfs file edit <project> <branch> <path> --old-string S --new-string S [--replace-all] [--expected C] [-m msg]
Node    await layerfs.file.edit(project, branch, path, { old_string, new_string, replace_all, expected, message })
Python  layerfs.file.edit(project, branch, path, *, old_string, new_string, replace_all=None, expected=None, message=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path |
| `old_string` option | text | the text to replace; required |
| `new_string` option | text | its replacement; required |
| `replace_all` option | bool | replace every occurrence instead of exactly one |
| `expected` option | name | refuse `Conflict` unless the Branch's head is this Commit |
| `message` option | text | the Commit message; `-m` in the shell; default `Apply filesystem changes` |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `replaced`, `merged`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, replaced, merged, overwritten, overwritten_count, outcome, branch, parent, root, message, created_at` |
| call | writes one call-log line; MCP `work` |

Without `--replace-all` the old text must occur exactly once; with it, every
occurrence is replaced. The long form says how many as `replaced` (1 for a
plain edit); the short form shows it only when more than one. The file is
scanned and its tail shifted in 32 KiB windows, so memory does not grow with
the file ([`write/edit.rs`](../../../../engine/crates/layerfs-store-service/src/write/edit.rs)).

| Refusal | When | Next |
| --- | --- | --- |
| `NotFound` | the old text is not in the file (raised `NoMatch`), or the path is missing | |
| `InvalidInput` | the old text occurs more than once without `--replace-all` (`matches`, a count); the file is not text or not a file | `fix`: the same edit with `--replace-all` |

```console
$ layerfs file edit shop/api agent /docs/a.txt --old-string two --new-string three -m 'edit a'
{"commit":"125470c85d202c48b9d6e8f7c9632e4981412d22596bb3cfd36c2eebe3e0ee3f00"}
$ layerfs file edit shop/api agent /docs/a.txt --old-string nope --new-string three
{"error":"NotFound","message":"the old text is not in /docs/a.txt"}
```

### file.rm

Remove a path; one Commit.

```text
CLI     layerfs file rm <project> <branch> <path> [--recursive] [--expected C] [-m msg]
Node    await layerfs.file.rm(project, branch, path, { recursive, expected, message })
Python  layerfs.file.rm(project, branch, path, *, recursive=None, expected=None, message=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path |
| `recursive` option | bool | remove a folder with everything in it |
| `expected` option | name | refuse `Conflict` unless the Branch's head is this Commit |
| `message` option | text | the Commit message; `-m` in the shell; default `Apply filesystem changes` |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `merged`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, merged, overwritten, overwritten_count, outcome, branch, parent, root, message, created_at` |
| call | writes one call-log line; MCP `work` |

`/` is refused. A folder that is not empty needs `--recursive`
(`InvalidInput` with a `fix` that adds it); it is then removed depth first,
one page of entries per level at a time
([`write/recursive.rs`](../../../../engine/crates/layerfs-store-service/src/write/recursive.rs)).

```console
$ layerfs file rm shop/api agent /src --recursive
{"commit":"1244862d14dfce305c52f94fea57d0a31815c22d7d7cb8d03f1568433a06200a65"}
```

### file.mkdir

Make a folder and its missing parents; one Commit.

```text
CLI     layerfs file mkdir <project> <branch> <path> [--mode M] [--expected C] [-m msg]
Node    await layerfs.file.mkdir(project, branch, path, { mode, expected, message })
Python  layerfs.file.mkdir(project, branch, path, *, mode=None, expected=None, message=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `path` | text | an absolute path |
| `mode` option | text | permission bits of the folder when it is made, in octal, such as 0755; missing parents get 0755 |
| `expected` option | name | refuse `Conflict` unless the Branch's head is this Commit |
| `message` option | text | the Commit message; `-m` in the shell; default `Apply filesystem changes` |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `merged`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, merged, overwritten, overwritten_count, outcome, branch, parent, root, message, created_at` |
| call | writes one call-log line; MCP `admin` |

`--mode` applies to `path` only; missing parents get `0755`. An existing
folder keeps its mode and the call answers `unchanged`; a file or symlink at
`path` or at a parent is `InvalidInput`.

```console
$ layerfs file mkdir shop/api agent /src/lib
{"commit":"128ee09ba9808b5a908d949bf541bd24416f1283b6f3575fe4c15ce089a3246c09"}
```

### file.mv

Move a path; one Commit.

```text
CLI     layerfs file mv <project> <branch> <from> <to> [--expected C] [-m msg]
Node    await layerfs.file.mv(project, branch, from, to, { expected, message })
Python  layerfs.file.mv(project, branch, from_, to, *, expected=None, message=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `from` | text | the absolute path moved |
| `to` | text | the absolute path it moves to |
| `expected` option | name | refuse `Conflict` unless the Branch's head is this Commit |
| `message` option | text | the Commit message; `-m` in the shell; default `Apply filesystem changes` |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `merged`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, merged, overwritten, overwritten_count, outcome, branch, parent, root, message, created_at` |
| call | writes one call-log line; MCP `work` |

One rename: the entry keeps its inode serial. `to` must not exist and must
not be `from` or under it (`Exists`, `InvalidInput`).

```console
$ layerfs file mv shop/api agent /src/lib/a.txt /src/lib/b.txt
{"commit":"126c1e9504d75d9d52ab2037db141f6ca96a8118de9519809ea01f3411e6fce4ed"}
```

### file.cp

Copy a path, a folder with everything in it; one Commit.

```text
CLI     layerfs file cp <project> <branch> <from> <to> [--expected C] [-m msg]
Node    await layerfs.file.cp(project, branch, from, to, { expected, message })
Python  layerfs.file.cp(project, branch, from_, to, *, expected=None, message=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `from` | text | the absolute path copied |
| `to` | text | the absolute path of the copy |
| `expected` option | name | refuse `Conflict` unless the Branch's head is this Commit |
| `message` option | text | the Commit message; `-m` in the shell; default `Apply filesystem changes` |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `merged`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, merged, overwritten, overwritten_count, outcome, branch, parent, root, message, created_at` |
| call | writes one call-log line; MCP `admin` |

Copies a path, a folder with everything in it, with `mv`'s path rules. New
entries get new inode serials; a file seen twice (a hard link) becomes a
link to its first copy, tracked in the private overlay's indexed records,
not in memory; bytes are copied in 32 KiB windows.

```console
$ layerfs file cp shop/api agent /docs/a.txt /src/lib/a.txt
{"commit":"12d6ccf6b37d590640d1bddd29622928ad04aa505886f8000d4c8b0bc56af0aa3c"}
```

### file.apply

Apply a list of changes; one Commit, all or none.

```text
CLI     layerfs file apply <project> <branch> [--expected C] [-m msg] [--from <file>] (content from stdin)
Node    await layerfs.file.apply(project, branch, changes, { expected, message })
Python  layerfs.file.apply(project, branch, changes, *, expected=None, message=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |
| `changes` | json | a JSON list of changes; the shell reads stdin or `--from <file>` |
| `expected` option | name | refuse `Conflict` unless the Branch's head is this Commit |
| `message` option | text | the Commit message; `-m` in the shell; default `Apply filesystem changes` |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `merged`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, merged, overwritten, overwritten_count, outcome, branch, parent, root, message, created_at` |
| call | writes one call-log line; MCP `admin` |

A list of changes applied in order as one Commit: each change sees the ones
before it, and the first refused change stops the call with nothing
committed (the refusal keeps its own code and names the change by its
index). An empty list commits nothing and answers `unchanged`. In the shell
the JSON comes from stdin, or from `--from <file>`.

| `op` | Fields | Effect |
| --- | --- | --- |
| `write` | `path`, `content` (text or bytes), `mode` | create or replace a file |
| `edit` | `path`, `old_string`, `new_string`, `replace_all` | replace a text |
| `mkdir` | `path`, `mode` | make it and missing parents |
| `chmod` | `path`, `mode` | set permission bits |
| `rm` | `path`, `recursive` | remove a path |
| `cp` | `from`, `to` | copy, keeping hard links |
| `mv` | `from`, `to` | move |

Any other field is refused `InvalidInput`, so a folder scope sees every path
([`file/apply.rs`](../../../../api/crates/layerfs-api/src/file/apply.rs)).

Run (content shown as in the [2.0.0 manual](../../../versioned/2.0.0/calls.md#file-14); not run for this page):

```bash
printf '%s' '[{"op":"write","path":"/b.txt","content":"b\n"},{"op":"mkdir","path":"/src"},{"op":"mv","from":"/docs/a.txt","to":"/src/a.txt"}]' \
  | layerfs file apply shop/api agent -m batch    # (not run)
```

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument, change or cursor; a write to `main` (raised `ReadOnly`); a folder where a file is needed, a binary file read as text, a non-empty folder without `--recursive`; a path outside a folder scope (`out of scope: path`) | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs), [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NotFound` | the Store, project, Branch, Commit or path is missing; a path the project hides | [`resolve.rs`](../../../../control/crates/layerfs-store/src/resolve.rs), [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Exists` | `mv` or `cp` onto a path that exists | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `Conflict` | the head is not `--expected`, or the Branch moved during the write and a changed path conflicts; nothing is committed; `commit` names the head | same |
| `Capacity` | the Commit had no room | same |
| `Busy`, `Unresolved`, `Unavailable` | a move or remove holds the Store; the worker is not reached | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `Unknown` | a read's worker answer was lost (call again); a write's Commit ended unknown or its answer was lost: the Commit may exist, so read `branch get` or `commit list` before writing again | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs), [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Related

- [branch family](branch.md), [commit family](commit.md) (`commit diff --path` compares one path between two Commits), [workspace family](workspace.md) for mounted writes.
- [Store calls](../../engine/store-calls/README.md), [reads](../../engine/store-calls/reads.md).
- [Errors and events](../calls/errors-and-events.md).
