# branch family

[Architecture](../../../README.md) › [API](../../README.md) › [Call table](../README.md) › branch family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `branch` calls.

| | |
| --- | --- |
| Component | [Call table](../README.md) |
| Source | [`api/crates/layerfs-api/src/branch`](../../../../../api/crates/layerfs-api/src/branch), [`api/crates/layerfs-control/src/store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs), [`engine/crates/layerfs-store-service/src/methods`](../../../../../engine/crates/layerfs-store-service/src/methods) |

## Calls

Six calls, in help order
([`branch/mod.rs`](../../../../../api/crates/layerfs-api/src/branch/mod.rs)).
Each entry runs `layerfs_control::store::calls::run`, which names one Store
method of the engine's SDK facade
([`methods/dispatch.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs)).
Result keys are snake_case after the dispatcher.

| Call | Arguments | Options | Returns | Store method |
| --- | --- | --- | --- | --- |
| [`list`](../../../../../api/crates/layerfs-api/src/branch/list.rs) | `project` (Project) | `page_size`, `page_pos` | `{ items, has_more, next_page_pos }`; each item `{ name, id, base, head, forked_from, … }`, `main` included | `project.list_branches` |
| [`get`](../../../../../api/crates/layerfs-api/src/branch/get.rs) | `project`, `branch` (Name) | — | the one list item whose `name` or `id` is `branch` | `project.list_branches` with `name` |
| [`fork`](../../../../../api/crates/layerfs-api/src/branch/fork.rs) | `project`, `name` (Name), `from_branch` (Name), `from_commit?` (Name) | — | `{ kind: "Forked", branch, base, head, … }` | `branch.fork` |
| [`merge`](../../../../../api/crates/layerfs-api/src/branch/merge.rs) | `project`, `name`, `a`, `b` (Name) | `message` (Text), `units` (Json) | `{ kind: "Merged", fast_forward, branch, sources, merge_base, lww, units, … }` | `branch.merge` |
| [`publish`](../../../../../api/crates/layerfs-api/src/branch/publish.rs) | `project`, `branch` (Name), `commit?` (Name) | — | `{ kind: "Added", source, published_layer, based_on, overwrote_count, overwrote, overwrote_has_more }`, or `UpToDate` / `NoChanges` | `branch.publish` |
| [`diff`](../../../../../api/crates/layerfs-api/src/branch/diff.rs) | `project`, `from_branch`, `to_branch` (Name) | `paths_only`, `page_size`, `page_pos`, `cursor` | `{ kind: "ok", from, to, items, has_more, next_page_pos, next_cursor }` | `branch.diff_branches` |

Result fields are built in
[`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs)
and [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs).
`branch.get` of `main` answers `{ name: "main", id: null, base, head: null }`.

## Arguments and options

`store/calls.rs` renames table arguments to the Store method's names:

| Table name | Store field | Call |
| --- | --- | --- |
| `page_size`, `page_pos` | `pageSize`, `pagePos` | `list`, `diff` |
| `branch` | `name` | `get` |
| `from_commit`, else `from_branch` | `from` | `fork` |
| `a`, `b` | `branch`, `with` | `merge` |
| `from_branch`, `to_branch` | `from`, `to` | `diff` |
| `paths_only` | `hunks` = not `paths_only` | `diff` |

- `PAGE` is `page_size` and `page_pos`; `DIFF` adds `paths_only` (Bool)
  and `cursor` (Text); `MESSAGE` is `message` (Text)
  ([`calls/table.rs`](../../../../../api/crates/layerfs-control/src/calls/table.rs)).
- The Store's paging takes `pageSize` in 1..=1000, default 50
  ([`operator/args.rs`](../../../../../engine/crates/layerfs-store-service/src/operator/args.rs)).
- `units` is a list of patterns, each a bare pattern or
  `{ name, patterns }` ([`methods/branch/merge.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/branch/merge.rs)).
- Under a folder scope, Branch names are composed as `<namespace>.<name>`
  and `branch.diff` is refused as history
  ([`calls/scope.rs`](../../../../../api/crates/layerfs-control/src/calls/scope.rs)).
  A scoped `branch.list` pages through every Branch with `pageSize` 1000 and
  keeps only the namespace's, named relative to it.

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad argument; an empty or out-of-range page | [`calls/check.rs`](../../../../../api/crates/layerfs-control/src/calls/check.rs), [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchStore`, `NoSuchProject` | the project's Store or project is missing | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | a named Branch, Commit or Layer is missing | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs), [`store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs) |
| `ReservedName`, `NameTaken` | `fork` or `merge` to `main` or to an existing name | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `Conflict`, `NoMergeBase` | `merge` conflicts (nothing is created), or the sides share no history | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `NoCommit` | `publish` of a Branch with no saved Commit | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `Busy`, `Unresolved` | the Store's record holds another operation's `op` | [`common/op.rs`](../../../../../api/crates/layerfs-control/src/common/op.rs) |
| `Unavailable` | the Store's worker is not recorded or does not answer | [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) |
| `Failed` | a list made no progress | [`store/calls.rs`](../../../../../api/crates/layerfs-control/src/store/calls.rs) |
| `Unknown` | **uncertain:** a worker's answer was lost, or the Store reported an unknown outcome | [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs), [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |

An `Unknown` `fork`, `merge` or `publish` may have published. Read the
Branch with `branch.get` or `branch.list` before acting again. See
[error kinds](../errors.md#error-kinds) and
[uncertain outcomes](../errors.md#uncertain-outcomes).

## Where it runs

```text
 branch.<call>
   |  store::calls::run --> resolve::target (records) --> store_call
   v
 lease::claim (Store lock, record re-read, `op` check)
   |
   +-- PostgreSQL Store ------> open::run in the caller's process
   +-- sqlite, Executor::Local -> open::run in the caller's process
   +-- sqlite, Executor::Worker -> worker op "store.call" { op, args }
                                   open::run on the worker's Store file
```

See [`store/call.rs`](../../../../../api/crates/layerfs-control/src/store/call.rs)
and [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs).
A sqlite Store runs locally when its machine's platform executes locally on
this host (`native`, except on Windows). The caller holds the Store's lease
while the worker runs the call. No daemon serves a Store call. See the
[routing table](../../routing/routing-table.md).

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

- [Table](../table.md) and [Errors](../errors.md).
- [commit family](commit.md) and [file family](file.md): the other Store calls.
- [Routing table](../../routing/routing-table.md).
- [Engine: Store calls](../../../engine/store-calls/README.md), [branches](../../../engine/store-calls/branches.md).
- [Fork and merge flow](../../../flows/merge.md).

---

← [Call table: project family](project.md) · [Up: Call table](../README.md) · [Call table: commit family](commit.md) →
