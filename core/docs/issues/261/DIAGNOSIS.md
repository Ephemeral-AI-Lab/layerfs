# Issue 261: public mounted-write diagnosis

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The [prospective 100-write contract](../../../../docs/roadmap/0.1/0.1.7/issue261-mounted-writes-spec.md)
was committed at `0736d50c3` before the harness and sample. One labelled
diagnostic ran on clean source `b094a4a1f178e01922e084bdea511aa907919801`,
product seal `2a19f0d5c594c120c90da636d060cb7eecfd1b9114feb683e9e8550553cb6c6f`.
The [raw receipt and logs](evidence/100-v1/) are append-only. The one later
[512-write diagnostic](evidence/512-v1-fail/) used a separately committed
[selection](../../../../docs/roadmap/0.1/0.1.7/issue261-512-diagnostic-spec.md),
failed at FUSE WRITE callback 258, and is not a replacement for the 100 row.
The corrected [100-write phase receipt](evidence/100-v2-phase/) and the
[one-read treatment receipt](evidence/100-v3-ledger/) have distinct source and
scenario identities and remain separate diagnostics.

## The 100-write public row

One `WorkspaceApi::mount` attached the old head. One
`WorkspaceApi::exec(&mount.id, "/fixtures/bin/write-separated data.bin 100")`
ran one shell-launched writer outside the mount. Its single fd issued 100
successful one-byte `pwrite` calls at offsets `0,2,…,198`. One explicit
`WorkspaceApi::commit` returned a new head. The independent read-only oracle
checked the old and new head, old and new full 8,194-byte files, mode, length,
no extra paths, parent relationship and exactly 100 separated changed runs.
Verifier status was PASS in **14.593 ms**; unmount and sandbox deletion returned
success. The old head was `12eb4d3c…5f3fd9a02`; the new head was
`12c72604…8774d6b75a`. Full identities are in the receipt.

| Metric | Observed | Scope |
| --- | ---: | --- |
| Exec | 498.503 ms | Public SDK call, monotonic |
| Writer through write 100 | 486.070 ms | Writer's own monotonic clock |
| Commit | 40.677 ms | Public SDK call, separate from Exec |
| Complete command | 1,509.647 ms | Driver launch through exit; 15 s diagnostic limit |
| Cleanup | 560.473 ms | Driver status, unmount, sandbox log capture/delete |
| FUSE callback classes | LOOKUP 1, GETATTR 3, OPEN 1, WRITE 100, READ 0 | Workspace Status after Commit |
| Host Service calls | 4 | Workspace Status cumulative `upstream_calls` |
| Final extents / changed runs / replacement bytes | 200 / 100 / 100 | Daemon `LFS_PIECE_LOWER`, Service count, independent verifier |
| Store size | 618,496 → 897,024 bytes | Clone before/after; +278,528 bytes |
| History DB size | 86,016 → 86,016 bytes | Clone before/after; logical head still advanced |

The `write` Status class also counts namespace mutations in other workloads.
This command makes none, and the other mutation classes are zero, so its 100
counts are actual data WRITE callbacks, matching (but not inferred from) the
writer's 100 syscalls. Source requests `FOPEN_DIRECT_IO` and does not negotiate
FUSE writeback. FLUSH and RELEASE do not have individual Status counters in
this source; their exact callback counts and time are **unavailable**, not
assumed zero. The `other=0` class does not prove they did not occur.

The SDK issues one control operation each for mount, Exec, Commit, Status and
unmount. The daemon launches `/bin/sh -c` once and its local Exec loop may poll
Workspace revision about once per second; this Exec was under one second.
There is no per-write SDK, control or Service RPC in the FUSE write call path.
The host and daemon LFT1 events show Commit's SaveFile, portable metadata and
History operations; the latter is the sole head publication. The observed
Workspace upstream count of four fits one Exec Inspect and three Commit
operations. The Service
logs show three `storage.finish.transaction_begin`/SQLite commit phases for
the three Commit operations. Output progress is four lines, not 100 frames.
The exact transport frame count is **unavailable** in this build.

## Within-run work, not resamples

