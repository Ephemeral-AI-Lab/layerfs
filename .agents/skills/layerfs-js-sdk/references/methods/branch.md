# branch calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. Results were captured from the built `layerfs`
> command in a private home on 2026-11-02; ids are shortened.

```js
branch.list(project, { page_size, page_pos })
branch.get(project, branch)
branch.fork(project, name, from_branch, from_commit?)
branch.merge(project, name, a, b, { message, units })
branch.publish(project, branch, commit?)
branch.diff(project, from_branch, to_branch, { paths_only, page_size, page_pos, cursor })
```

`main` is not a Branch: it is the project's published line of Layers. It shows
in `branch.list`, you can read it and fork from it, and it changes only through
`branch.publish`.

## `branch.list` and `branch.get`

`branch.list(project, { page_size, page_pos })`, `branch.get(project, branch)`

```bash
layerfs branch list shop/api --text
```

```
NAME   ID            BASE           HEAD           FORKED_FROM
main   -             3256e1f13d0b…  -              -
work   1197d2181af4… 3275838904b6…  12ad2061f04f…  3275838904b6…
```

Each item is `{ name, id, base, head, forked_from }`. `base` is the `main`
Layer underneath; `head` is the newest Commit, `null` before the first one.
`branch.get` returns one item, or `NotFound` (`no Branch <name>`).

## `branch.fork`

`branch.fork(project, name, from_branch, from_commit?)`

Creates a Branch from another Branch's head, or from one of its Commits. It
copies nothing, so it is instant.

```js
await layerfs.branch.fork("shop/api", "work", "main");
await layerfs.branch.fork("shop/api", "old", "work", "12c7ef811a491a01…");   // full 66-hex id
```

```json
{"kind":"Forked","branch":"work","base":"3275838904b6…","head":null}
```

Forking `main` gives `head: null`; forking a Branch inherits its head.

| Code | Meaning | Do |
| --- | --- | --- |
| `NameTaken` | A Branch with that name exists | Use it, or pick another name |
| `ReservedName` | The name is `main` | Pick another name |
| `NoSuchBranch` | `from_branch` doesn't exist | Check `branch.list` |
| `InvalidInput` | `from_commit` is not a full Commit or Layer id (a short prefix is refused) | Copy the full id from `commit.list` |

One working Branch is enough for linear work. Fork per feature when work runs
in parallel.

## `branch.merge`

`branch.merge(project, name, a, b, { message, units })`

Merges Branches `a` and `b` into a **new** Branch `name`. `a` and `b` don't
change. The new name comes second, as in `fork`.

```js
const m = await layerfs.branch.merge("shop/api", "work-on-main", "work", "main", { message: "merge main in" });
```

```json
{"kind":"Merged","fast_forward":false,"branch":"m3","head":"1285e865f74a…","root":"34b5d29b8909…",
 "sources":[{"ref":"c1","node":"12b0855b8d51…"},{"ref":"c2","node":"12854bae4ba6…"}],
 "merge_base":"12ad2061f04f…","conflicts":[],"lww":[],"conflicts_truncated":false,
 "lww_truncated":false,"units":[…],"units_truncated":false}
```

`fast_forward: true` means one side already contained the other. `lww` lists
files decided by `a`'s [`/.lww`](../rules/lww.md), each
`{ path, kept, deleted, rule }`.

**Merge units** (`units`, a JSON list; shell `--units '["src/"]'`): each entry
is a pattern in the `/.lww` syntax, named by itself, or `{ name, patterns }`.
A unit covers what its patterns match and everything below; a path two units
match belongs to the first listed. When both sides changed a unit, the whole
unit comes from the side with the later write (ours on a tie): no text merge,
no conflict and no `/.lww` rule inside it, and the losing side's changes there
are dropped. The result's `units` reports each one:

