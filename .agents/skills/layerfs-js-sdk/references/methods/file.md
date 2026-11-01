# file calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. The family was `fs` in 1.1.0 (renamed by O13).
> Results were captured from the built `layerfs` command and the Node package
> in a private home on 2026-11-02; ids are shortened.

```js
// reads: the Branch's head, or the Commit given
file.read(project, branch, path, commit?, { start_line, end_line })
file.stat(project, branch, path, commit?)
file.list(project, branch, path, commit?, { page_size, page_pos })
file.glob(project, branch, path, pattern, commit?, { page_size, page_pos })
file.grep(project, branch, path, pattern, commit?, { page_size, page_pos })
file.blame(project, branch, path, commit?, { from_line })
file.diff(project, left_branch, left_path, right_branch, right_path, { left_commit, right_commit })

// writes: one Commit on the Branch's head each
file.write(project, branch, path, content, { message, mode })
file.edit(project, branch, path, { old, new, message })
file.rm(project, branch, path, { message })
file.mkdir(project, branch, path, { message, mode })
file.mv(project, branch, from_path, to_path, { message })
file.cp(project, branch, from_path, to_path, { message })
file.apply(project, branch, changes, { message })
```

Examples use `P = "shop/api"` and the Branch `"work"`.

## Shared rules

- Paths are absolute and canonical (`/src/cart.js`). A relative path is refused
  `InvalidInput`.
- **Reads** see committed state only, never a mounted Workspace's uncommitted
  changes. Give a full Commit id as `commit` to read an older state; on `main`
  a Layer id works too. In this build the Commit decides what is read and the
  Branch is not checked against it, so pass a Commit of that Branch.
- **Writes** change one Branch (never `main`: `ReadOnly`) and make exactly one
  Commit, or nothing. An already-mounted Workspace doesn't see the change: mount
  again, or let its next `workspace.commit` merge it in. Parent folders must
  exist; `file.mkdir` creates missing parents. `message` is optional UTF-8, at
  most 4096 bytes.
- **Pages:** `list`, `glob` and `grep` return `{ items, has_more, next_page_pos }`.
  `page_size` is 1–1000, default 50. A page can be shorter than `page_size`;
  that is not the end while `has_more` is true. Deep offsets skip linearly, so
  narrow the path or pattern rather than paging far.
- **Single-path reads** start with `{ kind: "ok", snapshot, root, path }`;
  `snapshot` labels what was read, such as `"work@a99d99faf59f"`.

```js
const all = [];
for (let pos = 0; pos !== null; ) {
  const page = await layerfs.file.glob(P, "work", "/", "src/**/*.js", { page_size: 1000, page_pos: pos });
  all.push(...page.items);
  pos = page.has_more ? page.next_page_pos : null;
}
```

**Shared refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `NoSuchPath` | The path, or a write's parent folder, doesn't exist | `file.list` the parent; `file.mkdir` first |
| `NoSuchBranch`, `NoSuchCommit` | Wrong Branch or Commit | `branch.list`, `commit.list` |
| `ReadOnly` | A write on `main` | Write on your Branch, then `branch.publish` |
| `HeadMoved` | Another writer committed on the Branch during the write; nothing was written | Read the files again, then redo your change |
| `IsDirectory`, `NotDirectory` | A folder where a file is needed, or the other way round | `file.stat` it |
| `InvalidInput` | A bad argument: page bounds, line numbers, a relative path, a short Commit id | Read `message` |
| `Failed` | The call failed before committing | Stop and report `message` |

Write refusals carry `detail.index` (0 for single writes; the failing change
for `apply`).

## `file.read`

```js
const r = await layerfs.file.read(P, "work", "/src/math.js", { start_line: 2, end_line: 3 });
```

```json
{"kind":"ok","snapshot":"work@a99d99faf59f","root":"f55fa9660a73…","path":"/src/math.js",
 "text":"  return a + b;\n}\n","start_line":2,"end_line":3,"line_truncated":false,
 "truncated":false,"next_line":null,"size":46}
```

`text` keeps line endings. `truncated` is true when more lines remain in the
range; `next_line` is where the next window starts. **One answer holds at most
2000 lines and 48 KiB** (a 3000-line file came back as lines 1–2000 with
`next_line: 2001`). A huge single line comes back as a prefix
(`line_truncated`); read the rest in a Workspace. Shell `--text` prints the
text alone.

**Refusals:** `IsDirectory`, `Binary` (NUL or invalid UTF-8: look at it in a
Workspace), plus the shared ones.

**Read a whole file, window by window:**

