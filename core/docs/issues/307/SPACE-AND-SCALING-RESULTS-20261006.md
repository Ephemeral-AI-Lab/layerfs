# Acquisition space and scaling results, 2026-10-06

> **Status:** Free-page retention fixed in new acquisition Stores; performance and strict allocation qualification remain FAIL.

Final product source `dd43af598440847834d86fd10bbb7589e0e2cd62` batches bounded page reclamation after the first source `85a4e96711fe38ced4d09cc7f38bf205748f447c`. Both eight-case campaigns are complete and retained. New-source final Stores have zero freelist pages and empty acquisition tables; every selected row passes cold-content attestation, root/inventory/sample proof, cleanup and the unchanged30s/19s command/proof bounds. v1 has zero latency PASS; v2 has three latency PASS, zero strict allocation PASS and zero joint PASS. S7/S8/S9 runtime acceptance remains incomplete.

## Final latency and allocation versus cluster one

The control is the qualified public Project Init at `197d2fb7d0a141d7a9350852022febeec3255bf2`, with the same Durable/Disposable profiles, workloads and workers. One sample per changed source/case; no best-of selection or unchanged acceptance resampling. Reclamation is paid inside the complete product clock. The earlier83.627ms receipt remains historical.

| Profile/files | Cluster-one ns | Final ns | Difference | Speed | Cluster-one B | Final B | Difference B | Storage/joint |
| --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | --- |
| durable/100 | 79,759,708 | 84,882,625 | +6.422939% | PASS | 5,255,168 | 5,287,936 | +32,768 | FAIL/FAIL |
| durable/1000 | 201,566,000 | 227,102,750 | +12.669175% | FAIL | 20,545,536 | 20,602,880 | +57,344 | FAIL/FAIL |
| durable/10000 | 2,492,429,625 | 2,617,345,250 | +5.011801% | PASS | 305,070,080 | 305,467,392 | +397,312 | FAIL/FAIL |
| durable/100000 | 7,724,523,333 | 9,766,435,541 | +26.434152% | FAIL | 514,965,504 | 515,579,904 | +614,400 | FAIL/FAIL |
| disposable/100 | 38,747,750 | 41,161,250 | +6.228749% | PASS | 5,222,400 | 5,255,168 | +32,768 | FAIL/FAIL |
| disposable/1000 | 129,258,375 | 168,017,459 | +29.985743% | FAIL | 20,537,344 | 20,578,304 | +40,960 | FAIL/FAIL |
| disposable/10000 | 1,645,276,292 | 1,880,471,250 | +14.295165% | FAIL | 305,074,176 | 305,491,968 | +417,792 | FAIL/FAIL |
| disposable/100000 | 5,558,569,958 | 7,111,097,500 | +27.930341% | FAIL | 514,940,928 | 515,551,232 | +610,304 | FAIL/FAIL |

Speed is `10*current_ns <= 11*control_ns`; final allocated DB/WAL/SHM must be at most the same-profile control. No allowance is introduced. Raw clocks, phases, SQL metrics, page accounting, source identities and every v1/v2 row are in the [ledger](checks/space-scaling-20261006/ledger.json).

## Space mechanism and remaining overhead

Durable100000 falls from551,370,752 B in the restored campaign to515,579,904 B:35,790,848 B returned. Disposable100000 falls from551,313,408 B to515,551,232 B:35,762,176 B returned. All16 new campaign Stores have `freelist_count=0` and zero `init_operation`, `init_entry` and `init_native_file` rows. These are final allocations after the complete lifecycle, not peaks.

The final overage is614,400 B (Durable100000) and610,304 B (Disposable100000), about0.12% of the controls. New pointer-map pages and acquisition schema/packing overhead remain real allocation. Removing free-page retention does not remove those costs or turn the strict gate into PASS. Existing mode0 Stores open without implicit migration and refuse physical maintenance; these improvements apply to newly created acquisition Stores.

Each normal cleanup job touches bounded rows and removes at most512 free pages. Final source accumulates less than512 pages of reuse headroom between jobs; release/explicit maintenance drain a smaller tail. No whole-Store VACUUM, separate database, weaker durability, extra constructor or hidden background completion is used. See [implementation/selection](SPACE-AND-SCALING-PLAN-20261006.md).

