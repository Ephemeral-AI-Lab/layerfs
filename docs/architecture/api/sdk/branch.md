# branch family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `branch` calls: list and read a project's Branches, fork one, merge one into another, publish one to `main` and remove one, within the [SDK](README.md). A Branch's Commits are read by the [commit family](commit.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/branch`](../../../../api/crates/layerfs-api/src/branch) (one file per call), [`layerfs-store/src/calls/branch.rs`](../../../../control/crates/layerfs-store/src/calls/branch.rs), [`layerfs-store/src/calls/op.rs`](../../../../control/crates/layerfs-store/src/calls/op.rs), [`layerfs-store-service/src/methods/branch`](../../../../engine/crates/layerfs-store-service/src/methods/branch), [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`history/units.rs`](../../../../engine/crates/layerfs-store-service/src/history/units.rs) |
| Runs by default | where the Store's calls run: the caller's process for a PostgreSQL Store and for a sqlite Store on a locally executing machine (`native`, except on Windows); else the Store machine's worker (`store.call`). No daemon serves a Store call |
| Design | [Store calls](../../engine/store-calls/README.md), [History](../../core/history/README.md), [merge flow](../../flows/merge.md) |

## Calls

```text
                 main (the project's line of publications)
                   |
   list / get      | fork <project> agent            fork <project> review agent --at C
                   v                                          |
                 agent ---- file.* writes,                    v
                   |        workspace.commit --------->  review
                   +------ merge <project> review agent ------+   (agent into review)
                   +------ merge <project> agent main --into joined
                   main <------- publish <project> agent
                   (next publication; --expected guards main's head)
```