These are four cumulative checkpoints inside the **same** 100-write run. The
time and I/O increments below are differences from the previous checkpoint.
The snapshot itself scans retained payload records, so these times are
diagnostic and include that overhead.

| Completed writes | 25-write time (ms) | Ledger reads | Ledger writes | Metadata page reads | Routine reclaim scans |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 25 | 102.593 | 738 | 341 | 141 | 24 |
| 50 | 107.016 | 775 | 350 | 150 | 25 |
| 75 | 126.197 | 979 | 452 | 184 | 25 |
| 100 | 150.265 | 1,187 | 537 | 225 | 25 |

At write 100, cumulative ledger counts were **3,679 direct 4 KiB reads and
1,680 direct 4 KiB writes**: 15,069,184 read bytes and 6,881,280 written
bytes, or **21,950,464 bytes of ledger transfer** for 100 supplied file bytes.
Metadata page reads were 700 (2,867,200 bytes). The last 25 writes incurred
61% more ledger reads and 57% more ledger writes than the first 25, while the
writer's 25-write interval grew 46%. This is a within-run association, not an
isolated ledger time measurement or an asymptotic proof.

The earlier #252 whole-registry scan is not the current cause: the source
uses a reclaim candidate set, and the diagnostic counted exactly 99 routine
record scans by write 100, with 0 deliberate lookup scans. At the fourth
checkpoint it held 100 payloads, all retained by live Local extents, and
reported 856,064 allocated private backing bytes. Source makes one 8,192-byte
private segment per accepted one-byte WRITE, with one create, one fallocate,
two aligned 4 KiB writes and one close: **100 acquisitions, 100 segment files,
200 aligned payload writes and 819,200 payload bytes** are source-derived
for the observed 100 callbacks. The difference, 36,864 bytes, is charged
metadata backing. Live metadata status reported 7 allocated pages and 2 roots;
these are a live state, **not cumulative page writes or allocations**.

## File extent B+ tree and cost model

Let `N` be accepted writes, `E` live extents, `H` file extent-tree height,
`R` final changed runs and `S` replacement bytes. Here `E=2N`, `R=N`,
`S=N`: each Local byte is followed by a Base gap or tail. A page can hold
124 extent records or 248 child references. At `N=100`, `E=200` requires
two leaves and a height-1 branch. The splice descends only into the touched
path, so extent **page reads** are bounded by `O(H + touched pages)` per
write, rather than a scan of all `E` extents.

Path copying still rewrites a touched leaf and its ancestors. The ownership
encoder enumerates every Local custody edge in each copied leaf and every
child edge in each copied branch; publication increments their references,
and old-root reclamation decrements them. Adjacent references on one 62-entry
ledger page are batched, but each ledger batch authenticates and writes a
whole direct-I/O page. The loops are in `binary_plus_tree/keyed/update.rs`
(`edges_raw`), `backing/ownership.rs` (`write_raw_page`),
`backing/ownership/ledger_batch.rs` (`change_refs_run`) and
`backing/metadata_reclaim.rs` (`cleanup_step`). The 100-row's rising ledger
and metadata reads are consistent with this path as leaves fill and the
branch appears. Page-format fanouts are fixed, so the source bound is
`O(N · (H · F + touched))` edge work, with `F ≤ 248`, plus 100 real FUSE and
payload acquisitions. It is `O(N log E)` when fixed fanout is treated as a
constant, but its direct-I/O constant is material. There is no evidence here
for a whole-registry `O(N²)` reclaim scan.

Commit reads the frozen final state rather than replaying 100 calls. The
dirty-inode cursor selects one file; lower/descriptor/replacement cursors are
monotone `O(E + R + S)` passes with bounded buffers. Observed daemon lower
counts were 200 pieces and 100 changes, and Service declared 100 replacement
bytes. The Service's count log reported 0 C1 nodes read/created and 0 emitted
payload bytes for this small-file save, so those counters do not price every
Store SQL statement. Total Store page visits/writes and exact control output
frame count remain **unavailable**; no zero is imputed.

## Resource and evidence limits

LFT1 sampled maximum RSS was 32,096,256 bytes for the host SDK process and
36,835,328 bytes for the daemon. Those are sampled process values, not exact
peaks, cgroup memory, anonymous/file-cache split or phase-local memory. The
cache contract is uncontrolled ordinary cache, so raw latency is
`INELIGIBLE` for a performance PASS even though the public functional route,
independent oracle and cleanup passed. No cold claim follows from cloning.

