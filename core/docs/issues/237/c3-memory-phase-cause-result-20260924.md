# #237: where promoted C3 sets its Init memory high-water

> **Status: Research; informative and not a product contract.** One
> separate, count-driven release diagnostic of promoted C3. It locates
> high-water increases within Core; it does not rerun v0.1.6 or
> replace the earlier matched t1 memory comparison.

## Boundary and custody

The [prospective plan](c3-memory-phase-cause-plan-20260924.md) froze
point readings of Darwin `proc_pid_rusage` current RSS/physical
footprint and `getrusage(RUSAGE_SELF).ru_maxrss` at ten source-defined
boundaries. The temporary instrument is retained as a
[compressed diff](evidence/c3-phase-memory-20260924/instrument.diff.gz)
with decompressed SHA-256
`0504af8a2ba01354ecf2b6d44664084fcf0da5034fd8bd94bd8c2da23d3ed61c`.
It was removed from product source after the run. The
[freeze record](evidence/c3-phase-memory-20260924/freeze.json),
[build receipt](evidence/c3-phase-memory-20260924/build.json),
[raw trace](evidence/c3-phase-memory-20260924/driver.stderr),
[receipt](evidence/c3-phase-memory-20260924/receipt.json) and
[derived timeline](evidence/c3-phase-memory-20260924/timeline.json)
pin the source and binary identities and the exact arithmetic.

One real release `Client::init_project` call used a fresh independent
copy of the same seed-1 SHAKE 100k/500-MB manifest, fresh content and
History Stores, and 0/126,206 resident source payload pages before
launch. Source metadata residency remains unqualified; performance
status is `INELIGIBLE`. The separate
[full reopened oracle](evidence/c3-phase-memory-20260924/verification.json)
passed all 101,001 paths and 500,000,000 bytes. The command and
verifier times are diagnostic only. Raw databases, copy and manifest
remain under
`benchmark-results/fs-bench-pro/issue237-c3-phase-memory-20260924-01/`
in this worktree. Curated file hashes are in
[SHA256SUMS.json](evidence/c3-phase-memory-20260924/SHA256SUMS.json).

## C3's own phase high-water

Each increase below is the **new process-lifetime high-water between
two markers in this one Core diagnostic**. It locates an interval,
not a mutually exclusive allocation category. Current RSS can be
lower than the high-water when temporary data has been released.

| Boundary | Current RSS | Lifetime peak RSS | New high-water since previous marker |
| --- | ---: | ---: | ---: |
| SDK t0, after Host setup | 4,866,048 B | 4,915,200 B | — |
| Import begins, after first Save setup | 22,691,840 B | 22,691,840 B | +17,776,640 B |
| **Scan ends**: all entries and jobs live | 98,287,616 B | 98,287,616 B | **+75,595,776 B** |
| File loop ends | 120,668,160 B | 125,386,752 B | +27,099,136 B |
| File Save ends | 111,804,416 B | 125,386,752 B | 0 B |
| Namespace entry | 111,820,800 B | 125,386,752 B | 0 B |
| Prerequisite Save ends | 118,390,784 B | 125,386,752 B | 0 B |
| Tree input fully assembled | 134,168,576 B | 134,168,576 B | +8,781,824 B |
| Filesystem tree build ends | 153,632,768 B | 153,632,768 B | +19,464,192 B |
| Namespace Save ends | 147,668,992 B | **156,467,200 B** | +2,834,432 B |
| History published | 147,685,376 B | 156,467,200 B | 0 B |

The first **75,595,776-B scan increase** includes a measured
54,753,736 B of simultaneously retained entry/job vector reservation
and name/path lengths: entries 14,680,064 B, jobs 23,068,672 B,
names 705,000 B, paths 16,300,000 B. Vector capacity is exact;
name/path lengths are lower bounds on their separate allocations.
The difference between that inventory lower bound and the scan's RSS
increase is **not** a defensible residual heap category: path
capacities, allocator overhead, directory enumeration and other
state were not measured as disjoint simultaneous charges.

File construction and Save then set another **27,099,136-B** high-water.
The Store writer, four producers, bounded handoff and SQLite connection
run while the entry list remains live; the job queue is drained.
Current RSS fell to 111,804,416 B by file-Save return, but the
125,386,752-B high-water remained on the process record.

Namespace construction set a further **31,080,448-B** high-water
after the file-loop mark. At tree input, the serial, metadata-root,
content-root, inode and directory vectors together reserved
**18,838,440 B**, while the entry list was still borrowed. The inode
vector alone reserved 11,534,336 B; metadata and content-root vectors
reserved 3,232,032 B each. The tree builder raised high-water another
19,464,192 B after that input was assembled, and Save finalization
raised it 2,834,432 B. These vector reservations and high-water
changes are not additive components of the final RSS peak.

The earlier [matched t1 comparison](c3-v016-exact-source-memory-result-20260924.md)
records 92,405,760 B for old v0.1.6 and 154,763,264 B for its
different C3 resource diagnostic: a **62,357,504-B gap**. This new
phase diagnostic reached 156,467,200 B under its own instrumented
identity, 1,703,936 B above that C3 observation. Its scan-end
high-water alone was **5,881,856 B above the old run's entire t1
high-water**. By namespace-Save end it was 64,061,440 B above old.
Do not promote this second C3 diagnostic time or peak as a replacement
sample or choose between its high-water and the earlier one.

## Cause and fix scope

The gap is **not one SQLite-cache knob**: the earlier C3 diagnostic
measured 8,767,488 B on each large Save connection, versus the old
Store's 34,604,032-B t1 connection-cache reading. Those are point
readings on different connection lifetimes, but the larger cache is
on the old route. This phase trace instead shows two large Core
materialization intervals:

1. [`scan_and_save`](../../../crates/layerfs-service/src/save/import/scan.rs)
   constructs every `PreparedEntry` and a separate `Job` holding a
   full path and cloned `Metadata` for every file **before** processing
   file jobs. Keeping the full job list is an internal implementation
   choice, not a public SDK requirement. A bounded scan/construct
   wave or a more compact job record is the first isolated treatment
   to consider. It must keep the source identity checks, deterministic
   entry indices, four-worker limit, and one real public call.
2. [`build_namespace`](../../../crates/layerfs-service/src/save/import/namespace.rs)
   retains the entry list and materializes several full-width arrays
   before calling C1 `build_filesystem` with borrowed slices. The
   **current C1 input API** makes the full inode slice natural; it is
   an internal architecture choice, not a promise of the public Init
   API. Avoiding that second peak would likely need a deliberate
   streaming/batched builder boundary or a narrower representation,
   with full identity/readback proof. Dropping one small auxiliary
   vector alone cannot explain the 31.08-MB interval.

The scan job list is an identifiable place to simplify, but the
31.08-MB later namespace rise means **there is no proven one-line fix
for the full 59.47-MiB matched peak gap**. Test a single scan/job
representation treatment first with a prospectively frozen memory
receipt and full oracle; use its result to decide whether the larger
C1 interface change is justified. Do not increase workers, change
cache policy, or relax the #229 sparse-history gate to make a memory
number pass.
