# Ordinary namespace operations

> **Status:** Implemented S4 checkpoint after `8d691ab8a`; S5 byte streams, S6
> orphan/lifetime/pressure ownership and S8 native kernel adaptation are separate
> unfinished milestones. Not release or performance qualification.

S5 update after `f5558fc22`: the same ordinary owner job now publishes bounded
`Write` windows and `SetAttributes { size }` through the engine's payload layers.
Append chooses the current EOF in that job. [Payload streams](31-payload-streams.md)
owns truncate/regrow, inherited reads, holes, updated work and remaining S6/S8
custody. Schema v8 inode rewrites maintain cutoff/epoch/height internally.

Workspace performs create, mkdir, symlink, link, unlink, rmdir, rename, chmod and
utimens over stable inode serials and checked names. Each operation publishes all
of its inode and name finals in one overlay transaction with one reply-attempt
ticket, or publishes nothing. Source: [operations](../../crates/layerfs-workspace/src/operations/types.rs),
[owner job](../../crates/layerfs-workspace/src/mutation/job.rs),
[driver](../../crates/layerfs-workspace/src/mutation/driver.rs),
[engine compound job](../../crates/layerfs-overlay/src/namespace/compound.rs).

## Where each decision is made

```text
 caller thread (native request / SDK)             SQL owner (one job at a time)
 ------------------------------------             ------------------------------
 acquire base source  ------------------------->  source row, base fenced
 reserve serial (local range; refill = 1 call)
 round: NamespaceJob{operation, facts} -------->  read CURRENT rows for touched keys
                                                  combine with supplied base facts
        <-- Needs{inode s, name (p,n)} ---------  undecided: nothing written
 fetch those immutable base facts
   (provider/content I/O, outside the owner)
 round: same job + more facts ----------------->  decide: Refused -> nothing written
                                                  or apply(): BEGIN, all finals,
        <-- Applied{publication, inode} --------  one ticket, COMMIT
 reply-send attempt -> ReplyAttempted(ticket)
 release base source
```

Filesystem semantics stay in Workspace; SQL stays in the overlay. The semantic
evaluation runs inside the owner job as data-only Workspace code, exactly like
the existing prepared install: it reads the rows current in that job, so no
earlier snapshot has to be revalidated and two operations in one directory never
conflict falsely. The owner performs no provider, content or kernel work.

A base fact is an answer about the source's immutable root: "serial `s` is this
inode" or "directory `p` binds name `n` to `s`/nothing". Install is fenced while
the source is owned, so a fetched fact cannot become false. Facts only
accumulate. An undisturbed operation over an inherited directory therefore takes
two owner rounds; under a directory created above the installed floor it takes
one, with no base demand. A further round happens only when another completed
operation rebinds a name this one depends on, to an inode whose fact it does not
have yet.

**One attempt.** A round that returns `Needs` or `Refused` has written nothing.
The single `apply` is the operation's only attempt; its error is returned as the
exact outcome and nothing repeats it. Fact rounds are readiness, not replay.
There is no round cap: each extra round is caused by another operation's
successful publication, so the system as a whole progresses, and a cap would
become a contention refusal the product contract forbids.

## Engine boundary

Schema v7 adds two inode columns. `born` is the generation that created the
serial locally, zero for an inode inherited from the base and carried on every
rewrite. `born > installed` means the bound base has no such inode, so its
children, if it is a directory, exist only locally. `entries` is a directory's
exact visible child count. Both are absolute view facts, so known install, which
removes sealed rows from the view without rewriting later ones, leaves them
valid.

`Overlay::apply(source, &Changes)` is the compound job: at most 4 inode finals,
2 name finals and one cell, all at the active generation. It refuses a closed
Workspace and a released source, checks the representable-value grammar, and is
atomic through the existing one-transaction path. A value refused after an
earlier write in the same transaction leaves no row, ticket, revision or count.
The windows bound one job; they are not totals. `Overlay::source_rows` gives the
job consistent point reads: an inode's latest row, and for a name both its
active row and its latest lower row.

R7 update, 2026-10-09 (directory link counts, overlay schema 22): a third
absolute view fact, `subdirs`, is a directory's exact number of child bindings
that are directories (zero for any other kind, by `CHECK` and by the engine's
value grammar). It travels with the whole row through `INODE_PUT`,
`INODE_LOOKUP`, `INODE_CAPTURE`, the orphan select and the fold copy, and adds
no statement to any job: every mutation that changes it already rewrites the
parent's row. `touched` in
[`eval.rs`](../../crates/layerfs-workspace/src/mutation/eval.rs) takes the
kinds of the binding a parent gains and loses and moves `entries` and
`subdirs` together:

