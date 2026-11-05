# Sandbox scratch lifecycle — B6 (#549)

> **Status:** implemented on `fix/549-scratch-lifecycle`, including its
> operator surface (`sandbox.scratch`, `--dry-run`). Not on `main`.
> Code: `api/crates/layerfs-control/src/sandbox/{scratch,clean,close,serve,record,runtime}.rs`.
> Tests: `api/crates/layerfs-control/tests/scratch_lifecycle.rs` (14 cases),
> `api/crates/layerfs-api/tests/sandbox_scratch.rs` (2 cases) and
> `api/crates/layerfs-cli/tests/scratch.rs` (1 case).

A local Sandbox's scratch is the one `tmp` its commands write throwaway files
into: `<machine>/native/run/<prefix><id>-<name>/tmp`. It is not the Store, not a
Workspace and not the host's `/tmp`, and a cleaned-up Sandbox must not leave it
behind. Before this change the record did not name it, so a cleaner could not
tell an owned scratch from a Store, a Workspace directory or a rebuilt
Sandbox's new scratch: the possible failures were leaking it or deleting
someone else's files.

## The record owns the scratch

A started local Sandbox reports its own scratch root and a fresh token; the
home record keeps it under `scratch`:

```jsonc
"scratch": {
  "root": "/…/machine/native/run/lfs-<id>-<name>/tmp",
  "createdAt": 1762480000000,
  "generation": 11451298734274452831
}
```

- `root` is the runtime's `scratch_directory()`.
- `createdAt` is when the start claimed it.
- `generation` is a fresh random positive `u64` per start.
- **Docker records none**: a container's `/tmp` is the container's and goes
  with it, so the home records no host path it must never touch.
- A record with no `scratch` owns nothing and cleans nothing.

`generation` here is a **scratch token**, not the record/CAS generation #548
(B5) defines. It answers only "is this still the Sandbox that claimed this
directory?". A record may later carry both as separate fields and separate
spaces; they are not interchangeable, and this cleanup does not depend on the
Sandbox state machine that #548 defines.

## The decision: one predicate, two ways in

`sandbox/scratch.rs` is domain code: it knows no Docker, worker, daemon or
Store engine, and it adds no trait. `keep(home, root, state, owner)` is the one
predicate every entry point uses, so the close, the pass it triggers and the
operator call cannot decide differently:

| Entry | Owner | When |
| --- | --- | --- |
| `clean(home, record, Owner::Close)` | the settled close that stopped the runtime | once per successful `sandbox.close`, once for a `NoSuchSandbox` close, and once for an id-less record (`close_unstarted`) |
| `sweep(home, machine, Mode::Apply)` | a bounded pass | once at the end of each successful close, for the closing record's machine |
| `sweep(home, machine, Mode::{DryRun, Apply})` | `sandbox.scratch(machine, { dry_run })` in `sandbox/clean.rs` | on demand, or periodically from cron or launchd |

`keep` allows the delete only when all of these hold:

1. the root is absolute, has no `.`/`..`, and its leaf is `tmp`;
2. it is strictly under this home, not the home itself;
3. it is not under `home/store/` and not under `home/sandbox/` (records and
   Workspaces);
4. it is not the host's `/tmp`, `/private/tmp` or `/var/tmp`;
5. its canonical target is inside the canonical home, and the root itself is
   not a symbolic link.

Then, immediately before the delete, `clean`/`sweep` re-reads the record: if
its `generation` is no longer the one the plan was made with, the plan aborts
(`Kept::Stale`) and the rebuilt Sandbox keeps its scratch.

`Owner::Sweep` keeps `running`, `creating`, `closing` and `unresolved`
records; only a settled `gone` record is swept. `Owner::Close` is the explicit
owner: a close that clears an uncertain intent may remove its scratch. A TTL is
never treated as ownership.

## Close never fails because the cleaner failed

A close has already stopped the runtime when it cleans. `sandbox.close` calls
`scratch::clean_on_close`, which is infallible by construction:

- on success it removes the owned scratch (or reports it was already gone);
- on any failure it appends the record name, root, code and message to
  `log/scratch.jsonl` and deletes nothing;
- it never returns an error, so a cleaner failure cannot turn a successful
  close into an error after the runtime is gone.

The close-triggered sweep is equally non-fatal (`sweep_scratch` discards its
result). A failure is retried by the next pass.

## The sweep is bounded and cannot be starved

- A pass visits `home/sandbox/<name>/record.json` records in name order.
- It filters to the pass's **machine** before the bounded batch, so records
  of other machines cannot crowd a settled record out of the pass.
- It charges the batch of `MAX_SWEEP` only for a root it may remove: a kept
  record and an already-gone root do not use the batch.
- A removed root is gone on the next pass, so a home with more than
  `MAX_SWEEP` settled records is covered over successive passes; the tests
  show a 67-record home reaching the last 3 on the second pass.

A pass deletes at most 64 roots by construction; there is no unbounded loop.

## Trigger surface: a close, and one bounded operator pass

