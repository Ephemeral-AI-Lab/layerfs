# #548 B5: remaining boxes, verification and missing E2E evidence

> Status: branch `agent-b548` (base `agent-b5`). This page is the branch's
> evidence record for #548's remaining boxes, not a release receipt.

## Environment

The LayerFS harness runs this branch in a Linux container workspace. Its
Docker E2E path is unavailable:

```
$ rustc --version
rustc 1.85.1 (4eb161250 2025-03-15)
$ cargo --version
cargo 1.85.1 (d73d2caf9 2024-12-31)
$ ls -l /var/run/docker.sock
ls: cannot access '/var/run/docker.sock': No such file or directory
$ command -v docker
(no output)
```

Build selection: `CARGO_TARGET_DIR=/tmp/layerfs-target`,
`CARGO_HOME=/tmp/cargo-home`, `CARGO_NET_OFFLINE=true` after
`cargo fetch --locked`. The test profile's first build took 17m47s; later
selections reuse it.

## Missing evidence: Docker E2E

**NOT RUN.** There is no Docker Engine socket and no `docker` CLI in this
sandbox, so a real container Sandbox cannot be created, stopped or
reclaimed. No Docker result is claimed here, and none is simulated.

These need a host with Docker before #548 can close:

- `sandbox.close` stopping and deleting a real container, with the tombstone
  recording the reason;
- `serve::reclaim` deleting a stopped container only when
  `reclaim.enabled` is on, it is not frozen, and it is not a dry run;
- the concurrent-exec boundary against a real runtime.

## Evidence (no Docker)

`cargo check -p layerfs-control -p layerfs-api --tests`: exit 0.

| Selection | Result |
| --- | --- |
| `cargo test -p layerfs-control --test sandbox_lifecycle` | **8 passed**, 0 failed |
| `cargo test -p layerfs-api --test sandbox_lifecycle` | **9 passed**, 0 failed |
| `cargo test -p layerfs-api --test sandbox_records` | 3 passed |
| `cargo test -p layerfs-control --test typed_records` | 7 passed |
| `cargo test -p layerfs-api --test home_layout_3` | 3 passed |
| `cargo test -p layerfs-api --test sandbox_family` | 0 passed, 1 failed (environment) |

`sandbox_family` fails at `project.init` before any Sandbox call:

```
project.init: ("Unsupported", "filesystem 0x794c7630 at
/tmp/lfs-sbx-home-12857/store does not carry SQLite locks between kernels;
direct Store access needs ext4, xfs or btrfs")
```

The container's workspace is on an overlay filesystem, so this is an
environment failure, not a B5 regression; it needs a lock-capable filesystem
(ext4, xfs, btrfs) to run.

What the selections cover, box by box:

- **Nine explicit Sandbox states** — `SandboxState` in
  `api/crates/layerfs-control/src/sandbox/lifecycle.rs` is one
  `#[non_exhaustive]` enum: `Creating`, `Provisioning`, `Starting`, `Ready`,
  `Busy`, `Draining`, `Closed`, `Lost`, `Unresolved`. Its one
  mapping, `sandbox_state`, derives the state from the record, its `op`
  standing/name/phase and its live leases; `SandboxRecord::state` returns
  the enum and `sandbox.get` / `sandbox.list` only serialize it under the
  existing `state` key. An absent `state` derives as before; a record
  written before layout 3 that still carries one is read through the same
  enum (`running`/`ready`, `gone`/`closed`, `closing`/`draining`,
  `unknown`/`unresolved`). `every_sandbox_state_is_explicit_and_reachable`
  covers all nine variants and their precedence;
  `old_records_default_to_the_derived_enum` covers the default and the old
  names; `sandbox_get_and_list_serialize_the_explicit_states` shows every
  state through `sandbox.list` and five through `sandbox.get`, with the
  record's existing keys unchanged. `failed` is deliberately not a Sandbox
  state: that name belongs to a machine record, and no Sandbox path writes
  it, so a legacy record carrying it derives from its facts instead.

- **Draining by CAS on `generation`** — `close.rs` writes `generation + 1`
  and the `sandbox.close` intent under the home and Sandbox locks;
  `every_close_transition_is_explicit` and the API stale/replay tests.
- **Exec lease deadline** — `ExecLease::begin` writes the deadline and a
  heartbeat; `a_generation_and_its_lease_are_record_facts` and
  `an_exec_lease_holds_a_close_and_is_released_on_drop`.
- **Close sequence: Draining, check, close/abort, record Closed/Lost** —
  `close_transition`, the API stale/lease/expired/panic tests, and the
  tombstone `reason`.
- **No kill while a lease is live; expiry is Lost/Unresolved** —
  `a_live_exec_lease_blocks_the_close` and
  `an_expired_exec_lease_is_lost_and_observable`.
- **Idempotent close; a stale generation aborts** —
  `a_stale_generation_close_changes_nothing` (no write, no tombstone; the
  replay is quiet and the stale replay is refused).
- **Failures are Unresolved; no unbounded retry** —
  `Draining + Unknown/Panic` in the matrix and
  `a_close_that_died_while_draining_is_unresolved_until_one_close_clears_it`
  (a read settles nothing; one explicit close clears it; the replay is a
  no-op).
- **Lost is first-class** — `with_lost`, `state()`, and `sandbox get` /
  `sandbox.list` showing `state: lost` with `close_reason`.
- **Last activity uses the monotonic clock** — `with_activity` and the
  record-facts test.
- **`reclaim.enabled`, `--dry-run`, freeze and the close reason** —
  `ReclaimPolicy::decide` (`each_reclaim_brake_stops_the_destructive_path`),
  `serve::reclaim` refusing `Lost` with the brake as `reason` before any
  delete, `sandbox.close --dry-run` changing nothing
  (`a_dry_run_close_decides_and_changes_nothing`), and the close recording
  `close_reason` at the CAS and on every outcome.
- **Reclamation keyed by machine/backend, never isolation** —
  `ReclaimKey` / `reclaim_backend` and
  `reclamation_is_keyed_by_machine_and_backend_not_isolation`: the
  `isolation` field never changes the key; the backend and machine do.
- **A concurrent exec started between the decision and the kill** —
  `an_exec_between_the_close_decision_and_the_kill_is_refused`: in
  `Draining`, `ExecLease::begin` is refused `Busy` and writes no lease, so
  the close cannot kill it.
- **Fault injection for a panic/exit during close** —
  `a_close_that_died_while_draining_is_unresolved_until_one_close_clears_it`
  writes the exact record a close leaves when its process ends after the
  generation CAS (dead owner, `draining`, uncertain) and proves the read
  model and the single clearing close;
  `a_started_crashed_close_is_unresolved_with_its_reason` covers the same
  state with an id.
