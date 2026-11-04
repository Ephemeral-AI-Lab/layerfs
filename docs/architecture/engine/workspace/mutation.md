# Mutation

[Architecture](../../README.md) › [Engine](../README.md) › [Workspace](README.md) › Mutation

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Planning and running changes.

| | |
| --- | --- |
| Component | [Workspace](README.md) |
| Source | [`engine/crates/layerfs-workspace/src/mutation`](../../../../engine/crates/layerfs-workspace/src/mutation) |

## Plans

```text
  caller           Workspace             overlay lock: one job     Store
    | Operation       |                        |                     |
    |---------------->| next_serial            |                     |
    |                 | (creating ops only)    |                     |
    |                 | prepare_mutation       |                     |
    |                 | no SQL, no base I/O    |                     |
    |                 |--- round 1: job ------>| decide over rows    |
    |                 |<-- Needs[...] ---------| writes nothing      |
    |                 | supply(view) ---- no lock held ------------->|
    |                 |<----------------------------- base facts ----|
    |                 |--- round 2: job ------>| decide again over   |
    |                 |                        | current rows;       |
    |                 |                        | apply(Changes):     |
    |                 |                        | one transaction     |
    |                 |<-- Applied{publication}|                     |
    |<-- Outcome -----|                        |                     |
    | reply attempt -> reply_attempted(publication)                  |
```

