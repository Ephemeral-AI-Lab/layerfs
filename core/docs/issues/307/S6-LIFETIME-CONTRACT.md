# S6 lifetime and maintenance algorithm

> **Status:** Selected implementation contract after S5 `a0dc7da9b`; not
> implementation, release or qualification evidence.

Definite, fenced failure releases the one namespace capture by a metadata
transition into a consolidation domain. Unknown history disposition retains the
capture and never enters this API. Keep only the active and this one lower
namespace domain; the next capture waits for consolidation readiness. Ordinary
mutations remain runnable while it waits. A bounded maintenance job transfers
lower name finals and effective cells into active state, preserving newer finals,
validity, shrink stamps, cutoffs and absolute `born` values. It does not mark a
file busy or merge its payload in the failure reply. One transferred cell is
at most 4096 bytes, atomic with its exact source deletion/reference advance.
Final cutoff composition occurs only after all lower cells are processed.

Open and lookup references are independent indexed lease rows. At last unlink,
create one independent orphan inode/domain (-1), an exact immutable base root,
and at most the existing two lower namespace domains. Later descriptor writes
use only that stable orphan domain and never join another namespace capture.
Maintenance composes the fixed lower domains into it, newest first; namespace
retirement cannot remove a referenced source. Lower references advance only
after every cell has been incorporated. Thus an orphan's depth is at most three
while migrating and one afterwards, independent of successful or failed Commit
count. A captured pre-unlink reader independently holds its own generation/root.
It can delay physical deletion, without extending the live orphan chain.

Minted open and captured-reader capabilities bind engine/namespace/owner/serial
or fixed generation/root/floor. Every use/release validates the exact SQL lease.
Base-source windows remain short and distinct from descriptor lifetime. Last
open/lookup/reader/operation release updates targeted eligibility; no whole-owner
or whole-namespace collection occurs. Removed namespace rows remain tombstones
until their base cannot bind the serial; orphan payload and scratch are deleted
only after last independent custody. Native FORGET/RELEASE mapping remains S8.

Maintenance uses backed ready items, indexed generation/keyset domains and
bounded weighted jobs: at most 14 cells (~64KiB), 64 small metadata/step records,
or one bounded transfer. Stale-cell work begins at the shrink boundary; later
lower boundaries rewind that cursor only into newly discarded work. Abandoned
steps use depth>height, and exact epoch/staleness is rechecked before deletion.
Retirement skips independently referenced rows; their releasing owner owns
targeted cleanup, without restarting a namespace scan. One failed maintenance
attempt is retained and stops automatic maintenance; no automatic replay.
The owner guarantees a maintenance turn after a bounded number of foreground
jobs and continues when idle. Queue readiness distinguishes consolidation from
the finite capture publication fence, so long maintenance does not freeze logs.

Every mutable job must admit/reserve actual physical capacity before its SQL
attempt, preserve a cleanup/transition headroom reserve, and report allocation,
freelist, reserved bytes, debt and definite/uncertain outcomes separately.
Physical capacity is shared by the daemon; per-Workspace logical attribution
does not imply exclusive pages. Reservation failure has no publication effects;
SQL FULL remains a one-attempt definite failure when rollback is known, and
unsafe SQL/COMMIT uncertainty retains/quarantines the exact state. No sync is
added to disposable backing. Real device ENOSPC is a separate proof from page
quota FULL. Reservation implementation and its conservative arithmetic must be
documented from the actual platform primitives before claiming this row complete.

For K transferred/reclaimed cells/rows, cumulative work is O(K indexed-key work
plus cell bytes and logarithmic staircase checks), in bounded service windows.
Capture/failure/install/last release do fixed ownership work rather than a
payload-sized fold. New captures apply readiness/backpressure rather than add
layers. Live source depth and foreground work have no Commit-count dimension.
There is no total file/edit/workspace/operation/time cap. Sustained service and
physical debt require actual progress/admission proofs, not an assumed rate.