| Call | Store operation (`StoreOp`) | Kind | MCP |
| --- | --- | --- | --- |
| [`branch.list`](#branchlist) | `Branches` (one page by name) | read | `admin` |
| [`branch.get`](#branchget) | `ListBranches` with the name | read | `admin` |
| [`branch.fork`](#branchfork) | `Fork` | write | `work` |
| [`branch.merge`](#branchmerge) | `Merge` | write | `admin` |
| [`branch.publish`](#branchpublish) | `Publish` | write | `admin` |
| [`branch.remove`](#branchremove) | `BranchRemove` | write | `admin` |

Each call file builds a typed request and calls one function of
[`layerfs_store::calls::branch`](../../../../control/crates/layerfs-store/src/calls/branch.rs).
That function sends one `StoreOp` with `on_project`
([`calls/op.rs`](../../../../control/crates/layerfs-store/src/calls/op.rs)),
which renames the table's snake_case arguments to the engine's and runs the
engine method on the project's Store, here or in its worker, and then maps
the engine's answer to the call's full map: the keys of its long form, which
the dispatcher cuts to the short form unless `detail` is asked. A refusal
goes through `next_fields`, which puts the fields the next call needs flat
beside it: the head a write found, as `commit`; how many Commits a prefix
matches, as `matches`; a merge's conflicting paths, as `conflicts`.

`main` is the project's line of publications. Where a call takes a Commit
(`--at`, `--expected`), it is a full id (66 hex characters) or a prefix of 8
or more characters, resolved by the Store in one call: a prefix more than one
Commit starts with is refused `InvalidInput` with `matches` (a count), one no
Commit starts with is `NotFound`. Where a call takes `from`, a Branch of that
name comes first, then a Commit. Every id in an answer is the full id.

Reads use the Store's read-only session and never take the publication lane;
a write does its heavy work first and holds the lane only for its final
publish step. Under a folder scope a Branch name is `<namespace>.<name>` and
`list` and `get` answer the name within the namespace
([`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs));
`branch.remove` is outside the scope's allow list and is refused.

### branch.list

A project's Branches, main included, in name order.

```text
CLI     layerfs branch list <project> [--prefix P] [--exclude-prefix P] [--limit 100] [--cursor C]
Node    await layerfs.branch.list(project, { prefix, exclude_prefix, limit, cursor })
Python  layerfs.branch.list(project, *, prefix=None, exclude_prefix=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `prefix` option | text | only Branches whose name starts with this text |
| `exclude_prefix` option | text | leave out Branches whose name starts with this text, such as dsh. |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the filters only |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor` |
| `items[]` short | `branch`, `commit` |
| `items[]` long | `branch, commit, forked_from, id, base, root` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `admin` |

`main` is a row; its `commit` is `main`'s head. Any other row's `commit` is
the Branch's head, or the Commit it was forked at while it has none of its
own; `forked_from` (long form) is the Commit it was forked at. A Branch
record has no time, so there is no `created_at`. `--prefix` keeps only the
names that start with it and `--exclude-prefix` leaves those out (such as
`dsh.`); the Store's name index skips what is left out instead of walking
it. The `next_cursor` carries the filters and the last name listed, and pins
no Commit, so a later page lists the Branches there are then; a cursor given
with other filters, or to another call, is refused `InvalidInput`. A scoped
list reads the namespace's names only and answers them without the
namespace.

Run on macOS native with a debug build:

```console
$ layerfs branch list shop/api
{"items":[{"branch":"agent","commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c"},{"branch":"main","commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c"}]}
```

### branch.get

One Branch and its head Commit.

```text
CLI     layerfs branch get <project> <branch>
Node    await layerfs.branch.get(project, branch)
Python  layerfs.branch.get(project, branch)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |

| Answer | Keys |
| --- | --- |
| short | `commit` |
| long | `commit, forked_from, branch, id, base, root` |
| call | writes one call-log line; MCP `admin` |

One `branch.list` row, read through the Store's name index. `commit` is the
head to pass as `--expected` to a later write. A name no Branch has is
`NotFound`. It is the read to make after a fork, merge or publish whose
answer was lost.

### branch.fork

A new Branch from another Branch's head, or from a Commit.

```text
CLI     layerfs branch fork <project> <name> [<from>] [--at C]
Node    await layerfs.branch.fork(project, name, from?, { at })
Python  layerfs.branch.fork(project, name, from_=None, *, at=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `name` | name | the new Branch |
| `from` | name | the Branch forked from; optional, default `main` |
| `at` option | name | fork at this Commit instead of the head of `from`: a full id, or a prefix of 8 or more characters |

| Answer | Keys |
| --- | --- |
| short | `commit` |
| long | `commit, outcome, branch, base, root` |
| call | writes one call-log line; MCP `work` |

Forks from the head of `from` (`main` when it is left off), or with `--at`
from one Commit of `from`'s history. The new Branch records no file: it is
one short write through the Store's single writer and reads no tree.
`commit` is the Commit the new Branch stands on.

| Refusal | When | Next |
| --- | --- | --- |
| `Exists` | the name is taken (raised `NameTaken`), a removed Branch's name included until compaction | `fix`: the same fork with `<name>-2` |
| `InvalidInput` | `name` is `main` (raised `ReservedName`); a prefix that more than one Commit starts with (`matches`) | |
| `NotFound` | `from` names no Branch or Commit; `--at` is not in `from`'s history | |
| `Unknown` | the reply was lost | `branch get <project> <name>` says whether it was made; a second fork is `Exists` |

Run on macOS native with a debug build:

```console
$ layerfs branch fork shop/api agent main
{"commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c"}
```

### branch.merge

Merge one Branch into another, or into a new Branch.

```text
CLI     layerfs branch merge <project> <branch> <from> [--into NEW] [--units JSON] [--expected C] [-m msg]
Node    await layerfs.branch.merge(project, branch, from, { into, units, expected, message })
Python  layerfs.branch.merge(project, branch, from_, *, into=None, units=None, expected=None, message=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | the Branch merged into; its `/.lww` rules apply |
| `from` | name | the Branch merged from |
| `into` option | name | write the result to a new Branch of this name and leave `branch` as it is |
| `units` option | json | merge units: a list of patterns or {name, patterns}; a unit both sides changed is taken whole from the side with the later write |
| `expected` option | name | refuse `Conflict` unless `branch`'s head is this Commit |
| `message` option | text | the merge Commit's message; `-m` in the shell; `Merge <branch> and <from>` when left off |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `fast_forward`, `overwritten`, `overwritten_count` |
| long | `commit, unchanged, fast_forward, overwritten, overwritten_count, outcome, branch, parent, second_parent, merge_base, root, units, units_count` |
| call | writes one call-log line; MCP `admin` |

Merges `from` into `branch`. When `branch` holds nothing `from` lacks, the
merge is a fast-forward (`fast_forward`) and no Commit is made; when
`branch` already holds `from`, nothing changes and the answer says
`unchanged`; otherwise one merge Commit is made on `branch`, `parent` being
`branch`'s head and `second_parent` `from`'s. `--into NEW` writes the result
to a new Branch and changes neither side. `main` takes changes only by
`publish`: merging into it without `--into` is `InvalidInput`.

The `/.lww` rules of `branch` decide paths both sides changed:
`overwritten` lists the paths a newer-write-wins rule took (the first 20,
then `overwritten_count`). `--units` groups paths into units; a unit both
sides changed comes whole from the side with the later write, and
overlapping units belong to the first listed
([`history/units.rs`](../../../../engine/crates/layerfs-store-service/src/history/units.rs)).
The long form's `units` reports each unit's decision (the first 20, then
`units_count`). All merge work happens before the publication lane; only
the final publish holds it.

| Refusal | When | Next |
| --- | --- | --- |
| `Conflict` | conflicting paths: nothing is changed, and `conflicts` lists them; or `branch`'s head is not `--expected`, with `commit` naming the head | merge again from the head |
| `Exists` | the `--into` name is taken | `fix`: the same merge with `--into <into>-2` |
| `InvalidInput` | merging into `main` without `--into`; the sides share no history (raised `NoMergeBase`); a bad `--units`; an ambiguous prefix (`matches`) | |
| `NotFound` | `branch` or `from` is missing | |
| `Failed` | a Store without merge scratch, or another failure that ran once | |

Run on macOS native with a debug build:

```console
$ layerfs branch merge shop/api agent main --into merged
{"commit":"12d12fbd01e3990284c374af5ed021fa2551db346d2a178cf4f1228794fc6f7f80","fast_forward":true}
```

### branch.publish

A Branch's head or Commit as main's next head, refusing a stale base.

```text
CLI     layerfs branch publish <project> <branch> [--at C] [--expected C] [--force]
Node    await layerfs.branch.publish(project, branch, { at, expected, force })
Python  layerfs.branch.publish(project, branch, *, at=None, expected=None, force=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch other than `main` |
| `at` option | name | publish this Commit instead of the Branch's head: a full id, or a prefix of 8 or more characters |
| `expected` option | name | refuse `Conflict` unless main's head is this Commit |
| `force` option | bool | publish over what main gained since the Branch forked |

| Answer | Keys |
| --- | --- |
| short | `commit`; when set: `unchanged`, `previous` |
| long | `commit, unchanged, previous, outcome, branch, source_commit, base, root, size` |
| call | writes one call-log line; MCP `admin` |

Publishes the Branch's head, or with `--at` one Commit of its history, as
`main`'s next head. `commit` is `main`'s new head. A Commit `main` already
holds, or whose tree equals `main`'s head, answers `unchanged` with
`main`'s head. `previous` is `main`'s head before, when it was not the
Branch's base.

Without `--force` the publish is refused `Conflict` when `main` gained a
publication since the Branch forked, so no work on `main` is dropped: merge
`main` into the Branch and publish again. It goes ahead when `main` is still
at the Branch's fork point, or when `main`'s head is this Branch's own
earlier publication and the Commit contains it (publishing again at
milestones); a Branch made by merging `main` in has `main`'s head as its
base and publishes as a fast-forward. `--force` publishes over what `main`
gained; the publications it replaced are the rows `commit list <project>
main` shows between the new head and `base`. `--expected C` refuses
`Conflict` unless `main`'s head is `C`. The add is one compare-and-set on
`main`'s head inside the lane, never retried.

| Refusal | When | Next |
| --- | --- | --- |
| `Conflict` | `main` gained a publication since the fork (without `--force`), or `main`'s head is not `--expected`; `commit` names `main`'s head | merge `main` in and publish again, or `--force` |
| `NotFound` | the Branch is missing or is `main`; it has no Commit; `--at` is not in its history (raised `NoCommit`, `NotInHistory`) | |
| `InvalidInput` | an ambiguous prefix (`matches`) | |
| `Unknown` | the reply was lost | publish the same Commit again: it answers `unchanged` if it landed |

Run on macOS native with a debug build:

```console
$ layerfs branch publish shop/api agent
{"commit":"32220c54f9cc0f24b9f32bbb17fd8a1748130284aa07cc1c39348ec5888118c916"}
$ layerfs branch publish shop/api agent --detail
{"commit":"32220c54f9cc0f24b9f32bbb17fd8a1748130284aa07cc1c39348ec5888118c916","unchanged":true,"previous":null,"outcome":"unchanged","branch":"agent","source_commit":null,"base":null,"root":null,"size":null}
```

### branch.remove

Remove one Branch; its name stays taken until compaction.

```text
CLI     layerfs branch remove <project> <branch>
Node    await layerfs.branch.remove(project, branch)
Python  layerfs.branch.remove(project, branch)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` | name | a Branch name |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, branch, removed_at` |
| call | writes one call-log line; MCP `admin` |

Retires one Branch in its Store: a marker hides it from listings and name
resolution, fork, merge, publish and mount of it are refused, and its
Commits and objects stay until a later
[`store.compact`](store.md#storecompact). Its name stays taken until then.

| Refusal | When | Source |
| --- | --- | --- |
| `InvalidInput` | the Branch is `main` (raised `ReservedName`) | [`methods/branch/remove.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/remove.rs) |
| `NotFound` | the Branch is missing or already removed | same |
| `InUse` | a Workspace of the Branch is mounted (raised `MountsActive`) | [`history/admission.rs`](../../../../engine/crates/layerfs-store-service/src/history/admission.rs) |
| `Busy`, `Unresolved` | another operation holds the Branch, or a gone owner left one uncertain | |
| `Unknown` | worker route: the answer was lost; it names the Branch | [`unknown.rs`](../../../../control/crates/layerfs-store/src/unknown.rs) |

Run on macOS native with a debug build:

```console
$ layerfs branch remove shop/api merged
{"ok":true}
```

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument, a cursor of other filters or another call, an ambiguous Commit prefix (`matches`); out of scope | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs), [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NotFound` | the project's Store, the project, a Branch or a Commit is missing (raised `NoSuchStore`, `NoSuchProject`, `NoSuchBranch`, `NoSuchCommit`) | [`resolve.rs`](../../../../control/crates/layerfs-store/src/resolve.rs), [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Busy`, `Unresolved` | a move or remove holds the Store, or left its outcome unknown | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer; a PostgreSQL server is unreachable. Nothing starts a stopped machine: `fix: layerfs machine start <machine>` | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `Unsupported` | a sqlite Store reached from another kernel, or on a removed platform's machine | [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| `Unknown` | a worker's answer was lost, or the Store reported an unknown outcome; after a fork, merge or publish, read `branch get` first | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Related

- [commit family](commit.md), [file family](file.md), [workspace family](workspace.md).
- [Store calls](../../engine/store-calls/README.md).
- [Merge flow](../../flows/merge.md), [compare and merge](../../core/compare-and-merge/README.md).
- [Errors and events](../calls/errors-and-events.md).
