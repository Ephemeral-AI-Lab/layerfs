# Resolving conflicts

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Read from the
> merge code and the compare-and-merge contract on `main`. Examples were
> exercised live in issue #334 Phase A at `b30e4fb8f`; IDs below are shortened.

## Two kinds of conflict

LayerFS merges in two places, and each can end in a `Conflict` refusal:

| | Commit conflict | Branch-merge conflict |
| --- | --- | --- |
| **Raised by** | `workspace.commit`, when the Branch moved since the mount and both sides changed the same lines | `branch.merge(a, b, name)`, when `a` and `b` changed the same lines |
| **Left behind** | Nothing committed. Markers are written into the conflicted files **in the Workspace**; the other side's clean changes are installed there too, and the Workspace now follows the new head | Nothing created. `a` and `b` are unchanged; no markers are written anywhere |
| **`detail`** | `base`, `head`, `conflicts`, `written`, `markers`, `lww` | `sources`, `mergeBase`, `conflicts`, `lww` |
| **Where you resolve** | In that Workspace, then Commit again | On working forks of the Branches, then merge again |
| **Overwrite instead** | `workspace.commit(ws, message, true)`, only before a conflict is written | Not available |

Each entry of `conflicts` is `{ path, kind, blocks, hunks }`:

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
| **Overwrite** | One side should win wholesale | `force` (`true`) on the first Commit after the head moved | No `force`: copy the chosen side's files onto `a`, or publish one Branch without merging |

A conflict is a code question, not a text question. Deleting markers, or taking one side
wholesale, makes Commit pass but can still break the program, so overwrite only when you
know the other side's change is unwanted.

## Commit conflicts

