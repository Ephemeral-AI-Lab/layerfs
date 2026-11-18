# Resolving conflicts

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Read
> from the merge code and the compare-and-merge contract; the scenarios were
> exercised live in issue #334 Phase A on 1.1.0, and a Branch-merge conflict
> was reproduced on 2.0 on 2026-11-02. Calls and refusal fields are the
> LayerFS 2.0 call surface's: the flat fields are from the spec rows and the
> source (`control/crates/layerfs-sandbox/src/daemon/values.rs`,
> `control/crates/layerfs-store/src/calls/op.rs`); no conflict was
> reproduced on the 2.0 call surface build.

## Where a conflict comes from

LayerFS merges in three calls, and each can end in a `Conflict` refusal:

| | Commit conflict | File-write conflict | Branch-merge conflict |
| --- | --- | --- | --- |
| **Raised by** | `workspace.commit`, when the Branch moved since the mount and both sides changed the same lines | a `file.*` write, when the Branch moved during the call and a path it changed conflicts | `branch.merge(project, branch, from)`, when both changed the same lines |
| **Left behind** | Nothing committed. Markers are written into the conflicted files **in the Workspace**, and the Workspace now follows the new head | Nothing written | Nothing changed: both Branches are as they were; no markers anywhere |
| **Flat fields** | `commit` (the head), `conflicts` (the first 20 paths), `conflicts_count` (only past 20) | `conflicts` when the Store reports them (from source; not run) | `conflicts` (the merge's conflict list) |
| **Where you resolve** | In that Workspace, then Commit again | Read the file again, redo the write | On the Branch (or working forks), then merge again |
| **Overwrite instead** | `force: true` (scenario 5) | — | Not available (scenario 9) |

The same code, `Conflict`, also means a head guard failed: `expected` named a
head that is not the Branch's (or, for `branch.publish`, `main`'s) head any
more. Then `commit` names the head it is at, nothing changed, and there are
no `conflicts`: read again and redo.

A refusal's `detail` is there only when the call passed `detail: true`, and
keeps the library's camelCase keys (`mergeBase`, `hunksOmitted`,
`conflictsTruncated`) even though answers are snake_case. A Commit
conflict's `detail` holds `head`, `base`, `markers`, `conflicts`, `lww`,
`conflictsTruncated`, `lwwTruncated`, `written` and `truncated`; there each
`conflicts` entry is `{ path, kind, blocks, hunksOmitted, hunks }` and a hunk
is `{ line, ours, theirs }` (seen on 2.0 before the call surface:
`{"line":1,"ours":["export const retries = 7;"],"theirs":["export const retries = 9;"]}`).
A Branch-merge conflict carries those entries as its flat `conflicts`.

| `kind` | Meaning | What the Workspace holds (Commit conflict) |
| --- | --- | --- |
| `content` | Both sides changed the same lines of a text file | The file with `<<<<<<< ` / `=======` / `>>>>>>> ` blocks around each overlap |
| `add/add` | Both sides created the same path with different text | As `content`, with no common base |
| `modify/delete`, `delete/modify` | One side changed a file the other deleted | The changed text inside one marker block |
| `binary` | Both changed a file that isn't text (or is too large to merge as text) | Your version, without markers |
| `mode` | Both changed a file's mode, to different modes | The merged file, with your mode |
| `type` | A file on one side, a folder or symlink on the other | Your version if present, else theirs |

Paths that [`/.lww`](lww.md) decided instead of conflicting are in the
`detail`'s `lww` list (`{ path, kept, deleted, rule }`), and in a successful
answer's `overwritten` list.

## Three ways to resolve

