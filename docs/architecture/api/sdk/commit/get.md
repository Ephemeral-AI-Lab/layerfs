# commit.get

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [commit](README.md) › get

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Shows one Commit of a Branch, found by its id or an id prefix of at least
eight characters.

| | |
| --- | --- |
| Family | [commit](README.md) |
| Library entry | [`api/crates/layerfs-api/src/commit/get.rs`](../../../../../api/crates/layerfs-api/src/commit/get.rs) |
| Implemented in | `layerfs_store::calls::commit_get` → `find_commit`, which pages Store method `branch.list_commits` ([`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs)) |
| Runs in | control's loop in the caller's process; each page where the Store's calls run |

## Signature

```js
// JavaScript
await layerfs.commit.get(project, branch, commit);
```

```python
# Python
layerfs.commit.get(project, branch, commit)
```

```bash
# CLI
layerfs commit get <store>/<project> <branch> <commit>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>` | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `branch` | Name | the Branch whose history holds the Commit; not `main` | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `commit` | Name | a full id, or a prefix of 8 or more characters | [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs) `find_commit` |

## Options

None.

## Returns

One `commit` value, the same shape as an item of
[commit.list](list.md#returns).

## Where it runs

```text
 caller process (control)                     Store (here or worker)
 pos = 0
 loop:
   store_call branch.list_commits ----------> one page, 1000 Commits
     { branch, pageSize: 1000, pagePos: pos }
   id == commit, or prefix (>= 8)? --> answer it (first match, newest first)
   next_page_pos > pos             --> pos = next
   next_page_pos <= pos            --> Failed (no progress)
   no next page                    --> NoSuchCommit
```

Each page is one Store call with its own lease claim; the walk grows with
the Branch's history. A prefix matching several Commits answers the newest.
Refused under a folder scope. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad project; a folder-scoped client | no | [`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NoSuchBranch` | no such Branch, or `main` | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchCommit` | not on this Branch's history | no | [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs) |
| `Failed` | a page made no progress | no | same, `page_of` |
| `NoSuchStore`, `NoSuchProject`, `Busy`, `Unresolved`, `Unavailable` | as for every Store call | no | [`call.rs`](../../../../../control/crates/layerfs-store/src/call.rs) |
| `Unknown` | a worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Examples

Not run; written from the call table.

```js
const c = await layerfs.commit.get("shop/api", "agent", "0a1b2c3d");
```

```python
print(layerfs.commit.get("shop/api", "agent", "0a1b2c3d")["message"])
```

```bash
layerfs commit get shop/api agent 0a1b2c3d
```

## Related

- Before: [commit.list](list.md). After: [commit.diff](diff.md),
  [file.read](../file/read.md) with `commit`.
- [Engine: Store calls](../../../engine/store-calls/README.md).

---

← [commit.list](list.md) · [Up: commit](README.md) · [commit.diff](diff.md) →
