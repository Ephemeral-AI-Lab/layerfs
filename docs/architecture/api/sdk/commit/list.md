# commit.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [commit](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists a Branch's Commits, newest first, one page at a time, with each
Commit's message, parents and storage statistics.

| | |
| --- | --- |
| Family | [commit](README.md) |
| Library entry | [`api/crates/layerfs-api/src/commit/list.rs`](../../../../../api/crates/layerfs-api/src/commit/list.rs) |
| Implemented in | `layerfs_control::store::calls::run` → Store method `branch.list_commits` ([`methods/branch/list_commits.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/branch/list_commits.rs), [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs)) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.commit.list(project, branch, { page_size, page_pos });
```

```python
# Python
layerfs.commit.list(project, branch, page_size=None, page_pos=None)
```

```bash
# CLI
layerfs commit list <store>/<project> <branch> [--page-size N] [--page-pos N]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>` | [`store/resolve.rs`](../../../../../api/crates/layerfs-control/src/store/resolve.rs) |
| `branch` | Name | a Branch; `main` has no Commits and is `NoSuchBranch` | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) `branch` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `page_size` | Int | 50 | 1..=1000 | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `page_pos` | Int | 0 | the last page's `next_page_pos` | same |

## Returns

```text
 { items: [ commit, ... ]  (newest first, from the head),
   has_more, next_page_pos | null }

 commit = { id, branch, parent | null, second_parent | null, base, root,
            via, message | null, merged_from | null, time,
            logical, new_canonical, new_stored, new_metadata,
            displaced | null, lww: [..], lww_truncated, units? }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `id` | text | the Commit id (66 hex) | [`sdk_values.rs`](../../../../../engine/crates/layerfs-bridge/src/sdk_values.rs) `commit_value` |
| `parent`, `second_parent` | text or null | first parent; the merged side for a merge | same |
| `base`, `root` | text | its base Layer and tree root | same |
| `via` | text | how it was made (a Workspace Commit, a `file.*` write, a merge, …) | same |
| `message`, `time` | text or null, int | message and time in ms | same |
| `logical`, `new_canonical`, `new_stored`, `new_metadata` | int | logical bytes and what this Commit added to the Store | same |
| `displaced` | text or null | the head it overwrote | same |
| `lww`, `lww_truncated` | list, bool | last-writer-wins facts, cut at 4 KiB | same |
| `units` | list | a merge's units, only when given | same |

Each item costs its message plus 1 KiB against the 48 KiB reply budget; a
page ends early when it is spent.

## Where it runs

```text
 caller process                       Store's machine worker / this process
 resolve::target; lease::claim
 store_call branch.list_commits --->  lookup Branch; commit_history pages
                                      of 128 from the head, skip page_pos
```

A read; no lane. Refused under a folder scope (`out of scope: history`).
See the [routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad project or paging; a folder-scoped client | no | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `NoSuchBranch` | no such Branch, or `main` | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchStore`, `NoSuchProject`, `Busy`, `Unresolved`, `Unavailable` | as for every Store call | no | [`store/call.rs`](../../../../../api/crates/layerfs-control/src/store/call.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`machine/worker/client.rs`](../../../../../api/crates/layerfs-control/src/machine/worker/client.rs) |

`commit.list` is the read to make after an uncertain
[workspace.commit](../workspace/commit.md) or `file.*` write.

## Examples

Not run; written from the call table.

```js
const { items } = await layerfs.commit.list("shop/api", "agent", { page_size: 10 });
console.log(items[0]?.message);
```

```python
for c in layerfs.commit.list("shop/api", "agent")["items"]:
    print(c["id"][:12], c["message"])
```

```bash
layerfs commit list shop/api agent --page-size 10
```

## Related

- Before: [branch.get](../branch/get.md). After: [commit.get](get.md),
  [commit.diff](diff.md), [branch.fork](../branch/fork.md) from a Commit.
- [Engine: branches, listings](../../../engine/store-calls/branches.md#lookup),
  [Core: History model](../../../core/history-model/README.md).

---

← [SDK: commit](README.md) · [Up: commit](README.md) · [commit.get](get.md) →
