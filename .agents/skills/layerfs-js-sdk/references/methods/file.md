# file calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Arguments and answer keys are the built table's
> (`layerfs help file`); outputs marked "seen" were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (ids shortened).
> Everything else is from the spec rows and the manual, not run.

```js
// reads: the Branch's head, or the Commit `at` names
file.read(project, branch, path, { at, line, limit })
file.stat(project, branch, path, { at })
file.list(project, branch, path, { at, limit, cursor })
file.glob(project, branch, pattern, path?, { exclude, at, limit, cursor })
file.grep(project, branch, pattern, path?, { include, exclude, ignore_case, at, limit, cursor })
file.history(project, branch, path, { at, traversal, limit, cursor })
file.blame(project, branch, path, { at, line, limit })

// writes: one Commit on the Branch each
file.write(project, branch, path, content, { mode, expected, message })
file.edit(project, branch, path, { old_string, new_string, replace_all, expected, message })
file.rm(project, branch, path, { recursive, expected, message })
file.mkdir(project, branch, path, { mode, expected, message })
file.mv(project, branch, from, to, { expected, message })
file.cp(project, branch, from, to, { expected, message })
file.apply(project, branch, changes, { expected, message })
```

Examples use `P = "shop/api"` and the Branch `"agent"`.

## Shared rules

- Paths are absolute, starting with `/` (`/src/cart.js`).
- **Reads** see committed state only, never a mounted Workspace's
  uncommitted changes. `at` reads one Commit of the Branch's history: a full
  id or a prefix of 8 or more characters. A prefix more than one Commit
  starts with is `InvalidInput` with `matches` (a count); a Branch name or a
  shorter prefix is `InvalidInput`. Every long form names what was read:
  `commit` (the full id), `root` and `path`.
- **Writes** change one Branch and make exactly one Commit, or nothing.
  `main` is refused `InvalidInput` (with a `fix` that forks a Branch). Each
  write answers `commit`, the new head's full id, plus, when set:
  `unchanged` (nothing changed; `commit` is the head), `merged` (the Branch
  moved during the call and the change was merged with it), `overwritten`
  (the paths [`/.lww`](../rules/lww.md) rules took, the first 20) and
  `overwritten_count` (only past 20). The long form adds `outcome`
  (`committed`, `merged` or `unchanged`), `branch`, `parent`, `root`,
  `message` and `created_at`.
- **`expected`** is the head guard: refuse `Conflict` unless the Branch's
  head is that Commit (an id or prefix); the refusal names the head as
  `commit`. Without it a moved head is merged.
- Without `message` the Commit says `Apply filesystem changes` (`-m` in the
  shell). An already-mounted Workspace doesn't see a write: its
  `workspace.status` answers `branch_moved: true`, and its next
  `workspace.commit` merges it in.
- `mode` is octal text, `"0000"` to `"7777"`, as `stat` answers it. A new
  file is `0644` and an existing one keeps its mode; `mkdir` gives the folder
  its `mode` and missing parents `0755`.
- **Pages:** `list`, `glob`, `grep` and `history` take `{ limit, cursor }`
  (`limit` 1 to 1000, default 100) and answer `{ items, next_cursor }`. The
  cursor carries the filters and the Commit the first page read, so a later
  page reads the same tree while the Branch moves on. A cursor given with
  other filters, or to another call, is `InvalidInput`; one whose Commit is
  gone, `NotFound`.

```js
const all = [];
let cursor;
do {
  const page = await layerfs.file.glob(P, "agent", "src/**/*.js", "/", { limit: 1000, cursor });
  all.push(...page.items);
  cursor = page.next_cursor;
} while (cursor);
```

**Refusals you will meet** (the raised code, under `detail: true`, is
`detail.code`):

| `error` | When | Do |
| --- | --- | --- |
| `NotFound` | No such path, Branch or Commit (`NoSuchPath`, `NoSuchBranch`, `NoSuchCommit`); a path `project.hide_path` hid; an edit's old text is absent | `file.list` the parent, `branch.list`, `commit.list` |
| `InvalidInput` | A write to `main`; a folder where a file is needed or the other way round; a binary file read as text; an edit text that matches more than once; a folder `rm` without `recursive`; a bad prefix, option or cursor | Read `message` and `fix` |
| `Conflict` | A path the write changed conflicts with what moved in; or the head is not `expected` | Read the files again, then redo the change |
| `Exists` | `cp` or `mv` onto a path that exists | Pick another target |
| `Unknown` | The outcome is not known | `commit.list` before acting again |

## `file.read`