## Matching100000-file reference attribution

A new owned isolated checkout runs the actual cluster-one public `layerfs_project::init`, not the old Service or a copied importer. Product seal matches the qualified reference; only a public-port diagnostic example/observer is added. The seed1/500MB source, four constructors, environment workers1 and WAL/FULL/fullfsync are the same. Current public Init uses the existing diagnostic wrapper. Instrumented/uncontrolled observations are causes/counts, not another acceptance pair. [Comparison](checks/space-scaling-20261006/attribution-comparison.json).

| Count | Cluster-one public Init | Current public Init |
| --- | ---: | ---: |
| Whole Init: SQL statements | 9,695 | 27,527 |
| Whole Init: VM steps | 14,741,551 | 61,067,367 |
| Whole Init: write commits | 429 | 546 |
| Immutable publication: SQL statements | 8,251 | 8,252 |
| Immutable publication: VM steps | 13,717,730 | 13,717,761 |
| Immutable publication: write commits | 296 | 297 |
| Saved-object location: SQL statements | 897 | 870 |
| Saved-object location: VM steps | 1,016,261 | 1,016,213 |
| Saved-object location: write commits | 0 | 0 |
| Pack/ordinal reservation: SQL statements | 524 | 532 |
| Pack/ordinal reservation: VM steps | 7,074 | 7,182 |
| Pack/ordinal reservation: write commits | 131 | 133 |

Current acquisition alone adds46,325,725 VM steps and114 acknowledged write commits. The total VM difference is46,325,816; all but91 of those extra steps are attributed to acquisition units. Immutable publication is effectively unchanged at13.72million VM steps. The remaining gap is the additional mutable acquisition work, rather than a changed committed payload. This count attribution is much stronger than inferring a cause from noisy wall ratios.

The reference retains input-sized entry/job/serial/inode/directory vectors. It supplies FileBacking to its generic filesystem constructor, but the successful fresh `base=None` branch uses a resident per-serial count array and final-row vector to avoid external reference-ordering runs. Supplying the backing and creating/removing its directory do not establish bulk run writes. The current source replaces the growing acquisition containers with indexed Store rows and bounded windows, then feeds the sorted directory/inode constructors directly. Restoring the old acquisition path would restore the old resource behavior; it is not selected. Native scratch syscall bytes/time and some reference SQL counters are unavailable, explicitly null. This explanation was corrected by the source review below; measurement receipts and verdicts are unchanged.

The phase timers report reference scan2.245s/files6.759s and current scan2.865s/files7.596s/tree0.511s/acquisition cleanup0.596s in their instrumented windows. Reference namespace construction and scratch cleanup lack their own public timer regions; do not assign its unaccounted root time to one mechanism. SQL, COMMIT, transaction, Save stages and phase clocks overlap and are never added or subtracted to manufacture an exclusive latency model. These diagnostic clocks do not establish isolated causal speed deltas.

Before correction, jobs made1614 calls/6456 statements; final source makes197 calls/984 statements. Deletion VM work falls11,777,603 to9,910,474. Batching cuts cleanup engine reprepares49 to17. Input/row bounds, order, exact charges, stale fencing and definite/uncertain outcomes are preserved. The same512-row removal job uses30,389 VM steps at both2000 and20000 stored entries. Common Storage plans/programs for both arms are in [matched plans](checks/space-scaling-20261006/common-storage-plans.json). Exact acquisition plans and the Apple SQLite3.51.0 full programs are retained in [SQL plans](checks/space-scaling-20261006/new-sql-plans.json) and public-test output. SQLite still owns bounded deletion-key/RETURNING scratch.

The four qualification tiers vary both file count and logical bytes (5/20/300/500MB). They show a larger absolute gap at100000 files but do not establish a pure scaling exponent. Source/count analysis identifies indexed per-entry work and bounded jobs; it does not prove quadratic behavior. Further latency work should target acquisition insertion/root-recording/removal and their actual page/sync costs.

## Proof, limits and retained failures

