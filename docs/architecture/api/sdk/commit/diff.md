# commit.diff

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [commit](README.md) › diff

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists the files that differ between two saved Commits of a project, with
line hunks unless asked for paths only.

| | |
| --- | --- |
| Family | [commit](README.md) |
| Library entry | [`api/crates/layerfs-api/src/commit/diff.rs`](../../../../../api/crates/layerfs-api/src/commit/diff.rs) |
| Implemented in | `layerfs_store::calls::commit_diff` → Store method `branch.diff_commits` ([`methods/branch/diff_commits.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/branch/diff_commits.rs), [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs)) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.commit.diff(project, from_commit, to_commit, { paths_only, page_size, page_pos, cursor });
```

```python
# Python
layerfs.commit.diff(project, from_commit, to_commit, paths_only=None,
                    page_size=None, page_pos=None, cursor=None)
```

```bash
# CLI
layerfs commit diff <store>/<project> <from_commit> <to_commit> \
  [--paths-only] [--page-size N] [--page-pos N] [--cursor PATH]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>` | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `from_commit`, `to_commit` | Name | each must be a full saved Commit or Layer id of the project (Branch names are `InvalidInput`); sent as `from`, `to` | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) `diff_commits` |

The two Commits may be on different Branches of the same project.

## Options

The `DIFF` set, as on [branch.diff](../branch/diff.md#options):
`paths_only` (Bool, sent as `hunks = !paths_only`), `page_size` (Int,
1..=1000, default 50), `page_pos` (Int) and `cursor` (Text, the last
listed path).

## Returns

```text
 { kind: "ok",
   from: { commit, root }, to: { commit, root },
   items: [ { path, kind, type, before, after, content, identity,
              metadata, hunks, hunks_omitted } ],
   has_more, next_page_pos, next_cursor }
```

The item fields are described on [branch.diff](../branch/diff.md#returns);
only the sides differ (`commit` and `root` instead of `ref`, `label` and
`root`).

## Where it runs

```text
 caller process                       Store's machine worker / this process
 resolve::target; lease::claim
 store_call branch.diff_commits --->  snapshot::resolve from, to
                                      both Commits? else InvalidInput
                                      walk both trees; hunks in budget
```

A read; no lane. Refused under a folder scope (`out of scope: history`).
See the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a side is not a saved Commit; bad paging; a folder-scoped client | no | [`read/compare.rs`](../../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| `NoSuchCommit` | a Commit is missing | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchStore`, `NoSuchProject`, `Busy`, `Unresolved`, `Unavailable` | as for every Store call | no | [`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const [newest, older] = (await layerfs.commit.list("shop/api", "agent", { page_size: 2 })).items;
const d = await layerfs.commit.diff("shop/api", older.id, newest.id);
```

```python
layerfs.commit.diff("shop/api", older_id, newer_id, paths_only=True)
```

```bash
layerfs commit diff shop/api <older-id> <newer-id> --paths-only
```

## Related

- Before: [commit.list](list.md). Compare Branch heads with
  [branch.diff](../branch/diff.md); one file with [file.diff](../file/diff.md).
- [Engine: reads, compare](../../../engine/store-calls/reads.md#compare).

---

← [commit.get](get.md) · [Up: commit](README.md) · [SDK: file](../file/README.md) →

## Layer endpoints

`commit diff` accepts Layer↔Layer and Commit↔Layer endpoints as well as
Commit↔Commit. The existing changed-inode/binding plan prunes unchanged
namespace state when its hints qualify; fallback and Capacity semantics are
unchanged. Use it for main-publication comparisons and first Commit versus
its base Layer. The legacy side field `commit` holds the supplied saved id
(including a typed Layer id); `root` remains the immutable filesystem root.
The existing `next_cursor` path resumes this saved comparison.
