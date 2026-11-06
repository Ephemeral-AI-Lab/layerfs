# store.push

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [store](README.md) › push

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Moves a sqlite Store's file from `native` to another machine, checked byte
for byte, so its projects are served from that machine afterwards.

| | |
| --- | --- |
| Family | [store](README.md) |
| Library entry | [`store/push.rs`](../../../../../api/crates/layerfs-api/src/store/push.rs) |
| Implemented in | `layerfs_control::store::push` → `transfer::phases::push` ([`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs)) |
| Runs in | the caller's process (records, locks, `op`, the bytes in transit); the source steps run here on `native`, the target steps in the target machine's worker as `store.file` |

## Signature

```js
// JavaScript
await layerfs.store.push(name, machine, { on_progress });
await client.store.push(name, machine, { on_progress });
```

```python
# Python
layerfs.store.push(name, machine, on_progress=None)
client.store.push(name, machine, on_progress=None)
```

```bash
# CLI
layerfs store push <name> <machine> [--progress]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a sqlite Store on `native` (`StoreName::parse`) | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs), [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_push` |
| `machine` | Name | a recorded machine other than `native` (`MachineName::parse`) | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) `push` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `on_progress` | Function | none | called with `{ phase, done, total }`; `done` and `total` are bytes during `send`, null otherwise; the CLI prints them with `--progress` | [`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs) `progress` |

## Returns

```text
 { store, from: "native", to: <machine>, bytes, heads }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `store` | text | the Store | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) `phases` |
| `from`, `to` | text | `native` and the target machine | same |
| `bytes` | int | the snapshot's size, the bytes sent | same |
| `heads` | text | hex digest of the Store's history heads, checked on the target | [`sqlite/transfer/steps.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) `digest` |

The record now names `machine: <to>` and keeps a `last_transfer`
`{ call, from, to, at, bytes, sha256, heads, retired }`.

## Where it runs

```text
 caller process (native side)                    target worker
 ----------------------------                    -------------
 check: clear a leftover copy, can_push,
        idle (no lease, no Sandbox), stat both ---> stat: exists? free?
        target free >= bytes + max(1 GiB, 10%)
 begin: home lock + Store lock, write op
 settle: close own copy, idle again, lease
 snapshot (sha256, heads; no -wal allowed)
 send: read 8 MiB pieces --------------------->  write <store>.incoming
 verify ------------------------------------->  sha256 + heads match
 op marked uncertain; install --------------->  rename into place
 record: machine = to, last_transfer
 retire: delete native's file, drop op
```

| Phase | Written to `op` first | A failure here is | Source |
| --- | --- | --- | --- |
| `check` | before `op` exists | certain; nothing changed | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) `run`, `reach` |
| `snapshot`, `send`, `verify` | yes | certain; the partial copy stays as `.incoming` | same, `phases` |
| `install` | yes, uncertain | uncertain only when the step answers `Unknown` | same |
| `record` | yes | **always uncertain**: both copies exist | same, `transfer` |
| `retire` | yes, certain | certain; the Store is on `to`, `last_transfer.retired` stays false | same |

Locks are held only for the checks and record writes, never across a
step. Each step runs once; a lost worker answer is never resent. See the
[routing table](../../../control/routing/routing-table.md).

## Errors

Every refusal's message starts `store push, <phase>:` and carries
`detail.phase`.

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad names; a PostgreSQL Store; the Store is not on `native`; `machine` is `native`; a folder-scoped client | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_push`, [`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs) |
| `NotFound` | no such Store or machine; no Store file at snapshot | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs), [`sqlite/transfer/steps.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| `InUse` | a live process holds the Store, or a Sandbox of one of its projects is recorded (`detail.related`) | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `store_idle`, [`sqlite/family.rs`](../../../../../control/crates/layerfs-store/src/sqlite/family.rs) `settle` |
| `Busy` / `Unresolved` | another operation runs on the Store / an earlier one's outcome is unknown | no | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) `blocks` |
| `Exists` | the target already holds a Store file of that name | no | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) `reach` |
| `Capacity` | the target has too little free disk (`detail.needed`, `detail.free`) | no | same |
| `Unavailable` | the target machine is stopped or its worker gone; names `layerfs machine start`, nothing is started | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Unsupported` | the target's worker does not report free disk (an older bundle) | no | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `Conflict` | the Store moved between check and `op` | no | same, `run` |
| `Failed` | an unfinished `-wal` at snapshot; a digest mismatch at verify; I/O | no, unless at `record` | [`sqlite/transfer/steps.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) |
| `Unknown` | **a worker's answer was lost, or any failure of `record`** | **yes** | [`sqlite/transfer/phases.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) `transfer` |

**After an uncertain push**, both copies are kept, `op` stays marked
uncertain and the event is appended to `log/uncertain.jsonl`. Read
[store.status](status.md) (its `op` and `state: "unresolved"`), then run
the same `store push` again: it first deletes the copy the record does not
name ([`sqlite/transfer/resolve.rs`](../../../../../control/crates/layerfs-store/src/sqlite/transfer/resolve.rs)).

## Examples

Not run; written from the call table.

```js
await layerfs.store.push("shop", "vm1", { on_progress: (p) => console.log(p.phase, p.done, p.total) });
```

```python
layerfs.store.push("shop", "vm1", on_progress=print)
```

```bash
layerfs store push shop vm1 --progress
```

## Related

- Before: [store.status](status.md), [machine.start](../machine/start.md)
  for the target. After: [store.pull](pull.md) brings it back; between two
  machines, pull then push.
- [Control: Home and records](../../../control/management/home-and-records.md),
  [Control: machines](../../../control/machines/README.md),
  [Errors: uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

---

← [store.connect](connect.md) · [Up: store](README.md) · [store.pull](pull.md) →
