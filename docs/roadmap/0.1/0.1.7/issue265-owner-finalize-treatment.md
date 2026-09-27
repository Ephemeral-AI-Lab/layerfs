# Issue 265: finish new-page ownership with its first ledger write

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Prospective owner-publication, failure and count-evidence treatment. Commit
> before the owner source change, runner change or new public diagnostic.

The retained balanced-source append/dispersed/repeated 100-write receipts at
`ebfa8f242` are the control. They have one public Mount, one generic-shell
Exec with exactly 100 one-byte FUSE callbacks, one explicit Commit, a separate
full old/new-head oracle and cache-`INELIGIBLE` raw latency. Their final ledger
read/write counts are 2,647/1,470, 3,156/1,756 and 3,059/1,489. Do not
resample these arms or promote their raw timing.

## Mechanism and failure ordering

`RootOwner::write_raw_page` and `write_page` currently write the new page's
ledger owner with `edges=false`, advance its child or Local custody edges, then
read and write the same owner to set `edges=true`. The last authenticated
4 KiB read and write are paid once per completed page. The owner is private
to the unfinished candidate until all edges succeed and the candidate seals.

Declare `edge_progress=(page,0)` **before** the first owner ledger write, and
write that owner with `edges=true`. During edge advancement, retain the
existing exact acknowledged prefix in `edge_progress`; clear it only after
all edges have succeeded. Cleanup already uses this prefix ahead of the
`edges` flag, so a failure after zero, some or all edge updates releases only
the advanced references. A definite failure before any edge is advanced
retains the page and its prospective slot under the existing pending/temporary
owner. An uncertain mutable ledger write still quarantines the arena. No
physical format, checksum, ledger identity, path identity, page identity,
quota, G1/G2 custody or error-driven fallback changes. Root publication stays
atomic, and one construction worker remains selected.

The keyed-page route must validate/extract its edge list before first setting
the owner, as the extent raw-page route already does. This prevents an
invalid keyed edge list from leaving a completed-edge flag without a usable
body. Both routes then use the same initial-owner ordering.

## Evidence and decision

Add one bounded, saturating product counter for successful new-page owner
finalizations. Export its cumulative value alongside the existing
25/50/75/100 backing snapshots; it must do no extra page I/O. One completed
new page predicts exactly **one fewer 4 KiB ledger read and one fewer 4 KiB
ledger write** than the retained balanced source. Reconcile the observed
per-checkpoint differences with this counter, separately from extent leaf,
branch, keyed metadata and cleanup edge counts. If the equation fails,
preserve the row and diagnose from its receipts; do not resample it.

Reuse the same sealed writer and closed 10 MiB master by validated independent
byte-copy clones in this worktree. Rebuild affected host SDK/verifier and
aarch64 daemon from locked Cargo release, seal binary/image/source/workload
identities, use only the worktree-local target and run lock, and keep one
append/dispersed/repeated attempt each at the new frozen source. The
15-second complete command, 9-second independent verifier, one construction
worker, cache contract and all 25/50/75/100 checkpoints stay fixed. Retain
every FAIL/INCOMPLETE/INELIGIBLE receipt. Raw Exec/Commit/complete walls may
guide diagnosis but cannot establish speed under the uncontrolled cache.

Functional admission requires exact old/new bytes, parent, path inventory and
final changed runs 1/100/1; 100 observed FUSE WRITE callbacks and four
upstream calls per case; quota/accounting and clean Unmount/Delete; and
old-root/G1/G2 custody through a focused native ext4 success proof. If the
existing 64 MiB native failure-injection route cannot fit the under-30 s
focused-test bound, record it as NOT_RUN and leave failure-path custody open.
The #248 mounted 4,097-write gate remains separate and NOT_RUN until #266's
FUSE refusal and #249's timer are resolved. No deadline, cache, workload or
worker relaxation may turn a miss into a pass.