```js
let line = 1, text = "";
for (;;) {
  const r = await layerfs.file.read(P, "work", "/src/cart.js", { start_line: line });
  text += r.text;
  if (r.next_line == null) break;
  line = r.next_line;
}
```

## `file.stat`

```json
{"kind":"ok","snapshot":"work@a99d99faf59f","root":"f55fa9660a73…","path":"/src/math.js","type":"file","inode":7,"size":46,"mode":420}
```

`type` is `file`, `dir` or `symlink`; `mode` is decimal (420 = `0644`).

```js
import layerfs, { LayerFSError } from "layerfs";
const exists = await layerfs.file.stat(P, "work", "/package.json").then(
  () => true,
  (e) => { if (e instanceof LayerFSError && e.code === "NoSuchPath") return false; throw e; },
);
```

## `file.list`

A folder's entries, as a page. Each item is `{ name, path, type, inode }`.

```bash
layerfs file list shop/api work / --text
```

```
NAME        PATH         TYPE  INODE
.gitignore  /.gitignore  file  2
.lww        /.lww        file  3
README.md   /README.md   file  4
src         /src         dir   5
```

## `file.glob`

Paths under `path` matching `pattern`, as a page of path strings. `*` and `?`
match within one name; `**` spans folders, and `**/` at a name boundary matches
zero or more folders.

```bash
layerfs file glob shop/api work / '**/*.js'
```

```json
{"kind":"ok","snapshot":"work@a99d99faf59f","root":"…","path":"/","items":["/src/index.js","/src/math.js"],"has_more":false,"next_page_pos":null}
```

## `file.grep`

Lines under `path` that contain `pattern`. **`pattern` is plain text, not a
regular expression**, whatever `layerfs help` says (checked: `retr.es` matched
nothing where `retries` matched). Each item is
`{ path, line, text, line_truncated }`; `skipped` counts binary files passed
over. It searches everything under `path`, `node_modules` included, so keep
`path` narrow. For regular expressions run `grep -rn` in a Workspace.

```json
{"kind":"ok","snapshot":"work@a99d99faf59f","root":"…","path":"/",
 "items":[{"path":"/src/index.js","line":1,"text":"export const retries = 1;\n","line_truncated":false}],
 "skipped":0,"has_more":false,"next_page_pos":null}
```

## `file.blame`

The Commit of each line, from `from_line` (default 1).

```json
{"kind":"ok","snapshot":"work@a99d99faf59f","root":"…","path":"/src/index.js",
 "lines":[{"n":1,"text":"export const retries = 1;","line_truncated":false,"clipped":false,
   "commit":"12a99d99faf5…","message":"one attempt"}],
 "origins":[],"versions":2,"comparison_coarse":false,"next_line":null}
```

## `file.diff`

The difference between two files; each side is a Branch and a path, with an
optional Commit option per side.

```js
const d = await layerfs.file.diff(P, "main", "/src/index.js", "work", "/src/index.js");
console.log(d.hunks.filter((h) => h.op !== " ").map((h) => h.op + h.text).join("\n"));
```

```json
{"kind":"Text","left":{"path":"/src/index.js","snapshot":"main@75838904b659","inode":6},
 "right":{"path":"/src/index.js","snapshot":"work@ad9dd3e33a2f","inode":6},
 "content_equal":false,"same_inode":true,"hunks_omitted":false,
 "hunks":[{"op":"-","text":"export const retries = 3;"},{"op":"+","text":"export const retries = 1;"},{"op":" ","text":""}]}
```

`kind` is `Text` (line diff in `hunks`), `Equal` or `Binary`. `op` is `" "`,
`"-"` or `"+"`. `hunks_omitted` is true when the diff was too large to return.
Shell: `layerfs file diff shop/api main /a.js work /a.js --left-commit <id>`.

## Write results

