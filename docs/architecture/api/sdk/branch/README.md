# branch family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › branch

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `branch` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/branch`](../../../../../api/crates/layerfs-api/src/branch), [`calls/branch.rs`](../../../../../control/crates/layerfs-store/src/calls/branch.rs), [`engine/crates/layerfs-store-service/src/methods`](../../../../../engine/crates/layerfs-store-service/src/methods) |

## Calls

```text
                 main (the project's line of Layers)
                   |
   list / get      | fork <name> main         fork <name> <branch> [commit]
                   v                                   |
                 agent  ---- file.* writes,            v
                   |         workspace.commit -->  review
                   |                                   |
                   +------ diff <from> <to> -----------+
                   +------ merge <new> <a> <b> --------+--> joined
                                                            |
                   main <---------- publish <branch> -------+
                   (next Layer; overwritten Layers reported)
```

Eight calls, in help order
([`branch/mod.rs`](../../../../../api/crates/layerfs-api/src/branch/mod.rs)).
Each entry forwards a typed request to its own function under
`layerfs_store::calls::branch` (`list`, `get`, `commits`, `get_commit`, and
the existing mutation/comparison functions), which selects one Store method
of the engine's SDK facade
([`methods/dispatch.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs)).
Result keys are snake_case after the dispatcher.

| Call | Arguments | Options | Returns | Store method |
| --- | --- | --- | --- | --- |
| [`list`](list.md) | `project` (Project) | `page`, `page_size`, legacy `page_pos` | `{ items, has_more, next_page_pos, consistency }`; each item includes typed `saved_head`; `main` included | `project.list_branches` |
| [`get`](get.md) | `project`, `branch` (Name) | — | branch details with typed `saved_head`, preserving `base`/`head` | `project.list_branches` with `name` |
| [`commits`](commits.md) | `project`, `branch` | `at_commit`, `page`, `page_size` | pinned `at_commit`, compact `items`, boolean `has_more`, scan facts | `branch.commits` |
| [`get_commit`](get_commit.md) | `project`, `branch`, `commit` | — | full typed record and distinct parent/base/source relationships | `branch.get_commit` |
| [`fork`](fork.md) | `project`, `name` (Name), `from_branch` (Name), `from_commit?` (Name) | — | `{ kind: "Forked", branch, base, head, … }` | `branch.fork` |
| [`merge`](merge.md) | `project`, `name`, `a`, `b` (Name) | `message` (Text), `units` (Json) | `{ kind: "Merged", fast_forward, branch, sources, merge_base, lww, units, … }` | `branch.merge` |
| [`publish`](publish.md) | `project`, `branch` (Name), `commit?` (Name) | — | `{ kind: "Added", source, published_layer, based_on, overwrote_count, overwrote, overwrote_has_more }`, or `UpToDate` / `NoChanges` | `branch.publish` |
| [`diff`](diff.md) | `project`, `from_branch`, `to_branch` (Name) | `paths_only`, `from_commit`, `to_commit`, `path_prefix`, `include_attribution`, `page`, `page_size`, legacy `page_pos`/`cursor` | pinned typed `from`/`to`, changed paths, independent flags and coverage; legacy calls retain their fields | `branch.diff_branches` |

Result fields are built in
[`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs)
and [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs).
`branch.get` of `main` preserves `{ name: "main", id: null, base, head: null }`
and adds typed `saved_head` for its current Layer. Side branches resolve their
Commit head, or their base Layer when no Commit exists. Main is project core
content; saved branch commits are checkpoints, without a new API family.

Main history follows publication Layer parents. Source Commits are distinct
links; publication alone does not reassign source line owners. Layer records
persist no publication timestamp: return null with `time_status: unavailable`,
never a borrowed source Commit time.

## Arguments and options

The typed control adapter in `calls/branch.rs` translates public fields to
the Store method's fields:

| Table name | Store field | Call |
| --- | --- | --- |
| `page_size`, `page_pos` | `pageSize`, `pagePos` | legacy `list`, `diff` |
| `page`, `page_size` | `page`, `pageSize` | one-based matching-result pages |
| `at_commit` | `atCommit` | pinned history start |
| `from_commit`, `to_commit` | `fromCommit`, `toCommit` | mixed Commit/Layer diff endpoints |
| `path_prefix`, `include_attribution` | `pathPrefix`, `includeAttribution` | bounded comparison selection |
| `branch` | `name` | `get` |
| `from_commit`, else `from_branch` | `from` | `fork` |
| `a`, `b` | `branch`, `with` | `merge` |
| `from_branch`, `to_branch` | `from`, `to` | `diff` |
| `paths_only` | `hunks` = not `paths_only` | `diff` |

- `PAGE` is `page_size` and `page_pos`; `DIFF` adds `paths_only` (Bool)
  and `cursor` (Text); `MESSAGE` is `message` (Text)
  ([`calls/table.rs`](../../../../../api/crates/layerfs-api/src/calls/table.rs)).
- The Store's paging takes `pageSize` in 1..=1000, default 50
  ([`operator/args.rs`](../../../../../engine/crates/layerfs-store-service/src/operator/args.rs)).
- `units` is a list of patterns, each a bare pattern or
  `{ name, patterns }` ([`methods/branch/merge.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/branch/merge.rs)).