Host scope:182 public Persistence/Project test bodies passed, with final changed-scope acquisition/reclamation proofs repeated only after the endpoint and batching changes; locked all-targets Clippy `-D warnings`, formatting,659-file boundary guard,40 guard self-tests and17 harness tests pass. Every test invocation had an explicit at-most120s timeout; no test hung. Global persistence remains macOS-only; no Linux Store or full runtime qualification is claimed. Existing unrelated owner files/containers and legacy source remain unchanged.

All diagnostic outcomes remain: initial preparation path error (no Init), missing cursor-key verifier environment followed by a separate corrected proof, initial reference wrapper build failure for unavailable fields (corrected before Init), a disabled VFS observation with successful Init/proof but absent I/O totals, and an enabled current VFS diagnostic killed at its15s bound during file publication. Its partial Store and outputs are retained; no larger budget or repeat acceptance sample follows. Reference VFS aggregate counts are available; matching complete current physical-I/O/sync counts are unavailable, not zero.

The full independent Stores/binaries/raw copies are in `benchmark-results/fs-bench-pro/space-scaling-retained-20261006`; [copy manifests](checks/space-scaling-20261006/closed-copy-manifest.json), [diagnostic custody](checks/space-scaling-20261006/diagnostic-copy-manifest.json) and [post-seals](checks/space-scaling-20261006/post-seals.json) bind the retained evidence. Raw qualified reference timing/allocation receipts are unchanged. Independent verifier coverage is every path/kind/directory metadata plus deterministic sampled payload, not a full-content oracle. Phase/system memory and device I/O remain unqualified.

## Source size

`85a4e9671`: Production LOC159916 ->160062 (delta+146); core94499 ->94645, root reference65417 unchanged. `dd43af598`:160062 ->160073 (delta+11); core94645 ->94656, reference unchanged. Exact git first-parent/staged/committed snapshots, the same Rust-aware counter and shipped SQL scope are recorded in the commit messages and [LOC receipts](checks/space-scaling-20261006/batching-loc.json). Tests/examples/docs/tools and legacy inline tests are excluded. No relocation, duplicate implementation or legacy retirement is claimed.


Diagnostic isolation: qualified v1/v2 samples had no own same-worktree build or
measurement overlap. Count diagnostic clocks include instrumentation and host
activity and are not an isolated causal speed pair. The enabled current VFS
observation exceeded15s while publishing file objects; it wraps every SQLite/VFS
call and emits per-port records. It was killed and retained without increasing
the cap; this separate instrumentation failure is not a qualified product timeout.

## Parallel source and cost review, 2026-10-06

The owner requests subagent analysis of time complexity, roundtrips, and why
cluster one used fewer steps, commits and writes. Three read-only reviews cover
current complexity, current SQL/transaction accounting, and the complete relevant
historical public Project Init path. Reference is 197d2fb7d0a141d7a9350852022febeec3255bf2;
current product is dd43af598440847834d86fd10bbb7589e0e2cd62. This is source analysis
and arithmetic on retained diagnostics, not a new performance treatment or
qualification. No product change, build, test or timing invocation is included.
[Review ledger and pinned historical excerpts](checks/space-scaling-20261006/parallel-cost-review.json).

### Why the reference avoids acquisition SQL

Historical `core/crates/layerfs-project/src/scan.rs` retains every prepared entry
and construction job, collects and sorts each directory's full child list, and
keeps a BFS directory frontier. Workers consume the resident job queue; the Save
owner writes each completed content root directly into `entries[index].content`.
Historical `namespace.rs` then creates full serial/inode/directory collections.
The generic filesystem builder additionally uses fresh-build count/value arrays
and topology maps. These ordinary process-memory updates and drops have no SQL
VM, write-acknowledgment or SQL deletion cost. They still pay allocations, copies,
native enumeration/stat calls, sorting, validation and canonical construction.
The pinned excerpts include exact historical paths, blob identities and line numbers.

Both versions use four file constructors and one Save owner, with bounded object
publication and acknowledged allocation. The inspected Save tree differs only
in the corrected pack-ID bound; publication and the sorted directory/inode
constructor entry points are shared. A Save lifetime is not one SQLite transaction.
The reference's global Store still uses WAL/FULL/fullfsync. Its lower commit count
does not come from weaker immutable Store durability.

