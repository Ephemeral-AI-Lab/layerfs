# Payload and metadata ownership integration review

Status: reviewed cross-owner defects repaired and verified with targeted retests;
aggregate host policy integration remains open. This is neither phase completion nor
full public-path acceptance. No benchmark/#122 case was run for this review.

Reviewed the payload/index callback graph, the Index physical-storage rewrite,
and new range consumers. Payload attempt 06 binds the first verified fixes and
physical Index source. Later append-frontier/cleanup-status findings are bound by
their targeted range retests below. V1 remains a **generic-FUSE** requirement;
LinuxKit names only the environment of the prior probe, not a product dependency
or authorization to require a specialized kernel/VM adapter.

## Findings requiring implementation or integration

### P1 resolved: an old physical reader incorrectly rejected an unrelated foreground write

The reviewed `Payload::write_from` returned `StorageFull("payload cleanup pending")`
whenever `state.truncate` was present. `append` also refused extension for every
pending truncation. After a successful relocation, an in-flight physical reader
can deliberately retain the old source arena and defer its tail truncation. The
active destination arena remains independent and may have ample admitted space.
The predicate rejected a new foreground write there.

Exact new reproducer:
`overlay_payload::tests::writes_continue_while_old_source_reader_defers_truncation`.
It retains one physical reader, releases another extent, runs bounded relocation
until only old-source truncation is waiting, checks available quota, then attempts
a one-byte write to the active arena. [Attempt 04](attempt-04.log) preserves the
original `StorageFull` failure (0.01 s).

The implemented correction distinguishes pending **active-destination rollback** from
pending **retired-source truncation**. With the current two-arena state machine,
the former has `truncate.arena == active`; the latter has a different arena and
cannot rotate again until truncation resolves. Keep rollback protected; permit
foreground allocation/append during retired-source cleanup, under real quota and
reservation checks. [Attempt 06](attempt-06.log) verifies the foreground write and
old physical reader together. [Attempt 05](attempt-05.log) is retained as an
unrelated range-module compilation failure; no test ran in that attempt. No
visibility or storage guarantee was weakened.

### P1 resolved in Index: read-only linked-root traversal needed retirement progress

`Index::get_linked` calls `retain`, which acquires a root ticket. Dropping that
returned root queues retirement. In the reviewed implementation neither operation
drains the retirement queue, so repeated read-only traversal can exhaust
`max_roots` even though the caller released every transient linked root.
`Ranges::load` acquires linked child roots, making this a live integration concern.

The Index agent confirmed the caller trace, reproduced exact read-only
exhaustion, and added one bounded retirement assist before ticket acquisition.
The earlier get/set or short traversal passes do not cover this sustained path.
No payload source change was needed for that fix. Payload attempt 06 binds the
resulting Index dependency; the Index agent retains the exact failed and passing
read-only tests in its own ledger.

### P1 resolved: truncated-owner append consumed input

`Ranges::append_from` correctly preserves an old source's high-water after
truncation, but Payload's physical-frontier check did not also require the passed
owner to end at that logical high-water. A truncated token could therefore append
bytes at the old source end, consume the caller's reader, and then fail
`join_tokens` because the owner intervals were not adjacent.

The new range test
`append_frontier_coalesces_without_reusing_truncated_origin_coordinates`
reproduced this exact `noncontiguous payload owners` error after truncating a
1,363-byte file to two bytes and appending one byte. Original failure is preserved
in `../phase2-ranges/attempt-01-tests.log`.

The shared fix now returns `Ok(None)` before reading input when
`owner.start + owner.len != source.high_water`. The range caller can then select
a fresh source/origin without reusing earlier coordinates. Clearing a flag in
only the range caller would have left sibling Payload callers broken.
The [unchanged exact range oracle passed](../phase2-ranges/attempt-03-append.log)
in 0.54 s. The affected standalone append regression also
[passed in payload attempt 07](attempt-07.log); the other payload cases were
retained with their concrete compatibility reasons in the payload ledger.

### P1 resolved: cleanup status omitted pending ownership work

