# branch methods

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Examples were
> captured live in issue #334 Phase A at `b30e4fb8f`. Opaque IDs are shortened;
> the full replies and trace are in `target/issue334/phase-a/`.

Examples use `layerfs` as connected in [SKILL.md](../../SKILL.md#connect-to-a-project) and work on the Branch
`"work"`. Paging works as described
in [fs reads](fs-read.md).

## `branch.fork(name, fromRef?)`

Creates a Branch. Metadata only: nothing is copied, so it's instant.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | string | yes | New Branch name |
| `fromRef` | string | no | `main` (default), a Branch, a Commit or a Layer |

**Returns:** `{ kind: "Forked", branch, base, head }`. Forking a `main` Layer gives `head: null`; forking a Branch with a Commit inherits
that head.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `NameTaken` | A Branch with that name exists | Use it, or pick another name |
| `ReservedName` | The name is `main` | Pick another name |
| `NoSuchBranch`, `NoSuchCommit` | `fromRef` doesn't exist | Check it with `project.list_branches` |

```js
await layerfs.branch.fork("cart");
```
```json
{ "kind": "Forked", "branch": "cart", "base": "3279dac7de68…", "head": null }
```

**Notes:** one working Branch is enough for linear work. Fork per feature when work
runs in parallel, and fork a throwaway Branch from `main` to verify a release.

## `branch.list_commits(branch, pageSize?, pagePos?)`

Lists a Branch's Commits, newest first, along first parents.

**Returns:** a page `{ items, hasMore, nextPagePos }`. Each item is a Commit record:

| Field | Meaning |
| --- | --- |
| `id` | Commit id |
| `branch` | Branch name |
| `parent`, `secondParent` | Parents (`secondParent` only for merges) |
| `base` | `main` Layer underneath |
| `root` | Filesystem root id |
| `message` | Commit message |
| `mergedFrom` | Merge source, for merge Commits |
| `time` | Commit time |

```js
const log = await layerfs.branch.list_commits("work", 10, 0);
console.log(log.items.map((c) => `${c.id.slice(0, 12)} ${c.message}`).join("\n"));
```

**Notes:** after an uncertain outcome (`Unknown`, `Failed`), this is how you check
whether a Commit happened.

## `branch.diff_commits(from, to, pageSize?, pagePos?)`

Lists what changed between two Commits (ids, not Branch names; use `diff_branches` for
those).

**Returns:** `{ kind: "ok", from, to, items, hasMore, nextPagePos }`. Each item:

| Field | Meaning |
| --- | --- |
| `path` | Changed path |
| `kind` | `"Added"`, `"Removed"` or `"Modified"` |
| `type` | `file`, `dir` or `symlink` |
| `before`, `after` | `{ type, inode }` or `null` |
| `content`, `metadata`, `identity` | What changed: bytes, mode/attributes, file identity |
| `hunks`, `hunksOmitted` | Line diff for text files, as in `fs.diff_files` |

**Refusals:** `InvalidInput` (not two Commits), `NoSuchCommit`.

```js
const [newest, previous] = (await layerfs.branch.list_commits("work", 2, 0)).items;
const d = await layerfs.branch.diff_commits(previous.id, newest.id);
console.log(d.items.map((c) => `${c.kind} ${c.path}`).join("\n"));
```

## `branch.diff_branches(from, to, pageSize?, pagePos?)`

Same as `diff_commits`, between the current heads of two Branches or `main`.

**Refusals:** `InvalidInput` (a Commit id instead of a Branch), `NoSuchBranch`.

```js
const d = await layerfs.branch.diff_branches("main", "work");
console.log(`${d.items.length}${d.hasMore ? "+" : ""} paths differ from main`);
```

## `branch.merge(a, b, name, message?, units?)`

Merges Branches `a` and `b` into a **new** Branch `name`. `a` and `b` don't change.

**Returns:** `{ kind: "Merged", fastForward, branch, head, root, sources, mergeBase,
conflicts: [], lww }`. `fastForward: true` means one side already contained the other.
`lww` lists files decided last-writer-wins by `a`'s
[`/.lww` rules](../rules/lww.md), each `{ path, kept, deleted, rule }`.

**Merge units** (`units`, optional, a list): each entry is a pattern in the
`/.lww` syntax, named by itself, or `{ name, patterns }`. A unit covers what
its patterns match and everything below; a path two units match belongs to
the first listed. When both sides changed a unit, the whole unit comes from
the side with the later write (ours on a tie): no text merge, conflict or
`/.lww` rule inside it, and the losing side's changes there are dropped.
`units` in the result lists those units, each `{ unit, kept, keptTime,
changedOurs, changedTheirs, dropped, crossLinks }`, with `unitsTruncated`; the
merge Commit keeps the unit list (`units` in `branch.list_commits`). A
malformed pattern or an empty or repeated name is refused `InvalidInput`.

The lists use byte windows: `lwwTruncated`, `conflictsTruncated` and `unitsTruncated` explicitly
report omitted decisions or conflict paths, including in `Conflict.detail`.
Each conflict's `blocks` is its original count; `hunksOmitted` says its full
hunk text did not fit. No merge work or source change is omitted. For an
incomplete conflict summary preserve the input Branches, page
`branch.diff_branches(a, b)` through every candidate path and inspect both
versions and `mergeBase` with `fs.read`/`fs.diff_files`. Resolve on working forks
of both inputs as described in [conflicts](../rules/conflicts.md).

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Conflict` | Overlapping changes; nothing was created. `detail` has `sources`, `mergeBase`, `conflicts` (`{ path, kind, blocks, hunks }`) and `lww` | See [resolving conflicts](../rules/conflicts.md#branch-merge-conflicts) |
| `NoMergeBase` | The Branches share no history | Fork from a common ref |
| `NameTaken` | `name` exists | Pick a new name |
| `NoSuchBranch` | `a` or `b` doesn't exist | Check the names |

```js
const m = await layerfs.branch.merge("work", "main", "work-on-main", "merge main in");
console.log(m.branch, m.fastForward);
```

**On `Conflict`** nothing is created and both Branches are unchanged. There's no
overwrite option: fix one side, rule generated files in `a`'s `/.lww`, or copy the
chosen side's files across, then merge again. See
[resolving conflicts](../rules/conflicts.md#branch-merge-conflicts).

## `branch.publish(branch, commit?)`

Makes `main` show the Branch's head, or the given Commit, by adding a Layer to `main`.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `branch` | string | yes | The Branch to publish |
| `commit` | string | no | A Commit on it; default its head |

**Returns**

| `kind` | Meaning |
| --- | --- |
| `"Added"` | `main` now shows that Commit. `publishedLayer` is the new Layer; `overwrote` lists `main` Layers that were skipped |
| `"UpToDate"` | `main` already shows it |
| `"NoChanges"` | Nothing to publish |

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `NoCommit` | The Branch has no Commit yet | Commit something first |
| `HeadMoved` | `main` moved during the call | Merge `main` in (below), then publish again |
| `InvalidInput` | A Layer id instead of a Commit | Pass a Commit id |

**Publishing replaces `main`'s view** with your Commit. Layers published to `main` after
your Branch's base that your Commit doesn't contain are skipped: they stay in history,
and `overwrote` lists them. If someone else may have published since you forked, merge
`main` in first, check the result, then publish the merged Branch:

```js
const m = await layerfs.branch.merge("work", "main", "work-release", "merge main before publish");
// mount m.branch and run the tests here, then:
const p = await layerfs.branch.publish(m.branch);
if (p.kind === "Added" && p.overwrote.length) console.log("warning: skipped", p.overwrote);
console.log(p.kind, m.branch);
```
