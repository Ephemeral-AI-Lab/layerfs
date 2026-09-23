# #237 prospective pack-space treatments

> Frozen before the first product edit or new public candidate call. The
> retained Core SDK 100k release control is `62e7204ca` with product seal
> `1d9d64ee874c4dc3b305c3be5021471b80abf757e0f9309bdba12457f57c5f84`;
> [its receipt](evidence/sdk-100k-release-20260924/receipt.json) and full
> readback stay unchanged. Its Store + History measured **554,098,688 B
> apparent / 558,145,536 B allocated** and **35,168,077 B** of pack capacity
> beyond declared used length. This is an unregistered diagnostic with
> unqualified namespace metadata cache, not admission evidence.

## C1: exact allocation when a new pack is already closing

Change only `LanePlacement::increment`'s selected capacity: when the current
write **creates** a pack and the next group displaces it **within the same
`select_many` call**, the pack cannot receive a later append, so allocate its
exact `used` length. Keep full 256-KiB capacity for a created pack that remains
open, and for every already-written pack. Existing SQLite incremental BLOB
write, 4-KiB page, framing, reader and one-owner semantics remain. Add one
external placement boundary assertion for exact closed versus full open rows
and retain public readback coverage. This is a deliberately narrow mechanism
screen; its recoverable byte total is unknown before the one candidate row.

## C2 only if C1 leaves most tail capacity

At a new, separately pinned product identity, close each selected pack at a
flush boundary, insert the exact assembled length, and start a new pack on
later calls rather than appending to a closed row. A demanded same-Save read
must continue to flush bytes and locators in the same transaction before it
reads them. Keep bounded queue/memory, pack BLOBs, canonical object identity,
four Init producers and one C2/SQLite owner. This may create more pack rows,
more SQL or higher RSS; it is **not** accepted merely because tail capacity
falls. C1 and C2 are distinct treatments, each with one sample and all
receipts retained. Do not run C2 if C1 already closes the material space gap.

## One-shot evaluation for either treatment

Use the existing sole Core SDK runner's explicit `--diagnostic-100k-release`
selection, locked Cargo `--release` examples, seed-1 SHAKE manifest
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`,
fresh independent source copy, zero-resident payload preflight/recheck and a
fresh output path. The harness and source fixture must remain byte-identical
to control; only product/test/architecture source changes. No control resample,
repeat of a candidate, timeout/worker/page-size/cutoff adjustment or
post-timer database compaction. The public SDK call, complete command, and
independent full 101,001-path/500-MB reopened oracle retain their original
boundaries. The 610-s driver and 600-s verifier watchdogs are safety bounds,
not new PASS gates; keep the 15-s command and under-10-s verifier expectations
visible, including control's 12.603-s verifier miss.

Record `st_size`, `st_blocks*512`, 4-KiB pages/freelist, pack row count,
`sum(length(data))`, `sum(header.used)`, tail capacity, object rows, full
oracle, lifecycle CPU/peak RSS, exact source/product/harness/binary seals and
competing work. The **space objective** is less apparent **and** allocated
Store + History bytes than the control, with no worse correctness or material
CPU/RSS/command degradation. C1 is complete only if its physical saving is
material; otherwise disclose it as a partial result and proceed to C2.
For C2, target at least **80% lower pack tail** (≤7,033,615 B) while retaining
no-worse combined Store allocation and a complete readback. A result that
misses either target remains a failed/rejected treatment. The #229 sparse
history space/readback guard remains open and is required before adoption;
do not treat dense Init alone as that proof. No speed or v0.1.6 compactness
claim follows from metadata-unqualified or cross-fixture rows.