| Change | Parent `subdirs` |
| --- | --- |
| mkdir | +1; the new directory starts at 0 |
| rmdir, including the whiteout of a base subdirectory | −1 |
| directory renamed inside one parent, nothing replaced | unchanged |
| directory renamed over an empty directory of the same parent | −1 (the replaced one) |
| directory renamed to another parent | −1 on the old parent, +1 on the new |
| directory renamed over an empty directory of another parent | −1 on the old parent; +1 and −1 on the new |
| create, symlink, link, unlink, any rename of a file or symlink | unchanged |

A count that would leave its range, or exceed `entries`, is the definite
`InvalidRecord("directory child count")`; nothing is clamped. A removed
directory's row has no child, so its count is 0. The first local row of a base
directory starts from the base's derived count, exactly as `entries` starts
from the root-page count ([effective view](29-effective-base-view.md)). Commit
adds nothing to the canonical namespace: the captured row and the count
derived from the root built from it agree by construction, which
[`directory_links_install.rs`](../../crates/layerfs-workspace/tests/directory_links_install.rs)
checks through the real producer and install. The reference count column
`nlink` keeps its meaning. Proofs:
[`directory_links.rs`](../../crates/layerfs-workspace/tests/directory_links.rs)
and the overlay's
[`directory_links.rs`](../../crates/layerfs-overlay/tests/directory_links.rs).

R7 update, 2026-10-09 (statement diet): `SourceRows::name` reads both rows
of a name with one seek of the name's rows (`NAME_LAYERS`, newest first,
at most two rows), and the compound job computes a name's inheritance once
per name instead of once to decide the row and again to write it.
`Changes::created` names the one serial a creating job reserved; it must be
one of the job's live inode finals. See the
[overlay note](19-daemon-overlay.md).

R2 directory custody extension2026-10-08: a cross-parent directory rename also
supplies `Changes::moved_directory`. The same transaction validates its final
binding and updates the indexed retained native parent, if present, without
visiting open handles. File/name changes and same-parent renames omit that work.
See [native directory custody](74-native-directory-custody.md).

R3 native extension 2026-10-08: `NamespaceJob::decide` is the shared decision
of the ordinary and the native publishing job. A mounted cross-parent directory
rename proves ancestry from the connection's retained parent index instead of a
caller-supplied destination path, and `Operation::StoreOpen` is a mapped store
clipped to the current size. See
[native mutation and kernel coherence](77-native-mutation-coherence.md).

A removed name becomes a whiteout only where something below still binds it:
the latest lower local row decides, and only when there is none does the
supplied "base binds this name" fact decide. Otherwise the active row is deleted
and no row is the final state. A file created and removed inside one generation
leaves no name row. Sealed rows are never rewritten.

## Semantics

| Operation | Published finals | Refusals |
| --- | --- | --- |
| create, mkdir, symlink | new inode (`born` = active generation), its name, parent mtime and `entries + 1`; a symlink's target is its one creation cell | Exists; parent Missing or NotDirectory; set-id/sticky bits outside the portable grammar NotPermitted; empty symlink target Missing |
| link | target `nlink + 1` (mtime unchanged), its new name, parent | NotPermitted for a directory or symlink; Missing for an absent or already removed inode; Exists; TooManyLinks |
| unlink, rmdir | name removed, target `nlink - 1` (directory: 0), parent mtime and `entries - 1` | Missing; IsDirectory / NotDirectory; NotEmpty when `entries != 0` |
| rename | source name removed, destination bound to the same serial, both parents, replaced inode's reference dropped | Missing; Exists under no-replace; NotDirectory / IsDirectory / NotEmpty for an incompatible or non-empty replaced inode; Invalid, AncestryRequired, AncestryMismatch for directory moves |
| chmod, utimens | the inode's mode and/or mtime | Unsupported for a symlink mode; NotPermitted for unrepresentable bits; Invalid nanoseconds; Missing |

Renaming a name onto itself, or onto another name of the same inode, succeeds
with no change and no ticket. A moved inode keeps its serial and gets no row;
nothing beneath a moved directory gets a row, because children are found by the
directory's serial in both the overlay and the base. A removed inode keeps a row
with no references so serial-addressed access cannot fall through to a still
existing base inode. [S6 custody](33-independent-custody.md) reclaims unowned
payload live and retains the tombstone until install. Exact descriptor writes
use the separate orphan domain. The root has zero canonical references and is never
removed.

**Emptiness** is the maintained `entries` count. A directory with no local row
has had no membership change, so its count is the base directory's root-page
entry count, one authenticated object. No enumeration, guard or parked request
is needed for rmdir or directory replacement.

