# S7 engine cost gate audit

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Milestone state: CHECKPOINT, incomplete. S7 remains unchecked.

Starts from reconciled local main `4ecea41983b673d62db90880b777a94565eac985`,
S6 product `983c2ee6d36a4417d8fff2d14db6b141f5386c8c`, tree
`be2744223a450eaa01b9f31c4e3c850bbd141d72`. Tracker #307 agrees that S5/S6
are complete. The [S5/S6 handoff](HANDOFF-S7-S13.md) remains its stopping record.
The current owner selects [source organization](SOURCE-ORGANIZATION-S7-S13.md);
its S6-in-progress date is superseded by these identities, without editing that
unrelated side document or adopting its algorithms/capabilities.

## Implementation and observations

[Operation observations](../../architecture/36-operation-cost-observations.md)
add exact per-original-job SQL/allocation/payload receipts to daemon Completion,
separate foreground/maintenance aggregates, returned BLOB bytes, direct versus
trigger-inclusive changes, physical high-water allocation and freelist/identity
observation counts. Fixed aggregate receipts are credited through result lifetime.
All existing public paths remain; no schema, payload algorithm, SQLite version,
reservation size, resource cap, retry, sync or private namespace index is changed.

| Required dimension | Actual evidence / remaining gate |
| --- | --- |
| Complete owner SQL/queue/request accounting | Original-job receipt includes readiness parking, final attempted service and retained-result credits; native/transport requests remain S8/S9 |
| EXPLAIN + actual VM/row work | Exact payload/capture/maintenance plans and runtime profiles in [Linux overlay](checks/s7-costs/linux-layerfs-overlay.log) and [Workspace](checks/s7-costs/linux-layerfs-workspace.log); source/state checks and accounting triggers stay included |
| Bound/delivered/copy bytes | Statement bound/returned BLOB bytes plus actual cell/composed-window counters; driver/SQLite/transport/kernel copies remain unavailable |
| Allocation/storage/freelist | Entire 268435456-byte daemon reservation, actual high-water, logical pages, committed freelist credit, cookie queries, and precise Linux range calls included; sums of requested range bytes are not newly consumed disk |
| Worst/amortized/cumulative | Indexed point/keyset/cell/staircase/source/reclaim shapes below; complete installed/captured/orphan lifetime proofs retained |
| Page/dirty/journal/index/I/O | Logical page/freelist and conservative growth bounds available; exact per-operation dirty/overflow/index/journal/device-I/O peaks are not observed by the current safe driver API |
| Residency | Fixed API/codec/queue windows source-accounted; actual whole-system phase residency/pager/journal/kernel/host cache gate remains incomplete |
| Sustained service/debt | Live/idle automatic cleanup and logical debt upper bound proven; no sustained numerical arrival/service-rate or high-water acceptance campaign has run |

Linux complete 4096-byte write **plus reply-attempt release**: 17 statement
attempts, 21 SQLite executions, 727 VM steps, 13 changed rows including triggers,
5 direct changes, 128 metadata-root BLOB bytes returned, 4520 declared bound
bytes. Cell input/copy is4096 bytes, one full cell, zero partial cells and zero
codec window initialization. Two pre-BEGIN freelist queries and two range calls
request402653184 bytes in total (256+128 MiB classes); eight identity observations
use16 metadata calls. Subsequent dense read: five statements/105 VM,4160 returned
BLOB bytes (4096 data plus64 root binding/custody), no DML. Snapshot:47 pages,
0 free,192512 logical bytes,268627968 allocated/high-water bytes. No exclusive
physical I/O or resident memory number follows from those logical counters.

Linux real SQLite-full attempted operation records25 attempts/42 executions,
3006 VM,51 total/17 direct changed rows and one explicit rollback. Backed logical
counts and inode absence remain unchanged. Attempt costs are retained rather than
reported as zero because rollback succeeded. Linux owner open has4 attempts/5
executions/245 VM, one freelist cookie/range call, zero payload work and a receipt
which exactly matches its foreground aggregate. [Host count outputs](checks/s7-costs/costs-repaired.log)
retain the distinct SQLite3.51.0 profile, not a pooled platform result.

With database population N, request bytes W, staircase height S, returned keys K
and admitted jobs Q: point work isO(log N); keyset windows areO(log N+K), with
noncovering row seeks separately charged. Write touches<=ceil((W+4095)/4096)
cells; old fragmented byte history does not add cells. Partial stale-cell work
includesO(log S*log N) staircase seeks. M writes accumulate actual bounded
request/cell copies and indexed updates, without rebuilding growing file prefixes.
S6 maintains<=2 live namespace layers and<=3 parked orphan layers, then1; retained
Commit count does not enlarge current read depth. Automatic reclaim has output-sized
cumulative work in bounded weighted turns, not free/disappearing work. Known
deferred Commit collections/walks P3/P6/P7/P13/P14 remain S10 prerequisites.
A bound on these source operations does not replace the missing residency/service
proofs or make a complete integrated operation available.

## Verification and identity

Builds use Rust1.85.1 `--locked --all-targets --no-run` before test execution,
repository ARM64 flags and one construction worker. Host overlay45, Workspace32
and daemon10 pass; the independent SDK checkpoint has9 passes. Linux45/32/10 pass,
with two owned-device cases explicitly ignored. Commands,120-second ceilings,
actual walls and uncontrolled-cache declaration are in [host receipts](checks/s7-costs/host-covering-receipts.json)
and [Linux receipts](checks/s7-costs/linux-receipts.json). No hang occurred.
The longest host package command is22.017095417s; Linux is25.010065750s. These are
functional test commands, not performance samples or proof-budget exceptions.

Changed-scope all-target Clippy `-D warnings`, fmt, boundary568 and26 tool self-tests
pass. Linux retains the existing macOS-provider unused Backend warning. Raw
compiler/fixture failures remain in [failure ledger](checks/s7-costs/FAILURES.md).
[Identity receipt](checks/s7-costs/identity.json) pins source/build/workload/cache
inputs. Source-equivalent S6 physical range/admission and ext4 receipts are reused
at their original identity; the added counters do not relabel them. Native timestamp
FAIL/137 remains, and all cold speed/RSS/sustained-rate selections are NOT_RUN.

Production LOC is recorded in the checkpoint commit and exact first-parent/staged
receipt under core/target/cluster2-307/loc. Counter tools/production_loc.py remains
SHA256 c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb.
Root reference remains65417; all excluded predecessor product source stays counted.
No source retirement or algorithmic shrink is claimed.

## Next ready work

S7 requires actual complete-operation page/journal/IO/residency and sustained
service/debt observations under a prospective admissible cache/workload contract.
The new receipts supply causal attribution for that work. S9's local bound history
checkpoint is independent. S8 native acceptance requires the corrected published
fuser package and native requalification recorded in its separate audit.