`Payload::stats().cleanup_pending` originally included physical sweep/evacuation/
truncation but omitted both the in-memory release queue and disk release jobs.
After dropping the last one-byte slice, a maintenance step could transfer its
queued drop into a disk job and then report `cleanup_pending == false` before
the interval decrement. A caller trusting that status stopped with 4,096 bytes
still logically retained.

The range test `disk_child_links_and_exact_payload_slice_own_bytes_without_old_tree`
reproduced this at its final release assertion. Its earlier assertions had already
proved that the metadata graph was gone and only the one-byte payload token
remained; this was a false completion signal, not a hidden whole-tree owner.
The original failure is in the same range attempt-01 log. The shared status now
includes queued/disk releases and failed write reservations. The range oracle
and drain fixture remain unchanged; the
[exact slice cleanup rerun passed](../phase2-ranges/attempt-02-slice.log) in 0.02 s.

### ResourcePolicy and host aggregate admission remain unwired

The inspected `layerfs-workspace-core/src/limits.rs` exposes only
`max_spool_bytes` and `max_final_delta_memory_bytes`. HostBacking initialization
serializes those two values, and LiveOwner reads those two values. The new
Payload/Index limits are constructor-local. `Workspaces` counts active sessions
but does not reserve an aggregate host budget when creating each one.

Wire these existing component quantities into host admission before claiming
bounded concurrent Workspaces:

| Resource | Required accounting / wiring |
| --- | --- |
| Payload physical allocation | Both arenas' actual allocated blocks plus outstanding reservations; keep failed writes/cleanup charged |
| Metadata/index allocation | Actual allocation from both payload-catalog and Workspace/range indexes, including the Index agent's data and ownership/reverse catalogs; logical free-slot reuse is not physical release |
| Payload recovery headroom | One 64-KiB destination move unit and its bounded allocator/catalog work, unavailable to ordinary writes |
| Bounded scratch | Up to 64 KiB per admitted payload writer; 128 KiB copy/verification scratch for the single payload relocation; metadata-tree path/split scratch and the new Index relocation scratch |
| Ownership and reclamation | Payload owner/token count, reserved in-memory drop-queue capacity, disk release jobs, source/location entries, metadata root tickets and pending rollback/truncate state |
| Physical readers | Reserved reader-slot array and admitted concurrent readers; caller-owned output buffers need their own request/transfer reservation |
| Root preparation / replay / builders | Prepared-root slots, mutation/result bytes, replay window, active Commit snapshot/attempt, bounded builders and construction scratch |
| Concurrent Workspaces | A shared host owner charges the sum, rather than permitting each Workspace independently to consume the full host allowance |

The container's existing `LiveRuntime::Scheduler` has separate request/transfer/
live/control/lifecycle semaphores. Those are useful existing mechanisms, but its
128-MiB live and 32-MiB ordinary-transfer pools do not prove a cap on host
Workspace/index/Store RSS. Preserve host/container domains when extending the
wire contract, including explicit compatibility handling for changed frame
contents. No new numerical performance gate is proposed here.

The payload constructor's formerly infallible slot allocations were changed to
`try_reserve_exact`. Attempt 06 passes `oversized_slot_limits_return_resource_error`
for owner, writer, and reader caps. This is an input/admission correction, not
evidence of a measured capacity limit. Aggregate policy wiring above remains open.

## Ownership/error contract assessment

The current callback design is compatible with the payload implementation:

1. Metadata page allocation retains each bounded external token before the page
   becomes owned. After a successful callback it records that reference in the
   bounded rollback list. Failed page publication therefore has a corresponding
   release obligation.
2. Payload retain/release builds an uninstalled private catalog root and swaps
   `state.root` only after all required index operations succeed. An error before
   that swap leaves the externally requested count unchanged. Private catalog
   allocation/ownership debris remains the private Index's charged rollback work.
3. On final metadata-page reclamation, the Index advances its recorded child
   cursor and retains the exact pending release in bounded runtime state. A
   failed external callback leaves that pending item available for retry. Once
   the callback succeeds it is removed, preventing a second decrement.
