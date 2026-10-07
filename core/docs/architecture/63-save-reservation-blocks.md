# Per-Save allocation windows

> **Status:** Implemented source change after `656575d7a`; no new timing claim.

Storage::new selects ReservationBlocks defaults:4096 pack ids and16384 pooled
ordinals. Storage::with_reservations accepts checked startup settings within
the existing reserve port bounds (8191 packs and131072 ordinals per call).
Daemon bootstrap forwards its selected blocks into each independent producer.
The setting is not persisted as a canonical/physical format change and is not
a maximum Save size.

After acquiring its own bounded producer/index state, begin_save makes one
combined reserve call. It initializes both ranges only from that acknowledged
result. Pending output consumes ranges locally. A pack or pooled-value demand
that needs a new range makes one new reservation, with the configured block or
current bounded demand, whichever is larger. Processing continues through any
number of successful blocks. A refused/unknown allocation is never repeated;
Busy remains typed and before effect for that allocation attempt.

Per-Save ranges replace the old cross-Save pack tail and first-four-leaf ordinal
lookahead. Unused ordinal tail release at successful finish keeps its existing
exact conditional rule. Consumed ids are never reused; aborted/overlapped ranges
can leave gaps. No collection or destructive global cleanup is introduced.
Pack/record/locator formats, canonical identities, selection algorithms,
publication/reference-closure checks and memory windows remain unchanged.

Storage diagnostics count initial_reservations, reservation_refills and reserve
(their sum), including attempted calls. ordinal_reservations counts calls that
allocate ordinals, including the initial combined call. Publication remains its
own counter. For a successful operation the Store write equation is:

`initial reservation + required refills + publication batches + one atomic history transition`

Provider diagnostics distinguish successful transactions/commits from a refused
attempt. A Save with no new content still obtains its initial range and releases
its unused ordinal tail at finish; count that work rather than claiming a free
no-op. The existing pack/value format exhaustion and device/admission limits
remain explicit; no fixed block introduces a new total-file/Save/Commit limit.

This follows the [owner-approved count revision](../issues/307/SAVE-RESERVATION-DECISION-20261007.md).
[Proof receipts](../issues/307/PRE-S8-SAVE-BLOCKS-20261007.md) include both-range
refills, reconstructed canonical bytes, typed contention and independent Saves.
Init and Commit use this same implementation with separate mutable producer
state. The earlier accepted Init observation keeps its exact old source/binary
identity; this allocation change has no new performance sample or admission.