**#549 acceptance item 5 is met by `sandbox.scratch`.** One table entry —
`layerfs sandbox scratch <machine> [--dry-run]`, or
`layerfs.sandbox.scratch(machine, { dry_run })` in Node and Python, which are
built from the same table — reaches the pass `close` already runs, through the
same `keep` predicate and the same bounded `sweep`. Nothing else was added: no
second policy, no second traversal, no trait.

| Entry | When it runs |
| --- | --- |
| the closing record's pass (`sweep_scratch` in `sandbox/close.rs`) | once per successful `sandbox.close` and once per `NoSuchSandbox` close |
| `sandbox.scratch(machine, { dry_run })` | on demand, or periodically from cron or launchd |

- **`--dry-run` is exact.** It answers `planned`: the roots a real pass
  deletes, in record-name order, each with the record that owns it
  (`{ sandbox, root }`). The real pass answers that same list as `deleted`.
  Both are one `sweep` call with a different `Mode`, so the two lists cannot
  drift, and a dry run deletes nothing.
- **"Periodically" is the operator's schedule, not a timer we pretend to
  have.** The issue's "clean on close *or* periodically" is satisfied by the
  close-time pass and by this call run from cron or launchd; none is
  scheduled by LayerFS itself. No timer thread and no `doctor` trigger was
  added, so a home nobody schedules is documented as cleaning on close only.
- **It is keyed by machine.** A pass reads only the records whose `machine`
  is the argument, so another machine's settled scratch is that machine's
  pass to clean; the isolation class is never consulted. This is the local
  contract only: a pass reads the home it runs in, and B7's provider backend
  must satisfy the same contract on its own machine.
- A machine name with no records plans nothing and deletes nothing; the
  call is not refused, so a leftover home stays cleanable.

## Tests

`api/crates/layerfs-control/tests/scratch_lifecycle.rs`, 14 cases; every case
that can delete seeds Store, Workspace, overlay, mount and daemon-log
bystanders and asserts they survive.

| test | asserts |
| --- | --- |
| `a_close_removes_only_the_scratch_root` | one close deletes the root and nothing beside it; a second close is idempotent |
| `a_path_outside_the_recorded_root_is_never_deleted` | a Store, a Workspace, another home's root and a host temp path survive |
| `the_host_tmp_is_never_a_scratch_root` | `/tmp`, `/private/tmp` and `/var/tmp` are refused as roots |
| `a_stale_generation_aborts_before_deleting_a_rebuilt_sandbox` | a changed token keeps the new Sandbox's scratch |
| `an_unresolved_sandbox_is_not_swept_but_an_explicit_close_cleans_it` | a sweep keeps `unresolved`; the explicit close removes it |
| `a_running_sandbox_keeps_its_scratch_during_a_sweep` | a sweep keeps a `running` record |
| `a_settled_gone_record_is_swept_and_nothing_else_is_deleted` | a sweep deletes a `gone` root only |
| `dry_run_lists_exactly_what_apply_deletes` | `Mode::DryRun` plans exactly what `Mode::Apply` deletes, and deletes nothing |
| `a_sweep_is_keyed_by_machine_never_by_isolation_class` | another machine's pass leaves the root; its own machine's pass removes it |
| `a_clean_failure_is_recorded_and_never_changes_a_successful_close` | a failed generation re-read is logged to `log/scratch.jsonl`; `clean_on_close` cannot fail and deletes nothing |
| `a_sweep_reaches_a_settled_record_past_more_than_a_batch_of_unrelated_ones` | 70 records of another machine cannot starve the 71st, this machine's settled one |
| `a_pass_is_bounded_and_the_next_pass_reaches_the_next_batch` | a 67-record home deletes 64 then the remaining 3 |
| `a_symlinked_scratch_is_not_followed` | a symlinked root is kept and its target survives |
| `a_scratch_token_round_trips_through_the_record` | `root`, `createdAt`, `generation` survive the record and the shown view |

The operator surface has its own cases, through the same dispatcher every
front end calls (`layerfs_api::call`, and the shell):

| test | asserts |
| --- | --- |
| `api/crates/layerfs-api/tests/sandbox_scratch.rs::dry_run_lists_exactly_what_the_real_pass_deletes` | `sandbox.scratch`'s dry run plans exactly what its real pass deletes, each root with its record; a `running`, an `unresolved` and another machine's settled root survive both |
| `api/crates/layerfs-api/tests/sandbox_scratch.rs::the_pass_needs_a_machine_name` | a call without a machine is refused `InvalidInput` |
| `api/crates/layerfs-cli/tests/scratch.rs::dry_run_lists_and_the_real_run_deletes_the_same_roots` | the shell's `--dry-run` lists the same set the real run deletes and deletes nothing; a wrong option is exit 2, before any call |

Verified with cargo 1.85.1 on 2026-11-04 (guest `aarch64`,
`CARGO_TARGET_DIR=/tmp/layerfs-target`):

```console
$ cargo test --locked --no-fail-fast -p layerfs-control --test scratch_lifecycle \
    -p layerfs-api --test calls_table --test sandbox_scratch -p layerfs-cli --test scratch
32 passed; 0 failed   # 14 control, 15 table, 2 surface, 1 shell
```

