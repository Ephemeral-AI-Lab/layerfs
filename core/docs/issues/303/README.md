# Phase 7 cluster two — architecture and implementation design

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Issue [#303](https://github.com/Ephemeral-AI-Lab/layerfs/issues/303). Written
> 2026-10-05 against `main` `f96d97651be5299f153ccde2bc8d921dd58807ad`.
> This set is a design. It contains no product code and claims no
> implementation, measurement, qualification or release admission. No build,
> test or benchmark was run to produce it.

This is the single entry point for the cluster two design: Workspace, FUSE,
daemon, sandbox and Commit integration. It reconciles the prepared #301, #303
and #304 documents, the #305 and #306 experiments, the current source and the
completed cluster one work. It supersedes the untracked
`IMPLEMENTATION-PLAN.md` of 2026-10-03, which assumed PostgreSQL and MinIO.

"Cluster one" and "cluster two" are workstreams. C1, C2 and C5 in older
documents mean `layerfs-content`, `layerfs-storage` and `layerfs-history`.

## Documents

| # | Document | Read it for |
| --- | --- | --- |
| 01 | [Architecture](01-architecture.md) | Who owns what, which process and database, the locks, the trust boundary |
| 02 | [Base plus overlay](02-base-overlay.md) | The schema, generations, extents, names, caches, the limitation inventory, remaining limits |
| 03 | [Mutation hot path](03-mutation-hot-path.md) | What a mutation does before it is acknowledged, per-operation costs, repeated edits, maintenance, targets |
| 04 | [Concurrency and Commit](04-concurrency-commit.md) | The state machine, capture, construction, outcomes, fold, open-unlinked files, worked interleavings |
| 05 | [FUSE assessment](05-fuse-assessment.md) | What #305 and #306 do and do not show, the current mount, the target profile, coherence, the optimization matrix |
| 06 | [Cluster one integration](06-cluster-one-integration.md) | Exact APIs, bridge operations, the store host, prerequisites, failure boundaries |
| 07 | [Implementation and validation](07-implementation-validation.md) | Slices, file ownership, removal against relocation against estimate, tests, qualification |
| 08 | [Decisions and provenance](08-decisions-provenance.md) | Source provenance, superseded assumptions, every decision, the disposition matrix, owner questions |

An implementer reads 01, 02, 03 and 04 in order, then 06, then 07. A reviewer
of evidence reads 05 and 08.

## Claim labels

Important claims carry one of these, in square brackets:

| Label | Meaning |
| --- | --- |
| implemented and source-verified | Read in source on `main` at the cited path and line. "Source-verified" alone is the short form |
| measured diagnostic evidence | A number from a retained report. It keeps that report's status; none is a `PASS` |
| owner requirement | Stated by the owner in the task brief or on an issue |
| proposed design | Decided in this set; not implemented |
| unresolved question | No decision or no evidence; usually an owner question in [08 §7](08-decisions-provenance.md#7-questions-only-the-owner-can-answer) |

## The recommended architecture

[proposed design]

```text
   host (macOS)                                  sandbox container (Linux)
   store host: storage + history                 daemon: FUSE + Workspace + content + overlay
   one owner thread                                 |
        |                                           +-- ws-1.db   one SQLite file per Workspace
        v                                           +-- ws-2.db   names, inodes, payload extents
   store.sqlite  GLOBAL STORE  <------ bridge ----- base client: objects by identity, cached
   immutable objects and history                    Commit: finalized objects -> host Save

   view = active generation over captured generation over committed base
```

1. **The global Store stays on the host**, opened writable by one process. The
   daemon reads the committed base by fetching canonical objects by identity,
   authenticating each one, and caching them under their immutable identity.
2. **Each Workspace has its own SQLite file** in the container, with one
   connection, an in-memory rollback journal and no sync. There is no
   write-ahead log, so no checkpoint exists.
3. **A mutation is one short SQL transaction** on the affected rows, then the
   reply. It never reads base payload, never constructs or publishes, never
   scans the file or the Workspace.
4. **Payload is stored as extents** of exactly the bytes written, replaced in
   place on rewrite. Truncate to zero swaps the stream.
5. **Commit captures with one statement.** Later writes go to the next
   generation. Construction runs on its own thread in the daemon from the
   captured rows and streams finalized objects to a Save on the host. The
   history transition is conditional. A known success installs with one
   statement.
6. **At most two versions of anything are live.** A Commit that does not
   succeed folds its captured rows back before returning.
7. **The mount uses the owner-promoted cached profile**, made correct by one
   invariant: the view changes only through the kernel or through a daemon
   operation that notifies the kernel.

## Consequential decisions

Each reverses or replaces something a prepared document marked decided. Detail
and evidence: [08 §3](08-decisions-provenance.md#3-decisions-of-this-design).

| Decision | Replaces |
| --- | --- |
| One overlay file per Workspace (K1) | One database per daemon with Workspace-prefixed keys |
| Construction in the daemon, storage and history on the host, joined by new bridge operations (K2) | Engines wired in the daemon; a control-only bridge |
| In-memory journal, exclusive locking, one connection, no sync (K3) | WAL with reader connections and explicit checkpoints |
| Byte-exact extents with in-place overwrite; a write never reads the base (K5) | A fixed 4 KiB block grid with copy-up |
| Fold after a failed Commit; two live generations at most (K8) | One extra layer per failed attempt |
| Open-unlinked retention confined to the inode (K9) | A single retire floor |
| Save finish, stage and transition as separate calls; the stage is discarded by exact token on conflict (K15) | "One conditional transaction" |
| `Uncertain` defines no resolution until the owner rules (K16) | A lookup rule that was never adopted |
| Mutations wait; nothing returns `EBUSY` for contention (K13) | Refusal-based coherence |

## Limitations removed

Traced to source in [02 §10](02-base-overlay.md#10-limitation-inventory).

| Removed | Where it was |
| --- | --- |
| Per-file edit and piece counts (4,096; 8,193) | Root reference engine |
| The emergent edit cap between 8,192 and 10,240 writes, which refused a Commit after it was published | Phase 4.5 reconcile budget |
| Page-file creation, read-back and hashing on every mutation | Phase 4.5 private backing |
| The WAL check and inline checkpoint after every write | #305 prototype |
| One 8 MiB memory budget shared by all Workspaces | Phase 4.5 |
| 128 open handles; 32 captures; index depth 7 | Phase 4.5 |
| The 256 MiB cap on Commit input | Bridge prepared stream |
| The 4 GiB file constant on the Workspace path | Bridge contract (pending O-11) |
| `EBUSY` for overlapping callbacks; one base read at a time per daemon | Phase 4.5 |
| One Workspace, one Exec, one control session per daemon | Daemon |

## Limits that remain

Real limits, listed with their kind in
[02 §11](02-base-overlay.md#11-what-grows-with-what).

- **Cluster one format:** names of 255 UTF-8 bytes; file, directory and symlink
  only; mode and mtime only (no ownership, no ctime); hard links on regular
  files only; 16 MiB per canonical object; inode serials up to `i64::MAX`.
- **Cluster one refusals still present:** a heavily fragmented edit of one file
  can exceed `EDIT_DEFERRED_LIMIT` at Commit; one directory's changed names
  must fit in memory at Commit; a hole is committed as zeros.
- **Resources:** the disk quota per Workspace, the configured counts
  (Workspaces, Execs, upstream connections), cache budgets, the commands'
  descriptor limit.
- **Engine and platform:** SQLite's database size, 64-bit offsets, 128 KiB per
  FUSE request.

## Proposed performance targets

Proposals for the owner to freeze prospectively; none is a result. Full list
and qualification shape:
[03 §8](03-mutation-hot-path.md#8-proposed-targets).

- **T1** A mutating request is acknowledged after exactly one overlay
  transaction and zero checkpoints, sync calls, file creates, bridge calls,
  kernel notifications or maintenance steps.
- **T2** Statement ceilings per request: sequential write 2 with the append
  hint and 5 without; create 4; unlink 4; rename 8; truncate to zero 3.
- **T3** Statements and pages per write are flat against the write index up to
  100,000 writes to one file.
- **T4** Zero `EBUSY` with four Execs writing and with a writer running through
  a whole Commit.
- **T5** A mutation waits behind at most one bounded step during Commit and
  retirement.
- **T6** Exec with the overlay within a frozen factor (proposed 1.25) of a
  passthrough under the same mount profile, in matched arms.
- **T7** Fixed cost of a tool call against the 100 ms reporting line.

## What the evidence does and does not support

[measured diagnostic evidence]

- No retained cell of #305 or #306 exercised product code or this design.
- The owner promoted the cached mount profile as a candidate; it is not
  implemented or qualified.
- Stage B of #305 failed its required verifier and Stage C was not run.
- #306 compares different architectures under different caps and is not a
  ranking.
- Every number is one observation with no residency proof: `INELIGIBLE`.
- The evidence supports removing requests. It attributes no time to SQLite,
  base acquisition or construction.

## Questions for the owner

Seventeen questions are in
[08 §7](08-decisions-provenance.md#7-questions-only-the-owner-can-answer), each
answerable in one line. Five block the first slices:

| # | Question |
| --- | --- |
| O-1 | Amend the permanent benchmark hosting rule so the overlay SQLite, and canonical construction, may run in the container? |
| O-2 | Placement: construct in the daemon with host-side validation, or construct on the host? |
| O-3 | Must a Workspace survive a daemon process crash? |
| O-4 | May an unknown history outcome be settled by exact reads of the stage and the Commit? |
| O-12 | Who owns the cluster one prerequisites now that #302 is closed? |

## Not done

- No implementation was started.
- No benchmark was run and none is admissible until O-1 is answered.
- Nothing was compiled: the Linux build of `layerfs-content` and of a bundled
  SQLite is unverified.
- The untracked #303 plan and #304 study in the cluster two worktree were read
  and not modified.
