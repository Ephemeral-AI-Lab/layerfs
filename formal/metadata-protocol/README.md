# PostgreSQL metadata protocol: TLA+ model (#811 V1)

> **Status:** Verification artifact for issue #811 V1. It is not product code
> and not a test gate. It models the PostgreSQL Store's metadata protocol at
> `origin/layerfs_v2_0_0` = `ca4be0bf3c` (variant **A**) and at the unmerged
> writer-reopen commit `1a7338ffa2` on `claude/writer-reopen` (variant
> **B**). The source pins are those two commits. Nothing here was run against
> a server.

## Why TLA+ and TLC

The question is whether a protocol holds over *every* interleaving of a few
actors under a bounded number of faults, and whether it stays live under
fairness. TLC checks exactly that: it explores every reachable state of
small instances breadth-first, gives the shortest counterexample trace, and
checks temporal properties (`<>`, `[]<>`, leads-to) under weak fairness. P
mostly does systematic testing, with random or bounded-depth search by
default: that is useful for long executions but it does not prove that a
bounded instance has no bad state, and its liveness checking is weaker.
The faults here (a lost reply whose statement may still run later, a
backend that dies, a lane connection that dies between two statements) are
nondeterministic choices, which TLA+ expresses directly. The shortest
counterexample from BFS maps onto source lines almost one action per
statement.

Tools: `tla2tools.jar` **v1.7.4** (TLC 2.19 of 08 August 2024), from
<https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar>,
sha256 `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`,
kept in `~/.cache/layerfs-811/tools/` (not in the repository). Java 17.

## Files

| File | What |
| --- | --- |
| `Admission.tla` | GC admission: session advisory keys (Store and Branch, shared and exclusive, levels counted per backend, released when the backend ends), `gc_intent` rows, mount (one message, or statement by statement), unmount release, `branch.remove`, compaction, the writer's quarantine (A) or reopen (B), the candidate fixes |
| `Commit.tla` | Commit and id metadata: `store_lane` row and lane barrier, fused Commit with its attempt token, the lane merge with a nested write and its lane check, `force` (displaced), lost-reply resolution (`resolve_retained`, `pass_lane`), the generation fence, the id allocator (block reservation by compare-and-set, compare-and-set rewind), A and B |
| `gen-configs.sh` | Writes every `*.cfg` (the bounds are there, one line each) |
| `run.sh` | Runs TLC on each config (default: all), each under `~/.cache/layerfs-811/tools/to 120`; prints exit code, wall time and whether the result is the expected one; logs go to `logs/` (ignored) |
| `trace.py` | Prints a log's counterexample compactly (action names and changed variables) |

Run: `./run.sh` (all, about 4 minutes in total) or `./run.sh adm_B_sep`.
Exit 0 is a clean check, 12 an invariant violation, 13 a temporal
violation, 124 the 120 s timeout. `run.sh` exits non-zero only when a result
differs from the expectation listed in it.

## What is modelled

One action is one client turn on a writer session (one statement, or one
pipelined message), one server event, or one fault. Server transactions run
atomically when the locks they wait for are free, except where the source
sends two statements another backend can fall between: the mount's Admit
transaction (scan, then insert and COMMIT) and the nested write's lane check
then COMMIT.

- **Sessions.** Each service has one writer session with a FIFO turn
  (`Admission::enter`); a transaction holds the turn to its COMMIT, and a
  held lane holds it for the whole hold. Every `run_*` call passes
  `Session::ready` (`transaction.rs:362` at `1a7338ffa2`): A refuses a
  quarantined session before sending anything; B reopens it
  (`reopen.rs:118-160`), once per turn, outside a held lane. B also
  quarantines first when the server already closed the connection.
- **Reopen (B)**, steps 1-4 of `reopen.rs` `settle`: end every lost backend by
  pid and start (one still running a request may stay "not proven gone"),
  read the generation (moved: `Stale`), take every `held_ops` key again all
  or none (refused: `Busy`), delete each recorded orphan's row once. A
  failed reopen closes its new connection and fails the call.
