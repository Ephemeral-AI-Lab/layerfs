# Issue #192 optimization decisions

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This record selects the implementation after `0819f3f39833d477d9ed6d878a50691c3c046a83`.
The [source checkpoint](evidence/optimization-selected-checkpoint-20260920.json)
binds the inspected C2 source, recovered edits and unchanged dependency lock.
It implements the direction in [the optimization specification](optimization-spec.md).
Selections below are design decisions, not proof that the source implements them.
The original receipt directories remain immutable. Multiple writers and accepted
duplicate encoding/bytes are requirements, not options.

## D01 — save-owned catalog and compatibility

Select candidate schema 7, with the existing application identifier and canonical
profile 1. Opening schema 6 or another schema fails explicitly; no migration,
promotion, downgrade, or mixed-binary support is introduced. Schema creation and
shape validation change together. Preserve MEMORY journal, synchronous OFF, zero
busy timeout, and the current codec/identity formats.

`saves` owns a monotonic, never-reused save ID, an optional active slot, and a
publication sequence. A private save has an active slot and no publication;
publication replaces the slot with NULL and sets a sequence in one transaction.
`objects` uses `(object_id, save_id)` as its primary key. `object_packs`, metadata
value groups and persisted candidate hints carry or derive their save owner.
Foreign keys and save-scoped indexes support bounded lookup and deletion.

An ordinary read sees published saves. A save sees the publication snapshot taken
at acquisition plus its own rows, including its own earlier committed batches.
No other private save supplies logical references, delta bases, pooled values or
exact-reuse winners. Every dependency acquisition uses the same scope. A pack
ceiling remains only a range check; it is never publication authority.

Select two simultaneous private save slots per Store. A failed cleanup or unknown
outcome keeps its persisted slot occupied; ordinary reopen preserves that state
and published roots. It does not guess cleanup or resume a save. Slot exhaustion
refuses new saves. The service's admitted writer limit cannot exceed this bound.

Duplicate locator metadata is accepted along with duplicate physical bytes. With
two active slots, at most two valid locators for one ObjectId can arise: before
the first publication only the two active saves can insert; after it, new saves
see an eligible winner and compare/reuse it. Saves already active at that moment
can still finish their private copy. A failed save retains its slot until its
locators are gone. Lookup refuses an over-bound catalog rather than scanning an
arbitrary number of candidates. Choose the oldest eligible locator deterministically.

## D02 — allocation and cleanup

Keep monotonic pack and metadata-ordinal allocators in the policy row. Reserve
IDs only under a bounded database transaction; never use a stale MAX+1 cursor.
Pack tails belong to one save. A writer can append only to its own private pack.
No writer modifies a published pack or another save's tail.

Reserve the exact fresh-value count for one inode leaf before encoding its value
groups. Reservations may leave ordinal gaps on failure. Ordinals are never reused.
The pooled candidate index walks visible catalog groups in ordinal order and
validates non-overlap, widths, digests and ordinal membership; it must no longer
assume a contiguous global cursor. A save's fixed publication snapshot prevents a
late foreign publication below its cursor from becoming a missed dependency.

Persist the current pooled-window start and reserved-value count with ordinal
allocation. Advance this window by whole value groups before exceeding 131,072
reserved values; failed/private reservations also consume window capacity. A cold
index scans only this indexed ordinal interval, then filters by save visibility.
There can be at most 131,072 one-value groups in that interval, even when private
rows are excluded. This removes the unbounded history scan without assigning
foreign private values to a writer. Gaps/overlap may reduce reuse; exact canonical
values and the existing bounded ordered-set lookup remain authoritative.

Definite abort rolls back the current transaction, then deletes only that save's
locators, hints, value groups and packs in bounded pages. The last transaction
removes the private save row and releases its slot. Failed cleanup remains failed
and retains ownership. Unknown commit/rollback outcomes quarantine the save:
no replay, second cleanup, deletion, or presumed publication occurs.

## D03 — bounded database arbitration and collision proof

Use one shared in-process arbitration mutex per native Store file identity, also
used by independent handles/read sessions for that file. Bound its registry and
remove expired weak entries. The lock protects database access and short native
read waves or write transactions, never upload waits, an entire save, construction,
full encoding or prefix trials. At most the admitted operations wait at this seam;
there is no separate work queue, SQL retry loop or SQLite busy handler. External
process contention still fails on the single SQLite acquisition attempt.