Every write returns one of:

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `Committed` | `branch`, `head`, `commit` | A new Commit; `head` is its id; `commit` is its record (as in [`commit.list`](commit.md#commitlist)) |
| `UpToDate` | `branch`, `head`, `root` | Nothing changed, so no Commit was made |

```json
{"kind":"Committed","branch":"work","head":"12c7ef811a49…","commit":{"id":"12c7ef811a49…","branch":"work",
 "parent":null,"second_parent":null,"base":"3275838904b6…","root":"0a12a8d94231…","via":"fs",
 "message":"add math","merged_from":null,"time":1793574067721,"logical":118,"new_canonical":888,
 "new_stored":400,"new_metadata":317,"displaced":null,"lww":[],"lww_truncated":false}}
```

No request-size limit showed in this pass: a 4 MB `file.write` committed. For
very large or many-file changes a Workspace step is still the better tool.

## `file.write`

Creates or replaces one file. `content` is a string (text) or a `Buffer` /
`Uint8Array` (bytes; Python `bytes`). In the shell the content comes from
stdin, or from `--from <file>`; UTF-8 input is sent as text, anything else as
bytes. `mode` sets the permission bits.

```js
await layerfs.file.write(P, "work", "/src/cart.js", source, { message: "cart: add Cart class" });
```

```bash
printf 'export function add(a, b) {\n  return a + b;\n}\n' | layerfs file write shop/api work /src/math.js -m "add math"
```

**Refusals:** `IsDirectory`, plus the shared ones.

## `file.edit`

Replaces exactly one occurrence of `old` with `new`. It streams large files, so
it is the cheap way to change a few lines of a big file. Shell: `--old <text>`
or `--old-from <file>`, `--new <text>` or `--new-from <file>`.

```js
await layerfs.file.edit(P, "work", "/src/index.js", { old: "retries = 3", new: "retries = 1", message: "one attempt" });
```

```bash
layerfs file edit shop/api work /src/index.js --old "retries = 3" --new "retries = 1" -m "one attempt"
```

| Code | Meaning | Do |
| --- | --- | --- |
| `NoMatch` | `old` doesn't occur (`detail.matches: 0`) | `file.read` the area; the text differs (whitespace, an earlier edit) |
| `AmbiguousMatch` | `old` occurs more than once (`detail.matches`) | Include more surrounding lines |
| `InvalidInput` | `old` is empty (the message still says `oldText`) | Pass the text to replace |
| `Binary` | The file is not text | Use a Workspace |

On `NoMatch` or `AmbiguousMatch` nothing changed. Read the file as it is now
(`file.grep` for the text, then `file.read` around those lines) before choosing
a new `old`.

## `file.apply`

Applies an ordered list of changes as **one** Commit, or nothing. The best way
to scaffold or make any multi-file change. Field names are snake_case:

| Change | Fields |
| --- | --- |
| write | `{ op: "write", path, content }` (`content` may be bytes) |
| edit | `{ op: "edit", path, old, new }` |
| mkdir | `{ op: "mkdir", path }` |
| rm | `{ op: "rm", path }` |
| cp | `{ op: "cp", from_path, to_path }` |
| mv | `{ op: "mv", from_path, to_path }` |

Later changes see earlier ones: a `mkdir` before a `write` into that folder
works. The shell reads the JSON list from stdin or `--from <file>`.

```js
const files = {
  "/package.json": JSON.stringify({ name: "shop", type: "module", scripts: { test: "node --test" } }, null, 2) + "\n",
  "/src/server.js": "export function start() {}\n",
  "/test/server.test.js": "import { test } from 'node:test';\ntest('loads', () => {});\n",
};
const dirs = [...new Set(Object.keys(files).map((p) => p.slice(0, p.lastIndexOf("/"))).filter(Boolean))];
const r = await layerfs.file.apply(P, "work", [
  ...dirs.map((path) => ({ op: "mkdir", path })),
  ...Object.entries(files).map(([path, content]) => ({ op: "write", path, content })),
], { message: "scaffold: package, server, first test" });
```

**When a change fails** the refusal is the first failing change's, with
`detail.index` (zero-based) naming it; nothing is committed. Live:
`{"code":"NoMatch","message":"/README.md","detail":{"matches":0,"index":1}}`.
Fix that change, or the state it assumed, and apply the whole list again.

## `file.mkdir`, `file.rm`, `file.cp`, `file.mv`

- `mkdir` creates a folder and any missing parents; an existing folder gives
  `UpToDate`. `NotDirectory` when a file is in the way.
- `rm` removes a file or a whole folder tree. `NoSuchPath` when it is missing.
- `cp` copies a file or folder tree; modes, symlinks and hardlinks within the
  copied set are kept, timestamps are new.
- `mv` moves or renames, keeping identity and timestamps.
- For `cp` and `mv`: `Exists` when `to_path` exists (live:
  `{"code":"Exists","message":"/README.md",…}`); `InvalidInput` for the same
  path or a copy into its own subtree.

```js
await layerfs.file.mkdir(P, "work", "/src/routes/admin", { message: "routes: admin folder" });
await layerfs.file.mv(P, "work", "/src/basket.js", "/src/cart.js", { message: "cart: rename basket" });
await layerfs.file.rm(P, "work", "/src/legacy", { message: "remove legacy folder" });
```
