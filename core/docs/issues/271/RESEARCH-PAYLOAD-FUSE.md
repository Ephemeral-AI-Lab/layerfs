# Issue 271 research: tiny payloads and the mounted WRITE boundary

> **Status:** Research; informative and not a product contract.

Source inspected at `c3d2ad68be1f437b156f09700d6d18396d01edca` (four-hop
product source `933b3457c916519f458137e4c4f19662c8e9228e`). This note
proposes instrumentation and architecture; it does not change the 25 s gate or
promote the [one 60 s diagnostic](FOURHOP-4097-EXTENDED-DIAGNOSTIC.md).

## What is actually known

The retained [receipt](evidence/fourhop-4097-extended-v1/run/receipt.json) and
[daemon log](evidence/fourhop-4097-extended-v1/run/driver.stderr) report
4,097 observed FUSE WRITEs, 27.918267 s raw SDK Exec and 0.434905 s Commit.
At the 4,096th callback checkpoint, cumulative `own_payload` time was
12.082509 s and `write_file` time was 15.649716 s, with 220,377 owner-ledger
reads and 119,492 writes. The two callback clocks exclude some surrounding
admission, reply and scheduling cost; they are **not** disk-I/O or CPU profiles.
The former includes `maintain_backing`, and the latter includes another
`maintain_backing` plus notification. The receipt's uncontrolled cache makes
cross-run latency **INELIGIBLE** for speed qualification. The original
[4,097 gate remains FAIL](FOURHOP-GATE-FAIL.md).

The 4,096th checkpoint also reports 4,096 retained payload records, 4,095
routine payload scans, and 18,751,488 allocated backing bytes, including
1,974,272 metadata bytes. The remaining **16,777,216 bytes = 4,096 × 4 KiB**
is consistent with one charged 4 KiB inline segment per one-byte write. It is
allocated backing accounting, not a phase-local RSS or a measured device-byte
count. The source's inline format makes that accounting expected:
[`segments::planned`](../../../crates/layerfs-workspace/src/backing/segments.rs)
returns 4 KiB for lengths 1..=4,016 (lines 5–13, 27–43).

## Source path and round trips

