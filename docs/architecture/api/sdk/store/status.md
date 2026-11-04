# store.status

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › status

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Shows one Store's live state: who holds it, the operation on it, and its
file (sqlite) or its server (PostgreSQL), changing nothing.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`api/crates/layerfs-api/src/store/status.rs`](../../../../../api/crates/layerfs-api/src/store/status.rs) |
| Implemented in | `layerfs_control::store::family::run` → `status` ([`store/status.rs`](../../../../../api/crates/layerfs-control/src/store/status.rs)) |
| Runs in | the caller's process; a `store.file` `stat` in the machine's worker for a sqlite Store that is not local; one connection to the server for PostgreSQL |

## Signature

```js
// JavaScript
await layerfs.store.status(name);
await client.store.status(name);
```

```python
# Python
layerfs.store.status(name)
client.store.status(name)
```

```bash
# CLI
layerfs store status <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a Store name (`StoreName::parse`) | [`common/names.rs`](../../../../../api/crates/layerfs-control/src/common/names.rs) |

## Options

None.

## Returns

```text
 { name, kind, state, holders: [pid], op, down_reason, ...by kind }

 sqlite:      machine: <name>
              file: { exists, free, bytes, wal_bytes }
                  | { unavailable: <message> }

 PostgreSQL:  server: { reachable: true, server_version, latency_ms,
                        latency_us, schemas: { layerfs: <bytes> | null } }
                    | { reachable: false,
                        error: { code: "Unavailable" | "Denied", message } }
                    | { unavailable: <message> }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `name`, `kind` | text | the Store and `sqlite` or `postgresql` | [`status.rs`](../../../../../api/crates/layerfs-control/src/store/status.rs) |
| `state` | text | `idle`, `in_use`, `busy`, `down` or `unresolved`, as on [store.list](list.md#returns) | [`record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) `state` |
| `holders` | list of int | pids of live processes other than this one that hold its lease; stale leases are ignored, not removed | [`common/home/lease.rs`](../../../../../api/crates/layerfs-control/src/common/home/lease.rs) `read_holders` |
| `op` | map or null | the recorded intent `{ id, name, phase, started, uncertain }` | [`common/op.rs`](../../../../../api/crates/layerfs-control/src/common/op.rs) `to_sdk` |
| `down_reason` | text or null | why the machine did not answer `stat` (sqlite only) | [`status.rs`](../../../../../api/crates/layerfs-control/src/store/status.rs) |
| `machine` | text | sqlite: the machine that holds the file | same |
| `file` | map | sqlite: whether the file exists, free bytes on its filesystem, its bytes and its `-wal` bytes | [`transfer/steps.rs`](../../../../../api/crates/layerfs-control/src/store/transfer/steps.rs) `stat` |
| `server` | map | PostgreSQL: one connection's result; `schemas` maps `layerfs` to its size, null when the schema is missing | [`store/postgres.rs`](../../../../../api/crates/layerfs-control/src/store/postgres.rs) `status`, [`connect.rs`](../../../../../api/crates/layerfs-control/src/store/connect.rs) `status` |

Result keys are snake_case in every front end, so `serverVersion` arrives
as `server_version` ([`calls/keys.rs`](../../../../../api/crates/layerfs-control/src/calls/keys.rs)).
A PostgreSQL Store is never `down`: an unreachable server shows in `server`.

## Where it runs

```text
 caller process                               worker / server
 --------------                               ---------------
 record_of(name); lease::read_holders (no lock)
 sqlite:
   place::plan --> Local:  steps::run "stat"
               --> Worker: store.file stat ------> stat under its root
   failure (machine stopped, worker gone) --> file.unavailable, state down
 PostgreSQL:
   secrets::url(record)  (password reference resolved here)
   postgres::status ------------------------------> one connection
   any refusal --> server.unavailable
```

| Store kind | Where the work happens | Source |
| --- | --- | --- |
| sqlite on a local machine (`native` on Linux and macOS) | this process | [`common/place.rs`](../../../../../api/crates/layerfs-control/src/common/place.rs) |
| sqlite elsewhere | the machine's worker, if it answers; nothing is started | same, `Worker::call` |
| PostgreSQL | this process dials the server; a build without `postgresql` shows `server.unavailable` | [`store/postgres.rs`](../../../../../api/crates/layerfs-control/src/store/postgres.rs) |

See the [routing table](../../routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a record whose `kind` is not `sqlite` or `postgresql`; a folder-scoped client | no | [`record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-control/src/calls/scope.rs) |
| `NotFound` | no Store of that name | no | [`record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) |
| `Unavailable` | a 1.1.0 Store whose machine is not known (`layerfs import-env` fixes it) | no | [`record.rs`](../../../../../api/crates/layerfs-control/src/store/record.rs) `StoreRef::machine` |
| `Failed` | the record or lease folder cannot be read | no | [`common/home/records.rs`](../../../../../api/crates/layerfs-control/src/common/home/records.rs) |

An unreachable machine or server is reported in the answer, not as an
error. `status` is the call an `Unresolved` refusal names: read `op` here
before clearing it (a new push or pull for a transfer, else
[store.remove](remove.md)).

## Examples

Not run; written from the call table.

```js
const s = await layerfs.store.status("shop");
if (s.state === "unresolved") console.log(s.op);
```

```python
s = layerfs.store.status("shop")
print(s["state"], s["holders"], s.get("file") or s.get("server"))
```

```bash
layerfs store status shop
```

## Related

- Before: [store.list](list.md). After an `Unknown` or `Unresolved`:
  [store.push](push.md), [store.pull](pull.md) or [store.remove](remove.md).
- [Errors: uncertain outcomes](../../call-table/errors.md#uncertain-outcomes),
  [Engine: Store](../../../engine/store/README.md),
  [Engine: PostgreSQL](../../../engine/store/postgresql.md).

---

← [store.get](get.md) · [Up: store](README.md) · [store.create](create.md) →
