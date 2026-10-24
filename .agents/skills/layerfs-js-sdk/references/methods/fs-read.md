# fs read methods

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Examples were
> captured live in issue #334 Phase A at `b30e4fb8f`. Opaque IDs are shortened;
> the full replies and trace are in `target/issue334/phase-a/`.

Examples use `layerfs` as connected in [SKILL.md](../../SKILL.md#connect-to-a-project) and work on the Branch
`"work"`.

**Shared rules**

- `ref` is `main`, a Branch name, a Commit id or a Layer id. Paths are absolute
  (`/src/cart.js`).
- Reads see committed state only: what a Branch or Commit holds, never a mounted
  Workspace's uncommitted changes. Look inside a Workspace with `workspace.exec`.
- Single-file answers start with `{ kind: "ok", snapshot, root, path }`; `snapshot`
  labels the state that was read, such as `"work@9f3c0a1b2c4d"`.

**Pages.** `list`, `grep`, `glob` (and `branch.list_commits`, the diffs and
`project.list_branches`) return `{ items, hasMore, nextPagePos }`. `pageSize` is 1–1000,
default 50; `pagePos` defaults to 0. A page can be shorter than `pageSize` when it hits
its byte bound; that never means the end while `hasMore` is true. Deep offsets skip
linearly, so narrow the path or pattern rather than paging far.

```js
const all = [];
for (let pos = 0; ; ) {
  const page = await layerfs.fs.glob("work", "src/**/*.js", "/", 1000, pos);
  all.push(...page.items);
  if (!page.hasMore) break;
  pos += page.items.length;
}
```

**Shared refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `NoSuchPath` | The path doesn't exist in that ref | `fs.list` the parent |
| `NoSuchBranch`, `NoSuchCommit` | Wrong ref | `project.list_branches`, `branch.list_commits` |
| `InvalidInput` | Bad argument: page bounds, line numbers, a relative path | Read `message` |

## `fs.read(ref, path, startLine?, endLine?)`

Reads a line range of a text file.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `ref` | string | yes | What to read from |
| `path` | string | yes | A regular file |
| `startLine` | number | no | 1-based, default 1 |
| `endLine` | number | no | Inclusive; default: the end of the file |

**Returns**

| Field | Meaning |
| --- | --- |
| `text` | The lines, with their line endings |
| `startLine`, `endLine` | First and last line returned (`endLine` is `null` if none) |
| `truncated` | True if more lines remain in the range, or a line was cut |
| `lineTruncated` | True if a single huge line was cut to a prefix |
| `nextLine` | Where the next window starts, or `null` |
| `size` | File size in bytes |

**Limits:** one answer holds at most 2000 lines and 48 KiB. A huge single line comes
back as a prefix; the rest of that line can't be read with `fs.read` (use `workspace.exec`).

**Refusals:** `IsDirectory`, `Binary` (NUL or invalid UTF-8: inspect it with `workspace.exec`), plus
the shared ones.

```js
const r = await layerfs.fs.read("work", "/src/cart.js", 1, 40);
```
```json
{
  "kind": "ok",
  "snapshot": "work@6a9c59c9084a",
  "root": "67fe8ac697a4…",
  "path": "/src/cart.js",
  "text": "const TAX = 0.08;\n// TODO: line 2\n// TODO: line 3\n// TODO: line 4\n// TODO: line 5\n// TODO: line 6\n// TODO: line 7\n// TODO: line 8\n// TODO: line 9\n// TODO: line 10\n// TODO: line 11\n// TODO: line 12\n// TODO: line 13\n// TODO: line 14\n// TODO: line 15\n// TODO: line 16\n// TODO: line 17\n// TODO: line 18\n// TODO: line 19\n// TODO: line 20\n// TODO: line 21\n// TODO: line 22\n// TODO: line 23\n// TODO: line 24\n// TODO: line 25\n// TODO: line 26\n// TODO: line 27\n// TODO: line 28\n// TODO: line 29\n// TODO: line 30\n// TODO: line 31\n// TODO: line 32\n// TODO: line 33\n// TODO: line 34\n// TODO: line 35\n// TODO: line 36\n// TODO: line 37\n// TODO: line 38\n// TODO: line 39\n// TODO: line 40\n",
  "startLine": 1,
  "endLine": 40,
  "lineTruncated": false,
  "truncated": false,
  "nextLine": null,
  "size": 1712
}
```

**Read a whole file, window by window**

```js
let line = 1, text = "";
for (;;) {
  const r = await layerfs.fs.read("work", "/src/cart.js", line);
  text += r.text;
  if (r.nextLine == null) break;
  line = r.nextLine;
}
```

## `fs.list(ref, path?, pageSize?, pagePos?)`

Lists one folder.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `path` | string | no | Folder, default `/` |

**Returns:** a page; each item is `{ name, path, type, inode }` with `type` one of
`file`, `dir`, `symlink`. Names are in byte order.

**Refusals:** `NotDirectory`, plus the shared ones.

```js
const page = await layerfs.fs.list("work", "/src");
```
```json
{
  "kind": "ok",
  "snapshot": "work@6a9c59c9084a",
  "root": "67fe8ac697a4…",
  "path": "/src",
  "items": [
    {
      "name": "basket.js",
      "path": "/src/basket.js",
      "type": "file",
      "inode": 2124
    },
    {
      "name": "cart.js",
      "path": "/src/cart.js",
      "type": "file",
      "inode": 2123
    },
    {
      "name": "routes",
      "path": "/src/routes",
      "type": "dir",
      "inode": 2247
    },
    {
      "name": "server.js",
      "path": "/src/server.js",
      "type": "file",
      "inode": 2183
    }
  ],
  "hasMore": false,
  "nextPagePos": null
}
```

## `fs.stat(ref, path)`

Describes one path.

**Returns:** `{ kind: "ok", snapshot, root, path, type, inode, size }`. `size` is
a file's length in bytes, a symlink's target length (as `lstat` reports it in a
mounted Workspace) and 0 for a folder.

```js
const exists = await layerfs.fs.stat("work", "/package.json").then(() => true, (e) => { if (isRefusal(e, "NoSuchPath")) return false; throw e; });
```

## `fs.grep(ref, pattern, path?, pageSize?, pagePos?)`

Finds lines that contain `pattern` in every text file under `path`.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `pattern` | string | yes | Plain text, matched as bytes. **Not a regular expression.** Not empty |
| `path` | string | no | Folder or file to search, default `/` |

**Returns:** a page; each item is `{ path, line, text, lineTruncated }`. `skipped`
counts binary files passed over. Items come in path order, then line order.

```js
const hits = await layerfs.fs.grep("work", "TODO", "/src");
```
This live capture shows the first two of 50 `items`; the raw output retains all rows.

```json
{
  "kind": "ok",
  "snapshot": "work@6a9c59c9084a",
  "root": "67fe8ac697a4…",
  "path": "/src",
  "items": [
    {
      "path": "/src/cart.js",
      "line": 2,
      "text": "// TODO: line 2\n",
      "lineTruncated": false
    },
    {
      "path": "/src/cart.js",
      "line": 3,
      "text": "// TODO: line 3\n",
      "lineTruncated": false
    }
  ],
  "skipped": 0,
  "hasMore": true,
  "nextPagePos": 50
}
```

**Notes:** searches everything under `path`, `node_modules` included. Keep `path` narrow.
For regular expressions, run `grep -rn` with `workspace.exec` in a Workspace.

## `fs.glob(ref, pattern, path?, pageSize?, pagePos?)`

Finds paths matching a glob pattern.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `pattern` | string | yes | `*` and `?` match within one name; `**` spans folders; `**/` at a name boundary matches zero or more folders, including direct children |
| `path` | string | no | Where to search, default `/` |

**Returns:** a page whose `items` are absolute path strings.

```js
const tests = await layerfs.fs.glob("work", "test/**/*.test.js", "/");
```
```json
{
  "kind": "ok",
  "snapshot": "work@6a9c59c9084a",
  "root": "67fe8ac697a4…",
  "path": "/",
  "items": [
    "/test/routes/cart-api.test.js"
  ],
  "hasMore": false,
  "nextPagePos": null
}
```

The output above preserves the Phase A capture at `b30e4fb8f`, which exposed
issue #334 BUG-001 by omitting a direct child. After the matcher fix, Phase B
step 005 checked `fs.glob('work', 'test/**/*.test.js', '/')` against the new
project and returned `items: ['/test/health.test.js']`; its full reply is retained
in `target/issue334/phase-b/005-test-m1.out`. The external matcher regression
also covers direct children and several nested levels.

## `fs.diff_files(leftRef, leftPath, rightRef, rightPath)`

Compares two files, from any refs and paths.

**Returns**

| Field | Meaning |
| --- | --- |
| `kind` | `"Text"` (line diff in `hunks`), `"Equal"` or `"Binary"` |
| `left`, `right` | Each side's ref, path and inode |
| `contentEqual` | True if the bytes are identical |
| `sameInode` | True if both sides are the same file identity |
| `hunks` | Lines as `{ op, text }`, where `op` is `" "`, `"-"` or `"+"` |
| `hunksOmitted` | True if the diff was too large to return |

```js
const d = await layerfs.fs.diff_files("main", "/src/cart.js", "work", "/src/cart.js");
console.log(d.hunks.filter((h) => h.op !== " ").map((h) => h.op + h.text).join(""));
```
```json
{
  "kind": "Text",
  "left": {
    "path": "/src/cart.js",
    "snapshot": "main@79dac7de68cf",
    "inode": 2123
  },
  "right": {
    "path": "/src/cart.js",
    "snapshot": "work@6a9c59c9084a",
    "inode": 2123
  },
  "contentEqual": false,
  "sameInode": true,
  "hunksOmitted": false,
  "hunks": [
    {
      "op": "-",
      "text": "const TAX = 0.07;"
    },
    {
      "op": "+",
      "text": "const TAX = 0.08;"
    },
    {
      "op": " ",
      "text": "// TODO: line 2"
    },
    {
      "op": " ",
      "text": "// TODO: line 3"
    },
    {
      "op": " ",
      "text": "// TODO: line 4"
    },
    {
      "op": " ",
      "text": "// TODO: line 5"
    },
    {
      "op": " ",
      "text": "// TODO: line 6"
    },
    {
      "op": " ",
      "text": "// TODO: line 7"
    },
    {
      "op": " ",
      "text": "// TODO: line 8"
    },
    {
      "op": " ",
      "text": "// TODO: line 9"
    },
    {
      "op": " ",
      "text": "// TODO: line 10"
    },
    {
      "op": " ",
      "text": "// TODO: line 11"
    },
    {
      "op": " ",
      "text": "// TODO: line 12"
    },
    {
      "op": " ",
      "text": "// TODO: line 13"
    },
    {
      "op": " ",
      "text": "// TODO: line 14"
    },
    {
      "op": " ",
      "text": "// TODO: line 15"
    },
    {
      "op": " ",
      "text": "// TODO: line 16"
    },
    {
      "op": " ",
      "text": "// TODO: line 17"
    },
    {
      "op": " ",
      "text": "// TODO: line 18"
    },
    {
      "op": " ",
      "text": "// TODO: line 19"
    },
    {
      "op": " ",
      "text": "// TODO: line 20"
    },
    {
      "op": " ",
      "text": "// TODO: line 21"
    },
    {
      "op": " ",
      "text": "// TODO: line 22"
    },
    {
      "op": " ",
      "text": "// TODO: line 23"
    },
    {
      "op": " ",
      "text": "// TODO: line 24"
    },
    {
      "op": " ",
      "text": "// TODO: line 25"
    },
    {
      "op": " ",
      "text": "// TODO: line 26"
    },
    {
      "op": " ",
      "text": "// TODO: line 27"
    },
    {
      "op": " ",
      "text": "// TODO: line 28"
    },
    {
      "op": " ",
      "text": "// TODO: line 29"
    },
    {
      "op": " ",
      "text": "// TODO: line 30"
    },
    {
      "op": " ",
      "text": "// TODO: line 31"
    },
    {
      "op": " ",
      "text": "// TODO: line 32"
    },
    {
      "op": " ",
      "text": "// TODO: line 33"
    },
    {
      "op": " ",
      "text": "// TODO: line 34"
    },
    {
      "op": " ",
      "text": "// TODO: line 35"
    },
    {
      "op": " ",
      "text": "// TODO: line 36"
    },
    {
      "op": " ",
      "text": "// TODO: line 37"
    },
    {
      "op": " ",
      "text": "// TODO: line 38"
    },
    {
      "op": " ",
      "text": "// TODO: line 39"
    },
    {
      "op": " ",
      "text": "// TODO: line 40"
    },
    {
      "op": " ",
      "text": "// TODO: line 41"
    },
    {
      "op": " ",
      "text": "// TODO: line 42"
    },
    {
      "op": " ",
      "text": "// TODO: line 43"
    },
    {
      "op": " ",
      "text": "// TODO: line 44"
    },
    {
      "op": " ",
      "text": "// TODO: line 45"
    },
    {
      "op": " ",
      "text": "// TODO: line 46"
    },
    {
      "op": " ",
      "text": "// TODO: line 47"
    },
    {
      "op": " ",
      "text": "// TODO: line 48"
    },
    {
      "op": " ",
      "text": "// TODO: line 49"
    },
    {
      "op": " ",
      "text": "// TODO: line 50"
    },
    {
      "op": " ",
      "text": "// TODO: line 51"
    },
    {
      "op": " ",
      "text": "// TODO: line 52"
    },
    {
      "op": " ",
      "text": "// TODO: line 53"
    },
    {
      "op": " ",
      "text": "// TODO: line 54"
    },
    {
      "op": " ",
      "text": "// TODO: line 55"
    },
    {
      "op": " ",
      "text": "// TODO: line 56"
    },
    {
      "op": " ",
      "text": "// TODO: line 57"
    },
    {
      "op": " ",
      "text": "// TODO: line 58"
    },
    {
      "op": " ",
      "text": "// TODO: line 59"
    },
    {
      "op": " ",
      "text": "// TODO: line 60"
    },
    {
      "op": " ",
      "text": "// TODO: line 61"
    },
    {
      "op": " ",
      "text": "// TODO: line 62"
    },
    {
      "op": " ",
      "text": "// TODO: line 63"
    },
    {
      "op": " ",
      "text": "// TODO: line 64"
    },
    {
      "op": " ",
      "text": "// TODO: line 65"
    },
    {
      "op": " ",
      "text": "// TODO: line 66"
    },
    {
      "op": " ",
      "text": "// TODO: line 67"
    },
    {
      "op": " ",
      "text": "// TODO: line 68"
    },
    {
      "op": " ",
      "text": "// TODO: line 69"
    },
    {
      "op": " ",
      "text": "// TODO: line 70"
    },
    {
      "op": " ",
      "text": "// TODO: line 71"
    },
    {
      "op": " ",
      "text": "// TODO: line 72"
    },
    {
      "op": " ",
      "text": "// TODO: line 73"
    },
    {
      "op": " ",
      "text": "// TODO: line 74"
    },
    {
      "op": " ",
      "text": "// TODO: line 75"
    },
    {
      "op": " ",
      "text": "// TODO: line 76"
    },
    {
      "op": " ",
      "text": "// TODO: line 77"
    },
    {
      "op": " ",
      "text": "// TODO: line 78"
    },
    {
      "op": " ",
      "text": "// TODO: line 79"
    },
    {
      "op": " ",
      "text": "// TODO: line 80"
    },
    {
      "op": " ",
      "text": "// TODO: line 81"
    },
    {
      "op": " ",
      "text": "// TODO: line 82"
    },
    {
      "op": " ",
      "text": "// TODO: line 83"
    },
    {
      "op": " ",
      "text": "// TODO: line 84"
    },
    {
      "op": " ",
      "text": "// TODO: line 85"
    },
    {
      "op": " ",
      "text": "// TODO: line 86"
    },
    {
      "op": " ",
      "text": "// TODO: line 87"
    },
    {
      "op": " ",
      "text": "// TODO: line 88"
    },
    {
      "op": " ",
      "text": "// TODO: line 89"
    },
    {
      "op": " ",
      "text": "// TODO: line 90"
    },
    {
      "op": " ",
      "text": "// TODO: line 91"
    },
    {
      "op": " ",
      "text": "// TODO: line 92"
    },
    {
      "op": " ",
      "text": "// TODO: line 93"
    },
    {
      "op": " ",
      "text": "// TODO: line 94"
    },
    {
      "op": " ",
      "text": "// TODO: line 95"
    },
    {
      "op": " ",
      "text": "// TODO: line 96"
    },
    {
      "op": " ",
      "text": "// TODO: line 97"
    },
    {
      "op": " ",
      "text": "// TODO: line 98"
    },
    {
      "op": " ",
      "text": "// TODO: line 99"
    },
    {
      "op": " ",
      "text": "// TODO: line 100"
    },
    {
      "op": " ",
      "text": "// TODO: line 101"
    },
    {
      "op": " ",
      "text": ""
    }
  ]
}
```

## `fs.blame(ref, path, fromLine?)`

Shows which Commit last changed each line.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `fromLine` | number | no | First line, default 1 |

**Returns:** `lines`, each `{ n, text, lineTruncated, commit, message }`; `versions`
(how many file versions were compared); `comparisonCoarse` (true when a large change
was reported as one removed/added block).

```js
const b = await layerfs.fs.blame("work", "/src/cart.js", 80);
```
This live capture shows the first two of 22 `lines`; the raw output retains all rows.

```json
{
  "kind": "ok",
  "snapshot": "work@6a9c59c9084a",
  "root": "67fe8ac697a4…",
  "path": "/src/cart.js",
  "lines": [
    {
      "n": 80,
      "text": "// TODO: line 80",
      "lineTruncated": false,
      "clipped": false,
      "commit": "12804de7ce8d…",
      "message": "A: fixtures"
    },
    {
      "n": 81,
      "text": "// TODO: line 81",
      "lineTruncated": false,
      "clipped": false,
      "commit": "12804de7ce8d…",
      "message": "A: fixtures"
    }
  ],
  "origins": [],
  "versions": 5,
  "comparisonCoarse": false,
  "nextLine": null
}
```