- Under a folder scope, Branch names are composed as `<namespace>.<name>`
  and `branch.commits`, `branch.get_commit`, and `branch.diff` are refused as history
  ([`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs)).
  A scoped `branch.list` returns namespace-relative names. Numbered queries
  enforce candidate/reply budgets; a page that cannot complete refuses `Capacity`.
- Saved queries reuse complete immutable IDs and matching-result pages, with
  no cursor requirement or fabricated totals. Do not mix page and legacy paging
  options. Branch inventory explicitly reports live consistency; saved history
  and diff remain pinned. See [saved-query contract](../saved-queries.md).

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument; an empty or out-of-range page | [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs), [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchStore`, `NoSuchProject` | the project's Store or project is missing | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | a named Branch, Commit or Layer is missing | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs), [`calls/branch.rs`](../../../../../control/crates/layerfs-store/src/calls/branch.rs) |
| `Capacity` | a numbered history/comparison page cannot fit declared work/reply bounds | [`read/query.rs`](../../../../../engine/crates/layerfs-store-service/src/read/query.rs) |
| `ReservedName`, `NameTaken` | `fork` or `merge` to `main` or to an existing name | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `Conflict`, `NoMergeBase` | `merge` conflicts (nothing is created), or the sides share no history | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `NoCommit` | `publish` of a Branch with no saved Commit | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `Busy`, `Unresolved` | the Store's record holds another operation's `op` | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Failed` | a list made no progress | [`calls/branch.rs`](../../../../../control/crates/layerfs-store/src/calls/branch.rs) |
| `Unknown` | **uncertain:** a worker's answer was lost, or the Store reported an unknown outcome | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |

An `Unknown` `fork`, `merge` or `publish` may have published. Read the
Branch with `branch.get` or `branch.list` before acting again. See
[error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Where it runs

```text
 branch.<call>
   |  typed calls::branch request --> resolve::target --> store_call
   v
 lease::claim (Store lock, record re-read, `op` check)
   |
   +-- PostgreSQL Store ------> open::run in the caller's process
   +-- sqlite, Executor::Local -> open::run in the caller's process
   +-- sqlite, Executor::Worker -> worker op "store.call" { op, args }
                                   open::run on the worker's Store file
```

See [`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs)
and [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs).
A sqlite Store runs locally when its machine's platform executes locally on
this host (`native`, except on Windows). The caller holds the Store's lease
while the worker runs the call. No daemon serves a Store call. See the
[routing table](../../../control/routing/routing-table.md).

## Example

Not run; written from the call table.

```js
await layerfs.branch.fork("shop/api", "agent", "main");
await layerfs.branch.merge("shop/api", "joined", "agent", "review", { message: "join" });
```

```python
layerfs.branch.fork("shop/api", "agent", "main")
layerfs.branch.merge("shop/api", "joined", "agent", "review", message="join")
```

```console
$ layerfs branch fork shop/api agent main
$ layerfs branch merge shop/api joined agent review -m join
```

## Related

- [Table](../../call-table/table.md) and [Errors](../../call-table/errors.md).
- [commit family](../commit/README.md) and [file family](../file/README.md): the other Store calls.
- [Routing table](../../../control/routing/routing-table.md).
- [Engine: Store calls](../../../engine/store-calls/README.md), [branches](../../../engine/store-calls/branches.md).
- [Fork and merge flow](../../../flows/merge.md).

---

← [project.remove](../project/remove.md) · [Up: SDK](../README.md) · [branch.list](list.md) →
