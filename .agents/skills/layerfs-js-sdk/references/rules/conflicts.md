# Resolving conflicts

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Read from
> the merge code and the compare-and-merge contract; the scenarios were exercised
> live in issue #334 Phase A on 1.1.0. Calls are updated for LayerFS 2.0 (#533).
> A Branch-merge `Conflict` was reproduced on 2.0 on 2026-11-02; Commit
> conflicts were not (no Sandbox was run in that pass).

## Two kinds of conflict

LayerFS merges in two places, and each can end in a `Conflict` refusal:

| | Commit conflict | Branch-merge conflict |
| --- | --- | --- |
| **Raised by** | `workspace.commit`, when the Branch moved since the mount and both sides changed the same lines | `branch.merge(project, name, a, b)`, when `a` and `b` changed the same lines |
| **Left behind** | Nothing committed. Markers are written into the conflicted files **in the Workspace**; the other side's clean changes are installed there too, and the Workspace now follows the new head | Nothing created. `a` and `b` are unchanged; no markers anywhere |
| **`detail`** | `base`, `head`, `conflicts`, `written`, `markers`, `lww` | `sources`, `mergeBase`, `conflicts`, `lww` |
| **Where you resolve** | In that Workspace, then Commit again | On working forks of the Branches, then merge again |
| **Overwrite instead** | Not available in this build (see scenario 5) | Not available |

A refusal's `detail` keeps camelCase keys (`mergeBase`, `hunksOmitted`,
`conflictsTruncated`) even though results are snake_case.

Each entry of `conflicts` is `{ path, kind, blocks, hunksOmitted, hunks }`; a
hunk is `{ line, ours, theirs }` (live: `{"line":1,"ours":["export const retries
= 7;"],"theirs":["export const retries = 9;"]}`).

| `kind` | Meaning | What the Workspace holds (Commit conflict) |
| --- | --- | --- |
| `content` | Both sides changed the same lines of a text file | The file with `<<<<<<< ` / `=======` / `>>>>>>> ` blocks around each overlap |
| `add/add` | Both sides created the same path with different text | As `content`, with no common base |
| `modify/delete`, `delete/modify` | One side changed a file the other deleted | The changed text inside one marker block |
| `binary` | Both changed a file that isn't text (or is too large to merge as text) | Your version, without markers |
| `mode` | Both changed a file's mode, to different modes | The merged file, with your mode |
| `type` | A file on one side, a folder or symlink on the other | Your version if present, else theirs |

`lww` lists the paths that [`/.lww`](lww.md) decided instead of conflicting.

## Three ways to resolve

| Way | When | Commit conflict | Branch-merge conflict |
| --- | --- | --- | --- |
| **Fix it** | Source code: both changes matter | Edit the files in the Workspace, then Commit | Write the reviewed result identically on both working forks, then merge |
| **Rule it** | Generated files: dependencies, lockfiles, build output | Clear the markers, add the path to `/.lww`, Commit | Add the path to `a`'s `/.lww`, merge again |
| **Take one side** | One side should win wholesale | Discard and redo on the new head (scenario 6) | Copy the chosen side's files onto `a`, or publish one Branch without merging (scenario 9) |

A conflict is a code question, not a text question. Deleting markers, or taking
one side wholesale, makes the Commit pass but can still break the program.

## Commit conflicts

Examples use `P = "shop/api"` and a Workspace `ws` mounted on `"work"`. Don't
mount again to inspect: a new mount is a new Workspace, without the markers.

### Scenario 1: both edited the same source lines (fix it)

1. **Learn what the other side did and why.** `commit.diff(P, base, head)` lists
   what changed; `file.diff` shows their edit to one file; `commit.list` gives
   their messages.
2. **Recall what your side did.** `file.read(P, "work", path, base)` is the file
   before either change; the markers show your lines next to theirs.
3. **Read the surrounding code:** callers and tests of the conflicted lines, in
   the Workspace (`exec` with `cat -n`, `grep -rn`) or with `file.grep`.