Compression and group construction happen before database write ownership.
Placement, allocator updates, pack bytes and locator rows are written together in
a transaction whose declared row/byte cost is checked before mutation. Reads and
bounded canonical collision validation may occur inside that transaction. No
transaction waits on a socket or user-supplied input.

At insertion, inspect all existing bounded candidates for each identity. A foreign
private candidate may establish a collision comparison, but never reuse or a
dependency. Reconstruct and authenticate both candidates under their respective
save scopes and compare exact canonical bytes and role/length. Unequal bytes,
corruption and failed reads abort the new save; they are not an alternate encoding
path. The arbitration lock orders comparison plus insertion, closing the race
where two preparations both observed absence. Identical private candidates retain
their independent ownership. This is object admission, not mutation replay.

Each save owns its content-candidate and pooled-value indexes. Persisted content
hints remain a bounded slot table and carry save ownership. Write changed hints in
bounded batches before publication; only published eligible hints seed another
save. Overwriting a hint can lose a future compression candidate, never content or
authority. Abort removes only hints still owned by that save. No operation-wide
map or global chunk reservation service is introduced.

Final publication validates that all physical groups and bounded hint batches are
finished, advances the publication sequence, and flips the one save row. Its work
does not scan or merge the save's object rows. Admission-time dependency checks,
immutable published dependencies and same-save ownership establish graph closure.
At most two policy/save rows change in this final transaction.

## D04 — load and memory selection

The proposed first large-input selection is 4 GiB per construct/read, four native
sessions and two admitted operations/writers, one construction producer per
operation. Metadata remains 32 KiB, body frames 16 KiB, edit replay 8 MiB and
edit count 256. Overall duration is separate from the five-second no-progress
budget; select a 600-second maximum requested overall budget for this profile.
The client may request less. These prospective values do not extend any existing
functional verification timeout or establish a qualified maximum file size.

Derive the input and result frame-count budget from their declared byte counts:
256 boundary frames plus ceil(bytes / 1024), plus one terminal frame. Reject
empty data frames and enforce exact final totals. This bounds tiny-frame work
without the unrelated 8,192-frame ceiling. Apply the same grammar to local stdin,
encrypted upload and result delivery. No per-frame ACK is added.

The [resource profile](10-resource-profile.md) freezes C/W, load/time limits and
simultaneous application ownership as a vector of byte/count ceilings. It replaces
the earlier socket-size admission assumption. Its OS/native observations and
O06/O10 qualification remain separate from the structural admission decision.

## D05 — caller, deadline and session ownership

Keep the Noise KK selection, pinned keys, request version and five operations.
Opaque `VerifiedPeer` belongs to `bridge::contract`; trusted native handshake or
private-key direct entry constructs it. A public key/request field cannot mint it.
Both direct and remote delivery enter the same authorization/admission handler.

The daemon starts one absolute deadline after local BEGIN decoding and passes it
through pipe I/O and the client call. The client sends a bounded remaining duration.
The service starts its own absolute deadline after remote BEGIN decoding and
passes that exact value to framing and the handler. Instants never cross machines;
transit time is paid by the initiating process and the remote clock does not claim
clock synchronization. A supplied deadline can only shorten the request budget.
Core finish is not preemptible and a known completed finish never becomes an abort
because response delivery or a later clock check failed.

Use ordinary TcpListener/connect_timeout and TCP_NODELAY. Normalize accepted
sockets to blocking mode. Do not set or validate a universal socket-buffer size.
A session owns a shutdown socket clone, worker handle and admission lifetime.
Stop admission and shutdown all live sockets before bounded worker collection.
Keep unresolved workers owned until explicit executable process exit; do not drop
their handles and call that cleanup. No FIFO/starvation or arbitrary synchronous
Read/Write cancellation guarantee is added.

## Integration gates

Implement and prove D01–D03 through public C2 APIs before increasing service
admission. Record the resulting schema/source checkpoint. Use the frozen D04 ownership
ledger when enabling the prospective profile. Then execute O01–O11 alongside
the superseding V/T/ENV dispositions. A=1 and old socket-ceiling receipts do not
qualify those gates; #193 stays blocked until #192 acceptance is complete.