- **Faults.** A lost reply: the request ran, or the backend still holds it
  and may run it later (until it ends); the client sees `Unknown`,
  quarantines and never resends. A killed writer backend. A killed lane
  connection. A generation bump (compaction swap, restore, upgrade), allowed
  only when no fenced transaction is open, as `LOCK ... ACCESS EXCLUSIVE
  NOWAIT` requires.
- **Admission actors.** Mounts on service S1 (optionally the one-message
  path first, falling back to statement by statement on a definite
  refusal), `branch.remove` on S2 (register on the lane, marker, release),
  compaction on its own handle session SC (never reopens), and `T`, an
  endless caller on S1 standing for any other thread's calls (a Commit, a
  status read, an unmount).
- **Commit actors.** Two Workspaces on two services, up to three Commit
  calls each; a Workspace with `force`; allocator handles reserving blocks,
  using an id, rewinding.

## Properties

| #811 guarantee | Property (module) |
| --- | --- |
| History writes are totally ordered | `TotalOrder`, `NoLostUpdate` (Commit) |
| No lost update; a conflict publishes nothing; `--expected` and `force` (`displaced`) | `NoLostUpdate`, `ForceOnlyDisplaces`, `ReportsExact` (a refused or moved Commit is never installed) |
| Attempt installed at most once; reported committed ⇒ installed; unprovable ⇒ `Unknown` | `AttemptOnce`, `ReportsExact` (the resolution says `Committed` or `NotCommitted` only when proven, otherwise the record stays `Unknown`) |
| No write of a fenced session lands after a generation bump (`StaleStore`) | `FenceHolds` |
| No nested write commits after its lane connection died (`Closed`) | `LaneLiveness`, judged at the lane check |
| Compaction and `branch.remove` never proceed while a mount is live; no live mount is left without keys | `Exclusion`, `RemovedClean`, `KeyedAdmission`, `KeysCover` (Admission) |
| No id reused | `IdsUnique` |
| Reopen: one reopen per call, keys re-taken all or none, orphaned rows removed, nothing resent | `OneReopenPerCall`, `KeysCover`, `OrphansRemoved`; nothing is resent by construction (no action sends twice) |
| Liveness: a quarantined writer eventually serves or refuses definitely | `Termination` (every call answers), `Serves` (B: `quar => <>(serving or definitely stale)`) |
| Liveness: orphaned rows are eventually removed | `OrphansRemoved` (`<>[]` no row of a failed mount) |

`KeyedAdmission`: every intent row commits while its own backend holds the
op's keys. `KeysCover`: a session that serves holds, on its current
backend, the keys of every op in its `held_ops`.

## Results

All runs: TLC 2.19, 14 workers, Apple M-series, `-deadlock` (terminated
actors are not deadlocks). Bounds are in each config's line of
`gen-configs.sh`: `kills / losts` are fault budgets; `MaxB` = 6 backend ids
(Admission), 6-8 (Commit).