4. **Decide what the code must do with both changes.** Usually a combination.
5. **Write the resolved file in the Workspace**
   ([write a file inside a mounted Workspace](../methods/workspace.md#workspaceexec)),
   check no marker is left, and run the tests there.
6. **Commit again.** While any conflicted file still holds a marker block,
   Commit refuses with `Conflict` and `detail.markers: true`.

Gathering the reading for one file, with `base` and `head` from the refusal's
`detail`:

```js
// inspect.mjs: node inspect.mjs <ws> <base> <head> <path>
import layerfs from "layerfs";

const P = "shop/api";
const [ws, base, head, path] = process.argv.slice(2);

const theirs = await layerfs.file.diff(P, "work", path, "work", path, { left_commit: base, right_commit: head });
const before = await layerfs.file.read(P, "work", path, base);
const merged = await layerfs.workspace.exec(ws, `cat -n '.${path}'`, { message: "read the merged file" });
const why = await layerfs.commit.list(P, "work", { page_size: 10 });

console.log("their change:\n" + theirs.hunks.filter((h) => h.op !== " ").map((h) => h.op + h.text).join("\n"));
console.log("before both:\n" + before.text);
console.log("now, with markers:\n" + merged.stdout);
console.log("recent messages:", why.items.map((c) => c.message));
```

### Scenario 2: a generated file conflicts (rule it)

A lockfile or build output both sides regenerated. Nobody should merge it by
hand:

1. **Clear the markers by regenerating the file** in the Workspace. Remove the
   marked file first, since tools can't parse it: `rm package-lock.json && npm
   install` writes a fresh lockfile; a clean build rewrites `dist/`.
2. **Add the path to `/.lww`** in the same Workspace, so the next merge decides
   it by itself. Prefer fixing `.gitignore` if the file belongs there.
3. **Commit.**

```js
await layerfs.workspace.exec(ws, "rm -f package-lock.json && timeout 110 npm install --no-audit --no-fund 2>&1", { message: "regenerate the lockfile" });
await layerfs.workspace.exec(ws, "grep -qx 'package-lock.json' .lww || echo 'package-lock.json' >> .lww", { message: "lww: lockfile" });
await layerfs.workspace.commit(ws, { message: "step 14: regenerate lockfile, rule it last-writer-wins" });
```

### Scenario 3: one side deleted a file the other changed

`modify/delete` or `delete/modify`: the Workspace holds the changed text inside
one marker block. Decide whether the file should exist:

- **Keep it:** remove the marker lines, leaving the text you want, then Commit.
- **Delete it:** `rm` it in the Workspace, then Commit.

Check the callers first (`grep -rn` in the Workspace): a deleted file may have
been replaced by another one.

### Scenario 4: a binary or very large file

`binary`: the Workspace keeps **your** version, without markers. Committing
again keeps it. If the file is generated, rule it in `/.lww` (scenario 2). If
it's an asset someone else replaced on purpose, discard the Workspace
(scenario 6) and redo your change on top of theirs.

### Scenario 5: your Workspace should win wholesale

In 1.1.0, `force` on the first Commit after the head moved published the
Workspace's view over the newer head without merging. **In this 2.0 build the
library does not pass `force` to the daemon** (`family/sandboxes.rs`), so a
forced Commit merges like a normal one. Until that is fixed:

- Check first with `workspace.status(ws)`: `head_moved_since_mount: false`
  means a Commit now will not merge.
- If the head moved and your view must win, commit as usual and resolve any
  conflict by keeping your side (scenario 1), or redo on the new head
  (scenario 6).

### Scenario 6: give up and redo

When the conflict is bigger than your change:
`workspace.unmount(ws, { discard: true })`, mount the current head, and redo
your change on top of it. Your conflicted attempt isn't recorded.

## Branch-merge conflicts

Nothing was created and both Branches are as they were, so resolving means
changing one of them and merging again. `detail.mergeBase` is the Commit where
they diverged.

### Scenario 7: both Branches edited the same source lines (fix it)

Read both changes, decide the combined file, and write it identically on
working forks of both sides. A combined file written on only one side can still
overlap the other side's diff and conflict. Keep the original Branches:

```js
// merge-fix.mjs: cart and checkout both changed /src/price.js
import layerfs from "layerfs";

const P = "shop/api";
const [mergeBase, path] = process.argv.slice(2);   // from the refusal's detail

const checkoutChange = await layerfs.file.diff(P, "checkout", path, "checkout", path, { left_commit: mergeBase });
const cartChange = await layerfs.file.diff(P, "cart", path, "cart", path, { left_commit: mergeBase });
console.log("checkout:\n" + checkoutChange.hunks.map((h) => h.op + h.text).join("\n"));
console.log("cart:\n" + cartChange.hunks.map((h) => h.op + h.text).join("\n"));
// Read the callers, decide what the code must do, then write the combination:
```

```js
// merge-apply.mjs: after deciding, write the reviewed result on two working forks
import layerfs from "layerfs";

const P = "shop/api";
const combined = "export const rate = 9;\nexport const cartRounding = true;\n"; // your reviewed resolution

await layerfs.branch.fork(P, "cart-fix", "cart");
await layerfs.branch.fork(P, "checkout-fix", "checkout");
await layerfs.file.write(P, "cart-fix", "/src/price.js", combined, { message: "resolve price.js: keep both changes" });
await layerfs.file.write(P, "checkout-fix", "/src/price.js", combined, { message: "resolve price.js identically" });
const m = await layerfs.branch.merge(P, "cart-checkout", "cart-fix", "checkout-fix", { message: "merge cart and checkout" });
console.log(m.kind, m.branch);
```

If other files still conflict, resolve each on both working forks. For an
`add/add` conflict the merge base has no file: read both Branches directly
instead of reading the missing base path.

### Scenario 8: a generated file conflicts (rule it)

The rules are read from `a` when the merge runs, so adding the path to `a`'s
`/.lww` is enough:

```js
const lww = await layerfs.file.read(P, "cart", "/.lww");
await layerfs.file.write(P, "cart", "/.lww", lww.text + "package-lock.json\n", { message: "lww: lockfile" });
const m = await layerfs.branch.merge(P, "cart-checkout", "cart", "checkout", { message: "merge cart and checkout" });
console.log(m.kind, m.lww);
```

Merge units (`branch.merge`'s `units` option) are the other tool: a unit both
sides changed comes whole from the later side. See
[branch](../methods/branch.md#branchmerge).

### Scenario 9: one side should win (no overwrite)

`branch.merge` has no `force`. Instead:

- **For some files:** copy the chosen side's version onto `a` first. Identical
  files merge cleanly.
  ```js
  const theirs = await layerfs.file.read(P, "checkout", "/src/price.js");
  await layerfs.file.write(P, "cart", "/src/price.js", theirs.text, { message: "take checkout's price.js" });
  ```
  (`file.read` returns at most 2000 lines or 48 KiB per call; read the whole
  file with [the window loop](../methods/file.md#fileread).)
- **For a whole Branch:** don't merge. Publish the Branch you want
  (`branch.publish` replaces `main`'s view), or fork your next work from it.