Reads a text file, or a window of its lines: `line` (1-based, default 1) and
`limit` (lines, 1 to 2000, default 2000). Short: `text`, with `next_line`
while lines are left and `line_truncated` when a long line was cut. A file
that is not text answers `binary: true` and its `size` instead of `text`.

```bash
$ layerfs file read shop/api agent /README.md                                  # seen
{"text":"# x\n"}
$ layerfs file read shop/api agent /README.md --text                           # seen
# x
$ layerfs file read shop/api agent /docs/a.txt --line 2 --limit 1 --detail     # seen
{"text":"two\n","next_line":null,"line_truncated":false,"binary":false,"size":8,"commit":"123bef1a…","root":"74647323…","path":"/docs/a.txt","start_line":2,"end_line":2}
```

**Read a whole file, window by window:**

```js
let line = 1, text = "";
for (;;) {
  const r = await layerfs.file.read(P, "agent", "/src/cart.js", { line });
  text += r.text ?? "";
  if (r.next_line == null) break;
  line = r.next_line;
}
```

## `file.stat`

`type` (`file`, `dir` or `symlink`), `size` and `mode` as octal text; the
long form adds `commit`, `root`, `path`, `inode` and `modified_at`.

```bash
$ layerfs file stat shop/api agent /docs/a.txt                                 # seen
{"type":"file","size":8,"mode":"0644"}
```

```js
import layerfs, { LayerFSError } from "@layerfs/node";
const exists = await layerfs.file.stat(P, "agent", "/package.json").then(
  () => true,
  (e) => { if (e instanceof LayerFSError && e.error === "NotFound") return false; throw e; },
);
```

## `file.list`

A folder's entries, one page: rows `{ path, type }` (long: `inode`); the
answer's long form adds `skipped_hidden`.

```bash
$ layerfs file list shop/api agent /                                           # seen
{"items":[{"path":"/README.md","type":"file"},{"path":"/docs","type":"dir"}]}
$ layerfs file list shop/api agent / --text                                    # seen
PATH        TYPE
/README.md  file
/docs       dir
```

## `file.glob`

Paths under `path` (default `/`) that match `pattern`; each row is a path.
`exclude` takes a glob in the `/.lww` syntax that matches a path or one of
its folders (`exclude: "node_modules"` leaves out every `node_modules`
folder, and the walk does not enter it).

```bash
$ layerfs file glob shop/api agent '**/*.txt'                                  # seen
{"items":["/docs/a.txt"]}
```

## `file.grep`

Lines under `path` (default `/`) that contain `pattern`. **`pattern` is
literal text, not a regular expression.** `ignore_case` folds ASCII letters;
`include` and `exclude` take globs as `glob`'s `exclude` does. Rows are
`{ path, line, text }` (the line without its end), with `line_truncated`;
the answer's long form adds `skipped_binary`. For regular expressions run
`grep -rn` in a Workspace.

```bash
$ layerfs file grep shop/api agent two                                         # seen
{"items":[{"path":"/docs/a.txt","line":2,"text":"two"}]}
```

## `file.history`

