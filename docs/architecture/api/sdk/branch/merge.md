# branch.merge

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [branch](README.md) › merge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Merges two Branches (either may be `main`) into a new Branch, leaving both
sources unchanged. A conflict creates nothing and reports the conflicting
paths.

| | |
| --- | --- |
| Family | [branch](README.md) |
| Library entry | [`api/crates/layerfs-api/src/branch/merge.rs`](../../../../../api/crates/layerfs-api/src/branch/merge.rs) |
| Implemented in | `layerfs_store::calls::branch_merge` → Store method `branch.merge` ([`methods/branch/merge.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/branch/merge.rs), [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) `merge`) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.branch.merge(project, name, a, b, { message, units });
```

```python
# Python
layerfs.branch.merge(project, name, a, b, message=None, units=None)
```

```bash
# CLI
layerfs branch merge <store>/<project> <name> <a> <b> [--message TEXT] [--units JSON]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>` | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `name` | Name | the new Branch; not `main`, not taken | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `a` | Name | the left side ("ours"), sent as `branch`; may be `main` | [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs) |
| `b` | Name | the right side ("theirs"), sent as `with`; may be `main`; not equal to `a` | same |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `message` | Text | none | the merge Commit's message | [`calls/table.rs`](../../../../../api/crates/layerfs-api/src/calls/table.rs) `MESSAGE` |
| `units` | Json | none | a list; each a glob pattern or `{ name, patterns: [..] }`. A unit both sides changed comes whole from the later write (`a` on a tie); overlapping units belong to the first listed | [`history/units.rs`](../../../../../engine/crates/layerfs-store-service/src/history/units.rs) |

## Returns

```text
 { kind: "Merged", fast_forward, branch, head, root,
   sources: [ { ref, node }, { ref, node } ], merge_base,
   conflicts: [], conflicts_truncated: false,
   lww: [ { path, rule, ... } ], lww_truncated,
   units: [ ...decisions ], units_truncated }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `fast_forward` | bool | one side was an ancestor of the other: the new Branch points at the newer side, no Save | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `branch`, `head`, `root` | text | the new Branch, its head Commit and root | same |
| `sources` | list | each side's name (`ref`) and node | same |
| `merge_base` | text | the common ancestor node | same |
| `lww` | list | paths settled by `/.lww` last-writer-wins rules (taken from `a`) | same; [Core: merge](../../../core/compare-and-merge/merge.md) |
| `units` | list | each unit's decision | same |
| `*_truncated` | bool | the list was cut to its byte window | [`sdk_values.rs`](../../../../../engine/crates/layerfs-bridge/src/sdk_values.rs) |

## Where it runs

```text
 caller process                       Store's machine worker / this process
 resolve::target; lease::claim
 store_call branch.merge ---------->  a == b? name main/taken? units valid?
                                      ancestor? --> merge_branch(newer)  [FF]
                                      merge_base  (none --> NoMergeBase)
                                      Save: units decide, merge_roots
                                      not clean --> drop Save --> Conflict
                                      save.finish, size, bindings
                                      lane: catalog.merge_branch  (publish)
```

Only the final `merge_branch` holds the publication lane; the merge work
happens before it. The merge Commit's base Layer is `main`'s when either
side is `main`, else `a`'s base. See the
[routing table](../../../control/routing/routing-table.md). Under a folder scope
`name`, `a` and `b` are composed as `<ns>.<x>`.

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | `a == b`; bad names or `units`; out of scope | no | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `ReservedName`, `NameTaken` | `name` is `main`, or exists | no | same |
| `NoSuchBranch` | a side is missing | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoMergeBase` | the sides share no history | no | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `Conflict` | conflicting paths; nothing created; `detail` has `sources`, `merge_base`, `conflicts`, `lww`, `units` and the truncation flags | no | same |
| `HeadMoved` | the catalog moved under the publish | no | same, [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchStore`, `NoSuchProject`, `Busy`, `Unresolved`, `Unavailable` | as for every Store call | no | [`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs) |
| `Failed` | a Store without merge scratch (`BackendUnavailable`); other failures | no | [`store/open.rs`](../../../../../engine/crates/layerfs-store-service/src/store/open.rs) |
| `Unknown` | **the worker's answer was lost, or the publish outcome is unknown** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

**After `Unknown`**, read [branch.get](get.md) for `name` before merging
again.

## Examples

Not run; written from the call table.

```js
try {
  await layerfs.branch.merge("shop/api", "joined", "agent", "main", { message: "join" });
} catch (e) { if (e.code === "Conflict") console.log(e.detail.conflicts); }
```

```python
layerfs.branch.merge("shop/api", "joined", "agent", "review",
                     units=[{"name": "lock", "patterns": ["package-lock.json"]}])
```

```bash
layerfs branch merge shop/api joined agent review --message join
```

## Related

- Before: [branch.diff](diff.md). After: [branch.publish](publish.md) of
  the merged Branch.
- [Fork and merge flow](../../../flows/merge.md),
  [Engine: branches](../../../engine/store-calls/branches.md#merge),
  [Core: Compare and merge](../../../core/compare-and-merge/README.md).

---

← [branch.fork](fork.md) · [Up: branch](README.md) · [branch.publish](publish.md) →