The reference native importer rejects symlinks and assigns separate serials per
regular path without native hard-link deduplication. Current Init preserves opaque
symlink targets and native hard-link identity, and rechecks the final pathname
in addition to descriptor observations. The measured unique regular-file fixture
remains comparable, but the reference is not a replacement for those capabilities.

### Local calls, SQL executions and acknowledgments

Current acquisition makes 913 local port invocations: 799 read and 114 write Session
transactions. These are in-process calls, not network RPCs. Each successful unit
uses explicit BEGIN/COMMIT, giving 1,826 SQL executions for transaction framing.
Read COMMITs do not imply journal writes or synchronization.

| Acquisition unit | Port/transaction calls | SQL executions | VM steps | Write acknowledgments |
| --- | ---: | ---: | ---: | ---: |
| Begin | 1 | 4 | 157 | 1 |
| Insert entries/native identities | 35 | 9,656 | 24,308,860 | 35 |
| Place wide-directory children | 1 | 1,004 | 93,105 | 1 |
| Complete file roots | 25 | 3,225 | 6,474,500 | 25 |
| Record directory roots | 1 | 36 | 79,203 | 1 |
| Read directory frontier | 4 | 19 | 21,254 | 0 |
| Read construction jobs | 197 | 984 | 1,611,805 | 0 |
| Read entries | 398 | 1,592 | 3,108,762 | 0 |
| Read file roots/alias counts | 197 | 788 | 708,276 | 0 |
| Read unplaced children | 3 | 12 | 9,131 | 0 |
| Discard working rows | 50 | 522 | 9,910,474 | 50 |
| Release | 1 | 8 | 198 | 1 |
| **Total** | **913** | **17,850** | **46,325,725** | **114** |

The total Init difference is +17,832 executions, +46,325,816 VM steps, +117 write
acknowledgments. Subtracting acquisition leaves -18 executions, +91 VM steps, +3
write acknowledgments. The three remaining writes are one publication and two
allocation reservations; the two history writes are unchanged. Diagnostic unit
aggregation includes a `storage.policy` observation during `Storage::new` before
the complete-Init snapshot, so summing all public-unit rows is not the definition
of that snapshot. This does not change writable-unit reconciliation.

Insertion's 9,656 executions decompose into 35 units times 4 framing/owner/charge
statements, plus 3,172 inputs times 3 statements: dependency preflight, native INSERT,
namespace INSERT. The unique fixture uses no per-entry fallback. Completion's 3,225
executions are 25 units times 4 framing/owner/charge statements plus 3,125 fixed 32-row
UPDATE inputs. Required owner checks distinguish stale handles from valid empty
windows; they cannot simply be deleted.

The retained 32-fresh-file provider diagnostic changed 68 executions/5,353 VM steps
to 7 executions/7,828 VM steps: batching removes 89.7% of executions but adds 46.2%
of VM work because preflight repeats indexed checks before insertion. The result
holds at 2,000 and 20,000 stored rows. This is an earlier source-qualified diagnostic,
not a new current timing result. Larger inputs alone do not establish an improvement.
[Retained profile/plans](checks/init-entry-window-20261006/06-persistence-covering.log).

### Complexity and cumulative work

Let E be entries, U unique native regular identities, A additional alias paths,
D directories, D' nonempty directory roots, V children of wide directories,
B processed unique content bytes, P materialized/copied/bound name/path bytes,
and N the shared indexed population including other live/abandoned operations.
Read/write/internal SQL windows remain 512/4096/32 rows and their existing byte
bounds. An operation prefix bounds useful rows; index height still depends on N.

Current metadata/index work is principally O((E+U+V+A) log N), with multiple
linear passes, fixed-window overhead and actual payload/pager work. Native path
processing requires P; an E/B-only bound would conceal deep-path bytes. Small
directory sorts are locally bounded; wide ordering uses indexed backing.
Cleanup's OFFSET is at most 4,095 within each soon-to-be-deleted prefix, so total
endpoint visits are O(E+U). No growing enumeration OFFSET or repeated global
scan establishes quadratic work in the reviewed ordinary acquisition path.
Physical pager/reclamation cost remains separately unqualified.

