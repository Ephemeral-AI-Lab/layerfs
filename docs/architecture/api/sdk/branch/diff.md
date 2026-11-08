# branch.diff

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [branch](README.md) › diff

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists the files that differ between two Branches' current trees (either
may be `main`), with line hunks unless asked for paths only.

| | |
| --- | --- |
| Family | [branch](README.md) |
| Library entry | [`api/crates/layerfs-api/src/branch/diff.rs`](../../../../../api/crates/layerfs-api/src/branch/diff.rs) |
| Implemented in | `layerfs_store::calls::branch_diff` → Store method `branch.diff_branches` ([`methods/branch/diff_branches.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/branch/diff_branches.rs), [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs)) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.branch.diff(project, from_branch, to_branch, { paths_only, page_size, page_pos, cursor });
```

```python
# Python
layerfs.branch.diff(project, from_branch, to_branch, paths_only=None,
                    page_size=None, page_pos=None, cursor=None)
```

```bash
# CLI
layerfs branch diff <store>/<project> <from_branch> <to_branch> \
  [--paths-only] [--page-size N] [--page-pos N] [--cursor PATH]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>` | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `from_branch`, `to_branch` | Name | a Branch or `main` each (a Commit id is `InvalidInput`: use [commit.diff](../commit/diff.md)); sent as `from`, `to` | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) `diff_branches` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `paths_only` | Bool | false | sent as `hunks = !paths_only` | [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs) `diff_options` |
| `page_size` | Int | 50 | 1..=1000 | [`methods/args.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `page_pos` | Int | 0 | offset from the last page's `next_page_pos` | same |
| `cursor` | Text | none | the last page's `next_cursor` (a path); resumes after it | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |

## Returns

```text
 { kind: "ok",
   from: { ref, label, root }, to: { ref, label, root },
   items: [ { path, kind: Added|Removed|Modified, type,
              before: { type, inode } | null, after: { ... } | null,
              content, identity, metadata,
              hunks: [ ... ] | null, hunks_omitted } ],
   has_more, next_page_pos, next_cursor }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `from`, `to` | map | the side's name, label and root | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `kind` (item) | text | `Added`, `Removed`, `Modified` | same, `change` |
| `content`, `identity`, `metadata` | bool | what changed: bytes, inode serial, attributes | same |
| `hunks` | list or null | line hunks of a text file whose content changed; null otherwise or with `paths_only` | same |
| `hunks_omitted` | bool | hunks did not fit the reply budget | same |
| `next_cursor` | text or null | the last listed path | same |

A directory whose only change is in its children is not listed.

## Where it runs

```text
 caller process                       Store's machine worker / this process
 resolve::target; lease::claim
 store_call branch.diff_branches -->  snapshot::resolve from, to
                                      walk both trees in path order
                                      per change: facts, hunks in budget
```

A read on the read-only session; no lane. See the
[routing table](../../../control/routing/routing-table.md). Refused under a folder
scope (`out of scope: history`).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a side is not a Branch or `main`; bad paging; a folder-scoped client | no | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NoSuchBranch` | a side is missing | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchStore`, `NoSuchProject`, `Busy`, `Unresolved`, `Unavailable` | as for every Store call | no | [`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const d = await layerfs.branch.diff("shop/api", "main", "agent", { paths_only: true });
d.items.forEach((i) => console.log(i.kind, i.path));
```

```python
page = layerfs.branch.diff("shop/api", "main", "agent")
while page["has_more"]:
    page = layerfs.branch.diff("shop/api", "main", "agent", cursor=page["next_cursor"])
```

```bash
layerfs branch diff shop/api main agent --paths-only
```

## Related

- Before: [branch.list](list.md). After: [branch.merge](merge.md),
  [branch.publish](publish.md), [file.diff](../file/diff.md) for one file.
- [Engine: reads, compare](../../../engine/store-calls/reads.md#compare),
  [Core: compare](../../../core/compare-and-merge/compare.md).

---

← [branch.publish](publish.md) · [Up: branch](README.md) · [SDK: commit](../commit/README.md) →

## Pinned saved endpoints and attribution

Optional `from_commit` and `to_commit` select full Commit or Layer IDs through
those branch contexts. All Commit/Layer combinations work; the response `from`
and `to` are full typed references `{id,kind,branch,root}` in the extended query
mode. Pin their IDs for subsequent file reads/comparisons and result pages.

`page` selects one-based matching-result pages with `page_size`. New endpoint,
`path_prefix` or `include_attribution` options also select this mode; legacy
`page_pos`/`cursor` cannot be mixed into it. Old branch-only calls keep their
legacy response and paging behavior. `path_prefix` matches full path components.

`include_attribution:true` discovers equal-content files whose aligned unchanged
lines have different known owners. Full namespace traversal avoids equal-root
pruning in this opt-in mode. Independent `attribution`, `attribution_coverage`
and `changed_owner_lines` accompany content/metadata/identity flags. Unknown
provenance remains null and is included as an unresolved candidate, not silently
classified unchanged. Returned-page `coverage.complete` discloses unknown path owners;
`scan.complete` concerns completion of the mandatory matching-result page.
Publication alone retains source Commit ownership.

Mandatory path summaries and seeking are bounded. The query refuses `Capacity`
when the requested page cannot complete within work/reply budgets; no paths are
silently skipped by page advancement. Text hunks which do not fit the path
summary are explicitly omitted; use `file.diff` at the same endpoints for the
bounded content/attribution view. Folder-scope refusal remains unchanged.
See [shared saved-query contract](../saved-queries.md).

Sequential numbered comparisons cache at most 128 weak-Store-keyed path
boundaries, qualified by both saved IDs, path prefix, attribution/hunk options and
page size. Namespace walks resume with indexed name boundaries instead of
replaying all earlier names. Cache eviction changes cost, never the matching
result definition. Request-level content comparison and attribution payload
admission are each bounded to 64 MiB; exhaustion refuses the mandatory page.
The cumulative provenance work budget is shared across paths. Inherent binary,
coarse or unavailable ownership remains explicitly unknown; query-budget
exhaustion never changes membership into a successful sparse result page.