## Acceptance (#549)

| Item | Where it is met |
| --- | --- |
| clean only the Sandbox's own scratch | `keep`/`owns` in `sandbox/scratch.rs`; `a_close_removes_only_the_scratch_root`, `a_path_outside_the_recorded_root_is_never_deleted` |
| the record is the owner: root, `createdAt`, `generation`, re-checked before the delete | `Scratch`, `SandboxRecord::started`; `a_scratch_token_round_trips_through_the_record`, `a_stale_generation_aborts_before_deleting_a_rebuilt_sandbox` |
| an `Unresolved` Sandbox is not auto-deleted | `Owner::Sweep` keeps `Kept::Unresolved`; `an_unresolved_sandbox_is_not_swept_but_an_explicit_close_cleans_it`; the surface case above |
| never touch host `/tmp`, Stores, Workspaces | `owns`, `host_tmp`; `the_host_tmp_is_never_a_scratch_root`, `a_path_outside_the_recorded_root_is_never_deleted`, every bystander assertion |
| clean on close or periodically | `clean_on_close` + `sweep_scratch` on close; `sandbox.scratch` on demand or from cron/launchd (above) |
| tests prove nothing else is deleted | `seed_bystanders`/`bystanders` in every control case, the `kept` reasons, and the surface cases' surviving roots |
| keyed by machine / backend, never by isolation class | `sweep` filters on `record.machine()` before its batch; `a_sweep_is_keyed_by_machine_never_by_isolation_class`, `a_sweep_reaches_a_settled_record_past_more_than_a_batch_of_unrelated_ones` |
| a rebuilt Sandbox keeps its scratch; a stale generation is refused | `Kept::Stale`; `a_stale_generation_aborts_before_deleting_a_rebuilt_sandbox` |
| seeded host `/tmp`, Store and Workspace files are untouched by the sweeper | the bystander cases above |
| `--dry-run` lists exactly the same set the real run deletes | `dry_run_lists_exactly_what_apply_deletes` (policy) and the call/CLI cases above |

## Diagrams

One hand-built Excalidraw scene in the `layerfs` collection (workspace
`9Zq6XgRe98z`), Comic Shanns 38/26 px. The editable source sits beside the PNG
— the scene the PNG was rendered from.

- [`scratch-lifecycle.png`](scratch-lifecycle.png),
  [`scratch-lifecycle.excalidraw`](scratch-lifecycle.excalidraw) — ownership is
  a record fact; the generation check immediately before the delete; close and
  sweep abort paths.

| Scene ID | Canvas | Source | Elements | PNG sha256 |
| --- | --- | --- | --- | --- | --- |
| `ADjXWMSzudG` | 1230 × 1020 | [`scratch-lifecycle.excalidraw`](scratch-lifecycle.excalidraw) | 58 live + 6 soft-deleted (64 total) | `e92b9d0fb3305a6e423c97458247c351fb41d62ad3b62833c049e16476ea28df` |

Live-element bounds are 1230 × 1020 (x 10…1240, y 10…1030), the PNG's canvas.

## Production LOC

`python3 tools/production_loc.py --root <tree>` on the base `d2c3f2c97` and on
this branch:

```text
Production LOC: 137010 -> 137365 (delta +355)   # 61c0d2908, the reviewed head
Production LOC: 137010 -> 137392 (delta +382)   # after the close/sweep fixes
```

The operator surface adds `sandbox/clean.rs`, one table entry and the two
surface test files; the figures above are not re-measured for it.

## Not in this change

- A timer inside the daemon, or a `doctor` trigger, that runs the sweep on a
  schedule: the periodic driver is the operator's cron/launchd invocation of
  `sandbox.scratch`, as above. Nothing schedules it for you.
- A provider backend's scratch on a worker machine: `sandbox.scratch` reads
  the home it runs in. B7 defines the contract a remote backend must satisfy.
- #548 (B5) record generation/CAS, `Draining`/`Lost` and the Sandbox state
  machine: this change uses its own scratch token and does not bind cleanup to
  that state machine.
- A TTL or age-based cleanup: a settled `gone` record and an explicit close
  are the only owners, and `createdAt` is kept for a future report rather than
  used as an ownership decision.
- The create-then-record window (`serve::create` returns the scratch before
  `record.started` persists it): a record-write failure there is a separate
  follow-up, unchanged by this PR.

## Links

- [#549](https://github.com/Ephemeral-AI-Lab/layerfs/issues/549) — B6 Sandbox
  scratch lifecycle.
- [#545](https://github.com/Ephemeral-AI-Lab/layerfs/issues/545) — the Sandbox
  compute plane.
- [#548](https://github.com/Ephemeral-AI-Lab/layerfs/issues/548) — B5 record
  generation and `Lost`/`Unresolved` semantics: a different generation space.
- [#536](https://github.com/Ephemeral-AI-Lab/layerfs/issues/536) — the Sandbox
  record, `op` intents and the close contract.