| Config | Variant | Actors, faults | Properties | Result | Distinct states | Wall |
| --- | --- | --- | --- | --- | --- | --- |
| `adm_A` | A | 2 mounts (both paths), remove, compaction, T; 1 kill, 1 lost | 4 safety | clean | 35,040 | 3 s |
| `adm_A_live` | A | 2 mounts, compaction, T; 1 kill, 1 lost | `Termination` | holds | 7,156 | 2 s |
| `adm_A_orphans` | A | 1 mount, T; 1 lost | `OrphansRemoved` | **violated** (documented limit at A) | 50 | 1 s |
| `adm_B_sep` | B | 1 mount (statement by statement), T; 1 kill | 4 safety | **violated** `KeyedAdmission` (the known race) | 124 | 2 s |
| `adm_B_one` | B | 1 mount (one message), T; 1 kill | 4 safety | **violated** `KeysCover` (new) | 65 | 2 s |
| `adm_B_rm` | B | 1 mount, remove; 1 kill | 4 safety | **violated** `KeyedAdmission` (same race, `branch.remove`) | 189 | 1 s |
| `adm_B_excl` | B | 1 mount, remove, compaction; 1 kill | `Exclusion`, `RemovedClean` | **violated** `Exclusion` (the consequence) | 725 | 2 s |
| `adm_B_reopen1` | B | 1 mount; 1 kill, 1 lost | `OneReopenPerCall` | **violated** (two reopens in one mount call, one per turn) | 152 | 1 s |
| `adm_B_live` | B | 1 mount (both), T; 1 lost | 3 liveness | **violated** `OrphansRemoved` (new, orphan noted too late) | 161 | 1 s |
| `adm_B_term` | B | 2 mounts, compaction, T; 1 kill, 1 lost | `Termination`, `Serves` | holds | 170,566 | 51 s |
| `adm_B_F1` | B + F1 + orphan noted in-turn | 2 mounts (both), remove, compaction, T; 1 kill, 1 lost | 4 safety | clean | 383,199 | 13 s |
| `adm_B_F1_fixed` | B + F1 + both orphan fixes | same | 4 safety | clean | 374,691 | 11 s |
| `adm_B_F1_reach` | B + F1 + fixes | 1 mount, T; 1 kill | probe `NoMountAfterReopen` | violated as intended: a mount that reopened mid-registration is admitted | 98 | 2 s |
| `adm_B_F1_live` | B + F1 + orphan noted in-turn | 1 mount, T; 1 lost | 3 liveness | **violated** `OrphansRemoved` (new, late commit after "not proven gone") | 129 | 3 s |
| `adm_B_F1_fixed_live` | B + F1 + both orphan fixes | 2 mounts, compaction, T; 1 kill, 1 lost | 3 liveness | holds | 73,701 | 59 s |
| `adm_B_F1_reopen1` | B + F1 + fixes | 1 mount; 1 kill, 1 lost | `OneReopenPerCall` | violated (as `adm_B_reopen1`) | 122 | 2 s |
| `adm_B_F1late` | B + F1 after the lock's turn | 1 mount, T; 1 kill | 4 safety | **violated** `KeysCover` | 94 | 3 s |
| `adm_B_F2` | B + F2 per call | 1 mount, T; 1 kill | 4 safety | **violated** `KeysCover` | 109 | 2 s |
| `adm_B_F2all` | B + F2 session-wide + orphan noted in-turn | 2 mounts (both), remove, compaction, T; 1 kill, 1 lost | 4 safety | clean | 249,595 | 22 s |
| `commit_A` | A | 2 Workspaces (one forces), 3 Commits each; 1 writer kill, 1 lane kill, 2 lost, 1 bump | 8 safety | clean | 147,632 | 5 s |
| `commit_B` | B | same | 8 safety | clean | 1,900,273 | 27 s |
| `commit_B_lost2` | B | 2 Workspaces, 2 Commits each; 1 kill, 2 lost | 8 safety | clean | 30,972 | 3 s |
| `ids_A` / `ids_B` | A / B | 2 allocator handles, 2 rounds each; 1 kill, 1 lost | 8 safety | clean | 4,853 / 22,911 | 2 s / 2 s |
| `ids_B_lost2` | B | 2 handles, 2 rounds; 2 lost (late reservations and rewinds) | 8 safety | clean | 19,855 | 2 s |
| `commit_mut_nobarrier` | B, no lane barrier | 2 Workspaces; 1 lane kill | 8 safety | violated `NoLostUpdate` (as it must) | 238 | 1 s |
| `commit_mut_nolanecheck` | B, no lane check | same | 8 safety | violated `NoLostUpdate` | 213 | 1 s |
| `commit_mut_noend` | B, resolution does not end the old backend | 2 Workspaces; 1 lost | 8 safety | violated `ReportsExact` | 181 | 2 s |
| `commit_mut_nofence` | B, no fence | 2 Workspaces; 1 bump | 8 safety | violated `FenceHolds` | 53 | 1 s |
| `ids_mut_nocas` | B, rewind without compare-and-set | 2 handles | 8 safety | violated `IdsUnique` | 297 | 2 s |

