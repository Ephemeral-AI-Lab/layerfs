# Sandbox scratch lifecycle — B6 (#549)

> **Status:** implemented on `fix/549-scratch-lifecycle`, not on `main`.
> Code: `api/crates/layerfs-control/src/sandbox/{scratch,close,serve,record,runtime}.rs`.
> Tests: `api/crates/layerfs-control/tests/scratch_lifecycle.rs` (14 cases).

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
predicate both entry points use:

| Entry | Owner | When |
| --- | --- | --- |
| `clean(home, record, Owner::Close)` | the settled close that stopped the runtime | once per successful `sandbox.close`, and once for a `NoSuchSandbox` close |
| `sweep(home, machine, Mode::{DryRun, Apply})` | a bounded pass | once at the end of each successful close, for the closing record's machine |

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

## Trigger surface: narrowed, not claimed

**#549 acceptance item 5 (`--dry-run` lists exactly what a real run deletes)
is left unchecked for this PR.** `Mode::DryRun` and `Report` are library-only:
there is no operator call, CLI flag, Node or Python function that reaches
them, and this PR claims no operator surface.

The same applies to "periodically": the sweep runs once at the end of a
successful close, not on a timer and not from `doctor`. Adding a periodic
driver or `sandbox.scratch --dry-run` would add a public call (call table,
Node and Python); that is a separate, explicitly requested change. The policy
and the bounded dry-run pass are implemented and tested; the trigger is not
pretended to exist.

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

## Not in this change

- An operator surface (`sandbox.scratch`, `--dry-run` CLI, Node/Python
  bindings) and a periodic driver: the policy and report are library-only.
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