The Commits that changed one path, newest first (was `file.revision`).
Rows `{ commit, change }` with `message`; `change` is `added`, `modified` or
`removed`. The long row adds `parent`, `created_at`, the path's `type`,
`size` and `mode` after the Commit, and which of `content`, `metadata` and
`identity` changed. `traversal` is `provenance` (default: follows the path
into the Branches it came from) or `branch` (this Branch's own Commits). `at`
starts at that Commit instead of the head.

```bash
$ layerfs file history shop/api agent /docs/a.txt                              # seen
{"items":[{"commit":"125470c8…","change":"modified","message":"edit a"},{"commit":"32919f55…","change":"added"}]}
```

## `file.blame`

The Commit that last changed each line, from `line` for `limit` lines (as
`read`). `ranges` are `{ start, end, commit }` runs of lines with one owner
(a range without `commit` has none found); `commits` names each owner once
(`{ commit, message }`; long adds `created_at`); `next_line`, and `partial`
when its history budget ran out or a line was cut.

```bash
$ layerfs file blame shop/api agent /docs/a.txt                                # seen
{"ranges":[{"start":1,"end":1,"commit":"32919f55…"},{"start":2,"end":2,"commit":"125470c8…"}],"commits":[{"commit":"32919f55…"},{"commit":"125470c8…","message":"edit a"}]}
```

To compare one file between two Commits or Branches, use
[`commit.diff`](commit.md#commitdiff) with `{ path, with: ["patch"] }`.

## `file.write`

Creates or replaces one file. `content` is a string (text) or a `Buffer` /
`Uint8Array` (bytes; Python `bytes`). In the shell the content comes from
stdin, or from `--from <file>` (a fourth positional is a usage error).

```js
await layerfs.file.write(P, "agent", "/src/cart.js", source, { message: "cart: add Cart class" });
```

```bash
$ layerfs file write shop/api agent /README.md -m 'Update the title'   # seen; the content came on stdin
{"commit":"12c19c06…"}
$ layerfs file write shop/api agent /README.md --detail                # seen; new content on stdin
{"commit":"123bef1a…","unchanged":false,"merged":false,"overwritten":[],"overwritten_count":null,"outcome":"committed","branch":"agent","parent":"12c19c06…","root":"74647323…","message":"Apply filesystem changes","created_at":"2026-11-18T22:04:15Z"}
```

## `file.edit`

Replaces `old_string` with `new_string` (both required). Without
`replace_all` it needs exactly one occurrence: none is `NotFound`, more than
one `InvalidInput` with `matches` and a `fix` that adds `--replace-all`. With
`replace_all` every occurrence is replaced and the answer says how many as
`replaced`. It is the cheap way to change a few lines of a big file. Shell:
`--old-string S` / `--old-string-from <file>`, `--new-string S` /
`--new-string-from <file>`.

```js
await layerfs.file.edit(P, "agent", "/src/index.js", { old_string: "retries = 3", new_string: "retries = 1", message: "one attempt" });
```

```bash
$ layerfs file edit shop/api agent /docs/a.txt --old-string two --new-string three -m 'edit a'   # seen
{"commit":"125470c8…"}
$ layerfs file edit shop/api agent /docs/a.txt --old-string nope --new-string three             # seen, exit 125
{"error":"NotFound","message":"the old text is not in /docs/a.txt"}
```

On a refusal nothing changed. Read the file as it is now (`file.grep` for the
text, then `file.read` around those lines) before choosing a new
`old_string`.

## `file.apply`

Applies an ordered list of changes as **one** Commit, or nothing. The best
way to scaffold or make any multi-file change. Each change is an object with
an `op` and that single call's fields; any other field is `InvalidInput`:

| `op` | Fields |
| --- | --- |
| `write` | `path`, `content` (text or bytes), `mode` |
| `edit` | `path`, `old_string`, `new_string`, `replace_all` |
| `mkdir` | `path`, `mode` |
| `chmod` | `path`, `mode` |
| `rm` | `path`, `recursive` |
| `cp`, `mv` | `from`, `to` |

Later changes see earlier ones: a `mkdir` before a `write` into that folder
works. The shell reads the JSON list from stdin or `--from <file>`.

```js
const files = {
  "/package.json": JSON.stringify({ name: "shop", type: "module", scripts: { test: "node --test" } }, null, 2) + "\n",
  "/src/server.js": "export function start() {}\n",
  "/test/server.test.js": "import { test } from 'node:test';\ntest('loads', () => {});\n",
};
const dirs = [...new Set(Object.keys(files).map((p) => p.slice(0, p.lastIndexOf("/"))).filter(Boolean))];
const r = await layerfs.file.apply(P, "agent", [
  ...dirs.map((path) => ({ op: "mkdir", path })),
  ...Object.entries(files).map(([path, content]) => ({ op: "write", path, content })),
], { message: "scaffold: package, server, first test" });
```

```bash
# from the manual's run (ids shortened there)
printf '%s' '[{"op":"write","path":"/b.txt","content":"b\\n"},{"op":"mkdir","path":"/src"},{"op":"mv","from":"/docs/a.txt","to":"/src/a.txt"}]' \
  | layerfs file apply shop/api agent -m batch
```

When a change fails, nothing is committed; fix that change, or the state it
assumed, and apply the whole list again.

## `file.mkdir`, `file.rm`, `file.cp`, `file.mv`

- `mkdir` makes a folder and its missing parents; an existing folder answers
  `unchanged`.
- `rm` removes a path; a folder that is not empty needs `recursive`
  (`InvalidInput` with a `fix` that adds it).
- `cp` copies a path, a folder with everything in it.
- `mv` moves or renames a path.

```bash
$ layerfs file mkdir shop/api agent /src/lib                                   # seen
{"commit":"128ee09b…"}
$ layerfs file cp shop/api agent /docs/a.txt /src/lib/a.txt                    # seen
{"commit":"12d6ccf6…"}
$ layerfs file mv shop/api agent /src/lib/a.txt /src/lib/b.txt                 # seen
{"commit":"126c1e95…"}
$ layerfs file rm shop/api agent /src --recursive                              # seen
{"commit":"1244862d…"}
```