The mutations show the Commit and id properties are not vacuous: removing
the barrier, the lane check, the backend termination before the lane pass,
the fence or the rewind's compare-and-set each breaks the property it
protects. In the Admission module variant B as shipped is itself the
non-vacuity witness, and `adm_B_F1_reach` shows that F1 is checked on the
race path itself.

## Counterexamples

Line numbers are at `1a7338ffa2` (B) unless stated.

### 1. The known race: reopen between `session_lock` and `run_admit` (`adm_B_sep`)

```text
MLock    m1's session_lock takes S/B shared on backend 1   admission.rs:944
Kill     backend 1 dies (keys gone)
Reopen   m1's run_admit turn: ready() sees the closed     admission.rs:950,
         connection, reopens on backend 4; held_ops lacks  transaction.rs:362,
         m1, so no key is taken                            reopen.rs:238-243
MScan    Admit: fence, probe, conflict scan pass
MCommit  m1's row commits with no key held  -> KeyedAdmission violated
MHold    hold_op records the keys of dead backend 1        admission.rs:961
```

Verdict: **real product bug** in B (confirmed from source: `register_separately`
takes the keys in one turn and admits in another; `run_admit` goes through
`Session::ready`, which reopens; `retake_keys` reads only `held_ops`, which the
op enters only after `run_admit` returned). A also checks the same path clean:
there the second turn is refused or fails on the dead connection.

Consequence (`adm_B_excl`): with the mount keyless, a `branch.remove` on S2
takes its exclusive Branch key, registers (its scan does not see the mount's
uncommitted row) while the mount's scan did not see the removal's row, and
both rows commit: `Exclusion` violated, removal proceeds beside a live
mount. The same holds for compaction's exclusive Store key.

`adm_B_rm`: `branch.remove` itself registers through `register_separately`
on the Store service writer (`removal.rs:37` → `catalog.rs:361` →
`admission::register` → `admission/mount.rs:43`), lock then `session.run(true)`
(`admission.rs:954`): a reopen between them inserts the removal row with no
exclusive key. Same bug, same fix.

### 2. New: the one-message mount's `hold_op` runs after its turn (`adm_B_one`)

```text
MOne     the one message admits m1 on backend 1 (keys + row)   history/mount.rs:251
         decided_turn ends; hold_op not yet run                admission/mount.rs:113-124
Kill     backend 1 dies
Reopen   T's call reopens on backend 4; held_ops lacks m1
MHold    hold_op(m1) -> S1 serves with m1 held, no keys    -> KeysCover violated
```

Verdict: **real**, narrow window (another thread must win the writer's turn
between `decided_turn` returning and `hold_op` at `admission/mount.rs:124`,
and the backend must die in it). The row exists, so compaction and removal
are still refused by the row scan, but the mount has no keys for the rest of
its life, which is the "no live mount is left without keys" guarantee the
reopen claims (step 3) broken: its exclusion rests on the row scan alone.
Fixed by F1 applied to the one-message path.

### 3. F2 per call is not enough (`adm_B_F2`) and F1 after the turn is not enough (`adm_B_F1late`)

F2 refusing only the registering call's own reopen: `T`'s call reopens
instead, between MCommit and MHold (or between the two turns), and the mount
ends up held without keys. F1 done as a separate step after the lock's turn
ended: the kill and `T`'s reopen fall between the turn and `hold_op`; the op
then sits in `held_ops` without keys and admits keyless. Both are model
results about candidate fixes, not product code.