The 512 selection is retained separately as **FAIL**: write 258 returned
`EBUSY`, only the 128 and 256 checkpoints were reached, no Commit ran, and
the writer exited 1. The outer unmount/delete methods returned success, but
the daemon log said `sandbox shutdown retained: Busy`; therefore complete
Workspace custody cleanup is **unverified** even though the original receipt's
outer `cleanup_status` says PASS. Its ledger counts are diagnostic only and
are not used as a 100-write speed arm or a replacement receipt. Source review
found a post-reply mutation-permit interval widened by the optional v1
snapshot; the next callback may see `Busy` there. The failure at callback 258
does not prove that exact site, so the product's refusal cause is not claimed
as certain. The separately specified v2 100-write diagnostic moves snapshots
before the reply and preserves v1 evidence.

## Corrected 100-write phase attribution and shared fix

The v2 diagnostic moved the optional snapshot before the FUSE reply, kept the
mutation permit through the reply attempt, and recorded cumulative time in
`own_payload` and `write_file`. It reused the same prepared old-head master
and unchanged writer. The independent oracle passed. By write 100,
`own_payload` totaled **205.601 ms** and `write_file` **243.501 ms**. The former
includes routine backing maintenance before creating the new payload; the
latter includes path copying, ownership publication and projection
completion/invalidation. Their sum is 449.102 ms of the writer's 454.243 ms.
This shows that both sides of a write carry substantial work; it is not an
isolated payload-device or ledger-device timer. The v2 cumulative ledger
counts were identical to v1: 3,679 reads and 1,680 writes.

`Arena::read_owner` leaves a verified ledger page in the I/O window.
`RootOwner::write_raw_page` and `write_page` used to call `set_owner` after
that read to mark their edge list complete; `set_owner` read the same 4 KiB
page again. The [prospective treatment](../../../../docs/roadmap/0.1/0.1.7/issue261-100-ledger-treatment.md)
changed both shared page-publication paths to write the already verified page
with the same final owner record, checksum, direct write and failure handling.
The v3 public run and independent oracle passed. It retained 100 actual FUSE
WRITEs, 200 final extents, 100 changed runs, 100 replacement bytes, one Commit,
clean unmount and sandbox deletion.

| 100-write diagnostic | v2 before | v3 after | Signed change |
| --- | ---: | ---: | ---: |
| Ledger 4 KiB reads | 3,679 | 3,440 | **−239** |
| Ledger 4 KiB writes | 1,680 | 1,680 | 0 |
| Metadata page reads | 700 | 700 | 0 |
| Cumulative `own_payload` | 205.601 ms | 206.400 ms | +0.800 ms |
| Cumulative `write_file` | 243.501 ms | 229.483 ms | −14.018 ms |
| Public Exec | 469.417 ms | 455.678 ms | −13.739 ms |
| Public Commit | 38.191 ms | 40.930 ms | +2.739 ms |
| Complete command | 1,420.762 ms | 1,422.639 ms | +1.877 ms |

The exact **239-read** reduction with unchanged writes and extent/Commit
counts supports the intended mechanism: one authenticated read removed per
completed new page. The raw Exec and publication times moved in the same
direction, while complete command wall did not. Both rows were instrumented,
uncontrolled-cache diagnostics (`admission_eligible=false`); they are not a
cache-matched speed comparison or a latency PASS. The remaining 3,440 ledger
reads and 1,680 ledger writes are real 4 KiB ownership work, including edge
reference changes during path copying and reclaim. No whole-registry scan or
per-write Service RPC reappeared.

The [full 4,097 public gate](evidence/gate-4097-not-run.json) is **NOT_RUN** at this source. The 100-write case is
the current optimization target; this narrow correction has not shown that
the remaining direct-I/O cost will fit the prospectively declared 25 s
complete-command exception. The separate 512 attempt refused before Commit,
and the 30 s product Exec timer remains owned by #249. No workload, deadline,
worker count or cache policy was changed to manufacture a passing gate row.
