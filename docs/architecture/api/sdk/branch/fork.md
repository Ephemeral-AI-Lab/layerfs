# branch.fork

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [branch](README.md) › fork

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Makes a new Branch that starts at another Branch's head, at `main`, or at
a given Commit. It writes metadata only and copies no files.

| | |
| --- | --- |
| Family | [branch](README.md) |
| Library entry | [`api/crates/layerfs-api/src/branch/fork.rs`](../../../../../api/crates/layerfs-api/src/branch/fork.rs) |
| Implemented in | `layerfs_control::store::calls::run` → Store method `branch.fork` ([`methods/branch/fork.rs`](../../../../../engine/crates/layerfs-store-service/src/methods/branch/fork.rs), [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) `fork`) |
| Runs in | the caller's process for PostgreSQL and sqlite on `native`; else the Store's machine worker |

## Signature

```js
// JavaScript
await layerfs.branch.fork(project, name, from_branch, from_commit?);
```

```python
# Python
layerfs.branch.fork(project, name, from_branch, from_commit=None)
```

```bash
# CLI
layerfs branch fork <store>/<project> <name> <from_branch> [<from_commit>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `project` | Project | `<store>/<project>` | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `name` | Name | a new Branch name; not `main`, not taken | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `from_branch` | Name | the source Branch, or `main` | same |
| `from_commit` | Name, optional | a 66-hex Commit or Layer id; when given it replaces `from_branch` as the source | [`calls.rs`](../../../../../control/crates/layerfs-store/src/calls.rs) |

Control sends one `from`: `from_commit` if given, else `from_branch`.

| `from` | The new Branch starts at |
| --- | --- |
| `main` (or absent) | `main`'s head Layer, as its base, with no head |
| a Branch name | that Branch's head Commit, or its base Layer when it has none |
| a Layer id | that Layer |
| a Commit id | that Commit, on the Branch its metadata names (none: `InvalidInput`) |

## Options

None.

## Returns

```text
 { kind: "Forked", branch: <name>, base: <Layer id>, head: <Commit id> | null }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `kind` | text | `Forked` | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `branch` | text | the new Branch | same |
| `base`, `head` | text, text or null | where it starts | same |

The Branch id is derived from the project and the name
(`BranchId::named`).

## Where it runs

```text
 caller process                                   Store's machine worker
 resolve::target; lease::claim
 store_call branch.fork { name, from } ------->   lookups (read session)
                                                  catalog.fork: one Branch
                                                  row, the publish write
```

One short write through the Store's single writer; no tree is read. See
the [routing table](../../../control/routing/routing-table.md) and
[Store: History publication](../../../engine/store/history-publication.md).
Under a folder scope `name` and `from_branch` are composed as
`<ns>.<name>`, and `from_commit` (which the Store reads before
`from_branch`) is checked like any Commit or Branch reference: a 66-character hex id
stays as given; any other value is treated as a Branch name and composed
into the namespace (the Store then decides whether that Branch exists); a
name that cannot be composed, or a scope without `branch`, is refused
before any effect. An id of a Commit outside the
namespace is therefore not confined (an open gap,
[Scopes](../../call-table/table.md#scopes)).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a Commit with no Branch in its metadata; out of scope | no | [`history/branch.rs`](../../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `ReservedName` | `name` is `main` | no | same |
| `NameTaken` | a Branch of that name exists | no | same |
| `NoSuchStore`, `NoSuchProject` | not recorded | no | [`resolve.rs`](../../../../../control/crates/layerfs-store/src/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit`, `NotFound` | the source Branch, Commit or Layer is missing | no | [`history/lookup.rs`](../../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Busy`, `Unresolved` | a push, pull or remove holds the Store | no | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs) |
| `Unavailable` | the worker or server does not answer before the call is sent | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Unknown` | **the worker's answer was lost, or the Store reports an unknown outcome** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

**After `Unknown`**, read [branch.get](get.md) for `name`: it exists if the
fork published. Do not fork again blindly; a second fork is `NameTaken`.

## Examples

Not run; written from the call table.

```js
await layerfs.branch.fork("shop/api", "agent", "main");
```

```python
layerfs.branch.fork("shop/api", "retry", "agent", "0a1b…")  # a 66-hex Commit id
```

```bash
layerfs branch fork shop/api review agent
```

## Related

- After: [workspace.mount](../workspace/mount.md) on the new Branch,
  [file.write](../file/write.md), [branch.merge](merge.md).
- [Fork and merge flow](../../../flows/merge.md),
  [Engine: Store calls, branches](../../../engine/store-calls/branches.md#fork).

---

← [branch.get](get.md) · [Up: branch](README.md) · [branch.merge](merge.md) →
