# #552 acceptance — the `StoragePlacement` guard

> **Branch:** `agent-b552`; the code head where the commands below ran is
> `12c75ffa8569a2a541672933adbc325a92aad2441cfd720128a7bc42ef47851ec6`
> (the #557 guard commits `d009eaa92`…`42c4fa53a` plus the #552 work below).
> Docs-only commits `126284cbe42c` (this report) and later README/report
> edits follow it and change no code.
> **Base:** `main` at `9f97a7ec9`; the PR branch at `42c4fa53a` is the branch's
> first parent. Acceptance record for Ephemeral-AI-Lab/layerfs#552.
>
> LayerFS Commits on `agent-b552` carry the files; the repository's own `.git`
> history stops at the imported PR head. The evidence below is from calls run
> inside a Workspace, never from the host.

## What the branch already delivered

| Piece | Where |
| --- | --- |
| the decision | `api/crates/layerfs-control/src/common/place.rs`: `placement`, `StoreHome`, `require_store_home` |
| guard on `sandbox.create` | `api/crates/layerfs-control/src/sandbox/create.rs`, once the machine is resolved |
| guard on `store.push` | `api/crates/layerfs-control/src/store/transfer/phases.rs`, in `check`, before `held_now`/`reach` |
| `store.pull` not guarded | `transfer/phases.rs`; `pull` has no target machine, so the way off stays open |
| doctor finding | `api/crates/layerfs-control/src/doctor/checks.rs`: an error on a Store whose machine is `Remote`, or whose record cannot say |
| contract tests | `api/crates/layerfs-api/tests/storage_placement.rs` |

## Contract matrix (#552 acceptance)

Every row is a call through `layerfs_api::call`, not a unit test of the refusal
type. "Admitted" means the placement decision let the call reach its next check
or its bucket; the evidence names that next answer so a different refusal cannot
make the row pass.

| #552 row | call | record | exact evidence |
| --- | --- | --- | --- |
| local SQLite on its owning machine → admitted | `sandbox.create` on `dev`; `store.push shop dev` | machine `dev`: `local_sqlite: true` | `a_sqlite_store_on_its_owning_machine_is_admitted` — `Unsupported` (a Sandbox must run where its Store is), never `StoragePlacement`, no Sandbox folder; `a_machine_record_that_says_it_keeps_a_store_is_allowed` — the push reaches the worker dial, `Unavailable` |
| remote machine + local SQLite → `StoragePlacement` | `sandbox.create` on `dev`; `store.push shop dev` | `local_sqlite: false` | `a_sqlite_store_is_not_placed_on_a_machine_that_keeps_no_disk` — code `StoragePlacement`, detail `{store: "shop", machine: "dev", backend: "namespaces", reason: "its record says local_sqlite is false: …"}`; `a_push_to_a_machine_that_keeps_no_disk_is_refused` — the same for `store.push` |
| PostgreSQL → admitted | `sandbox.create` on `dev` for a PostgreSQL Store | `local_sqlite: false` | `a_postgresql_store_is_never_refused_by_placement` — refused by its own machine checks, never `StoragePlacement`, and no "cannot be placed" text |
| SQLite snapshot → admitted | `backup.create shop cold` | any; the snapshot is not a live Store | `a_sqlite_snapshot_is_admitted` — the call reaches the bucket and stops at the bucket record that names no provider (`Failed`); never `StoragePlacement`; the Store record is unchanged and no `backup.staging` exists |
| object storage as a live Store → refused | `store.connect` with `r2://`, `s3://`, `https://` | any | `object_storage_is_never_a_live_store` — `InvalidInput`: "cannot be live on object storage … backup and transfer only"; no Store folder, no Store record |

## Refusal side effects

The refusal is checked for the contract's "creates nothing":

- `a_sqlite_store_is_not_placed_on_a_machine_that_keeps_no_disk` asserts
  `sandbox/box` does not exist after the refused create; the guard runs before
  the name claim, `Op::begin`, `sandbox_allowed` and any worker dial.
- `a_push_to_a_machine_that_keeps_no_disk_is_refused` reads the Store's record
  before the call and asserts it is byte-for-byte unchanged after it, and that
  no `op` field was written; the guard runs in `check`, before `Op::begin`,
  `held_now` and `reach`.

## #552 boxes and evidence

### Scope

- [x] **Guard storage placement: a local SQLite Store on a Family 2 backend is
  refused with `StoragePlacement` (contract test).** `d009eaa92` added
  `require_store_home`; `c75f1564d` added the call-surface tests; this branch's
  `12bc03e`…`12c75ff` completes the matrix. Both guard points are exercised:
  `sandbox.create` and `store.push`, each returning `StoragePlacement`.
- [x] **No SQLite download / push back: a remote machine never pulls a live
  SQLite Store over the wire.** `store.pull` is not asked
  (`transfer/phases.rs`); a pull's target is always `native`, so no remote
  machine is ever a pull destination. `a_pull_from_a_machine_that_keeps_no_disk_stays_the_way_off`
  proves a pull from a `local_sqlite: false` machine is never
  `StoragePlacement`, and `a_push_to_a_machine_that_keeps_no_disk_is_refused`
  proves the opposite direction is refused before a worker is dialled.

### Acceptance

- [x] **Contract test matrix** — all five rows above; 10/10 tests pass in both
  the default build and with `--features machine`.
- [x] **The refusal names the Store, the machine and the reason, and creates
  nothing.** `assert_placement_detail` checks `store`, `machine`, `backend` and
  `reason` in the detail and repeats the reason in the message; the two
  side-effect assertions above cover "creates nothing".
- [x] **`check_product_boundary.py` passes; no new trait or field conflicts.**
  `PASS: scanned 1236 production Rust/SQL/JavaScript files; semantic review
  still required` (exit 0). The change adds one private function
  (`object_storage`) and no trait, record field or call-table entry.
- [x] **The guard is exercised from the admission path, not only from a unit
  test of the error type.** Every test calls `layerfs_api::call` on a real home;
  the two refusal rows go through `sandbox.create` and `store.push`.

## Commands and results

Run in a Workspace at code head `12c75ff`, `cargo 1.85.1`,
`CARGO_TARGET_DIR=/tmp/layerfs-target`; the fmt and boundary commands were
rerun at the final head `12ec5e40` with the same results:

```
cargo fmt --all -- --check
  exit 0
python3 tools/check_product_boundary.py
  PASS: scanned 1236 production Rust/SQL/JavaScript files; semantic review still required
  exit 0
cargo test -p layerfs-api --test storage_placement
  running 10 tests
  test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s
cargo test -p layerfs-api --features machine --test storage_placement
  running 10 tests
  test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
cargo test -p layerfs-control --test doctor
  running 4 tests
  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

The ten placement tests are:
`a_sqlite_store_is_not_placed_on_a_machine_that_keeps_no_disk`,
`a_postgresql_store_is_never_refused_by_placement`,
`a_sqlite_store_on_its_owning_machine_is_admitted`,
`a_sqlite_snapshot_is_admitted`, `object_storage_is_never_a_live_store`,
`a_push_to_a_machine_that_keeps_no_disk_is_refused`,
`a_pull_from_a_machine_that_keeps_no_disk_stays_the_way_off`,
`a_machine_record_without_local_sqlite_is_allowed`,
`a_machine_record_that_says_it_keeps_a_store_is_allowed`,
`a_machine_record_with_an_invalid_local_sqlite_is_refused`.

The doctor test `each_problem_gives_one_finding_naming_its_fix` now also covers
a machine whose `local_sqlite` is neither true nor false (Store `s8`), beside
the `local_sqlite: false` Store `s7`; both are errors and the doctor writes
nothing (its `tree()` snapshot is unchanged).

## Changes made for this acceptance

| LayerFS Commit | Change |
| --- | --- |
| `12bc03eba7bb` | `store.connect` refuses an object-storage URL as a live Store, before the URL is parsed or a record is written (`object_storage`) |
| `123a8eae08ff` | matrix tests: owning-machine admission, SQLite snapshot admission, object storage refusal; side-effect assertions on both `StoragePlacement` paths; doctor's malformed `local_sqlite` |
| `1268d02bc8a2` | the contract matrix, refusal side effects and doctor evidence in `README.md` |
| `12d9af468c44` | test-only read of the Store record unwrapped once |
| `1254b79a0e17` | the snapshot test's bucket record gets a parseable credential reference |
| `12c75ffa8569` | `cargo fmt` (the only source change: test formatting) |

## Not claimed

- Snapshot push/pull as a **live Store** route (B12) is future. This change
  proves the existing bucket snapshot (`backup.create`/`backup.restore`) is not
  refused by placement because it is not a live Store; it does not add a
  snapshot Store kind.
- The object-storage refusal is `InvalidInput` from `store.connect`, not
  `StoragePlacement`: object storage is not a machine, and the matrix row asks
  only that a live Store there is refused.
- No Family 2 adapter is added. The record field `local_sqlite` is the only
  fact the guard reads; no platform support table or host decides it.
