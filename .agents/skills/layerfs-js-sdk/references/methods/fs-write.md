# fs write methods

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Examples were
> captured live in issue #334 Phase A at `b30e4fb8f`. Opaque IDs are shortened;
> the full replies and trace are in `target/issue334/phase-a/`.

Examples use `layerfs` as connected in [SKILL.md](../../SKILL.md#connect-to-a-project) and work on the Branch
`"work"`.

**Shared rules**

- Each call changes one Branch (never `main`) and publishes **one** Commit, or nothing.
- An already-mounted Workspace doesn't see the change: mount again, or let its next
  `workspace.commit` merge it in.
- Parent folders must exist before a write. `fs.mkdir` creates missing parents.
- `message` is optional UTF-8, at most 4096 bytes; newline and tab are allowed.

**Returns (all seven)**

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `"Committed"` | `branch`, `head`, `commit` | A new Commit; `head` is its id; `commit` is its record: `id`, `parent`, `secondParent`, `base`, `root`, `message`, `time`, … |
| `"UpToDate"` | `branch`, `head`, `root` | Nothing changed, so no Commit was made |

**Shared refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `ReadOnly` | The Branch is `main` | Write on your Branch, then `branch.publish` |
| `NoSuchBranch` | No such Branch | `branch.fork` it, or check the name |
| `HeadMoved` | Another writer published on the Branch during the call; nothing was published | Read the files you changed again, then decide whether to apply your change |
| `NoSuchPath` | A path or its parent folder is missing | `fs.list` the parent; `fs.mkdir` first |
| `NotDirectory` | A file or a symlink is in the way of the path; `message` is that entry | `fs.stat` it; choose another path or remove it first |
| `TooLarge` | The complete encoded request did not fit one control call; nothing was sent | Inspect the input, then choose a smaller call or a larger atomic Workspace step |
| `InvalidInput` | A bad argument (message bytes, relative path, …) | Read `message`; fix the call |
| `Failed` | The call failed before committing, for example it could not read the Store; nothing was committed | Stop and report `message` |

One encoded control request fits at most 65,519 bytes, including paths, metadata
and tracing fields as well as content. `TooLarge` from this before-send admission
has null detail, without a change index. Do not estimate the allowance from content
bytes alone or resend an unchanged failed call. Splitting an `apply` creates
separate Commits and changes atomicity; use one ordinary Workspace step when the
larger change must stay atomic. The SDK never splits or replays it automatically.

## `fs.write(branch, path, content, message?)`

Creates or replaces one file with `content`.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `branch` | string | yes | Your Branch |
| `path` | string | yes | Absolute file path |
| `content` | string | yes | The whole new content |
| `message` | string | no | Commit message |

**Refusals:** `IsDirectory`, plus the shared ones.

```js
const r = await layerfs.fs.write("work", "/src/cart.js", source, "cart: add Cart class");
```
```json
{
  "kind": "UpToDate",
  "branch": "work",
  "head": "12804de7ce8d…",
  "root": "41a9cf49d9b2…"
}
```

## `fs.edit(branch, path, oldText, newText, message?)`

Replaces exactly one occurrence of `oldText` with `newText`. Streams large files, so it's
the cheap way to change a few lines of a big file.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `NoMatch` | `oldText` doesn't occur | `fs.read` the area; the text differs (whitespace, an earlier edit) |
| `AmbiguousMatch` | `oldText` occurs more than once | Include more surrounding lines to make it unique |
| `Binary` | The text being edited contains NUL or invalid UTF-8 | Use a Workspace for binary changes |
| shared | | See above |

```js
const r = await layerfs.fs.edit("work", "/src/cart.js", "const TAX = 0.07;", "const TAX = 0.08;", "cart: tax 8%");
console.log(r.kind, r.head);
```

On `NoMatch` or `AmbiguousMatch` nothing changed. Read the file as it is now
(`fs.grep` for the text, then `fs.read` around those lines) before choosing a new
`oldText`: the code may have changed since you last looked.

## `fs.apply(branch, changes, message?)`

Applies an ordered list of changes and publishes them as **one** Commit, or nothing. The
best way to scaffold or make any multi-file change.

| Change | Fields |
| --- | --- |
| write | `{ op: "write", path, content }` |
| edit | `{ op: "edit", path, oldText, newText }` |
| mkdir | `{ op: "mkdir", path }` |
| rm | `{ op: "rm", path }` |
| cp | `{ op: "cp", from, to }` |
| mv | `{ op: "mv", from, to }` |

Later changes see earlier ones: a `mkdir` before a `write` into that folder works.

**Refusals:** the refusal of the first change that fails, with `detail.index`
(zero-based) naming it. Nothing is published.

**Scaffold many files**

```js
const files = {
  "/package.json": JSON.stringify({ name: "shop", type: "module", scripts: { test: "node --test" } }, null, 2) + "\n",
  "/src/server.js": "export function start() {}\n",
  "/test/server.test.js": "import { test } from 'node:test';\ntest('loads', () => {});\n",
};
const dirs = [...new Set(Object.keys(files).map((p) => p.slice(0, p.lastIndexOf("/"))).filter(Boolean))];
const r = await layerfs.fs.apply("work", [
  ...dirs.map((path) => ({ op: "mkdir", path })),
  ...Object.entries(files).map(([path, content]) => ({ op: "write", path, content })),
], "scaffold: package, server, first test");
console.log(r.kind, r.head);
```

**When a change fails:** the refusal's `detail.index` points into your `changes` list. Nothing was published, so fix that change (or the state it
assumed: a missing folder, an `oldText` that moved) and apply the whole batch again.

## `fs.mkdir(branch, path, message?)`

Creates a folder and any missing parents. An existing folder is a no-op (`UpToDate`).

**Refusals:** `NotDirectory` (a file is in the way), plus the shared ones.

```js
await layerfs.fs.mkdir("work", "/src/routes/admin", "routes: admin folder");
```

## `fs.rm(branch, path, message?)`

Removes a file or a whole folder tree.

**Refusals:** `NoSuchPath`, plus the shared ones.

```js
await layerfs.fs.rm("work", "/src/legacy", "remove legacy folder");
```

## `fs.cp(branch, from, to, message?)`

Copies a file or folder tree. Keeps modes, symlinks and hardlinks within the copied set;
timestamps are new.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Exists` | `to` already exists | Choose a new destination, or `fs.rm` it first |
| `InvalidInput` | Same path, or a copy into its own subtree | Pick a destination outside `from` |
| shared | | See above |

## `fs.mv(branch, from, to, message?)`

Moves or renames a file or folder tree. Keeps identity and timestamps.

**Refusals:** the same as `fs.cp`.

```js
await layerfs.fs.mv("work", "/src/basket.js", "/src/cart.js", "cart: rename basket");
```