`MutationPlan` holds one operation and moves through three stages
([`plan.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/plan.rs)).

```text
            prepare_mutation
                  |
                  v
   +-------> [ Owner ] --- job() ---> one owner round
   |              |
   |   accept(result)
   |      |              |                 |
   |   Needs(non-empty)  Applied/Unchanged  Refused / error
   |      v              v                 v
   |  [ Base ]       [ Finished ]      [ Finished ]
   |      |
   +-- supply(view): fetch facts outside the owner
```

| Step | Does | Never does | Source |
| --- | --- | --- | --- |
| `prepare_mutation` | Checks the source engine and the reserved serial | SQL, provider I/O or serial allocation | [`plan.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/plan.rs) |
| `job` | Hands out the `NamespaceJob` while in `Owner` | — | [`plan.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/plan.rs) |
| `accept` | Takes exactly one owner result | Re-enter after a publication or error | [`plan.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/plan.rs) |
| `supply` | Fetches the named base facts | Any SQL job | [`plan.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/plan.rs) |

A refused preparation returns `MutationInputFailure` with the original
`Operation`. An error finishes the plan, and `operation()` still returns the
original input for custody. Nothing can replay a failed attempt.

## Jobs

```text
  NamespaceJob  (data only)            perform(db) inside one owner job
  +-------------------------+          +---------------------------------+
  | source     BaseSource   |          | decide(db):                     |
  | root       root serial  |  ----->  |   check descriptor custody      |
  | operation  Operation    |          |   rows = db.source_rows(source) |
  | now        Time         |          |   Eval over rows + facts        |
  | serial     reserved     |          |   -> Final(outcome)             |
  | facts      BaseFacts    |          |   -> Publish{Changes}           |
  +-------------------------+          | db.apply(source, Changes)       |
                                       |   one transaction -> Publication|
                                       +---------------------------------+
```

| `JobOutcome` | Meaning | State changed |
| --- | --- | --- |
| `Applied { publication, inode }` | Every effect published | Yes, once |
| `Unchanged { inode }` | Success with nothing to change (for example an empty write) | No |
| `Needs(Vec<Need>)` | Base facts to fetch before the next round | No |
| `Refused(Refusal)` | Definite pre-effect refusal | No |

Source: [`job.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/job.rs).
Before publishing, the job re-reads every changed name's layers and stamps
each `DirectoryEntryChange` with whether the base binds that name
(`inherited`). For unlink, rmdir and rename it marks the inode whose `nlink`
fell to 0 as `detached`.

### From job to overlay row

```text
  Operation::Create { parent: 7, name: "a.c", mode: 0o644 }
        |
        v  create::create over Eval
  Changes {
    created:            Some(serial 900)       first rows inserted
    inodes:             [ Inode 900 (File, nlink 1, born = active gen),
                          Inode 7 (mtime = now, entries + 1) ]
    directory_entries:  [ (7, "a.c") -> Bound { serial 900, inherited } ]
    cell, write, claims, predecessor, detached, moved_directory: None
  }
        |
        v  Overlay::apply(source, &changes)   one SQLite transaction
  overlay rows: inode 900, inode 7, entry (7, "a.c") in the active
  generation; one Publication awaiting its reply attempt
```

| `Changes` field | Holds | Set by |
| --- | --- | --- |
| `inodes` | Final values of every changed inode | All operations |
| `directory_entries` | `Bound` or `Removed` name rows | Namespace operations |
| `created` | The reserved serial; its rows must not exist | Create, mkdir, symlink |
| `cell` | Symlink target (one 4 KiB cell) | Symlink |
| `write`, `claims` | One payload window and its reference ranges | Writes |
| `detached` | Inode removed by this operation | Unlink, rmdir, rename |
| `moved_directory` | (directory, new parent) for the kernel parent index | Rename of a directory |
| `predecessor` | (moved file, replaced file) delta-base hint | Rename over a file |
| `open` | Descriptor authority for an orphan's mutation | Descriptor operations |

The overlay schema behind these rows is in
[Overlay: Namespace](../overlay/namespace.md) and
[Overlay: Payload](../overlay/payload.md).

## Facts

```text
  Eval asks:  rows.inode(s)?  -> local row decides
              facts.inode(s)? -> base fact decides
              neither         -> Need::Inode(s), round undecided

              name layers (active, lower)? -> local row decides
              parent born above floor?     -> base binds nothing
              facts.name(p, n)?            -> base fact decides
              neither                      -> Need::Name(p, n)
```

| Type | Holds | Source |
| --- | --- | --- |
| `Need` | `Inode(serial)` or `Name(parent, name)` | [`facts.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/facts.rs) |
| `BaseFacts` | Base inodes as overlay `Inode` values, and base name bindings | [`facts.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/facts.rs) |
| `VisitFacts` | `BaseFacts` tagged with the base root they came from (kernel visits) | [`facts.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/facts.rs) |
| `Eval` | One evaluation: rows of this job, facts, root, open serial, kernel mount | [`eval.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/eval.rs) |

Facts stay true while the source is owned, because install is fenced behind
its release. Supplying a bound name also supplies that child's inode and the
rest of a declared destination path, so an undisturbed operation settles in
two owner jobs. `FACT_WINDOW` (768 entries) bounds one job's facts; reaching
it clears the set and refetches, never refuses. A base directory fact carries
its entry count and derived child-directory count ([Base: Links](base.md#links)).

## Driver

`Workspace::mutate` is the plain driver for callers that own a `SourceView`
([`driver.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/driver.rs)):

```text
  serial = next_serial(allocator)   if operation.creates()
  plan   = prepare_mutation(view, operation, now, serial)
  loop:
    result = overlay.namespace(plan.job())     one owner job
    if plan.accept(result) is Some(outcome): return outcome
    plan.supply(view)                          outside every lock
```

| Caller | Route | Source |
| --- | --- | --- |
| SDK file writes | Same loop with `OverlayJobs::namespace`, then `reply_attempted` | [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| FUSE and NFS requests | `kernel_mutation_visit`: same evaluation, facts from memory inside the job | [Operations](operations.md#kernel-mutations) |

Between two rounds another operation may publish. The next round evaluates
the rows current in its own job, so it decides over the newer state and never
over a stale read.

## Related

- [Operations](operations.md): what each `Operation` changes.
- [Base](base.md): where facts are read.
- [Ports](ports.md): `OverlayJobs`, the boundary each round crosses.
- [State](state.md): the serial allocator.
- [Overlay: Namespace](../overlay/namespace.md): how `apply` writes rows.

---

← [Workspace: Operations](operations.md) · [Up: Workspace](README.md) · [Workspace: Ports](ports.md) →