| Way | When | Commit conflict | Branch-merge conflict |
| --- | --- | --- | --- |
| **Fix it** | Source code: both changes matter | Edit the files in the Workspace, then Commit | Write the reviewed result on the Branch merged into (and identically on `from`'s working fork when needed), then merge |
| **Rule it** | Generated files: dependencies, lockfiles, build output | Clear the markers, add the path to `/.lww`, Commit | Add the path to the `/.lww` of the Branch merged into, merge again |
| **Take one side** | One side should win wholesale | `force: true`, or discard and redo on the new head (scenarios 5, 6) | Copy the chosen side's files onto the Branch, or publish one Branch without merging (scenario 9) |

A conflict is a code question, not a text question. Deleting markers, or taking
one side wholesale, makes the Commit pass but can still break the program.

## Commit conflicts

Examples use `P = "shop/api"` and a Workspace `ws` (such as `tests:agent`)
mounted on `"agent"`. Don't mount again to inspect: a new mount is a new
Workspace, without the markers.

### Scenario 1: both edited the same source lines (fix it)

1. **Learn what the other side did and why.** `commit.diff(P, head,
   { from: base, with: ["patch"] })` shows what changed; `commit.list` gives
   their messages.
2. **Recall what your side did.** `file.read(P, "agent", path, { at: base })`
   is the file before either change; the markers show your lines next to
   theirs.
3. **Read the surrounding code:** callers and tests of the conflicted lines, in
   the Workspace (`exec` with `cat -n`, `grep -rn`) or with `file.grep`.
4. **Decide what the code must do with both changes.** Usually a combination.
5. **Write the resolved file in the Workspace**
   ([write a file inside a mounted Workspace](../methods/workspace.md#workspaceexec)),
   check no marker is left, and run the tests there.
6. **Commit again.** While any conflicted file still holds a marker block,
   Commit refuses `Conflict` (its long form's `detail.markers` is `true`).

Gathering the reading for one file:

```js
// inspect.mjs: node inspect.mjs <ws> <path>
import layerfs, { LayerFSError } from "@layerfs/node";

const P = "shop/api";
const [ws, path] = process.argv.slice(2);

let refusal;
try {
  await layerfs.workspace.commit(ws, { message: "step 9: try commit", detail: true });
} catch (e) {
  if (!(e instanceof LayerFSError) || e.error !== "Conflict") throw e;
  refusal = e;
}
if (refusal) {
  const { head, base } = refusal.detail;                      // detail: true above
  console.log("conflicting paths:", refusal.conflicts, refusal.conflicts_count ?? "");
  const theirs = await layerfs.commit.diff(P, head, { from: base, path, with: ["patch"] });
  const before = await layerfs.file.read(P, "agent", path, { at: base });
  const merged = await layerfs.workspace.exec(ws, `cat -n '.${path}'`);
  const why = await layerfs.commit.list(P, "agent", { limit: 10 });
  for (const row of theirs.items) console.log("their change:\n" + (row.hunks ?? []).filter((h) => h.op !== " ").map((h) => h.op + h.text).join("\n"));
  console.log("before both:\n" + (before.text ?? ""));
  console.log("now, with markers:\n" + (merged.stdout ?? ""));
  console.log("recent messages:", why.items.map((c) => c.message));
}
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
await layerfs.workspace.exec(ws, "rm -f package-lock.json && timeout 110 npm install --no-audit --no-fund 2>&1");
await layerfs.workspace.exec(ws, "grep -qx 'package-lock.json' .lww || echo 'package-lock.json' >> .lww");
await layerfs.workspace.commit(ws, { message: "step 14: regenerate lockfile, rule it newer-write-wins" });
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

`workspace.commit(ws, { force: true })` commits over a head that moved since
the mount instead of merging with it: what the other writer committed is
replaced, and the answer's long form names it as `displaced`. Check first
with `workspace.status(ws)`: no `branch_moved` means a plain Commit will not
merge. Use `force` only when you mean to discard the other side; otherwise
resolve by keeping your side (scenario 1), or redo on the new head
(scenario 6).

### Scenario 6: give up and redo

When the conflict is bigger than your change:
`workspace.unmount(ws, { discard: true })`, mount the current head, and redo
your change on top of it. Your conflicted attempt isn't recorded.

## File-write conflicts

A `file.*` write merges with a head that moved during the call; only a path
it changed that the other side also changed is refused `Conflict`, and
nothing is written. Read the file again (`file.read`), decide, and redo the
write. To refuse any moved head instead of merging, pass
`expected: <the head you read>`.

## Branch-merge conflicts

Nothing changed and both Branches are as they were, so resolving means
changing one of them and merging again. With `detail: true` the refusal's
`detail.mergeBase` is the Commit where they diverged (from source; not
re-run on this build).

### Scenario 7: both Branches edited the same source lines (fix it)

Read both changes, decide the combined file, and write it identically on
working forks of both sides; a combined file written on only one side can
still overlap the other side's change and conflict. Keep the original
Branches, and merge with `into` so neither changes:

```js
// merge-fix.mjs: cart and checkout both changed /src/price.js
import layerfs from "@layerfs/node";

const P = "shop/api";
const [mergeBase, path] = process.argv.slice(2);   // from the refusal's detail

for (const side of ["checkout", "cart"]) {
  const d = await layerfs.commit.diff(P, side, { from: mergeBase, path, with: ["patch"] });
  for (const row of d.items) console.log(side + ":\n" + (row.hunks ?? []).map((h) => h.op + h.text).join("\n"));
}
// Read the callers, decide what the code must do, then write the combination:
```

```js
// merge-apply.mjs: after deciding, write the reviewed result on two working forks
import layerfs from "@layerfs/node";

const P = "shop/api";
const combined = "export const rate = 9;\nexport const cartRounding = true;\n"; // your reviewed resolution

await layerfs.branch.fork(P, "cart-fix", "cart");
await layerfs.branch.fork(P, "checkout-fix", "checkout");
await layerfs.file.write(P, "cart-fix", "/src/price.js", combined, { message: "resolve price.js: keep both changes" });
await layerfs.file.write(P, "checkout-fix", "/src/price.js", combined, { message: "resolve price.js identically" });
const m = await layerfs.branch.merge(P, "cart-fix", "checkout-fix", { into: "cart-checkout", message: "merge cart and checkout" });
console.log(m.commit, m.fast_forward ?? false);
```

If other files still conflict, resolve each on both working forks. For an
`add/add` conflict the merge base has no file: read both Branches directly
instead of reading the missing base path.

### Scenario 8: a generated file conflicts (rule it)

The rules are read from the Branch merged into (`branch`) when the merge
runs, so adding the path to its `/.lww` is enough:

```js
const lww = await layerfs.file.read(P, "cart", "/.lww");
await layerfs.file.write(P, "cart", "/.lww", lww.text + "package-lock.json\n", { message: "lww: lockfile" });
const m = await layerfs.branch.merge(P, "cart", "checkout", { into: "cart-checkout", message: "merge cart and checkout" });
console.log(m.commit, m.overwritten ?? []);
```

Merge units (`branch.merge`'s `units` option) are the other tool: a unit both
sides changed comes whole from the later side. See
[branch](../methods/branch.md#branchmerge).

### Scenario 9: one side should win (no overwrite)

`branch.merge` has no `force`. Instead:

- **For some files:** copy the chosen side's version onto the Branch merged
  into first. Identical files merge cleanly.
  ```js
  const theirs = await layerfs.file.read(P, "checkout", "/src/price.js");
  await layerfs.file.write(P, "cart", "/src/price.js", theirs.text, { message: "take checkout's price.js" });
  ```
  (`file.read` returns at most 2000 lines per call; read the whole file with
  [the window loop](../methods/file.md#fileread).)
- **For a whole Branch:** don't merge. Publish the Branch you want
  (`branch.publish` replaces `main`'s view; `force: true` publishes over a
  `main` that moved), or fork your next work from it.
