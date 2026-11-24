# Lean verification of the GC / mount-admission lifecycle

## Scope and verdict

The lifecycle (Sandbox create/mount/unmount/close/force/remove × Store GC
`branch.remove`/`store.compact` × Machine incarnation/restart × Workspace mount
registration) is a state machine with a small set of safety invariants. Lean 4 can
formalize and prove the **abstract design** invariants (Level 2). It cannot prove the
Rust implementation (Level 3+) or the DB/FUSE/network semantics (Level 4).

## The five levels (re-checked)

| Level | Content | Work | Assurance | Do? |
| --- | --- | --- | --- | --- |
| 0 | spec only (types/state, no proofs) | days (~100–300 lines) | design clarified, no machine check | yes |
| 1 | executable model + property tests (`#eval`/`Plausible`) | 1–2 weeks | holds on tested states | yes |
| 2 | abstract model + safety invariants proven | 4–8 weeks (a first complete model + 8 invariants, with Lean experience) | design-level proofs | yes (recommended) |
| 3 | refinement (Rust ↔ Lean; Aeneas/Verus) | 1–3 person-months | code-level, only pure core functions | optional |
| 4 | end-to-end (DB transactions, crash, concurrency, FUSE) | years (seL4/CompCert scale) | strongest | no |

Corrections to the earlier sketch:
- The Level 2 estimate of 2–6 weeks was optimistic; 4–8 weeks is realistic, and the
  model will be revised while proving.
- "Liveness" is only weak: the operator is the only escape. "No automatic release" is
  a **safety** property, not liveness.
- Level 3 with Aeneas/Verus is practical only for the pure functions
  (`settle_place`, the ordered compare, the PlaceView merge); the async/worker/DB
  parts are not.
- The model must abstract the transaction as atomic: a `Step` either commits or not.

## The model (Level 0–2)

### State

- `Intent`: `state : Op | Done | Uncertain`, `op`, `ownerBoot`, `machineInc`,
  `sandboxGen`, `leaseExpires`, `fence`, `evidence`
- `Machine`: `incarnation : Option Ordered` (the ordered token)
- `Sandbox`: `generation : Option Ordered`, `tombstone : Bool`
- `Store`: `branches`, `intents`, `retired`
- the machine / Sandbox / Store-GC lifecycles

### Transitions

Sandbox create/mount/unmount/close/force/remove; machine restart/restore; admission
(the place compare); `branch.remove`; `store.compact`; operator
preview/settle/release/archive.

### The invariants

1. A live mount is never reclaimed by `branch.remove`/`store.compact`.
2. Settle/release only on a provably Dead or explicitly Uncertain row.
3. Missing / old / unordered evidence is never Dead.
4. Ordered rule: current newer ⇒ Dead; current older ⇒ Uncertain; equal ⇒ Live.
5. Compact barrier: a live intent blocks `store.compact`.
6. Fence: a stale fencing token cannot release the row.
7. Audit: a retained `done` row survives `store.compact` (`Copy::Done`).
8. The operator is the only path to end an unprovable row (no automatic release).

### Proof shape

`theorem step_preserves (h : Inv s) : Inv (step op s)`, case analysis per transition;
`omega` for the ordered compare; the DB transaction assumed atomic.

## What Lean does NOT cover

- The Rust implementation (Level 3 only for the pure core functions)
- SQLite/PostgreSQL transactions, crash recovery, WAL
- Concurrency/linearizability of worker restarts
- FUSE/kernel behavior
- The live Store operator run (settle/release the 6, `store.compact` the 331)

## Plan

1. Lean 4 model + the 8 invariants (Level 0–2), reviewed against the design.
2. Property tests in Lean; align them with the Rust property/integration tests.
3. Optional Level 3 for `settle_place` and the PlaceView merge.
4. Document the mapping in `mount-authority-lease.md`.

## Relation to the lean phase-3 design

The lean design (0 new tables, 4 `gc_intent` columns, ordered
`MachineRecord.incarnation` / `SandboxRecord.generation`, machine-level tombstone,
retained `done` row audit, `Copy::Done`, per-machine 2.0 conversion) is the subject
of the invariants. Lean verifies the design; the Rust tests + FUSE verify the
implementation; the live operator run is out of scope.