The benchmark writer [opens one fd and issues one `pwrite` of one byte per
iteration](../../../benchmark/fs-bench-pro/writers/write-separated.c) (lines
41–68). The writable FUSE open replies with `FOPEN_DIRECT_IO`
([adapter.rs](../../../crates/layerfs-fuse/src/adapter.rs), lines 163–194).
That mode bypasses the FUSE page cache for read and write data; the [Linux
FUSE I/O mode documentation](https://kernel.org/doc/html/latest/filesystems/fuse/fuse-io.html)
does not turn 4,097 one-byte syscalls into one write request. The adapter
records each WRITE callback, validates flags and the handle, obtains an
exclusive mutation permit, calls `Workspace::own_payload`, then calls
`ProjectionMutationPermit::write_file`, and only then sends the WRITE reply
([adapter.rs](../../../crates/layerfs-fuse/src/adapter.rs), lines 496–578;
[coherence.rs](../../../crates/layerfs-workspace/src/runtime/coherence.rs),
lines 433–479). There is one host SDK Exec call around the shell writer, not
one SDK or Service RPC per byte.

For each accepted WRITE, the request/reply crossing is the writer's syscall →
kernel FUSE request → daemon callback → FUSE reply → syscall return. The
current projected write also sends one synchronous inode-invalidation message
to the kernel **before** the reply: `mutate_file` calls
`complete_projection_mutation`, which invokes the bound notifier
([write.rs](../../../crates/layerfs-workspace/src/filesystem/write.rs), lines
335–351, 709–743; [coherence.rs](../../../crates/layerfs-workspace/src/runtime/coherence.rs),
lines 568–630; [mount.rs](../../../crates/layerfs-fuse/src/mount.rs), lines
247–263). In the locked `fuser` 0.18.0 source, `inval_inode` ultimately sends
one `/dev/fuse` `writev`; its return means the notification send succeeded,
not a later FUSE WRITE reply acknowledgement. The adapter explicitly lacks
checked reply delivery ([adapter.rs](../../../crates/layerfs-fuse/src/adapter.rs),
lines 565–575). This is one extra userspace→kernel message per write, but its
time is **unmeasured**. The [libfuse invalidation contract](https://libfuse.github.io/doxygen/fuse__lowlevel_8h.html)
describes inode attribute/data invalidation and potential blocking with dirty
writeback pages; this mount does not negotiate writeback caching
([adapter.rs](../../../crates/layerfs-fuse/src/adapter.rs), lines 69–86).

Within `own_payload`, the actual order is metadata/payload maintenance,
registration and quota reservation, existence check, `O_EXCL|O_DIRECT` file
creation, identity metadata, `fallocate`, block-count observation, one aligned
4 KiB write containing the one byte and owner header, then close
([payload.rs](../../../crates/layerfs-workspace/src/backing/payload.rs), lines
410–457, 490–690, 716–738; [segments.rs](../../../crates/layerfs-workspace/src/backing/segments.rs),
lines 222–233, 273–328). The [reader](../../../crates/layerfs-workspace/src/backing/reader.rs)
later reopens the segment and performs an aligned 4 KiB header read and
identity verification (lines 58–85); Commit's local-piece source obtains the
payload by id and reader ([ownership.rs](../../../crates/layerfs-workspace/src/backing/ownership.rs),
lines 648–661; [commit/source.rs](../../../crates/layerfs-workspace/src/commit/source.rs),
lines 169–175). The exact syscall count and elapsed time for these operations
are not yet measured.

`maintain_backing` first reclaims eligible metadata roots and then drains
released payload ids ([reclaim.rs](../../../crates/layerfs-workspace/src/backing/reclaim.rs),
lines 290–299). It is called in both `own_payload` and the file mutation
([payload.rs](../../../crates/layerfs-workspace/src/backing/payload.rs), line
736; [write.rs](../../../crates/layerfs-workspace/src/filesystem/write.rs),
line 395). The payload registry uses a `BTreeMap` plus a release-candidate
`BTreeSet`, so the routine path does not walk all previous records
([payload.rs](../../../crates/layerfs-workspace/src/backing/payload.rs), lines
28–49, 258–285; [reclaim.rs](../../../crates/layerfs-workspace/src/backing/reclaim.rs),
lines 121–184). Moving reclamation beyond Exec would merely move necessary
work across the measurement boundary or retain charged dead pages.

Let `N` be accepted one-byte writes, `K` the number of genuinely released
owners, `H` the extent-tree height and `E` the touched page's bounded edge
count. The callback/message count is **Θ(N)** for this writer and direct-I/O
mount, including Θ(N) invalidation sends under the current policy. Registry
lookup/insertion is `O(log N)` per owner, routine payload cleanup is
`O(K log N)` overall, and tiny payload allocation is Θ(N) separate files and
Θ(4 KiB × N) charged segment bytes. Tree publication additionally pays for
its copied path and ownership edges, roughly `O(H × E)` per write with bounded
4 KiB page fanout and sponsor depth; the sibling research note examines that
term. These bounds do **not** imply sustained `O(N²)` on the observed range.

## Architecture opportunities

1. **Inline tiny bytes in immutable extent pages.** Add a new authenticated
   piece representation that stores small replacement bytes with the page
   instead of a separate payload id, custody page and 4 KiB file. This could
   remove the Θ(N) file create/open/unlink and 4 KiB payload allocation terms
   for the one-byte shape. It does not remove the FUSE request/reply, the
   copied extent page, or page-owner ledger work. The current 32-byte
   `PieceRecord` has only Base/Local/Zero kinds and requires Local payload and
   custody ids ([format.rs](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/format.rs),
   lines 11–69), so this is a format and custody change, not a spare-byte
   tweak. Both old and new heads must decode exact bytes, and quota must charge
   the containing immutable pages until their last root reference is released.

2. **Append-only pack for small payloads.** Keep the current logical payload
   ids but map many immutable records into owned pack files, reducing per-byte
   file creation and Commit reopen costs toward `O(N / records_per_pack)`.
   Direct I/O still requires aligned reads/writes. If every acknowledged byte
   forces a 4 KiB read-modify-write of a shared partial page, this only removes
   namespace syscalls; it does **not** remove Θ(4 KiB × N) physical I/O and
   adds race, failure-progress and identity complexity. A buffered, bounded
   page assembly scheme could amortize writes, but needs an explicit
   cache/residency and failure contract before its latency can qualify.
   Pack offsets, immutable slot identity, independent old-root pinning,
   per-slot cleanup and quota refund need exact definitions.

3. **One callback-local acquisition/publication transaction.** The adapter
   currently acquires a separately owned payload before it can publish a
   candidate ([adapter.rs](../../../crates/layerfs-fuse/src/adapter.rs), lines
   545–563). A direct write path could reserve bytes and metadata together,
   build an inline or packed candidate from the callback's borrowed buffer,
   and publish once. It could eliminate duplicate admission/maintenance and
   abandoned owner work, but must preserve error outcomes, partial allocation
   ownership, old roots, and quota on all failure points. This is a candidate
   **only after** timing shows those two boundaries cost material time.

4. **Prove an origin-specific notification reduction.** Direct I/O bypasses
   file data cache, and the product already omits its own notification for
   projected size/CREATE where the kernel owns invalidation
   ([write.rs](../../../crates/layerfs-workspace/src/filesystem/write.rs), lines
   709–721). A projected WRITE might similarly avoid `inval_inode` only if
   kernel response semantics are proved to keep inode size/mtime and all open
   handles coherent before the next observable operation. Direct I/O alone
   proves nothing about attribute cache or a concurrent SDK mutation. Test
   old/new reads and `stat` through multiple fds, append and repeated writes,
   external SDK writes, failure replies and supported kernels. Until then,
   batching or moving notification after the reply weakens the current
   publication boundary and is not a safe optimization. Changing `max_write`
   cannot coalesce distinct one-byte syscalls; enabling writeback caching would
   change acknowledgment and the declared cache contract.

Even deleting **all** 12.083 s of the measured acquisition bucket would leave
at least 15.65 s of the measured publication bucket at this checkpoint, so it
cannot by itself produce a 2× Exec gain. A 2× reduction from 27.918 s would
need roughly 13.959 s saved. The bucket total and raw Exec are at different
boundaries, so this is only an opportunity ceiling, not a forecast.

## First-pass telemetry, then a mechanism decision

Split the two existing callback clocks at their actual source boundaries:

| Existing bucket | Additive sub-buckets to time and count | Instrumentation point |
| --- | --- | --- |
| `own_payload` | `maintain_backing` and `PayloadHost::acquire`, plus wrapper remainder | Immediately before/after [payload.rs](../../../crates/layerfs-workspace/src/backing/payload.rs) lines 736–737. |
| `maintain_backing` | metadata-root maintenance and payload-candidate maintenance | Around the two calls in [reclaim.rs](../../../crates/layerfs-workspace/src/backing/reclaim.rs) lines 293–297; tag the caller as acquisition versus publication, or use separate before/after snapshots. |
| `write_file` | core `publish_file_mutation`, checked notifier delivery, plus wrapper remainder | Around [write.rs](../../../crates/layerfs-workspace/src/filesystem/write.rs) lines 341–350 and the delivery call in [coherence.rs](../../../crates/layerfs-workspace/src/runtime/coherence.rs) lines 575–580. The second `maintain_backing` belongs to publication core until separately split. |

Use cumulative call counts and monotonic nanoseconds, including failed calls;
record the deltas from before Exec at the existing 512-WRITE checkpoints and
at the end. Preserve the current accepted-WRITE counter as the denominator,
and report failures separately. `BackingStatus` is already public operator
telemetry with cumulative counters ([types.rs](../../../crates/layerfs-workspace/src/types.rs),
lines 229–248; [payload.rs](../../../crates/layerfs-workspace/src/backing/payload.rs),
lines 739–756), so it can carry these source-wide totals without a new
benchmark-only hook or per-write stderr. The existing `LFS_WRITE_SAMPLE`
emits sparse status snapshots ([write_sample.rs](../../../crates/layerfs-fuse/src/write_sample.rs),
lines 52–78). Product `src/` forbids performance-only counters and branches;
these must be ordinary, bounded operator diagnostics available on the real
path. A diagnostic-only benchmark parser may consume the public totals.

If `PayloadHost::acquire` dominates, drill into `symlink_metadata`, `openat`,
`fstat`, `fallocate`/block observation, aligned write and close counts/times;
if checked delivery dominates, inspect notifier `writev` timing and inode
semantics. This second pass should use aggregate counters rather than log each
call. Correlate a new source identity with `strace`/eBPF syscall counts if
available. Declare counter overhead and cache state, retain failures, and do
not rerun an unchanged arm. All such rows remain diagnostic until the full
public oracle, quotas, cleanup and original 25 s gate pass.