**Cycles.** Cluster one stores no child-to-parent binding and Workspace keeps no
resident ancestry. A directory changing parent therefore carries
`destination_path`: the destination parent's names from the root. The publishing
round resolves that path in the current view, at most 256 point steps by the
canonical component limit, requires it to end at the destination parent and
refuses if the moved directory is on it. Directories have exactly one binding,
so that path is the whole ancestry. The evidence is verified, never trusted: a
stale path is AncestryMismatch, an absent one AncestryRequired. Two opposing
moves cannot both publish, because the second one's path no longer resolves.
This is live-view protection; it does not replace the bounded topology evidence
canonical Commit validation still needs (P14).

**Time and permissions.** Callers supply the operation time; Workspace owns no
clock. Portable metadata has one mtime, reported as ctime, so chmod, link and
rename leave the affected inode's mtime unchanged and directory membership
changes set the parent's. Modes follow the canonical grammar: 0777 for files,
0777 exactly for symlinks, 01777 for directories. Access decisions by user
identity are not made here; they belong to the mount's default-permissions
profile and command identity (S8). `namespace_refs` is the canonical reference
count, 1 for a directory and 0 for the root, not a POSIX directory link count;
the kernel mapping is selected in S8. Names outside the canonical grammar,
device nodes, ownership changes and exchange-rename are not representable and
are refused by the adapter before an operation exists.

**Serials.** Workspace allocates from locally held ranges reserved through the
`InodeSerials` port, 1024 per refill, one allocator call outside every lock. The
SDK implements the port over the owning history catalog under per-call
authority. A serial taken for an operation that is then refused is consumed,
never recycled. The refill size is a window, not a limit on created inodes.

**Enumeration.** Listing is the existing bounded three-way name merge, resumed
after the last visited name. A name bound for the whole enumeration is returned
exactly once. A name created, removed or renamed during it may or may not
appear, and a reply consumed only partly resumes after its last consumed name.
A directory created above the floor is listed with no base demand.

## Work and evidence

One compound job reads a fixed set of point rows and writes at most six rows
plus its ticket and frontier. Every statement is a primary-key seek, upsert or
delete: O(log N) B-tree work per touched key, independent of the directory's
size and of the namespace. The cycle proof adds two point reads per destination
path component. Base facts cost the canonical visited path for one inode and one
name. Nothing is resident per name, inode or handle; the only per-Workspace
resident state added is the unconsumed serial ranges.

The retained [engine profile](../issues/307/checks/s4-namespace/compound-profile.log)
and [complete-operation profile](../issues/307/checks/s4-namespace/operation-profile.log)
pair the actual plans with runtime counters at 128, 1,024 and 4,096 sibling
names. Statement counts, VM steps and changed rows are identical at every scale,
with zero full-scan steps, sorts, automatic indexes and re-prepares. These are
deterministic work diagnostics on one host, not latency samples, cache-cold
claims or large-workload qualification. Page, journal, pager and kernel
residency remain S7/S8 evidence. Each complete operation currently also pays a
source acquire and release and a reply-attempt release as separate owner jobs;
consolidating them is a later cost decision, not a hidden saving.

Public tests exercise a real content-built root with aliases and symlinks
through the direct engine and through the real daemon owner with concurrent
threads, a blocked provider and an interleaved capture. See the
[S4 exit audit](../issues/307/S4-EXIT-AUDIT.md) for the criterion map, checks and
remaining obligations.


## Resumable mutation plan, R2 component

[`Workspace::prepare_mutation`](../../crates/layerfs-workspace/src/mutation/plan.rs)
creates an owned `MutationPlan` without SQL, provider I/O or serial allocation.
Creating operations declare `Operation::creates()`; their caller supplies one
serial from the same scope's authoritative allocator. Preparation validates its
shape/range and exact Workspace route, returning the original owned operation on
refusal. Source ownership remains with the caller; constructing/dropping a plan
never acquires/releases the source or guesses a publication outcome.

The plan has Owner, Base and Finished stages. Owner exposes the existing
`NamespaceJob`, whose decision and publication logic is unchanged. `accept`
consumes that original result. Only a nonempty Needs outcome opens the Base stage;
Applied, Unchanged, Refused and original service failures finish it. Base `supply`
uses the same existing immutable fact routine, with the exact retained source,
and no SQL. A failed base demand keeps input, accumulated facts and needs in the
finished plan. The caller receives the original error and retains it alongside
that plan. No later job is exposed after terminal success or failure.

The synchronous `Workspace::mutate` now drives this same plan. A native executor
can park separately around its original SQL pending handle and its immutable-read
admission before calling supply. It must still supply bounded request credits,
healthy reader admission, original error/completion custody and actual source/
reply disposal. The plan is not an executor and does not make existing blocking
ports safe for native workers. Existing fact-cache/ancestry behavior is unchanged;
this component does not establish arbitrary-depth topology qualification.

[Component results](../issues/307/checks/r2-mutation-plan-20261008/18-results.md)
cover preparation/owner demand separation, interleaving before the deciding round,
one publication, original failures and retained input after a base/source refusal.