### 4. New: an orphan noted after its turn is missed by the reopen (`adm_B_live`)

```text
MOneLost  m1's message reply is lost; the row committed; S1 quarantined
TCall     T's call reopens: release_orphans finds none     reopen.rs:286-310
MNote     note_orphan(m1) only now                          admission/mount.rs:46-49
          no further quarantine -> the row stays until the process exits
```

Verdict: **real** (confirmed: `note_orphan` runs in `register_mount` after
the turn that saw `Unknown` ended). The statement-by-statement path has a
second instance: after an Admit `Unknown`, the Err arm's `session_unlock`
(`admission.rs:966`) is itself a turn through `ready()` and reopens before
`note_orphan` runs (the trace of `adm_B_reopen1` shows it). Liveness, not
safety: the orphaned row refuses compaction and `branch.remove`
(`MountsActive`) for the service's life, exactly the A limit the reopen was
meant to clear.

### 5. New: a late commit of a backend not proven gone outlives the orphan release (`adm_B_F1_live`)

```text
MOneLost  reply lost; backend 1 still holds the message (pend)
Reopen    step 1 cannot prove backend 1 gone (pg_terminate_backend timed
          out) and proceeds; step 4 deletes m1's row: none yet    reopen.rs:164-168
LateExec  backend 1 commits m1's row, then exits
          the orphan list is empty -> the row stays
```