Logical working-row mutations are E+2U+A+V+D' before cleanup and
2E+3U+A+V+D' including cleanup, excluding operation accounting and index/page work.
For this fixture E=101001, U=100000, A=0, V=1000, D'=1001: 504,003 logical mutations,
including deletion of 201,001 working rows. These are not physical writes.

Two entry passes give 398 calls:2*(ceil(101001/512)+1 terminal read). The job and
file-root streams each give 197 calls:ceil(100000/512)+1. Cleanup needs 50 jobs:
ceil(201001/4096). The completion fullscan counter 96,875 equals 3125*31 advances
of fixed VALUES inputs; directory completion similarly reports 32*31=992. Those
counters do not indicate growing scans of the stored population.

Three additional shapes deserve separate count cases: an existing-identity
boundary can repeatedly rebuild a 32-row lookahead, bounded O(32 A log N);
4096-byte paths can make a 512-length sizing lookahead feed only about 62 useful
jobs, causing bounded repeated scalar visits; and one alias can enable a whole
extra E-entry validation pass even when A is small. They are absent from or
minor in the unique short-path fixture and cannot be credited from its timing.

Current Init directly streams indexed rows into the sorted directory and inode
constructors, with no FileBacking, generic reference reducer or whole-namespace
serial/count/value arrays. Fresh construction uses bounded page/right-spine state
and no base-tree traversal. Historical fresh construction used the same lower-level
sorted engine. The owner services SQL and Save output on the thread that feeds
the 512-job queue and accepts the four-slot result channel: SQL service can
backpressure constructors, but worker starvation/queue-wait time is not measured.

### Optimization candidates and what each can change

| Candidate | Work addressed | Limits and proof |
| --- | --- | --- |
| Reuse independent-prefix bindings; use minimal statement projections; skip provably empty native INSERTs | Rust allocation/rebinding and fixed SQL input work | Provider-local; preserve indexed dependency checks, error order, aliases and bounds. Exact EXPLAIN/profile needed; does not remove write commits. |
| Redesign duplicated preflight/insertion checks | Potentially substantial insertion VM work | Replace the evidence, rather than remove uniqueness/identity checks. Preserve first-error order and atomic outcomes; failed INSERT then replay remains forbidden. |
| Direct typed row decoding and a narrow directory-binding stream | Copied columns, row mapping and allocation in entry passes | Current first pass does not need all seven entry columns. A new projection is a port extension; SQL joins can add indexed probes instead of saving work. |
| Pass validated owner data to live discard/release helpers | 51 duplicate SELECTs | Small effect. Abandoned-operation entry points still need their own validation. |
| Single-statement owned read snapshots | Up to 1,794 framing/owner executions across 598 single-payload read units | Requires deliberate Session/port semantics and stale-versus-empty encoding. The 201 two-pass path units still require a consistent snapshot. No write syncs removed. |
| Compact the acquisition representation/lifecycle | E/U-shaped row/index insertion, root growth and deletion, potentially cleanup unit count | Larger schema/compatibility design; keep ordering, native identity/evidence, aliases, exact charges, custody and cleanup-before-publication. Benefit is unmeasured. |
| Coordinate compatible bounded atomic jobs | Commit/synchronization overhead | Owning API/transaction change; no transaction across source I/O/construction, no whole-Init transaction, no relaxed durability or increased frozen caps. |

For the observed fixture, insertion/root-recording/removal dominate. Alias-only
streams, alias lookahead and bulk wide-child placement are legitimate separate
capability targets, but they do not explain the measured main gap. The current
direct streamed constructors should remain. The report recommends concrete
mechanisms, not a forecast of parity or a new accepted candidate.

SQL executions, VM steps, row mutations, dirty pages, VFS write/sync calls and
device writes are distinct. The complete matching current VFS totals are still
unavailable after the retained observer timeout. Reference VFS submitted bytes
and older Durable100 observations cannot establish a current 100000-file write
ratio. No durability/profile/topology change or new speed/storage PASS follows
from this review.
