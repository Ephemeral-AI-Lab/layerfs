# Storage placement — `StoragePlacement` (#545 B0-150 / B7-223)

> **Status:** implemented and accepted for [#552](https://github.com/Ephemeral-AI-Lab/layerfs/issues/552).
> `api/crates/layerfs-control/src/common/place.rs`, guard points in
> `sandbox/create.rs` and `store/transfer/phases.rs`, tests in
> `api/crates/layerfs-api/tests/storage_placement.rs`; the evidence is in
> [`552-ACCEPTANCE.md`](552-ACCEPTANCE.md).

A local sqlite Store's file must be on the machine its calls run on. A machine
that keeps no disk of ours — a Family 2 machine: no worker, no daemon, no
mount, instances that stop with their provider — cannot hold one, and nothing
may quietly route a Store there. The guard is one decision in the placement
layer; both placement paths ask it **before** they reach a machine, so a
refusal leaves nothing behind.

## The fact

| source | says | written by |
| --- | --- | --- |
| `machine/<name>/record.json` field `local_sqlite` | whether *this* machine keeps a sqlite Store's file | a Family 2 backend, when it creates or connects its machine — no platform this build names writes it |

The machine record is the only fact. The policy asks no platform support
table and no host, so the same record gets the same answer everywhere:

| record | answer |
| --- | --- |
| `local_sqlite: true` | `Machine` — allowed |
| `local_sqlite: false` | `Remote` — refused `StoragePlacement` |
| field absent | `Machine` — allowed; a backend that did not say keeps its Store, the compatible answer for every platform this build names |
| anything other than `true`/`false` | `InvalidInput` |
| no machine record | allowed here; the step that resolves the machine refuses `NotFound` |

Only a Family 2 backend that writes `local_sqlite: false` changes a
machine's behaviour. The older fallback to the platform's support row — where
a sqlite Store's calls run **on this host** — is gone: it made this policy
depend on the host and disagreed with the machine record. On Linux it also
refused a `lima` machine whose row had no sqlite route, even though its record
never said it keeps no Store; the record-only answer restores the compatible
answer on every host.

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

## Contract matrix (#552)

Every row is a call through `layerfs_api::call` in
`storage_placement.rs`, never a unit test of the refusal type alone. "Admitted"
means the placement decision let the call reach its next check or its bucket,
and the cell names that next answer so the row cannot pass on a different
refusal.

| #552 row | call in the test | machine record | evidence |
| --- | --- | --- | --- |
| local SQLite on its owning machine → admitted | `sandbox.create` on `dev` for Store `shop` on `native`; `store.push shop dev` | `local_sqlite: true` | `a_sqlite_store_on_its_owning_machine_is_admitted`: the next answer is `Unsupported` (a Sandbox must run where its Store is), never `StoragePlacement`; `a_machine_record_that_says_it_keeps_a_store_is_allowed`: the push goes on to dial the worker (`Unavailable`) |
| remote machine + local SQLite → `StoragePlacement` | `sandbox.create` on `dev`, and `store.push shop dev` | `local_sqlite: false` | `a_sqlite_store_is_not_placed_on_a_machine_that_keeps_no_disk`, `a_push_to_a_machine_that_keeps_no_disk_is_refused`: both `StoragePlacement`, detail `{store, machine, backend, reason}` |
| PostgreSQL → admitted | `sandbox.create` on `dev` for a PostgreSQL Store | `local_sqlite: false` | `a_postgresql_store_is_never_refused_by_placement`: the call is refused by its own machine checks, never by placement |
| SQLite snapshot → admitted | `backup.create shop cold` (a snapshot to a bucket) | any | `a_sqlite_snapshot_is_admitted`: the call reaches the bucket and stops at the bucket record that names no provider (`Failed`), never `StoragePlacement`; the Store's record is unchanged and nothing is staged |
| object storage as a live Store → refused | `store.connect` with `r2://…`, `s3://…`, `https://…` | any | `object_storage_is_never_a_live_store`: `InvalidInput` naming object storage as backup and transfer only; no Store folder and no record is left |

A `StoragePlacement` refusal is also checked for side effects: the refused
create leaves no `sandbox/<name>` folder, and the refused push leaves the
Store's record byte for byte as it was, with no `op` intent.

## Tests

`api/crates/layerfs-api/tests/storage_placement.rs`, through the call surface,
on homes whose machine `dev` records what each case asks about — every case
holds on every host and in both builds of this layer (with and without the
`machine` feature), because the fact is the record, not the host:

| test | asserts |
| --- | --- |
| `a_sqlite_store_is_not_placed_on_a_machine_that_keeps_no_disk` | `sandbox.create` is `StoragePlacement`; detail names `store`, `machine`, `backend`, `reason` |
| `a_postgresql_store_is_never_refused_by_placement` | the same create is refused by its own machine checks, never by placement |
| `a_sqlite_store_on_its_owning_machine_is_admitted` | `local_sqlite: true` admits `sandbox.create`; it stops at the Store's own machine rule (`Unsupported`), never `StoragePlacement`, and makes no Sandbox folder |
| `a_sqlite_snapshot_is_admitted` | `backup.create` is never `StoragePlacement`; it stops at the bucket record, leaving the Store's record and files unchanged |
| `object_storage_is_never_a_live_store` | `store.connect` with r2, s3 or https object storage is `InvalidInput` ("backup and transfer only"); no Store folder or record is made |
| `a_push_to_a_machine_that_keeps_no_disk_is_refused` | `store.push` is `StoragePlacement`; the same push to a machine that keeps its Store goes on to dial its worker (`Unavailable`); the refused push leaves the record unchanged |
| `a_pull_from_a_machine_that_keeps_no_disk_stays_the_way_off` | `store.pull` is never `StoragePlacement` |
| `a_machine_record_without_local_sqlite_is_allowed` | the field absent is `Machine`: a `lima` push goes on to dial its worker (`Unavailable`), never `StoragePlacement` |
| `a_machine_record_that_says_it_keeps_a_store_is_allowed` | `local_sqlite: true` is `Machine`: `store.push` goes on to dial its worker (`Unavailable`) |
| `a_machine_record_with_an_invalid_local_sqlite_is_refused` | a `local_sqlite` that is not true or false is `InvalidInput` |

## Doctor

`layerfs doctor` reports an error on a `store/<s>` whose `sqlite.machine` names
a machine whose record says `local_sqlite: false`, or whose `local_sqlite`
cannot be read: the two write paths make that state unreachable from this
build, but an old or hand-edited home is still worth a report. The finding
names `store pull <s>` and the machine record to repair. The test
`each_problem_gives_one_finding_naming_its_fix` covers both the `false` field
(Store `s7`) and a field that is neither true nor false (Store `s8`).

## Not in this change

- The SQLite snapshot route (push/pull of a snapshot instead of a live Store)
  as a live Store route stays future (B12). A snapshot is
  `backup.create`/`backup.restore` to a bucket, admitted because it is not a
  live Store; `store.pull` is the only route that moves a live sqlite file off
  a machine today.
- Any Family 2 adapter. The guard lands first on purpose: the fact is a record
  field, so an adapter cannot ship without it.

## Diagrams

Both are hand-built Excalidraw scenes in the `layerfs` collection (workspace
`9Zq6XgRe98z`), Comic Shanns throughout, title 38 px and body 26 px. The
editable `.excalidraw` sources sit beside the PNGs — the scene each PNG was
rendered from, element for element.

- [`architecture.png`](architecture.png),
  [`architecture.excalidraw`](architecture.excalidraw) (35 elements) — Store
  placement: Family 1 vs Family 2, the fact (the machine record's
  `local_sqlite`: `absent` is `Machine`, `false` is `Remote`), the one
  decision, the two guard points, and the PostgreSQL exception.
- [`sequence.png`](sequence.png), [`sequence.excalidraw`](sequence.excalidraw)
  (30 elements) — create/push → query the placement → the record's
  `local_sqlite` → allow or refuse, with what each answer lets happen.

| Scene | Id | Canvas | Source | PNG sha256 |
| --- | --- | --- | --- | --- |
| `SandboxComputePlane — #545 storage placement` | `5bQ0SFRz1wG` | 1210 × 1032 | [`architecture.excalidraw`](architecture.excalidraw) | `d580bf445d0cdb11c85f55dbc30bcd1cbe8f85a6d46459e9c823040c9e96b5b4` |
| `SandboxComputePlane — #545 placement sequence` | `V2KzRbdKl9` | 1210 × 1026 | [`sequence.excalidraw`](sequence.excalidraw) | `1794b36ec286f1ebd2346d76f0c4e0e47ce54cfdb51676cfe7d03667b737e849` |

Each source's bounds are the PNG's canvas, every text element is Comic Shanns
at 26 or 38 px, no text passes x = 1234, and re-rendering either scene yields
the PNG byte for byte (same sha256). The sources hold the scene's live
elements (`type`, `version`, `source`, `appState`, `elements`, `files`), as
[#547's](../547/) do.

## Links

- [#552](https://github.com/Ephemeral-AI-Lab/layerfs/issues/552) — this
  guard's issue: the #552 contract matrix and its acceptance boxes.
- [#545](https://github.com/Ephemeral-AI-Lab/layerfs/issues/545) — the sandbox
  compute plane; B0-150 is this guard and B7-223 is `store.push`.
- [#536](https://github.com/Ephemeral-AI-Lab/layerfs/issues/536) — C5
  (`store.push` / `store.pull`) and H11 (where a Store's calls run).
- [#535](https://github.com/Ephemeral-AI-Lab/layerfs/issues/535) — §7.3 the
  support table, G3 (a PostgreSQL Store is on no machine).