Examples use `layerfs` and a Workspace `ws` mounted on `"work"`, as in
[SKILL.md](../../SKILL.md#one-script-for-each-group).

### Scenario 1: both edited the same source lines (fix it)

1. **Learn what the other side did and why.** `branch.diff_commits(base, head)` lists
   what changed; `fs.diff_files(base, path, head, path)` shows their edit to one file;
   `branch.list_commits` gives their messages.
2. **Recall what your side did.** `fs.read(base, path)` is the file before either
   change; the markers show your lines next to theirs.
3. **Read the surrounding code:** callers and tests of the conflicted lines, in the
   Workspace (`exec` with `cat -n`, `grep -rn`) or on `head` with `fs.grep`.
4. **Decide what the code must do with both changes.** Usually a combination.
5. **Write the resolved file in the Workspace**
   ([write a file inside a mounted Workspace](../methods/workspace.md#workspaceexecws-command-message)),
   check no marker is left, and run the tests there.
6. **Commit again.** While any conflicted file still holds a marker block, Commit
   refuses with `Conflict` and `detail.markers: true`, even with `force`.

Gathering the reading for one file, with `base` and `head` copied from the refusal's
`detail`:

```js
// inspect.mjs
import { LayerFS } from "@layerfs/agent";

const layerfs = await LayerFS.open("shop");   // end with await layerfs.close()
// sandbox, wsPath: the Workspace whose commit conflicted, from its workspace.mount. Don't mount
// again to inspect: a new mount is a new Workspace, without the markers.
const [sandbox, wsPath, base, head, path] = process.argv.slice(2);
const ws = { sandbox, path: wsPath };

const theirs = await layerfs.fs.diff_files(base, path, head, path);
const before = await layerfs.fs.read(base, path);
const merged = await layerfs.workspace.exec(ws, `cat -n '.${path}'`, "read the merged file");
const why = await layerfs.branch.list_commits("work", 10, 0);

console.log("their change:\n" + theirs.hunks.filter((h) => h.op !== " ").map((h) => h.op + h.text).join(""));
console.log("before both:\n" + before.text);
console.log("now, with markers:\n" + merged.stdout);
console.log("recent messages:", why.items.map((c) => c.message));
```

### Scenario 2: a generated file conflicts (rule it)

A lockfile or build output both sides regenerated. Nobody should merge it by hand:

1. **Clear the markers by regenerating the file** in the Workspace. Remove the
   marked file first, since tools can't parse it: `rm package-lock.json && npm install`
   writes a fresh lockfile, a clean build rewrites `dist/`.
2. **Add the path to `/.lww`** in the same Workspace, so the next merge decides it by
   itself. Prefer fixing `.gitignore` if the file belongs there (see [lww](lww.md)).
3. **Commit.**

```js
// ws: the Workspace holding your changes. Don't mount again: a new mount is a new Workspace,
// without these changes or any conflict markers.
await layerfs.workspace.exec(ws, "rm -f package-lock.json && timeout 110 npm install --no-audit --no-fund 2>&1", "regenerate the lockfile");
await layerfs.workspace.exec(ws, "grep -qx 'package-lock.json' .lww || echo 'package-lock.json' >> .lww", "lww: lockfile");
await layerfs.workspace.commit(ws, "step 14: regenerate lockfile, rule it last-writer-wins");
```

### Scenario 3: one side deleted a file the other changed

`modify/delete` or `delete/modify`: the Workspace holds the changed text inside one
marker block. Decide whether the file should exist:

- **Keep it:** remove the marker lines, leaving the text you want, then Commit.
- **Delete it:** `rm` it in the Workspace, then Commit.

Check the callers first (`grep -rn` in the Workspace): a deleted file may have been
replaced by another one.

### Scenario 4: a binary or very large file

`binary`: the Workspace keeps **your** version, without markers. Committing again keeps
it. If the file is generated, rule it in `/.lww` (scenario 2). If it's an asset someone
else replaced on purpose, discard the Workspace (scenario 6) and redo your change on
top of theirs.

### Scenario 5: your Workspace should win wholesale (overwrite)

For example, a full rebuild that supersedes stale output, or a change you know replaces
the other side's. Check **before** committing, and force only then:

```js
// ws: the Workspace holding your changes. Don't mount again: a new mount is a new Workspace,
// without these changes or any conflict markers.
import { LayerFS } from "@layerfs/agent";
import { LayerFSBackend } from "@layerfs/backend";

const shop = await LayerFS.open("shop");
const backend = LayerFSBackend.of(shop);
const s = await backend.daemon.workspace.status(ws);
if (s.headMovedSinceMount) {
  const c = await shop.workspace.commit(ws, "step 15: full rebuild", true);
  console.log(c.kind, "displaced", c.displacedHead);
}
```

- `force` publishes your Workspace's view over the newer head without merging. The
  other side's Commits stay in history, but their changes are no longer on the Branch:
  `displacedHead` names where they were.
- It works only on the first Commit after the head moved. Once a conflict is written,
  the Workspace already holds the merge and markers block every Commit; resolve them,
  or discard and redo.

### Scenario 6: give up and redo

When the conflict is bigger than your change: `workspace.unmount(ws,
true)`, mount the current head, and redo your change on top of it. Your conflicted
attempt isn't recorded.

## Branch-merge conflicts

Nothing was created and both Branches are as they were, so resolving means changing
one of them and merging again. `detail.mergeBase` is where they diverged.

### Scenario 7: both Branches edited the same source lines (fix it)

Read both changes, decide the combined file, and write it identically on working forks
of both sides. A combined file written on only one side can still overlap the other
side's diff hunk and conflict, even if it contains the other side's lines. Keep the
original Branches:

```js
// merge-fix.mjs: cart and checkout both changed /src/price.js
import { LayerFS } from "@layerfs/agent";

const layerfs = await LayerFS.open("shop");   // end with await layerfs.close()
const [mergeBase, path] = process.argv.slice(2);   // from the refusal's detail

const checkoutChange = await layerfs.fs.diff_files(mergeBase, path, "checkout", path);
const cartChange = await layerfs.fs.diff_files(mergeBase, path, "cart", path);
console.log("checkout:\n" + checkoutChange.hunks.map((h) => h.op + h.text).join(""));
console.log("cart:\n" + cartChange.hunks.map((h) => h.op + h.text).join(""));
// Read the callers, decide what the code must do, then write the combination:
```

```js
// merge-apply.mjs: after deciding, write the reviewed result on two working forks
import { LayerFS } from "@layerfs/agent";

const layerfs = await LayerFS.open("shop");   // end with await layerfs.close()
const combined = "export const rate = 9;\nexport const cartRounding = true;\n"; // replace with your reviewed resolution

await layerfs.branch.fork("cart-fix", "cart");
await layerfs.branch.fork("checkout-fix", "checkout");
await layerfs.fs.write("cart-fix", "/src/price.js", combined, "resolve price.js: keep cart's and checkout's changes");
await layerfs.fs.write("checkout-fix", "/src/price.js", combined, "resolve price.js identically");
const m = await layerfs.branch.merge("cart-fix", "checkout-fix", "cart-checkout", "merge cart and checkout");
console.log(m.kind, m.branch);
```

If other files still conflict, inspect their hunks and resolve each on both working
forks. For an add/add conflict the merge base has no file: read both Branches directly
instead of calling `diff_files` or `read` on the missing base path.

### Scenario 8: a generated file conflicts (rule it)

The rules are read from `a` when the merge runs, so adding the path to `a`'s `/.lww` is
enough:

```js
const lww = await layerfs.fs.read("cart", "/.lww");
await layerfs.fs.write("cart", "/.lww", lww.text + "package-lock.json\n", "lww: lockfile");
const m = await layerfs.branch.merge("cart", "checkout", "cart-checkout", "merge cart and checkout");
console.log(m.kind, m.lww);
```

### Scenario 9: one side should win (no overwrite)

`branch.merge` has no `force`. Instead:

- **For some files:** copy the chosen side's version onto `a` first. Identical files
  merge cleanly.
  ```js
  const theirs = await layerfs.fs.read("checkout", "/src/price.js");
  await layerfs.fs.write("cart", "/src/price.js", theirs.text, "take checkout's price.js");
  ```
  (`fs.read` returns at most 2000 lines or 48 KiB per call; read the whole file with
  [the window loop](../methods/fs-read.md#fsreadref-path-startline-endline).)
- **For a whole Branch:** don't merge. Publish the Branch you want
  (`branch.publish` replaces `main`'s view), or fork your next work from it.
