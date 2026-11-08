# store.pull

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › pull

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Brings a sqlite Store's file from the machine that holds it back to
`native`, checked byte for byte: [store.push](push.md) with the sides
swapped.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`store/pull.rs`](../../../../../api/crates/layerfs-api/src/store/pull.rs) |
| Implemented in | `layerfs_control::store::pull` → `transfer::phases::pull` ([`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs)) |
| Runs in | the caller's process (records, locks, `op`, the bytes in transit); the source steps in the source machine's worker as `store.file`, the target steps here on `native` |

## Signature

```js
// JavaScript
await layerfs.store.pull(name, { on_progress });
await client.store.pull(name, { on_progress });
```

```python
# Python
layerfs.store.pull(name, on_progress=None)
client.store.pull(name, on_progress=None)
```

```bash
# CLI
layerfs store pull <name> [--progress]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a sqlite Store on a machine other than `native` | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs), [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_pull` |

There is no target argument: a pull always ends on `native`. To move a
Store between two other machines, pull it, then push it.

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `on_progress` | Function | none | called with `{ phase, done, total }`; bytes during `send`, null otherwise; CLI `--progress` | [`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs) `progress` |

## Returns

```text
 { store, from: <machine>, to: "native", bytes, heads }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `store` | text | the Store | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) `phases` |
| `from`, `to` | text | the machine the record named, and `native` | same |
| `bytes` | int | the snapshot's size | same |
| `heads` | text | hex digest of the history heads, checked on `native` | [`sqlite/transfer/steps.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) `digest` |

## Where it runs

```text
 source worker                    caller process (native side)
 -------------                    ----------------------------
                                  check: clear a leftover copy, can_pull,
 stat <------------------------------- idle, stat both, free disk on native
                                  begin (locks, op), settle (lease)
 snapshot <--------------------------- snapshot: sha256, heads
 read 8 MiB pieces --------------> write <store>.incoming under the home
                                  verify, op marked uncertain, install
                                  record: machine = native, last_transfer
 remove the Store's folder <---------- retire, drop op
```

The phases, what each writes to `op` and which failures are uncertain are
the same as a push; see [store.push](push.md#where-it-runs). The source
is retired with the worker step `remove` (its whole Store folder under the
worker's root), not `retire`. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

Every refusal's message starts `store pull, <phase>:` and carries
`detail.phase`.

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad name; a PostgreSQL Store; the Store is already on `native`; a folder-scoped client | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_pull` |
| `NotFound` | no such Store; no file on the source at snapshot | no | [`record.rs`](../../../../../control/crates/layerfs-store/src/record.rs), [`sqlite/transfer/steps.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| `InUse` | a live process holds it, or a Sandbox of one of its projects is recorded | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle` |
| `Busy` / `Unresolved` | another operation runs / an earlier one's outcome is unknown | no | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Exists` | `native` already holds a Store file of that name | no | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) `reach` |
| `Capacity` | `native` lacks bytes plus headroom (`detail.needed`, `detail.free`) | no | same |
| `Unavailable` | the source machine is stopped or its worker gone; nothing is started | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Conflict` | the Store moved between check and `op` | no | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `Failed` | a `-wal` left on the source; a digest mismatch; I/O | no, unless at `record` | [`sqlite/transfer/steps.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| `Unknown` | **the worker's answer was lost, or any failure of `record`** | **yes** | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) `transfer` |

**After an uncertain pull**, read [store.status](status.md), then run
`store pull` again: it deletes the copy the record does not name first
([`sqlite/transfer/resolve.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs)).
A `Failed` snapshot on a `-wal` means a process ended with the Store open
on its machine; open it there once (any Store call) and pull again.

## Examples

Not run; written from the call table.

```js
const moved = await layerfs.store.pull("shop");
console.log(`${moved.bytes} bytes from ${moved.from}`);
```

```python
layerfs.store.pull("shop", on_progress=print)
```

```bash
layerfs store pull shop --progress
```

## Related

- Before: [store.status](status.md), [machine.start](../machine/start.md)
  for the source. After: [store.push](push.md) to another machine, or
  [backup.create](../backup/create.md).
- [Control: Home and records](../../../control/management/home-and-records.md),
  [Errors: uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

---

← [store.push](push.md) · [Up: store](README.md) · [store.remove](remove.md) →
