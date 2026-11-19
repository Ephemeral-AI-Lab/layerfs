# branch calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Outputs marked "seen" were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (ids shortened); the
> rest is from the table, the spec rows and the manual, not run.
> `branch.commits`, `branch.get_commit` and `branch.diff` are gone with no
> alias: use [`commit.list`, `commit.get` and `commit.diff`](commit.md).

```js
branch.list(project, { prefix, exclude_prefix, limit, cursor })
branch.get(project, branch)
branch.fork(project, name, from?, { at })
branch.merge(project, branch, from, { into, units, expected, message })
branch.publish(project, branch, { at, expected, force })
branch.remove(project, branch)
```

`main` is the project's published line. It is listed, you can read it, fork
from it and list its publications with `commit.list`, and it changes only
through `branch.publish`: a write to it, a merge into it without `into` and
a mount of it are `InvalidInput`.

Where a call takes a Commit (`at`, `expected`) it is a full id or a prefix of
8 or more characters. Where it takes `from`, a Branch of that name comes
first, then a Commit.

## `branch.list` and `branch.get`

`list` pages by name (`limit` 1 to 1000, default 100). A row's short form is
`{ branch, commit }` (`commit`: the head, or the publication a Branch with no
Commit of its own stands on); the long form adds `forked_from`, `id`, `base`
and `root`. `prefix` keeps only the names that start with it,
`exclude_prefix` leaves those out (such as `dsh.`). `get` answers one row,
short `{ commit }`.

```bash
$ layerfs branch list shop/api                                                 # seen
{"items":[{"branch":"agent","commit":"32919f55…"},{"branch":"main","commit":"32919f55…"}]}
$ layerfs branch get shop/api agent                                            # seen
{"commit":"32919f55…"}
```

`NotFound` for a Branch that does not exist.

## `branch.fork`

Creates Branch `name` from the head of `from` (`main` when left off), or with
`at` from one Commit of `from`'s history (`NotFound` when it is not in it).
It copies nothing, so it is instant. Short: `{ commit }`, the Commit the new
Branch stands on; long: `commit, outcome ("forked"), branch, base, root`.

```js
await layerfs.branch.fork("shop/api", "agent", "main");
await layerfs.branch.fork("shop/api", "old", "agent", { at: "12c7ef81" });
```

```bash
$ layerfs branch fork shop/api agent main                                      # seen
{"commit":"32919f55…"}
```

A taken name is `Exists`; `main` as a new name is `InvalidInput`. One working
Branch is enough for linear work. Fork per feature when work runs in
parallel.

## `branch.merge`

Merges `from` into `branch`: a fast-forward when `branch` holds nothing
`from` lacks, else one merge Commit on `branch` (first parent: `branch`'s
head). When `branch` already holds `from`, nothing changes and the answer
says `unchanged`. **`into: "NEW"` writes the result to a new Branch** and
leaves both sides as they are (1.x's only form). `expected` refuses
`Conflict` unless `branch`'s head is that Commit. Merging into `main` without
`into` is `InvalidInput`; publish instead. The rules of `branch`'s
[`/.lww`](../rules/lww.md) apply.

Short: `{ commit }`, with `unchanged`, `fast_forward`, `overwritten` (the
paths `/.lww` rules took, the first 20) and `overwritten_count` when set;
long: `commit, unchanged, fast_forward, overwritten, overwritten_count,
outcome ("merged", "fast_forward" or "unchanged"), branch, parent,
second_parent, merge_base, root, units, units_count`.

```bash
$ layerfs branch merge shop/api agent main --into merged                       # seen
{"commit":"12d12fbd…","fast_forward":true}
```

Merging into an existing Branch (no `into`) was not run in this pass.

```js
const m = await layerfs.branch.merge("shop/api", "agent", "main", { message: "merge main in" });
```

**Merge units** (`units`, a JSON list; shell `--units '["src/"]'`): each entry
is a pattern in the `/.lww` syntax, named by itself, or `{ name, patterns }`.
A unit covers what its patterns match and everything below; a path two units
match belongs to the first listed. When both sides changed a unit, the whole
unit comes from the side with the later write: no text merge, no conflict and
no `/.lww` rule inside it, and the losing side's changes there are dropped.
The long form's `units` reports each unit (the first 20; `units_count` past
that). A malformed pattern or an empty or repeated unit name is
`InvalidInput`.

**Refusals**

| `error` | When | Do |
| --- | --- | --- |
| `Conflict` | Overlapping changes (nothing changed; the conflicts are flat in `conflicts`, more under `detail: true`), or `branch`'s head is not `expected` (named as `commit`) | See [resolving conflicts](../rules/conflicts.md#branch-merge-conflicts) |
| `InvalidInput` | The Branches share no history (`NoMergeBase`), a merge into `main`, a bad `units` | Fork from a common Branch; publish; fix `units` |
| `Exists` | `into` names a Branch that exists | Pick a new name |
| `NotFound` | `branch` or `from` doesn't exist | Check the names |

## `branch.publish`

Makes `main`'s next head the Branch's head, or with `at` one Commit of its
history (a Commit `main` already holds answers `unchanged`). Without
`expected` it refuses `Conflict` when `main` gained a publication since the
Branch forked, so another agent's work is never dropped by accident;
`expected: <commit>` refuses `Conflict` unless `main`'s head is that Commit;
`force: true` publishes over it on purpose. Either refusal names `main`'s
head as `commit`. Short: `{ commit }` (the new publication), with
`unchanged` and `previous` (`main`'s head before, when it was not the
Branch's base); long: `commit, unchanged, previous, outcome ("published" or
"unchanged"), branch, source_commit, base, root, size`.

```bash
$ layerfs branch publish shop/api agent                                        # seen
{"commit":"32220c54…"}
$ layerfs branch publish shop/api agent --detail                               # seen, again
{"commit":"32220c54…","unchanged":true,"previous":null,"outcome":"unchanged","branch":"agent","source_commit":null,"base":null,"root":null,"size":null}
```

**Publishing replaces `main`'s view** with your Commit. If someone else
published since you forked, merge `main` into your Branch, check the result,
then publish:

```js
const P = "shop/api";
await layerfs.branch.merge(P, "agent", "main", { message: "merge main before publish" });
// mount "agent" in a Sandbox and run the tests here, then:
await layerfs.branch.publish(P, "agent");
```

## `branch.remove`

Removes one Branch; its name stays taken until compaction, and its Commits
and objects stay until a later `store.compact`. Short: `{"ok":true}`; long:
`{ ok, outcome: "removed", branch, removed_at }`.

```bash
$ layerfs branch remove shop/api merged                                        # seen
{"ok":true}
```
