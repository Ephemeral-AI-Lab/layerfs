# 04 — Concurrency and Commit

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. Cluster one signatures quoted here were read at that
> commit; the workflow built on them is a proposal. Claim labels are defined in
> the [entry point](README.md#claim-labels).

## 1. Concurrency model

[proposed design, implementing owner requirements R4–R7]

| Actors | Meet at | Rule |
| --- | --- | --- |
| Several Execs in one Workspace | `Workspace.core` | Requests wait for the mutex. Each mutating request is one transaction. Nothing is refused for contention |
| A Commit and the Execs of its Workspace | `Workspace.core`, for capture, install and bounded reads of captured rows | The Commit never holds the mutex across construction, a bridge call or a wait |
| A second Commit request for the same Workspace | The Commit slot | Refused at once with a typed result (§10) |
| Workspaces of one daemon | Nothing on a request path | They share the disk, the base cache budget, the upstream pool and the store host |
| Commits of different Workspaces | The store host's queue | Interleaved one accepted object or one history call at a time. No daemon-wide and no host-wide lock spans a whole Commit |
| Daemons committing to one Branch | The conditional history transition | One is published, the others conflict |

No transaction, in the overlay or in the global Store, spans command execution,
construction or transport. [source-verified for the Store side: "a metadata
transaction is short and never spans a caller's upload, C1 construction or C2
save completion", `core/crates/layerfs-history/src/catalog.rs:12-15`]

## 2. State machine

[proposed design]

```text
                         commit request, overlay has changes
        +------+   admit    +-----------+  capture   +--------------+
        | Idle | ---------> | Admitted  | ---------> | Constructing | --+
        +------+            +-----------+  one UPDATE+--------------+   |
           ^  ^                                             |           | definite failure
           |  |                                    Save::finish ok      | or cancellation
           |  |                                             v           |
           |  |                                      +-----------+      |
           |  |                                      |  Staging  | -----+
           |  |                                      +-----------+      |
           |  |                              stage token held           |
           |  |                                             v           |
           |  |    Committed / UpToDate            +---------------+    |
           |  +----- install (one UPDATE) <------- | Transitioning |    |
           |          then retire in background    +---------------+    |
           |                                         |           |      |
           |                              HeadMoved  |           | outcome unknown
           |                                         v           v      v
           |                              discard stage     +-----------+
           |                              by exact token    | Uncertain |  captured rows stay frozen;
           |                                         |      +-----------+  the mount keeps working
           |                                         v
           |        failure returned         +-----------+
           +-------------------------------- |  Folding  |  captured rows relabelled into the
                                             +-----------+  active generation, bounded steps

   A commit request with an empty overlay returns NoChanges from Idle and captures nothing.
```

The Commit slot is `Idle` exactly when `ws.frozen IS NULL`. Every state other
than `Idle` refuses a second Commit (§10).

## 3. Capture

[proposed design]

```text
  lock Workspace.core
     if the active generation holds no row:  unlock; return NoChanges
     UPDATE ws SET frozen = active, active = active + 1 WHERE frozen IS NULL
     context := { C = frozen, base binding (Branch, head, base Layer, root R, scope, profile) }
  unlock
```

Cost: one statement, whatever changed. Exactness:

- Every mutating request is one transaction under the same mutex as the
  `UPDATE`. A request is therefore entirely before capture, with all of its
  rows in `C`, or entirely after it, with all of its rows in `A`. No accepted
  request is split across generations.
- The active generation number is read inside each mutating critical section
  and never cached in a handle.
- A `write()` larger than 128 KiB is several FUSE requests. Capture can fall
  between them, exactly as a concurrent reader can observe a partial write.
- Capture includes what the daemon has acknowledged. Dirty pages of a shared
  writable mapping that the kernel has not yet sent are not in it
  ([05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design)).
- Capture does not wait for a busy inode. If a large shrink is half done, the
  captured row already has the new size, and the extents still to be deleted
  lie beyond it.

## 4. Construction from the captured state

[proposed design over source-verified cluster one signatures]

One construction thread per Commit (root `AGENTS.md` §3.8). Its input is the
stable pair: base root `R` and the rows of generation `C`. Nothing written
after capture can reach it, and nothing it reads can change.

```text
  captured rows (keyset pages, one short critical section each)
        |
        +-- changed regular file --> EditSequence + EditSource over its extents
        |                               -> apply_edits(base content root)     or
        |                               -> construct_stream                   (new or fully replaced)
        +-- new symlink ----------> SymlinkTarget -> content root
        +-- changed mode / mtime -> apply_patches / build_attribute_tree -> metadata root
        |
        v
  per-inode roots in the Commit scratch database
        |
  PreparedRows over captured dentry and inode rows + scratch
        |
  update_filesystem  ----------------------------->  candidate filesystem root
        |
  every finalized object --> Save session on the store host (synchronous backpressure)
```

**Reading captured rows.** The thread takes `Workspace.core` for one keyset
page or one extent at a time and releases it before any chunking, hashing or
sending. A mutation waits behind at most one such read. No long snapshot is
held: none is needed, because the rows cannot change.

**File content.** For a captured inode row with size `S`, `lower_len` `L` and a
base file of length `B`:

| Case | Route | Input |
| --- | --- | --- |
| The serial is new (`born > folded`), or `L = 0` | `construct_stream(policy, &capacities, source, &mut sink, scope)` | A `Read` over `[0, S)`: extent bytes where an extent exists, zeros elsewhere |
| Otherwise | `apply_edits(policy, &capacities, &reader, EditRequest { root, edits, source }, &mut sink, scope)` | Edits in ascending order: one `Edit::overwrite(s, e)` per maximal run of adjacent extents below `L`; then, if `L < B` or `S != B`, one `Edit::new(L, B, S - L)` whose replacement is the content of `[L, S)` |

Overwrites keep coordinates unchanged and the final edit comes last, so no edit
reaches into an earlier replacement, as the contract requires
(`core/crates/layerfs-content/src/file/edit/input.rs:1-15`). Earlier history is
never replayed: a block rewritten a thousand times contributes one run.

`apply_edits` reads replacement bytes more than once (a compare pass, then a
construct pass, `file/edit/apply.rs:62-74`) and addresses runs by index. The
captured extents are an immutable, random-access source, so both hold without a
spool. The run list of the file under construction is written to the **Commit
scratch database**, a second SQLite file owned by the construction thread, so
`EditSequence::edit_at(index)` is a point read and the run count is not held in
memory.

**Namespace.** `update_filesystem(&mut FilesystemObjects::new(reader, sink),
&rows, backing)` with a `PreparedRows` implementation:

| Contract method | Answered from |
| --- | --- |
| `directories()` | Keyset scan of captured `dentry` rows, grouped by `parent`, names already in byte order. One `DirectoryUpdate { parent, changes }` per directory |
| `inodes()` | Keyset scan of captured `inode` rows joined with the scratch roots; one `InodeUpdate { serial, value }` per row |
| `new_inodes()` | Captured rows with `born > folded`, in serial order |
| `directory_for(parent)`, `value_for(serial)`, `new_position(serial)` | Point reads of the same rows and of the scratch database |
| `base()`, `scope()`, `root_serial()` | The capture context |

The cursors are replayable passes over immutable rows, as the contract requires
(`filesystem/rows/source.rs:1-6`).

**Base objects during construction** come from the base client. Objects this
Commit has already emitted are inserted into `BaseCache` as they are sent, so
later steps find them locally; a miss is read through the Save session, which
sees its own pending objects (`impl AuthenticatedObjects for Save`,
`core/crates/layerfs-storage/src/save/provider.rs:90-94`).

**Construction policy** (`storage.policy().construction()` and its
`capacities()`) is fetched from the store host once per daemon and validated.
The daemon never chooses a cutoff or a chunking profile itself.

**Unverified, to settle in slice S10:** how `update_filesystem` treats a name
bound to `None` that the base does not bind, and whether removing a base inode
needs anything beyond unbinding its last name.

## 5. Save, stage, transition, install, retire

[proposed design over source-verified signatures]

| # | Step | Where | Call | Overlay lock |
| --- | --- | --- | --- | --- |
| 1 | Admit, capture | daemon | §3 | one statement |
| 2 | Open the Save | store host | `Storage::begin_save()` | none |
| 3 | Construct | daemon | §4; objects to `Save::accept` | one page or extent per read |
| 4 | Finish | store host | `Save::finish()`; on error `Save::take_failure()` | none |
| 5 | Stage | store host | `stage_changes(&StageRequest { workspace, branch, expected_head, expected_base, expected_root: R, construction_base_root: R, intended_commit_base, candidate_root, profile, scope, generation: C })` → `StageRecord` with its token | none |
| 6 | Transition | store host | `commit_staged(&CommitStagedRequest { workspace, token })` → `Committed(CommitRecord)` or `UpToDate { head, root }` | none |
| 7 | Install | daemon | one critical section, below | one statement plus one per open unlinked inode |
| 8 | Retire | daemon | maintenance steps ([03 §7](03-mutation-hot-path.md#7-maintenance)) | one bounded step at a time |

Steps 4, 5 and 6 are three separate bridge calls on purpose: a lost reply then
identifies which step is in doubt (§6).

**Five completions that are not the same thing.**

```text
  construction complete   the candidate root exists; objects were accepted by the sink
  Save complete           Save::finish succeeded; the required objects are stored
  history published       commit_staged returned Committed or UpToDate
  overlay installed       ws.folded advanced; the base binding is R'
  retired                 the captured rows are deleted
```

Each implies the ones above it and none of the ones below. There is no
checkpoint completion in the overlay. `Handles::checkpoint` on the store host
is a host lifecycle call and is not part of a Commit.

**Install.**

```text
  lock Workspace.core
     for each open unlinked inode with a captured row:      (bounded by open files)
         UPDATE inode SET pinned = 1 WHERE ino = ? AND gen > folded AND gen <= frozen
         remember the base content it inherits from (§7)
     UPDATE ws SET folded = frozen, frozen = NULL
     base binding := (head = record.id, root = R', …)
  unlock
```

One statement moves every reader from "captured rows over `R`" to "`R'`". The
two are the same content by construction, with the same inode numbers, sizes,
modes and times. Rows of the active generation need no change: they describe
differences from the layer below, and that layer has the same content before
and after. No kernel notification is needed.

A request that planned a read before install and fetches base ranges after it
is still correct. Its overlay bytes were copied inside its critical section,
and its base ranges name immutable content roots.

## 6. Outcomes

[source-verified for how each appears; proposed for the overlay reaction]

| Outcome | How it appears | Published? | Overlay reaction |
| --- | --- | --- | --- |
| **Committed** | `CommitStagedOutcome::Committed(record)` | Yes | Install, retire |
| **No net change** | `CommitStagedOutcome::UpToDate { head, root }`; the stage is already removed | The Branch already has it | Install with the reported head and root; retire |
| **Conflict** | `HistoryError::HeadMoved` inside `WithStage { stage: Retained(..) }` | No | `discard_stage` with the exact token, **first**: one stage row per Workspace exists (`core/crates/layerfs-persistence/sql/sqlite/history.sql:113-114`) and a losing transition keeps it (`core/crates/layerfs-persistence/src/history/commit.rs:122-129`), so otherwise this Workspace could never stage again. Then fold. No automatic rebase |
| **Definite failure before staging** | A `ContentError`; a `StorageError` other than `UnknownOutcome`; `HistoryError::{InvalidInput, Missing, Capacity, Integrity}` from `stage_changes` | No | Drop the Save. Waves it already published stay as unreferenced objects; there is no collector and nothing is deleted on a guess. Fold |
| **Host busy** | `HistoryError::Busy`, or a refused persistence call | No | The store host serialises its callers, so this should not occur. If it does, it is a definite refusal of that step: nothing changed; fold; no loop |
| **Cancelled** | The caller asked | No | Before step 5: stop at the next page, drop the Save, fold. Between steps 5 and 6: `discard_stage(token)` returning `Removed`, then fold. During step 6: not cancellable |
| **Uncertain during the Save** | `StorageError::UnknownOutcome`, or the bridge dropped during steps 2–4 | **No.** History is untouched until step 5 | Fold; report a definite failure. The store host's session is quarantined after a real unknown persistence outcome (`core/crates/layerfs-persistence/src/backend/sqlite/transaction.rs:153-155`) and needs an operator reopen |
| **Uncertain at staging** | `HistoryError::UnknownOutcome`, or the reply to step 5 was lost | No, but a stage row may exist and its token is unknown | `Uncertain`. Not foldable into a retry: a second `stage_changes` would be refused while the row exists |
| **Uncertain at the transition** | `HistoryError::UnknownOutcome`, or the reply to step 6 was lost | **Unknown** | `Uncertain` |
| **Daemon dies** | — | Whatever step 6 did | Every overlay is discarded; the Workspace is lost. If it died between steps 6 and 7 the Branch has advanced and the caller learns that by reading the Branch. A stage row for that Workspace may remain on the host |
| **Store host dies** | The bridge drops | By phase, as above | By phase, as above |

**The `Uncertain` state.** The captured generation stays frozen. The mount
keeps working on the active generation. Further Commits are refused with
`CommitUncertain`. A clean close is refused. Nothing is resent, discarded or
rolled back.

`core/AGENTS.md` says: preserve "exact unknown-outcome refusal; never resend or
delete on a guess". The cluster one handbook says it "does not grant an
uncertain-outcome replay procedure". This design therefore **defines no
resolution**. It records that an exact one is available in the current API and
asks the owner to rule (O-4):

```text
  stage(workspace)  -> present with our token:  the transition did not apply
                       (commit_staged removes the stage in the transaction that advances the head,
                        core/crates/layerfs-persistence/src/history/commit.rs:161-191)
                    -> absent:  commit(CommitId::derive(candidate_root, expected_head, base_layer))
                                   present                        -> Committed -> install
                                   absent, candidate_root == R    -> UpToDate
                                   absent otherwise               -> the stage was removed elsewhere
```

These are two reads. Nothing is resent and nothing is deleted. Until the owner
permits them, an `Uncertain` Workspace can be used and force-closed, but not
committed.

## 7. Open-unlinked files

[proposed design]

An inode with no name left and at least one open handle keeps its rows. The
lifetime signal is the per-inode open count maintained by OPEN and RELEASE; a
file mapped after `close` still holds its kernel file reference, so RELEASE
arrives only when the mapping goes.

| Moment | What happens |
| --- | --- |
| Last name removed while open | The inode row at the active generation is kept with `nlink = 0`, its stream and its `lower_len`. Reads and writes through the handle are ordinary |
| Last name removed while not open | The row and its stream are released at once ([02 §6](02-base-overlay.md#6-names-and-inodes)) |
| Capture while it is open and unlinked | The captured row has `nlink = 0`. The Commit removes the inode from the namespace, or never adds it if it was new. A later write through the handle makes a new row in the active generation |
| Install | The captured rows of every open unlinked inode are marked `pinned`, so retirement skips them. The daemon remembers, in memory, the content root and length of the base file those rows inherit from, because the new base no longer contains the inode. Old roots stay readable |
| Read after install | Active row, then the pinned row, then the remembered base content |
| Unlinked **after** capture | The Commit published the file as linked. After install the new base contains it, the active row has `nlink = 0`, and no pin is needed |
| Last close | Its rows in the active generation and its pinned rows are deleted; their streams go to `reclaim`. A captured row of a Commit still in flight is left for retirement or fold: captured rows are never deleted under a running Commit |
| Fold | Relabelled like any other row |

Retention is confined to that inode. One long-lived deleted-but-open file does
not hold back the retirement of anything else. The number of such inodes is
bounded by the number of open files.

Handle-free operation would remove OPEN and RELEASE, and with them this signal.
It then needs exact kernel lookup counting instead, which is why it is not in
the first slice ([05 §7](05-fuse-assessment.md#7-optimization-disposition) row 9).

## 8. Fold after a Commit that did not succeed

[proposed design]

After a conflict, a definite failure or a cancellation, the Workspace holds two
generations: `C` with the captured changes and `A` with what was written since.
Both are the Workspace's own uncommitted state. Before the failure is returned
to the caller, the Commit thread **drains `C` into `A`** in bounded steps:

| Captured row | Action |
| --- | --- |
| A name with no row in `A` | Relabel: `UPDATE dentry SET gen = A` |
| A name that `A` also has | `A` wins. Delete the captured row. If `A`'s row is a whiteout and the captured row had `below = 0`, the base does not bind the name: delete both. Otherwise `A`'s row inherits the captured row's `below` |
| An inode with no row in `A` | Relabel the row. Its stream is not touched |
| An inode that `A` also has | Merge into `A`'s row: `lower_len = min` of the two; captured extents at or beyond `A`'s `lower_len` are dropped; the smaller stream's surviving extents are replayed onto the larger with the ordinary write path, where `A`'s bytes win; the emptied stream goes to `reclaim`. The inode is marked busy for the merge |

Then `UPDATE ws SET frozen = NULL`.

- Each step is one transaction that leaves the view unchanged, so mutations and
  reads interleave freely.
- It terminates under continuous writes: no new row can ever appear in `C`.
- Work is one small-row relabel per captured key, plus a payload merge only for
  files written in both generations, proportional to the smaller side.
- The stream indirection is what makes the relabel cheap: no payload row
  carries a generation.

This is what keeps "at most two versions per key" true after any number of
failed attempts. The prepared design left one extra layer per failed attempt.

A Workspace in `Uncertain` does not fold: its captured generation may have been
published.

## 9. Worked interleavings

[proposed design]

**Repeated writes to one block across a Commit.**

```text
  t1  write X = A          extent in stream s1 (generation 1)
  t2  write X = B          same extent, in place                 s1: B
  t3  capture              frozen = 1, active = 2
  t4  write X = C          inode row at generation 2, stream s2  s1: B   s2: C
  t5  write X = D          in place in s2                        s1: B   s2: D
  t6  construction reads X from s1                               -> B
  t7  install R'           folded = 1; base(R') has X = B
  t8  retire               s1 deleted                            s2: D   view: D over base(R') 
```

Two versions of X exist from t4 to t8, never more. The Commit published B, the
bytes present at t3.

**Truncate racing a write, two Execs.**

```text
  Exec 1: truncate(f, 0)        Exec 2: pwrite(f, 4096 bytes at offset 8192)
```

Each is one transaction, so there are two outcomes. Truncate first: size 12,288
with a hole below 8,192. Write first: size 0. A capture between them contains
exactly the first. If the truncate is a large shrink in several transactions,
the write waits on the inode's busy flag and sees the finished shrink.

**Rename racing unlink.** `rename(a, b)` and `unlink(a)` from two Execs: either
the unlink removes `a` and the rename returns `ENOENT`, or the rename moves it
and the unlink returns `ENOENT`. A rename is one transaction, so a capture
never sees `a` gone and `b` not yet present.

**Open-unlinked across a Commit.**

```text
  t1  open(tmp); write 1 MiB        rows at generation 1
  t2  unlink(tmp)                   nlink = 0, rows kept (open count 1)
  t3  capture                       the Commit excludes tmp
  t4  write through the handle      new row at generation 2 over the captured one
  t5  install                       captured row pinned; everything else of generation 1 retired
  t6  read through the handle       generation 2 extents, then the pinned row
  t7  close                         both rows deleted; streams reclaimed
```

**Metadata change during a Commit.** `chmod` at t4 writes an inode row at the
active generation. The Commit publishes the old mode. After install the active
row lies over the new base and reports the new mode, which the next Commit
publishes.

**Reads during install.** A read that began before install planned against
"captured over `R`", one that begins after against "`R'`". Both return the same
bytes.

**Write to a file while the Commit is constructing it.** The write goes to a
new stream in the active generation. Construction reads the captured stream,
which no writer can name.

**Two Workspaces committing to one Branch.** Each constructs and saves
independently. The second `commit_staged` gets `HeadMoved`; that Workspace
discards its stage, folds, and reports a conflict.

## 10. Second Commit, close and unmount

[proposed design]

**A second Commit request while one is in flight** is refused at once with
`CommitInFlight { phase }`, or `CommitUncertain`. It is never queued and never
starts a second capture: cluster one admits one stage per Workspace, a queued
Commit would capture at a moment the caller did not choose, and the two-version
bound depends on it. A caller that wants "commit again when done" waits for the
first result and calls again; the new Commit then contains everything written
meanwhile.

| Request | Active Execs | Open handles | Commit in flight |
| --- | --- | --- | --- |
| Unmount | Refuse `Busy { execs }` | Wait for in-flight requests; event-driven, no polling loop | Allowed. A Commit does not need the mount |
| Clean close | Refuse `Busy { execs }` | Refuse `Busy { handles }` | Refuse `Busy { commit phase }` |
| Forced close | Terminate the Execs; wait for in-flight requests to return | Handles die with the mount | Before staging: cancel and drop. Staged: `discard_stage(token)`. In the transition or `Uncertain`: the caller must acknowledge that the outcome is unknown; the close then discards local state only and touches nothing on the host |
| Then | Unmount; close the connection; unlink the database file | | |

Whether a close with uncommitted changes discards or refuses is a product
policy (owner question O-14).
