# Storage placement — `StoragePlacement` (#545 B0-150 / B7-223)

> **Status:** implemented. `api/crates/layerfs-control/src/common/place.rs`,
> guard points in `sandbox/create.rs` and `store/transfer/phases.rs`, tests in
> `api/crates/layerfs-api/tests/storage_placement.rs`.

A local sqlite Store's file must be on the machine its calls run on. A machine
that keeps no disk of ours — a Family 2 machine: no worker, no daemon, no
mount, instances that stop with their provider — cannot hold one, and nothing
may quietly route a Store there. The guard is one decision in the placement
layer; both placement paths ask it **before** they reach a machine, so a
refusal leaves nothing behind.

## The fact, in order

| source | says | written by |
| --- | --- | --- |
| `machine/<name>/record.json` field `local_sqlite` | whether *this* machine keeps a sqlite Store's file | a Family 2 backend, when it creates or connects its machine — no platform this build names writes it |
| the platform's row in the one support table (`layerfs_machine::support`, rendered by `connect/SUPPORT.json` and `machine.platforms`) | where a sqlite Store's calls run on this host — `sqlite: in_process` or `worker`, else no route | the platform table, per host |

A record that writes `local_sqlite: false` is refused without consulting the
row; a record that does not say takes its platform's answer; anything other
than `true`/`false` is `InvalidInput`. Every platform built here is Family 1
(`sqlite: in_process | worker`), so no machine that exists today changes
behaviour. A machine with no record is not refused here: the step that
resolves the machine refuses `NotFound`.

## The decision

`common::place::require_store_home(home, store_ref, machine)`:

- a **PostgreSQL** Store is on no machine (#533 G3): placement does not apply
  and it is never refused;
- a **sqlite** Store is `Ok` when the machine's store home is `Machine`, else
  refused `StoragePlacement` with

```json
{ "store": "<store>", "machine": "<machine>", "backend": "docker|native|namespaces|null",
  "reason": "its record says local_sqlite is false: …" }
```

The message repeats the four and names the fix (a machine that holds the file,
or `store connect <store>`). Nothing is dialled, started or written, and no
`op` intent is taken: the decision precedes every side effect.

## Guard points

| call | where | why there |
| --- | --- | --- |
| `sandbox.create` | once the machine is resolved, before the name is claimed and before `sandbox_allowed` | with no `machine` a create is routed to the Store's own machine; on a Family 2 machine that route is exactly the one that must not happen |
| `store.push` | its `check` phase, after the sides are resolved and before `held_now` and `reach` | the push must not dial a worker, snapshot, send or install anything on a disk that stops with its instances |
| `store.pull` | not asked | pulling the Store off such a machine is the way off it; a guard there would strand the data |

`store.pull` and `machine.create`/`machine.connect` stay allowed on a Family 2
platform: only the *Store* cannot live there, not the compute.

## Tests

`api/crates/layerfs-api/tests/storage_placement.rs`, through the call surface,
on a home whose machine `dev` records `local_sqlite: false` — so they hold on
every host and in both builds of this layer (with and without the `machine`
feature):

| test | asserts |
| --- | --- |
| `a_sqlite_store_is_not_placed_on_a_machine_that_keeps_no_disk` | `sandbox.create` is `StoragePlacement`; detail names `store`, `machine`, `backend`, `reason` |
| `a_postgresql_store_is_never_refused_by_placement` | the same create is refused by its own machine checks, never by placement |
| `a_push_to_a_machine_that_keeps_no_disk_is_refused` | `store.push` is `StoragePlacement`; the same push to a machine that keeps its Store goes on to dial its worker (`Unavailable`) |
| `a_pull_from_a_machine_that_keeps_no_disk_stays_the_way_off` | `store.pull` is never `StoragePlacement` |
| `a_machine_record_that_does_not_say_whether_it_keeps_a_store_is_refused` | a `local_sqlite` that is not true or false is `InvalidInput` |

## Not in this change

- The doctor invariant the review asked for beside the two calls (`store/<s>`
  whose `sqlite.machine` names a `local_sqlite: false` machine is a `doctor`
  error): not built here; the two write paths make the state unreachable from
  this build, and an old or hand-edited home is still worth a report.
- The SQLite snapshot route (push/pull of a snapshot instead of a live Store):
  `store.pull` is the only route off today.
- Any Family 2 adapter. The guard lands first on purpose: the fact is a record
  field, so an adapter cannot ship without it.

## Diagrams

Both are hand-built Excalidraw scenes in the `layerfs` collection (workspace
`9Zq6XgRe98z`), Comic Shanns throughout, title 38 px and body 26 px. The
editable `.excalidraw` sources sit beside the PNGs — the scene each PNG was
rendered from, element for element.

- [`architecture.png`](architecture.png),
  [`architecture.excalidraw`](architecture.excalidraw) (35 elements) — Store
  placement: Family 1 vs Family 2, the fact in order, the one decision, the
  two guard points, and the PostgreSQL exception.
- [`sequence.png`](sequence.png), [`sequence.excalidraw`](sequence.excalidraw)
  (30 elements) — create/push → query the placement → allow or refuse, with
  what each answer lets happen.

| Scene | Id | Canvas | Source | PNG sha256 |
| --- | --- | --- | --- | --- |
| `SandboxComputePlane — #545 storage placement` | `5bQ0SFRz1wG` | 1210 × 1032 | [`architecture.excalidraw`](architecture.excalidraw) | `cf18d3ca0e29ca4bf187051d7dbf9077e242468128585135efcbf3e6791ac7c2` |
| `SandboxComputePlane — #545 placement sequence` | `V2KzRbdKl9` | 1210 × 1026 | [`sequence.excalidraw`](sequence.excalidraw) | `83c483213fdcf001bc10913dbaebc8f843577503307ea8b27689d76a81060bf9` |

Each source's bounds are the PNG's canvas, every text element is Comic Shanns
at 26 or 38 px, no text passes x = 1234, and re-rendering either scene yields
the PNG byte for byte (same sha256). The sources hold the scene's live
elements (`type`, `version`, `source`, `appState`, `elements`, `files`), as
[#547's](../547/) do.

## Links

- [#545](https://github.com/Ephemeral-AI-Lab/layerfs/issues/545) — the sandbox
  compute plane; B0-150 is this guard and B7-223 is `store.push`.
- [#536](https://github.com/Ephemeral-AI-Lab/layerfs/issues/536) — C5
  (`store.push` / `store.pull`) and H11 (where a Store's calls run).
- [#535](https://github.com/Ephemeral-AI-Lab/layerfs/issues/535) — §7.3 the
  support table, G3 (a PostgreSQL Store is on no machine).