Verdict: **real but narrow** (confirmed: `settle` runs `release_orphans` after
`end_lost` whatever it proved; the module doc and the docs table say "a
backend not proven gone stays recorded and the reopen proceeds"). It needs a
backend that outlives `pg_terminate_backend`'s `lock_timeout` bound while
still about to commit. Liveness only.

### 6. Observation: one reopen per *turn*, not per call (`adm_B_reopen1`)

`register_mount` makes several writer turns (lock, Admit, the Err arm's
unlock). A kill after the lock and a lost Admit reply give two reopens in
one SDK call, one in each turn. No resend and no loop: each turn attempts
at most one reopen. Not a bug; the docs' "one attempt per call" should say
"per writer turn" (`postgresql.md` Writer reopen; `reopen.rs` module doc).
F1 does not change it.

### 7. Observation: the nested write's lane check precedes its COMMIT

The lane check and `COMMIT` are two statements of one simple query, so the
lane connection can die between them and the write still commits after its
lane connection died. Safety holds because the barrier keeps the next lane
holder out until that COMMIT ends: `commit_mut_nobarrier` shows a lost
update without it. `LaneLiveness` is therefore judged at the check; the
docs' guarantee "no nested write commits after its lane connection died"
is exact only as "none commits unless its lane transaction was in progress
at its check, and no later holder reads before it ends". Not a bug.

No counterexample was found in the Commit module (A or B) or in the id
allocator: lane order, heads, force, attempt tokens and their resolution,
the fence and the compare-and-set allocator hold within the bounds,
including late execution of a lost request and B's reopen.

## Fix comparison and recommendation

| Fix | Safety | Cost |
| --- | --- | --- |
| F1: the keys enter `held_ops` in the turn that took them | clean (`adm_B_F1`, with remove, compaction, both mount paths) | reuses the reopen's existing re-take; no new refusal |
| F1 after the lock's turn | violated (`adm_B_F1late`) | - |
| F2: the registering call refuses to reopen | violated (`adm_B_F2`): another thread's call reopens | - |
| F2 session-wide: no reopen by any call while a registration is between its keys and its `hold_op` | clean (`adm_B_F2all`) | a new cross-turn registration counter; every other call of the service (Commits, unmounts) is refused while it is set after a connection loss |

**Recommended: F1, done inside the turn.** It closes the known race, the
one-message window (2) and `branch.remove` (1, `adm_B_rm`) with the
mechanism the reopen already has (step 3 re-takes everything in
`held_ops`, all or none), and it refuses nothing new. F2 must be
session-wide to be safe, which turns one registration's misfortune into
refusals of unrelated calls, and it still needs the one-message window
covered.

The product change F1 implies (at `1a7338ffa2`):

1. `history/admission/session.rs` `session_lock` (the `run_bare` closure,
   lines ~60-100): decide "all granted, same generation, own backend" inside
   the closure and, when so, insert the op into `held_ops` there, while the
   turn and the state lock are still held (`Session::hold_op`; `held_ops` is
   locked under the state lock by `retake_keys` already, so the lock order
   state → `held_ops` is the existing one). `session_lock` gains the op id
   (`register_separately` passes `intent.op_id`; `StoreClaim`, which does not
   reopen, passes none).
2. `history/admission.rs` `register_separately` (944-968): drop the later
   `hold_op` on `Ok` (it is held already; a re-registration's earlier keys,
   returned by the in-turn insert, are unlocked after success or restored on
   failure). On `Err`, unlock while the op is still held (a reopen inside the
   unlock's own turn then re-takes and the unlock gives back exactly that
   level), then remove it from `held_ops` (as `begin_release` /
   `end_release` do for a release).
3. `backend/postgresql/history/mount.rs` `admit` (the `decided_turn`
   closure at 251): on `Admission::Admitted`, `hold_op` inside the closure,
   and remove the `hold_op` in `history/admission/mount.rs:124`.

With the two orphan fixes (both checked: `adm_B_F1_fixed`,
`adm_B_F1_fixed_live`):

4. Note the orphan in the turn that saw `Unknown`: in the `decided_turn`
   closure on `Admission::Unknown`, and for `run_admit`'s `Unknown` before
   that turn ends (or at least before the Err arm's `session_unlock`, which
   otherwise reopens first); `admission/mount.rs:46-49` then only covers
   what remains.
5. `reopen.rs` `settle` (164-170): release orphans only when step 1 left no
   lost backend (`state.lost` empty); otherwise keep them, and let a later
   top-level call that finds orphans waiting end the lost backends again and
   release them once all are gone (as `pass_lane` already ends them).

## Abstractions and limits (stated honestly)

- **Bounds.** Two services plus a compaction handle, at most two mounts,
  one removal, one compaction, two Workspaces with up to three Commits,
  two allocator handles with two rounds, one or two faults of each kind,
  six to eight backend ids, one generation bump. A bug that needs more
  actors or faults is not excluded.
- **Locks.** Advisory keys are Store and Branch only (the project key
  behaves as the Branch key); a lock wait is modelled as an enabling
  condition, not a `lock_timeout` (`Busy` is not modelled except as a
  reopen's refused re-take); the lane row and barrier are guards with no
  queueing order.
- **Atomic server work.** A fused Commit, a removal registration, a
  marker, a release and a reservation are each one atomic step; packs,
  merges and their content are not modelled; a three-way merge is "read the
  head under the lane". `--expected` is the bound head (an explicit
  expected head behaves the same in the model).
- **Lost replies.** A lost session-lock or unlock reply, a lost reopen
  statement and a lost removal reply are not modelled (the lock's lost reply
  only leaves extra keys until the backend ends). Late execution is
  modelled for the mount admission, the fused Commit, the nested COMMIT,
  reservations and rewinds.
- **Faults.** Compaction's own session is never faulted; a killed backend is
  a current writer or lane connection; transaction poolers, multi-host
  connection strings, failover and `synchronous_commit` (V3) are out of
  scope; durability is assumed (an acknowledged COMMIT is never lost).
- **Owner liveness.** Reconcile of dead owners (`settle_dead`, process exit)
  is not modelled, so `OrphansRemoved` is about rows a live service leaves.
- **Reopen id reuse.** A reopen that fails leaves nothing on the server, so
  the model reuses its backend id to keep `MaxB` small.
- **Fairness.** Weak fairness per actor and per server event; faults are
  unfair and bounded.
