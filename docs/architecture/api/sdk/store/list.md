# store.list

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › list

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Lists every Store of the home, sqlite and PostgreSQL, each with its record,
its projects and its current state.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`api/crates/layerfs-api/src/store/list.rs`](../../../../../api/crates/layerfs-api/src/store/list.rs) |
| Implemented in | `layerfs_control::store::family::run` → `list` ([`store/list.rs`](../../../../../api/crates/layerfs-control/src/store/list.rs)) |
| Runs in | the caller's process; one `store.file` `stat` in a machine's worker per sqlite Store that is not local |

## Signature

```js
// JavaScript
await layerfs.store.list();
await layerfs.client({ home: "/data/lfs" }).store.list();
```

```python
# Python
layerfs.store.list()
layerfs.client(home="/data/lfs").store.list()
```

```bash
# CLI
layerfs store list
```

## Arguments

None. A positional is refused `InvalidInput` by the argument check
([`calls/check.rs`](../../../../../api/crates/layerfs-control/src/calls/check.rs)).

## Options

None.

## Returns

```text
{ items: [ item, ... ] }       one item per folder under store/ with a record

 item (sqlite)                     item (PostgreSQL)
 +------------------------------+  +------------------------------+
 | name                         |  | name                         |
 | kind: "sqlite"               |  | kind: "postgresql"           |
 | machine: "native" | <name>   |  | url        (no password)     |
 | profile: "disposable"        |  | schema: "layerfs"            |
 | created, sqlite              |  | synchronous_commit, created  |
 | last_transfer?  backups?     |  | password?  (a reference)     |
 | op?                          |  | op?                          |
 | projects: [name]             |  | projects: [name]             |
 | state                        |  | state                        |
 +------------------------------+  +------------------------------+
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| record fields | map | the Store record as written, `name` first; `cursor`, `binding` and `v` are never shown | [`record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) `to_sdk` |
| `op` | map | the recorded intent as `{ id, name, phase, started, uncertain }`, only when the record holds one | [`common/op.rs`](../../../../../api/crates/layerfs-control/src/common/op.rs) `to_sdk` |
| `projects` | list of text | the names of the home's project records under `store/<name>/project/`, sorted | [`family.rs`](../../../../../api/crates/layerfs-control/src/store/family.rs) `public` |
| `state` | text | `idle`, `in_use`, `busy`, `down` or `unresolved` (below) | [`record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) `StoreRecord::state` |

`get` shows the same record fields; [store.get](get.md) describes them.
There are no paging fields.

| `state` | When (first match wins) |
| --- | --- |
| `busy` | the record's `op` belongs to a live process |
| `unresolved` | the record's `op` is uncertain and its process is gone |
| `down` | a sqlite Store whose machine did not answer `stat`, or whose kind or machine the record does not tell |
| `idle` | no other live process holds its lease |
| `in_use` | another live process holds its lease |

## Where it runs

```text
 caller process                                    machine worker
 --------------                                    --------------
 inventory::stores: read store/*/record.json
 lease::read_holders (no lock; stale leases kept)
 for each Store:
   PostgreSQL ---------> not contacted; never down
   sqlite on a local machine --> steps::run "stat"
   sqlite elsewhere ------------ store.file stat ---> steps::run "stat"
       a machine that failed once is down for its other Stores too
```

| Store kind | Where | What it reads | Source |
| --- | --- | --- | --- |
| sqlite on `native` (Linux, macOS) | this process | the file's size, `-wal` size and free bytes | [`list.rs`](../../../../../api/crates/layerfs-control/src/store/list.rs), [`transfer/steps.rs`](../../../../../api/crates/layerfs-control/src/store/transfer/steps.rs) `stat` |
| sqlite on another machine | that machine's worker | the same, under the worker's root | [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) `plan` |
| PostgreSQL | this process | records only; the server is not dialed | [`list.rs`](../../../../../api/crates/layerfs-control/src/store/list.rs) |

It starts no machine, takes no lock and changes nothing. See the
[routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | the client has a folder scope (`out of scope: store.list`) | no | [`calls/scope.rs`](../../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| `Failed` | a Store record or lease folder cannot be read, or a record is not a map | no | [`common/home/records.rs`](../../../../../api/crates/layerfs-control/src/common/home/records.rs), [`record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) |

A machine that does not answer is not an error: its Stores are `down`.

## Examples

Not run; written from the call table.

```js
const { items } = await layerfs.store.list();
for (const s of items) console.log(s.name, s.kind, s.state);
```

```python
for s in layerfs.store.list()["items"]:
    print(s["name"], s["kind"], s["state"])
```

```bash
layerfs store list
```

## Related

- Next: [store.status](status.md) for one Store's holders, `op` and size;
  [store.get](get.md) for its record alone.
- [Control: Home and records](../../control/home-and-records.md),
  [Routing table](../../routing/routing-table.md),
  [Engine: Store](../../../engine/store/README.md).

---

← [SDK: store](README.md) · [Up: store](README.md) · [store.get](get.md) →
