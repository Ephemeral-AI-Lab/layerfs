# branch.publish

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [branch](README.md) › publish

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Makes a Branch's head (or one of its Commits) `main`'s next Layer. It
overwrites: Layers added to `main` since the Branch was based are reported,
not merged.

| | |
| --- | --- |
| Family | [branch](README.md) |
| Library entry | [`api/crates/layerfs-api/src/branch/publish.rs`](../../../../../api/crates/layerfs-api/src/branch/publish.rs) |
| Implemented in | `layerfs_control::store::calls::run` → Store method `branch.publish` ([`methods/branch/publish.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/branch/publish.rs), [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) `publish`) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.branch.publish(project, branch, commit?);
```

```python
# Python
layerfs.branch.publish(project, branch, commit=None)
```

```bash
# CLI
layerfs branch publish <store>/<project> <branch> [<commit>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>` | [`store/resolve.rs`](../../../../../control/crates/layerfs-control/src/store/resolve.rs) |
| `branch` | Name | a Branch; `main` is `NoSuchBranch` | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) `branch` |
| `commit` | Name, optional | a 66-hex Commit id (a Layer id is `InvalidInput`); default the Branch's head | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |

## Options

None.

## Returns

```text
 Added:     { kind: "Added", source: { branch, commit },
              published_layer: { ...Layer, overwrote_count },
              based_on: <Branch base Layer>,
              overwrote_count, overwrote: [ <Layer> x <= 50 ],
              overwrote_has_more }
 UpToDate:  { kind: "UpToDate", layer }
 NoChanges: { kind: "NoChanges", head_layer }
```

| `kind` | Meaning | Source |
| --- | --- | --- |
| `Added` | a new head Layer of `main`; `overwrote` lists the first 50 Layers it skipped | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `UpToDate` | this Commit is already published as `layer` | same |
| `NoChanges` | the Commit's tree equals `main`'s head; nothing added | same |

## Where it runs

```text
 caller process                       Store's machine worker / this process
 resolve::target; lease::claim
 store_call branch.publish -------->  lookup Branch (never main)
                                      commit or head (none --> NoCommit)
                                      lane: add_layer { expected stack
                                            head, expected Branch base }
                                            moved --> HeadMoved (no retry)
                                      Added: read first 50 overwritten
```

The expected heads make the publish a compare-and-set. See the
[routing table](../../../control/routing/routing-table.md). Under a folder scope
`branch` is composed as `<ns>.<branch>` and `commit` is checked like any
Commit or Branch reference (a 66-character hex id stays as given; any other
value is treated as a Branch name and composed into the namespace, and the
Store decides whether that Branch exists; a name that cannot be composed, or
a scope without `branch`, is refused before any effect); see
[Scopes](../../call-table/table.md#scopes) for the ids that stay unconfined.

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a Layer id as `commit`; bad names; out of scope | no | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `NoSuchBranch`, `NoSuchCommit` | missing, including `main` as `branch` | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoCommit` | the Branch has no saved Commit | no | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `HeadMoved` | `main`'s head or the Branch's base moved since they were read | no | same |
| `NotInHistory` | the Commit is not on the Branch's history | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchStore`, `NoSuchProject`, `Busy`, `Unresolved`, `Unavailable` | as for every Store call | no | [`store/call.rs`](../../../../../control/crates/layerfs-control/src/store/call.rs) |
| `Unknown` | **the worker's answer was lost, or the add's outcome is unknown** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

**After `Unknown`**, read [branch.get](get.md) for `main`: its `base` is the
head Layer. Publishing the same Commit again answers `UpToDate` if it
landed.

## Examples

Not run; written from the call table.

```js
const r = await layerfs.branch.publish("shop/api", "agent");
if (r.kind === "Added" && r.overwrote_count > 0) console.warn("overwrote", r.overwrote);
```

```python
layerfs.branch.publish("shop/api", "agent")
```

```bash
layerfs branch publish shop/api agent
```

## Related

- Before: [branch.diff](diff.md) against `main`, [branch.merge](merge.md)
  with `main` to keep its Layers. After: [branch.fork](fork.md) from `main`.
- [Engine: branches, publish](../../../engine/store-calls/branches.md#publish),
  [Store: History publication](../../../engine/store/history-publication.md).

---

← [branch.merge](merge.md) · [Up: branch](README.md) · [branch.diff](diff.md) →
