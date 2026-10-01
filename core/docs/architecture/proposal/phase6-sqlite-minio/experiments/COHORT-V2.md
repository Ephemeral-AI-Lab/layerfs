# Phase 6 metadata adaptation of the six R1 workload shapes

> **Status: Research; informative and not a product contract.**
> Owner-selected Phase 6 prototype target, 2026-10-01; #294 under #293.
> Parent prototype/report source `d0a621eb77071768a77b1af0174371a328ed6eaa`.

## Scope and source

The owner requested the six workloads from task `Implement R1 infrastructure for
#287` and selected the Phase 6 SQLite prototype, rather than remeasuring Phase 5.
[Historical source capture](source-captures/r1-six-case-cohort.json) retains the
literal R1 case IDs/commands and source hashes. The old candidate is `b2c6bcc97`;
its published report is `5d693cc76`. Foreign trees/targets/artifacts stay read-only.

These new case IDs are **metadata component adaptations**, not the original SDK
cases. Keep original Exec/Commit/complete/proof receipts unchanged. The prototype
has no Exec, FUSE, CDC/CAS/compression, MinIO upload, namespace certification,
Branch publication or current-G2 canonical installation. Exec and full Commit
remain UNAVAILABLE; no ratio to the original elapsed times is permitted.

## Representation additions and exact selection

Reuse the current pinned native SQL stack/profile: MEMORY journal, synchronous
OFF, temp_store MEMORY, cache -512KiB, mmap zero, busy timeout zero, statement
cache64; one producer. Keep original v1 receipts and 512-row failure target.
New prototype-only tables: changed inode IDs keyed by generation, inode kind,
small immutable source values (fill/blob), and independently owned selected-view
pins. They do not allocate product format/schema/API identifiers. Base namespace
inodes start at root100; large file content uses inode1. Component-name bindings
and inode versions retain the existing born/dead semantics.

Large baseline content is a source reference describing exactly 10MiB of `A`,
not a generated/spooled/uploaded 10MiB payload. Actual replacement values are
small blob sources. This tests extent/source-coordinate metadata only. Sources
are not authenticated CAS objects. Unaffected package fixture files are omitted
from the large/many component fixtures; full original input/protocol topology is
not claimed equivalent.

New six-case registry, one sample each in this order:

| New ID suffix (`cohort-` prefix) | Pristine fixture and timed adaptation | Final metadata/source oracle |
| --- | --- | --- |
| clean-retained-4097 | root100/data.bin inode1, 10MiB A source; 4,097 one-byte writes at `(104729+i*2654435761)%10485760`, value `B+i%24`; first metadata submission; retain G1 view pin; no further mutation; second metadata submission; release pin | Exact retained dispersed bytes; final clean changed catalog empty |
| one-edit-retained-4097 | Same full prelude/first submission/pin; write X at offset0; second metadata submission; release pin | Pinned retained state exact; current byte0 X; other bytes retained |
| overwrite-4k | root100/large.bin inode1, 10MiB A source; write 4096 P bytes at offset5242880; metadata submission | Exact three-span partition and bytes |
| namespace67 | root100, packages101/old102/new103/subtree104/child105; grand106=`grand-base`, sibling107=`sibling-base`, 64 extras108..171=`x`; marker172=`baseline`; five subtree/child component renames in original order; create grand.txt.next inode173=`grand-new`, replace grand binding; metadata submission | Exact final parent/component graph, preserved 64 extras and sibling, replaced grand payload |
| components270 | root100; create 270 directories `d`, IDs200..469, each under previous; create file470=`leaf`; metadata submission | Exact 270-edge chain and terminal payload; one parent lookup per create |
| many128 | root100; create directory200 `many`; create files201..328, names f0..f127, payload `new-i`; metadata submission | Exact 128 bindings/sizes/payload sources |

Mutation callbacks are explicit direct SQL-prototype operations, not execution of
the shell commands. Root/parent kind checks use indexed inode IDs. Names remain
components, and rename never rewrites descendant paths. The prototype covers
this fixed valid schedule, not general POSIX cycle/permission/orphan/handle rules.
Removed grand inode/source retention is reported explicitly, not called garbage
collection of all unreachable payloads.

Metadata submission captures a generation in a short transaction, streams only
its changed inode records and required selected extent/child records through
keyset pages, then performs separate completion: clear captured changed IDs,
release submission, retire eligible inode/name/extent versions in 128-row batches.
One submission per Workspace and a distinct selected-view pin remain separate
owners. Retirement respects the oldest pin. Source-value GC, authenticated roots,
READY/final publication, exact Unknown and canonical adoption are NOT_RUN.
No candidate output supplies expected state. Python direct path/dictionary and
bytearray replay oracles independently check all final bindings/payload bytes,
selected metadata result streams and retained-view contents.

## Windows, timing and evidence

- Integer pages <=128 rows x <=7 int64 fields (<=7168 field bytes); name pages
  <=128 rows, names <=255 bytes, total admitted result field bytes <=16384.
  Sparse metadata references do not prove payload streaming or physical memory.
- Separate raw phase timers: prelude mutation, prelude metadata prepare/complete,
  final mutation, final metadata prepare/complete, retained-view result delivery.
  Metadata preparation includes capture, selected catalog and result output;
  completion includes the declared cleanup. No phase is called Exec/full Commit.
- Prelude remains inside operation and complete-command timers. Engine open/
  configuration is setup and reported separately; no per-submission reopen.
  Source-value SQL insertions and actual result-consumer writes are timed.
- Prepare closed masters once, independent ordinary byte copy per case, fresh
  output, one selection per new case/arm; no v1 or original R1 remeasurement.
- Numerical cache remains uncontrolled/INELIGIBLE, performance_claim=false and
  admission_eligible=false. No cold or product-speed claim. Complete commands
  <=15s (including components270 here); separate proof watchdog9.5s. Original
  R1's25s component exception is historical, not a new allowance.
- Record exact source/tree/binary/lock/spec/oracle/source-capture hashes, native
  version/profile, row/statement/VM/fullscan/sort counts, transaction maximum,
  catalog counts, source/reference/state sizes, preparation/clone/verification
  walls and cleanup. Memory remains UNAVAILABLE unless actual provider observes
  it; no zero-use claim or physical-containment substitution.
- Worktree-local nonblocking measurement lock, sealed release --locked build and
  root ARMv8 flags; no build overlaps timed work. Preserve all FAIL/TIMEOUT/
  NOT_RUN receipts and independent original evidence. No third-party edits.

Correctness requires complete declared metadata/payload-source equivalence,
valid nonoverlapping file extents, exact captured streams/pin, zero changed keys/
submissions/pins after completion and no stale selected version. Runtime cleanup
means those owners and eligible versions are drained; payload source GC is a
separate NOT_RUN capability. Representation targets remain cursor field bytes
<=16384 and changed rows per transaction<=512. Neither target is a user file/
edit/byte ceiling. Any failure remains partial even if commands fit their bounds.