```json
{"unit":"src/","kept":"theirs","kept_time":"2026-11-01T23:01:34.987892000Z","changed_ours":1,"changed_theirs":1,"dropped":1,"cross_links":0}
```

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Conflict` | Overlapping changes; nothing was created | See [resolving conflicts](../rules/conflicts.md#branch-merge-conflicts) |
| `NoMergeBase` | The Branches share no history | Fork from a common Branch |
| `NameTaken` | `name` exists | Pick a new name |
| `NoSuchBranch` | `a` or `b` doesn't exist | Check the names |
| `InvalidInput` | A malformed unit pattern, or an empty or repeated unit name | Fix `units` |

A live `Conflict` (stderr, exit 125). Its `detail` keeps camelCase keys:

```json
{"code":"Conflict","message":"c1 + c2 conflict in 1 file(s); nothing was created",
 "detail":{"sources":[…],"mergeBase":"12ad2061f04f…",
  "conflicts":[{"path":"/src/index.js","kind":"content","blocks":1,"hunksOmitted":false,
    "hunks":[{"line":1,"ours":["export const retries = 7;"],"theirs":["export const retries = 9;"]}]}],
  "lww":[],"conflictsTruncated":false,"lwwTruncated":false,"units":[],"unitsTruncated":false}}
```

The lists are bounded: `conflictsTruncated`, `lwwTruncated`, `unitsTruncated`
and each conflict's `hunksOmitted` say that something was left out of the
reply, never out of the merge. For an incomplete summary, page
`branch.diff(project, a, b)` through every path and read both sides.

## `branch.publish`

`branch.publish(project, branch, commit?)`

Makes `main` show the Branch's head, or the given Commit, by adding a Layer
to `main`.

```bash
layerfs branch publish shop/api work
```

```json
{"kind":"Added","source":{"branch":"work","commit":"12ad2061f04f…"},
 "published_layer":{"id":"3256e1f13d0b…","parent":"3275838904b6…","root":"14a98eac1696…",
  "source_branch":"work","source_commit":"12ad2061f04f…","logical":251,"overwrote_count":0},
 "based_on":"3275838904b6…","overwrote_count":0,"overwrote":[],"overwrote_has_more":false}
```

| `kind` | Meaning |
| --- | --- |
| `Added` | `main` now shows that Commit; `published_layer` is the new Layer; `overwrote` lists `main` Layers that were skipped |
| `UpToDate` | `main` already shows it (`{ kind, layer }`) |
| `NoChanges` | Nothing to publish (1.1.0 shape; not seen in this pass) |

| Code | Meaning | Do |
| --- | --- | --- |
| `NoCommit` | The Branch has no Commit yet | Commit something first |
| `NoSuchBranch` | No such Branch | Check the name |
| `HeadMoved` | `main` moved during the call | Merge `main` in (below), then publish again |
| `InvalidInput` | `commit` is not a full Commit id | Pass a full id |

**Publishing replaces `main`'s view** with your Commit. Layers published to
`main` after your Branch's base that your Commit doesn't contain are skipped:
they stay in history and `overwrote` lists them. If someone else may have
published since you forked, merge `main` in first, check the result, then
publish the merged Branch:

```js
const P = "shop/api";
const m = await layerfs.branch.merge(P, "work-release", "work", "main", { message: "merge main before publish" });
// mount m.branch in a Sandbox and run the tests here, then:
const p = await layerfs.branch.publish(P, m.branch);
if (p.kind === "Added" && p.overwrote.length) console.log("warning: skipped", p.overwrote);
```

## `branch.diff`

`branch.diff(project, from_branch, to_branch, { paths_only, page_size, page_pos, cursor })`

Files that differ between two Branches' heads (`main` counts as one). The
result is the same shape as [`commit.diff`](commit.md#commitdiff):
`{ kind: "ok", from, to, items, has_more, next_page_pos, next_cursor }`, with
`from` and `to` as `{ ref, label, root }`.

```bash
layerfs branch diff shop/api main work --paths-only
```

`paths_only` leaves out each item's `hunks` (they come back `null`). Resume a
long diff with `cursor: next_cursor`. Refused under a folder scope.
