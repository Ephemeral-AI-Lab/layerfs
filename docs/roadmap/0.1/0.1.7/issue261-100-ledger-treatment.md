# Issue 261: one redundant ledger read in page publication

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Declared before the product edit and candidate diagnostic. The corrected
`issue261-separated-100-v2` public route at source `f23263b72` completed 100
writes and its independent oracle. Four snapshots recorded cumulative 3,679
ledger page reads and 1,680 writes; the optional phase timers reported
205,600,621 ns in `own_payload` (including routine maintenance) and
243,501,079 ns in `write_file` (including publication/invalidation). This
supports targeting shared ownership ledger work; it does not isolate physical
device latency or make a cache-qualified speed claim.

In both `RootOwner::write_raw_page` and `RootOwner::write_page`, the producer
adds a new page's ownership edges, then calls `Arena::read_owner` to obtain
the authenticated ledger page and `Arena::set_owner` to mark `edges=true`.
`set_owner` immediately opens and reads that **same** 4 KiB ledger page again.
The page is unchanged between those calls while the metadata writer gate is
held. Candidate treatment: share the existing authenticated page already in
the I/O window for this final mark, preserving the same owner fields,
checksum, write, failure/quarantine behavior and ordering. Reuse the shared
loaded-page write path also used by `change_refs`; do not defer writes,
change page/ledger formats, bypass identity checks, drop custody, change
quotas, or move work across the write acknowledgement.

The count prediction is one fewer `ledger_reads` per successfully published
new extent or keyed metadata page, with ledger writes unchanged. This is a
generic page-publication fix, not a benchmark route. It does **not** remove
the separate O(Local extents/child edges) ownership work or the fixed
one-byte payload file cost. If the count moves but wall does not, retain the
result and report the remaining cost rather than broadening the treatment.

Candidate scenario `issue261-separated-100-v3` keeps v2's exact writer,
fixture/master-copy, one public mount/Exec/Commit, diagnostic image interval,
four progress and cumulative phase checkpoints, old/new-head verifier,
uncontrolled cache, 15 s complete command, under-10 s verifier and cleanup
rules. It runs **once** at one frozen changed-source identity. A fresh image
uses the changed locked release daemon; the sealed prepared master, SDK
driver/verifier and writer are reused only after compatibility and digest
checks. V1/V2 rows remain append-only and are not pooled or called a matched
latency speedup. No 512 rerun is part of this treatment.