4. The Index's ambiguous-header mechanism records the exact intended header
   sequence/count and replays that same write, rather than recomputing refs +/− 1.
   This is necessary for Payload's external error contract. The physical storage
   rewrite must preserve this property for page location/reverse-map updates.
5. Payload's own catalog Index has no external owner hooks. It must remain
   separate from the metadata Index that calls back into Payload; sharing those
   two instances would create reentrant locking and ownership recursion.

Observed lock order is:

```text
metadata Index storage
  -> Payload state
    -> private payload Index storage
      -> private payload Index retired queue

Payload maintenance -> brief drop-queue read (released before state lock)
  -> Payload state -> private payload Index storage
```

`OwnedRange::drop` takes only the payload drop queue; physical-reader drop takes
only Payload state. Root drop takes its own Index retirement queue. No reverse
edge back from the private payload index into metadata storage was found in the
reviewed implementations. Bulk payload reads/copies occur outside Payload state
locking; the single bounded move currently excludes destination writes through
`moving`, rather than retaining the mutex across the copy.

## Range ownership and pin ceilings

A retained `OwnedRange` subrange creates an independently counted, block-normalized
source interval. `Index::set_owned` then transfers an additional reference into a
disk leaf; dropping the temporary in-memory handle does not drop leaf ownership.
Snapshot root clones share the existing graph without per-block work.

The range agent was explicitly advised that a one-byte subrange/read-plan result
must own its exact payload token, and must not accidentally retain the entire
original range-tree root indefinitely alongside that token. A full source-root
lease is appropriate during lookup/preparation and for an intentionally complete
snapshot. It is not an equivalent replacement for an exact returned range lease.
The new range-tree integration needs its own tiny-range retention oracle.

Physical I/O leases are separately bounded and keep an old arena location until
the I/O completes. Pending truncation remains charged. Report that cleanup slack
separately; the existing 64-MiB-to-4-KiB claim was measured after obsolete physical
readers released, not while an arbitrarily stalled old reader held the arena.

Append high-water and joining also preserve distinct contracts: the source's
published high-water never rewinds; a failed unpublished append does not advance
it. Joining creates an independent token over adjacent same-source ranges and
does not shorten either earlier token. A truncation followed by append must not
reuse previously published source coordinates merely to enable coalescing.

## Final identities and remaining qualification

The final targeted checks bind Payload SHA-256
`cc25e6a7dba97a604eda493ea56c37d57aa83f3f111f575e70bbd9e034c771a4`,
Index SHA-256
`27622b044e6c0f71d94c64452b4e147f23ca32ce6d7f88b2157a3c49b79c1cc7`,
and ranges SHA-256
`0f9040dbb4f303356d6a31aceb7fad7568f6b4811c68f98e808a43d380515e65`.
Receipts: [payload attempt 07](attempt-07.json),
[range slice source](../phase2-ranges/attempt-02-source.json), and
[range append source](../phase2-ranges/attempt-03-source.json).

The Index backend keeps stable page IDs, moves one 4-KiB body under the storage
mutex, verifies the copy, and publishes location/reverse entries before tail
cleanup. Its `RETIRING` state prevents an overwritten victim body from being
decoded again on a retry. A failed tail truncation retains its explicit pending
receipt after logical release, preventing repeated reference decrements. The
reviewed path preserves the callback and exact-header intent ordering. Its
[live-source relocation-fault evidence](../phase2-components/index-relocation-live-faults-attempt01.log)
and [copy-verification/truncate-failure evidence](../phase2-components/index-relocation-copy-truncate-attempt01.log)
are maintained by the Index agent.

No remaining defect was found in the reviewed callback lock order or ambiguous
count handling. This is scoped source review supported by the retained component
checks, not proof of every concurrent public FUSE/SDK/lifecycle interleaving.
Aggregate ResourcePolicy wiring remains open. Full generic-FUSE V1, public-path
capacity/correctness acceptance, and the final benchmark campaign remain separate.
