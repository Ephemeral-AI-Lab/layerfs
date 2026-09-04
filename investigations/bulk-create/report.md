# Bulk create/delete: bounded immutable-root feasibility investigation

2026-09-04. Private prototypes on `codex/bulk-create-feasibility`; no production
integration, release change, merge or publication.

**Verdict:** five seconds for the prescribed 100k-file / 500-MiB create lifecycle
is **not demonstrated and not credible through Commit tuning alone**. It remains
a conditional research target requiring much cheaper live operations, bounded
staging/compilation, and checked final-state admission/refresh. Three seconds is
**unsupported and substantially higher risk**, not proven impossible. The latest
colocated candidate takes **107.908 s**. Dense-delete Commit can already be cheap:
**0.05565 s at 100k** in the original survivor experiment. A subsequent bounded
directory-page cache candidate reduces the observed delete lifecycle to
**30.168 s** (Exec **30.128 s**, Commit **0.03323 s**); see section 6. This still
misses the target, and is not a controlled 69.93→30.17 cache-only speedup.

The useful v0.1.1 lesson is to construct the final reachable state once. This
investigation applies that lesson in two different ways: final reference counts
for new inodes, and bounded survivor reconstruction for dense deletion. Neither
requires importing into an empty Store. **Delete changes the new root's reachability;
it does not delete historical content chunks from SQLite.**

## 1. Source identity, environment and measurement scope

Worktree: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-bulk-create-feasibility`.
Starting commit: `7a6e119acba8e5a7ecd22d96c24a42a4413b10af`, selected from
`codex/v013-phase1`, containing functional capacity repairs
`fbf32e84662d00993c033515e113437965395494`. We did not assume `main` was current.
The original checkout and Phase 1 ledger remain untouched. Later implementation-
branch benchmark/report changes were inspected, not silently incorporated.

Read all eight requested documents: the v0.1.3 bulk optimization notes, tiny-file
churn, testing rules, execution contract, ordinary execution contract, failure
repair amendment, and v0.1.1 architecture shift and namespace optimization spec.
The initially untracked optimization notes and their digest are retained in
[evidence](evidence/). Source-bound receipts, commands and input hashes are retained
per attempt; [follow-up summary](evidence/followup-summary.json) extracts raw timers.

Initial diagnostics acquired the shared measurement lock. The user subsequently
explicitly authorized independent exploratory containers without that lock.
Follow-ups use private containers, anonymous volumes, source/build caches and
sample Stores, with **2 CPUs, 2 GiB memory and memory+swap, 256 pids**. Builds,
preparation, measurement and verification within this investigation are serial.
Shared physical hardware interference is allowed: these results **do not qualify
under the frozen macOS/Docker profile**. No messages were sent to the other task
for these runs. Only selected inputs were prepared; existing qualified immutable
inputs and the sealed workload helper were reused.

The measured lifecycle is the sum of public SDK **Create → complete Exec →
Commit → visibility query → End**. Exec retains generation, the witness tree,
all prescribed POSIX operations, metadata normalization and root fsync. Nested
phase timers are never added twice. Independent verification, source builds,
fixture acquisition, process orchestration and outer container disposal are
reported separately. Product spool cleanup remains inside the product lifecycle.

[Retained corrected baseline](evidence/retained-summary.json), three create seeds:
median lifecycle **188.072 s**, Exec **95.260 s**, normalization within Exec
**65.044 s**, Commit **93.181 s**, refresh within Commit **44.215 s**. Individual
phase medians need not sum to the total median. No corrected baseline was rerun.

[Historical initializer](evidence/historical-100k-result.json): **2.766280 s** for
100k prepared files / **500 decimal MB**, eight host producers, eligible empty
Store; final inode/root construction **0.132914 s**, 131 admission transactions.
Its roughly 422k candidates / 543 MB are comparable in cardinality, not identical
work. It excludes source creation/normalization. SQLite stepping 0.886396 s and
commits 0.409148 s overlap the larger construction pipeline. Host CPU was about
13 seconds. It establishes an efficient construction reference, not a live POSIX
result or valid existing-Store failure cleanup.

## 2. Confirmed source bottlenecks and semantic constraints

### Live operations and ownership

[Workload](../../benchmark/fs-bench-pro/ordinary_workloads.rs) creates with
exclusive open, retries `write_at`, closes, then normalizes survivors with chmod
and utimensat through [set_metadata](../../benchmark/fs-bench-pro/workspace_common.rs).
100,634 normalization records mean **201,268 metadata syscalls**. The separately
reported 633 workload chmods omit normalization calls. The older proxy sample
has roughly 906k FUSE callbacks, including 203,216 lookups, 100,217 getattr,
201,901 setattr and 100k each create/write/flush/release.

[FUSE](../../crates/layerfs-fuse/src/filesystem.rs) applies setattr mutations then
returns attributes; create pins, release unpins, root fsync drains dirty state.
[Host mount](../../crates/layerfs-fuse/src/host_mount.rs) uses one FUSE worker;
[adapter](../../crates/layerfs-fuse/src/adapter.rs) uses one-second attribute TTL.
[ProxyClient](../../crates/layerfs-fuse/src/proxy_client.rs) has one TCP connection,
complete-directory positive/negative lookup caching, local attributes, reserved
NodeIds and new-directory handling. Thus many lookup/getattr requests, including
post-setattr attributes, already avoid remote lookup.

[Protocol](../../crates/layerfs-fuse/src/protocol.rs) already sends Write, reserved
create and release without immediate replies, retaining host failures until a
Fence/fsync acknowledgement. **Not every callback is a synchronous RPC.** Chmod
and mtime still synchronously contact the host. More threads/connections do not
pipeline this workload's sequential dependent syscalls.

Longer valid kernel TTLs, exact no-op metadata detection and fewer repeated base
binding lookups can remove work. Each requires correct ownership epochs,
invalidation, active-state/error checks and mutation ordering. A cached value
alone is not authority to acknowledge a mutation or reserve storage. Reads,
reopen, links, namespace conflicts, resource admission and barriers need current
validated state. Retain errors at their proper acknowledgement point.

To eliminate per-file remote metadata validation, use a **single authoritative
mutation owner near FUSE**. It must own current namespace/inode state, quotas,
handles and operation errors, with explicit SDK/Commit/Discard/pause/recovery
handoff. Validate before success; do not acknowledge early and defer validation.
Colocating existing Workspace/FUSE removes TCP but, as measured below, does not
remove enough local work. It also moves host CPU into the two-CPU cgroup.

### Nonempty closed-create batching

[ProxyClient](../../crates/layerfs-fuse/src/proxy_client.rs): nonzero writes enter
the write buffer; unpin calls `flush_write_locked`; `send_buffer_locked` removes
`PendingCreate`, sends reserved-create plus Write, leaving nothing for the
closed-create queue. This confirms the bypass.

The existing wire already supports offset/payload writes and optional mtime:
128 files, 128 writes/file, 16,384 aggregate writes, 16-MiB payload, 17-MiB frame;
client batches close at 128 files / 1 MiB. All prescribed file sizes fit.
A new bulk SDK API is unnecessary. But the
[host batch handler](../../crates/layerfs-workspace/src/projection.rs) can return
on write/mtime error with a batch-owned pin still held and only a prefix applied;
the generic port handler also ignores a short successful write count. Workspace
currently performs all-or-error append rollback; the batch boundary must enforce
its actual guarantee explicitly.

Before retaining nonempty files: read/reopen must drain the matching inode;
rename must validate both parents and destination (uncached does not mean absent);
hard links must preserve shared inode identity; flush/fsync must preserve error
ordering; truncate/delete must not resurrect buffered bytes; partial failure must
release owned pins and retain valid retry/error state. Open-unlinked data must
survive until the last handle. A bounded queue cannot hold all 500 MiB until the
later metadata pass. This unsafe one-line enablement was rejected, not measured.

### Mutable staging, construction, admission and refresh

[File I/O](../../crates/layerfs-workspace/src/file_io.rs) opens one spool per inode,
checks path/descriptor identity and high-water, appends with rollback, observes
allocation and installs the piece tree. At 100k: **100k spool opens**, 500k physical
allocation observations, 720,896,000 allocated bytes for 524,288,000 payload bytes.
The existing inline budget is 8 MiB, not permission to buffer 500 MiB in RAM.

A bounded segment backing can store immutable slices shared by independent file
piece trees. Charge allocation once per segment; retain slices through versions,
read plans, aliases and open-unlinked handles. Roll back only an exclusively
owned uncommitted tail, never truncate shared data to one file's length. Rewrites
need dead-range accounting/backpressure. Fsync, checkpoints, recovery, rebase and
Discard must follow segment ownership. The measured segment primitive below is
encouraging but does not yet implement those filesystem semantics.

[Frontier Commit](../../crates/layerfs-workspace/src/changes.rs) is already the
right sparse-change fallback. Repeated per-directory batches, intermediate inode
reference counts, final candidate conversion and refresh are avoidable work.
The current create still emits roughly 412k candidate objects / 567 MB through
**3,234 admission transactions**, usually capped at 127 objects.
[Refresh](../../crates/layerfs-workspace/src/lifecycle.rs) clones live nodes and
resolves paths again; the latest 100k Commit still reads **703,315 SQLite rows /
2.196 GB**. Reuse candidate identities bound to the successfully published root
while retaining attribute, alias and open-handle checks.

Final directories and inode counts should be built once when density warrants
it. Reuse exact portable metadata and canonical small-content builders where
eligible; the existing shortcut applies below 8,192 bytes, not every file.
Move owned slabs and carry bounded admission across directory boundaries using
normal **nonempty-Store membership/collision checks and conditional publication**.
The historical insert-tree capacity was 9,124,352 bytes, exceeding today's
8-MiB final-delta budget. New bounded ordered construction or compact/spilled
pairs is needed at that scale; calling the historical builder blindly is invalid.

[Capture](../../crates/layerfs-workspace/src/capture.rs) is single-file Running/
Ready state, not a multi-file pipeline. Safe overlap needs a fixed worker set and
bounded queue of private immutable content generations. Rewrites/truncation,
aliases, failed writes and deletion must invalidate versions correctly. Admit
only final reachable versions after quiescence; eager persistent CAS admission
must not accumulate discarded data. Drain workers and remove private artifacts
before Discard/End. Moving compilation into the final fsync without overlap
cannot improve total time.

## 3. Experiments, evidence and decisions

Each comparison runs one selected seed once per arm. Small tiers diagnose a
mechanism; the two 100k runs below test actual scale. Shared-host timings are
exploratory, not statistical speedup estimates. Every successful small A/B arm
passed canonical and fresh-FUSE-remount verification. No production path was
integrated or resource limit raised.

| Hypothesis and source | Observed result | Decision |
|---|---|---|
| Native Linux can execute the identical sealed 100k create workload below 5 s; `4fb02d46`. [Evidence](evidence/native-500-s1-r2/) | Planning + workload **2.566234 s**; normalization 0.376810 s; root sync 0.776943 s; separate verification **5.639 s**, outer disposal 2.333 s. | Supports continued research. No FUSE/Workspace/CAS; not a lifecycle or hardware lower bound. |
| Shared 4-MiB staging segments eliminate per-file physical opens; `78f254da`. [Evidence](evidence/segments-500-s1/) | **126 segments**; generation/stage 0.833903 s, cleanup 0.571631 s, planning 0.190304 s; RSS **24.9 MB**. Full byte comparison passed separately. | Pursue backing integration. Rollback/retained-slice primitive check passes; complete filesystem semantics unimplemented. |
| Host-boundary serial replies are costly; `8fce40e8`. [Evidence](evidence/rtt-10k/) | Checked Python echo, 10k requests: mean **167.765 µs** across Docker/macOS, **46.307 µs** Linux loopback. | Directional evidence only; includes Python overhead. Do not retain per-file remote acknowledgements as the target design. |
| Existing colocated owner alone is sufficient; `5a5db40d`. [Evidence](evidence/colocated-r1/) | 100k Exec **32.147 s**, Commit **126.901 s**, lifecycle **159.195 s**. Small qualification passed; separate 100k verifier OOM exit 137. | Reject colocation alone. Preserve failed Store and original evidence. |
| Exact metadata reuse removes significant Commit time; `fd7655ca`. [Evidence](evidence/container-metadata-10-s1-r2/) | 2k files: builds **2,143→2**, but Commit **0.734916→0.740459 s**. | Mechanism works; deprioritize standalone caching. File/content proofs match; namespace-root equality not claimed. |
| Larger existing frontier batches fix construction; `21c29290`, harness through `e58c54b4`. [Create](evidence/container-frontier-10-s1-r3/), [delete](evidence/container-frontier-10-s1-r5/) | 2k create flushes 34→3, Commit 0.728382→0.692108 s. Delete flushes 17→2, Commit 0.226535→0.275243 s; deferred peak grows to 4.204 MB. | Stop batch-size tuning. Fewer flushes alone give no decisive improvement. |
| Build dense-delete state from survivors, not removed inode updates; `b5dd2829`. [A/B](evidence/container-dense-delete-10-s1-r2/) | 2k delete Commit **0.226260→0.017289 s**, namespace 0.224763→0.015298 s; final 335 inodes/334 edges, insert capacity 41,216 B. | Strongest delete mechanism; advance once to 100k. |
| Encode each new inode with its final reference count once; `e5330e2c`, runner `a46ce7f3`. [A/B](evidence/container-final-create-10-s1/) | 2k create namespace **0.133512→0.007404 s**, Commit **0.733520→0.590594 s**, flushes 34→17. Candidate/admission counts unchanged. | Keep prototype. Unchanged Exec varied 2×, so do not attribute total lifecycle difference to it. |

### 100k dense deletion: final-state reconstruction works

[Scale evidence](evidence/container-dense-delete-500-s1/), binary source
`b5dd2829`, runner `249e570a`: Create **0.001789 s**, Exec **69.760838 s**,
Commit **0.055652 s**, query **0.000060 s**, End **0.108383 s**; call sum
**69.926721 s**. This is a verify-mode diagnostic: verification is outside the
call sum, and the dispatch-through-return window is 70.910641 s. Preparation's
local input copy/validation took 0.859310 s separately.

Commit walks **335 survivors / 334 edges**, with a **41,216-byte** insert tree,
344 candidates (334 reused, 10 inserted) and 2,140 snapshot rows. Namespace takes
0.022817 s, refresh 0.030902 s. It reuses the normal candidate/publication path.
No linked dirty content or additions/renames are eligible. A survivor/directory
cursor bound and conservative existing-policy memory reservation force fallback
for sparse/mixed/large-survivor cases. It does not walk deleted subtrees merely
to construct their removal from the final inode table. The live POSIX deletion
still traverses and unlinks every prescribed file.

Canonical and fresh-FUSE verification passed the surviving witness. SQLite file
size grew by 64 KiB (672,399,360→672,464,896), freelist remained 0; size alone is
not proof of retention. Cgroup peak **928,047,104 B**, no OOM/swap/throttle;
product process CPU **61.75 s**, total cgroup CPU about 68.57 s including verification.
This deletion binary predates the opt-in SQL trace repair below; no 100k delete
timing with that repair is claimed. The remaining dominant problem in this sample
is live-operation CPU/work, not Commit or deleting physical chunks. The next
diagnostic uses repaired instrumentation. There is no five-second delete lifecycle
claim.

### Immutable roots and bounded instrumentation

[Focused persisted-Store check](evidence/container-immutable-cas-r2/), source
`2c30e7ef`, proves every one of **51 unique preexisting SQLite object rows** has
identical id and bytes after deletion and Store close/reopen (600 empty fixture
files share objects). It also reads deleted content through the old root, checks
surviving hard-link identity/refcounts, and reads an open-unlinked handle across
Commit until final unpin. New roots remove bindings; old roots/content remain.
This is the correct LayerStack deletion rule, not a future GC optimization.

The check exposed a verification-harness issue: under `test-instrumentation`,
[schema.rs](../../crates/layerfs-layerstack-store/src/schema.rs) retained every SQL
statement string in an unbounded thread-local vector even when nobody requested
it. The benchmark never consumed this trace. The private repair makes capture
opt-in at `reset_sql_trace`, keeping explicit trace assertions available.
The trace/authenticated-read regression passed, as did an assertion that default
execution collects no SQL strings. Integrity, collision checks and counters stay
active. This removes unnecessary CPU/allocation work; it is an instrumentation
repair, not permission to drop verification. The old OOM is not retrospectively
relabeled as fixed evidence.

### 100k creation with final counts and opt-in SQL trace

[Scale evidence](evidence/container-final-create-500-s1/), source
`736fa13fa7a824f12b747014d8623f3523076f26`. Metadata cache and larger batches are
off. Final new-inode counts are on; no new workers or staging representation.

| Lifecycle phase | Seconds |
|---|---:|
| Create | 0.001222 |
| Complete Exec | **29.095951** |
| Commit | **78.739488** |
| Visibility query | 0.000059 |
| End | 0.071539 |
| **Complete lifecycle** | **107.908258** |

Exec completed **100k file writes / 524,288,000 bytes**, 100,634 normalization
records; normalization 5.366064 s and root sync 0.308867 s are nested in Exec.
Commit content 29.904022 s; namespace **0.156165 s**; candidate finish 7.645111 s;
local admission 2.992200 s; object admission 10.535728 s; publication 0.002731 s;
refresh **27.431951 s**. These top-level attribution phases exclude their nested
SQL timing breakdown. Counters: 100,633 precounted inodes,787 inode flushes,
1,868,160-byte deferred peak,411,653 candidates /566,847,850 bytes,3,234 admission
transactions,703,315 snapshot rows. Final counts remove repeated namespace work;
content construction, admission and checked refresh remain large.

The performance process used **77.60 CPU-seconds**, sampled RSS peak
**375,857,152 B**; cgroup CPU delta 84.772 s, peak **1,724,968,960 B** including
page cache, no OOM/swap/throttling before verification. That is lower process
memory than earlier instrumentation-heavy runs, but no controlled 100k trace-only
A/B was run: do not assign the entire timing/memory difference to either change.

The offline Cargo release build took 44.08 s (44.346 s build command) separately; process command wall 108.226762 s
versus 107.908258 s pure calls. Fixture copy, Docker startup and disposal remain
separately recorded in `commands.json`; none is hidden inside a performance claim.
Independent canonical and fresh-FUSE-remount verification **both passed**, checking
100,968 paths, 100,200 regular files and 525,336,576 bytes including the witness.
The separate verification command took **252.621721 s**. The cgroup peak remained
1,724,968,960 B through verification, with no OOM, swap or throttling. This is a
new source-bound successful result; it does not erase the earlier OOM or establish
a trace-only causal speedup. Product spool observations were zero after End; the
cleanup check found no FUSE mount or task worker, and owned container/volume
disposal completed in 0.291981 s outside the measured lifecycle.

### Preserved failures and rejected paths

Every attempt retains source/commands/exit codes; no result was overwritten or
relabeled. Native first preparation failed root mode/mtime qualification (no
workload). Metadata first build was terminated at stalled network index (143).
Frontier attempts 1/2 failed test-instrumentation imports/debug-only hooks; attempt 3
completed create but delete lacked a branch receipt; attempt 4 lost executable
mode when copying the reused binary (127); attempt 5 completed delete. Dense-delete
first build failed authenticated-reader trait selection; repaired before any
measurement. Initial persisted-SQL check failed `DatabaseBusy` because it inspected
while Store still owned its lock; the corrected test closes/reopens owners.
Colocated initial 100k verification OOM and its Store remain preserved locally.
These are not evidence of filesystem impossibility or successful qualification.

Rejected without claiming measurements: deferred-success metadata, all-RAM 500-MiB
staging, unbounded workers, skipping normalization/integrity/cleanup, direct
empty-Store initializer use, speculative persistent CAS accumulation, and physical
CAS deletion as part of unlink/Commit.

## 4. Five-second critical-path budget and three-second stretch

This is a **falsifiable allocation, not a measured prediction**. Live-operation
and admission budgets remain far from demonstrated. Private compilation can run
concurrently with later creation only with bounded resources and correct version
invalidation; count just its tail after Exec, not overlapping timers twice.

| Serial phase | 5-second allocation | Required advance |
|---|---:|---|
| Create |0.020 s|Real attach/startup. |
| Complete Exec |**2.600 s**|Authoritative local operations, cheap binding/metadata handling and staging; native 2.566 s is only a comparator. Colocated 29.096 s currently misses by 11×. |
| Commit private-compilation tail |0.100 s|Fixed bounded workers keep up during Exec; no hidden serial fsync compilation. |
| Final directory/inode state |0.180 s|Bounded construction once. Current 0.156 s is only the residual namespace phase; other structure remains charged within content. |
| Final candidate admission |**1.200 s**|Carried owned slabs, efficient nonempty-Store checking; historical nested SQL≈1.296 s is a reference, current 10.536 s misses. |
| Publication and checked refresh |0.120 s|Reuse authenticated final identities; current refresh 27.432 s misses badly. |
| Private staging/compiled cleanup |**0.570 s**|Segment cleanup primitive 0.572 s; combined artifact cleanup unmeasured. Retain SQLite historical chunks. |
| Query |0.010 s|One visibility query. |
| End |0.200 s|Join all task workers, complete owned cleanup. |
| **Total** |**5.000 s**|No headroom; substantial independent gates remain. |

Five seconds requires 20k files/s and 100 MiB/s; three requires 33,333 files/s and
166.7 MiB/s. Under two CPUs, five seconds supplies at most 10 CPU-seconds for all
colocated work, versus 84.772 currently observed. **CPU work must be eliminated,
not merely parallelized.** With a macOS host outside the cgroup, a bounded host
compiler can supply additional CPU, but requires ownership handoff and measured
transport/overlap. Historical construction's 13 host CPU-seconds cannot simply be
moved unchanged inside the two-CPU container and expected to finish in 3–5 s.

A three-second screen would require Create 0.02 + Exec **1.80** + Commit **0.99** +
query 0.01 + End 0.18. Within Commit: tail 0.02, structure 0.10, admission **0.55**,
publication/refresh 0.06, cleanup **0.26**. None is established. Exec must beat the
native helper comparator, admission/cleanup more than halve historical/primitive
observations, while all checks and bounds remain. Three seconds is therefore not
a supported production commitment.

A 3–5-second **full CLI command** additionally needs setup and teardown inside that
same budget. The retained macOS sample has 0.523 s preparation and 0.870 s total
process-minus-call residual (not a pure CLI timer). Similar overhead would require
roughly ≤3.6 s lifecycle for a 5 s command. A ready runtime may help, but changes the
fresh-container profile; no task workers may remain after End. First-use builds
and image acquisition cannot be assumed free. Independent full verification stays
separate; the native comparator's verifier alone took 5.639 s.

## 5. Recommended sequence and smallest next experiment

1. **Keep the instrumentation repair and final-count prototype separate for
   review.** Preserve explicit SQL-capture tests. Final-count production review
   must cover renamed new directories, aliases, replacement, error rollback and
   disconnected/open nodes; this investigation's focused checks are not exhaustive.
2. **Use bounded survivor reconstruction for eligible dense deletion**, retaining
   the existing incremental path for sparse/mixed changes. Review conservative
   memory accounting and fallback before broadening eligibility. Test publication
   conflict/injected failure through the normal candidate path. Never delete old
   SQLite chunks; final-state omission is sufficient.
3. **Measure live lookup/metadata work next.** Smallest next experiment: one
   `tiny-bulk-delete-10`, seed 1, diagnostic with counters for base directory lookup,
   already-materialized binding hits, authenticated reads and metadata loads.
   Hypothesis: readdir/lookup/unlink repeatedly resolve the same immutable binding.
   If confirmed, test one bounded directory-binding cache with overlay precedence,
   epoch invalidation on rebase/Discard and correct rename/alias behavior. Predict
   fewer authenticated reads/CPU; require that result before another 100k sample.
   This attacks the 69.76 s delete Exec and informs create lookup handling.
4. **Integrate bounded shared staging**, with independent file slices, checked
   append rollback, aggregate allocation, open-unlinked lifetime and cleanup.
   Reuse existing piece/read machinery where possible; invent a simpler backing
   if it satisfies the same bounds. Do not merely disable allocation observations.
5. **Build final candidate structures and checked refresh once**, then carry
   bounded admission across directory boundaries with normal collision/member
   checks and conditional publication. Preserve historical roots on failure.
   Metadata interning alone and larger flush counts are lower priorities.
6. **Add bounded private content overlap only after those pieces are sound.**
   A fixed small worker pool, generation checks, backpressure and full cancellation
   are prerequisites. Activate nonempty closed-create batching only after repairing
   its read/reopen/rename/partial-failure/pin-cleanup boundary. Run focused correctness
   first, broad affected verification once stable, then source-bound qualification
   only if production implementation is separately authorized.

Within the current architecture, final counts, dense survivors, bounded backing,
carried admission and identity-aware refresh can eliminate substantial work.
Achieving the 5-second live budget likely needs a different authoritative ownership
or deployment arrangement as well. Colocation alone has been tested and rejected
as sufficient. The remaining uncertainty is how cheaply a correct local owner can
execute all prescribed syscalls and keep bounded private compilation/admission
fed; this investigation does not confuse that unproven requirement with success.

All prototype commits are local and reviewable. Principal product experiments:
`8bd5de4d` metadata reuse, `21c29290` frontier batching, `07ba9f24`/`b5dd2829`
dense survivors, `e5330e2c` final counts, `a6985d41`/`177dc99a` retained-CAS test,
`2c30e7ef` opt-in tracing. Latest 100k candidate: `736fa13f`. Attempt source files
and binary hashes, including reused-binary versus runner identities, are in each
evidence directory. Scripts refuse existing output directories; new work must
use a new attempt identity. No production integration is recommended solely from
favorable timing: preserve these prototypes for review and test the live-operation
gate next.


## 6. Follow-up: live deletion and repeated FUSE directory pages

The Exec timer covers the public SDK `Client::exec_workspace_session`, waiting
for the sealed workload process to finish against the real LayerFS FUSE mount.
See [SDK caller](../../benchmark/fs-bench-pro/src/main.rs#L6515) and
[recursive POSIX deletion](../../benchmark/fs-bench-pro/ordinary_workloads.rs#L708).
The SDK launches/waits once; the workload's lstat/readdir/unlink/rmdir operations
then enter FUSE and the Workspace. Thus the slow timer is mostly filesystem work,
not SDK process-launch overhead. In this diagnostic, both owner and workload are
colocated in Linux; there is no macOS proxy TCP round trip per operation.

**Hypothesis:** the existing FUSE callbacks request a complete directory vector
on every kernel page, then skip to the supplied offset. The prescribed wide
directory contains 32,000 files at the 500-MiB tier. Repeated reconstruction and
dropping of whole listings makes work grow with directory size times page count.
Prediction: caching a completed listing across pages reduces Workspace listing
loads and rebuilt entry counts without changing syscalls or final contents.

Prototype source **`22b31552`** adds a cache per listing form in the existing FUSE
adapter. It holds at most one plain and one plus listing, capped at 4 MiB each
including vector/name capacities; oversized lists use the original fallback.
Offset zero reloads; releasedir frees the matching cache. The Host port supplies
a checked active-Workspace incarnation and mutation-generation token. Any
mutation or Workspace replacement invalidates reuse, including replacement with
the same immutable root but different materialized NodeIds. Unsupported ports
return no token and retain uncached behavior. No new worker, SDK bulk operation,
physical chunk deletion or resource-limit increase is involved.

[Small A/B evidence](evidence/container-directory-pages-10-s1/), 2,000 files,
seed 1, one run per arm using the same binary with the SQL-trace repair and dense
survivor construction enabled in both:

| Observation | Cache off | Cache on |
|---|---:|---:|
| Complete Exec | 0.338795 s | 0.327619 s |
| Complete lifecycle call sum | 0.357143 s | 0.348764 s |
| FUSE directory requests | 303 | 303 |
| Full listing loads | 303 | 286 |
| Entries rebuilt | 10,720 | 4,856 |
| Cache hits | 0 | 17 |

Both canonical and fresh-FUSE verification passed. The focused retained-root,
hard-link and open-unlinked regression also passed. The small timing difference
is not a significant speedup claim; counters establish that work was removed.
Its wide directory has only 640 entries, so one scaling run was warranted.

[100k scale evidence](evidence/container-directory-pages-500-s1/), binary
`22b31552`, runner **`b616ab3f`**, reused qualified seed-1 input:
Create **0.001684 s**, Exec **30.128114 s**, Commit **0.033233 s**, query
**0.000047 s**, End **0.004652 s**; complete call sum **30.167729 s**.
The 2,088 FUSE directory requests needed 1,266 full listing loads, rebuilt 203,796
entries and hit the cache **822 times**. Product process CPU was **21.81 s** and
peak RSS **99,721,216 B**. Whole-container peak, including separate verification,
was **159,010,816 B**; CPU delta about **26.216 s**, no OOM, swap or throttling.

The sealed workload completed all 100,000 unlinks and 633 directory removals.
Canonical and fresh-FUSE verification passed the surviving witness. The
verify-mode dispatch window was **31.168936 s**, versus **30.167729 s** product
calls; verification/observations outside the calls are not part of the lifecycle.
SQLite grew by the same 64 KiB and its freelist remained zero. Owned container,
volume and workers were removed. Preparation and outer teardown are recorded in
`commands.json`, separately from the product call sum.

**Decision:** keep this bounded page-reuse prototype for review, but do not claim
a 5-second lifecycle or production readiness. The earlier 69.93-second sample
predates opt-in SQL tracing as well as this cache; changing instrumentation and
shared-machine conditions prevents attributing the full difference to caching.
The same-binary small A/B and the scale counters support the mechanism. The
remaining 30 seconds and 21.81 process CPU-seconds demand further live-operation
work reduction. The next diagnostic should attribute immutable directory-binding
lookup, inode materialization and metadata decoding during lstat/unlink; a
bounded binding cache should only be added if those counters justify it.
Before production integration, add targeted mutation-during-enumeration,
rewind/reopen, cache-eviction and same-root Workspace-replacement regressions.
The current checks establish prescribed-workload correctness, not exhaustive
concurrent directory-stream semantics.

## 7. Priority correction after reviewing v0.1.1 again

The v0.1.1 architecture shift explicitly identifies repeated path resolution,
persistent point updates, intermediate structural retention and 127-object
admission as the old failure pattern. The current create path still contains
these mechanisms. Exact metadata interning was already known not to deliver a
standalone wall-time win; its value was as part of a corrected pipeline. The
investigation applied final-count/survivor construction, but devoted too much
attention to isolated caches and batch size before addressing that full pattern.

For create, prioritize a bounded candidate-identity handoff into checked refresh
first: eliminate per-path rediscovery without deleting integrity/alias checks or
spool cleanup. Then construct final directories/inodes once and move bounded
owned slabs through normal nonempty-Store admission, carrying batches across
directory boundaries. Preserve the sparse-change frontier. Treat exact metadata
reuse and canonical small-file construction as parts of that pipeline. Shared
mutable segments and versioned private compilation address Exec and overlap;
authoritative live metadata handling remains a separate owner/FUSE requirement.
The residual 156-ms namespace timer excludes structural work charged in the
29.90-second content phase; it was not proof that whole-tree construction was solved.

Source review also found an invalidation gap in the directory-cache prototype:
in-place rebase reset the mutation generation without copying the newly allocated
Workspace incarnation. The repair copies that incarnation and extends the focused
retained-root test to assert the token changes across Commit. This affects a
stream retained across Commit, not the measured delete workload, which closes
its directory streams before Commit. The old timings remain bound to `22b31552`;
no new performance measurement is claimed for the repaired source. The focused
check passed (including byte-exact retention of all 52 preexisting object rows
in this fixture); evidence source `c2713153` is retained under
`container-directory-epoch-regression`. No performance run was repeated.

## 8. Native Linux delete comparator

[Evidence](evidence/native-delete-500-s1/), source `a20bdb14`, uses the same sealed
helper, seed 1, prescribed tree and native Docker volume under 2 CPUs / 2 GiB.
There is no LayerFS SDK, FUSE or Commit. The existing witness was copied, then
the sealed create helper produced only the selected delete input as preparation.
The delete input passed full qualification before timing. This makes native
metadata/data warm; it is a diagnostic comparator, not frozen-profile admission
or a cold-cache claim. The exact helper hash matches prior corrected runs.

The complete native POSIX delete helper took **0.735762 s** (workload 735,761,584 ns,
planning 209 ns), including root fsync **0.018493 s**. It completed 100,000 unlinks,
100, 633 lstat calls, 633 directory removals, 633 enumerations and the prescribed
root metadata normalization. Independent final verification passed 335 paths /
200 witness files / 1,048,576 bytes. Docker-exec command wall was **0.834761 s**;
input creation 2.589207 s, qualification 3.934976 s, separate verification 0.292199 s,
and outer container/volume disposal 0.385221 s are excluded from the workload
number. Cgroup CPU delta was 0.735816 s; no OOM, swap or throttling. Memory peak
967,892,992 B was already reached in preparation/qualification and must not be
reported as deletion's private-memory use. All owned resources were removed.

The latest LayerFS delete Exec **30.128114 s** is about **40.95×** this native
helper observation. Its complete managed lifecycle is 30.167729 s. The earlier
native create comparator remains **2.566234 s** including planning versus LayerFS
Exec **29.095951 s** (about 11.34×); LayerFS create Commit adds 78.739488 s for a
107.908258-s lifecycle. Native helpers have no immutable-root publication phase,
so no native "Commit" equivalent or SDK lifecycle speedup is claimed.

The remaining LayerFS delete work is not physical CAS chunk reclamation.
`lookup_node` performs immutable directory lookup before materialization even if
an earlier listing already found that binding. `materialize_record` obtains
file length and portable metadata through canonical objects; `portable_metadata`
resolves and reads the mode and timestamp value graphs. The current plain
Workspace readdir also delegates to readdirplus, and unlink resolves its name
again before updating links, paths and mutation state. FUSE callback dispatch and
Workspace locking add overhead to these operations. These are source-confirmed
paths, not a measured attribution of each second. The main product process's
21.81 CPU-seconds in the latest delete sample supports prioritizing repeated
metadata/binding work over speculative disk-speed fixes. Directory-page caching
alone does not remove this per-entry work.


## 9. Implementation handoff and matched-initialization objective

The [two-phase specification](two-phase-optimization-spec.md) and
[Phase 1 implementation handoff](phase1-implementation-handoff.md) now require
initialization-like throughput for shared canonical construction/hash/storage
work under matched conditions, with the additional Commit obligations measured
and explained. File distribution alone is insufficient: compare exact paths,
metadata/witness, CPU/owner placement, memory, storage/cache conditions,
instrumentation, canonical object counts and actual task parallelism. The
historical 2.766-second initializer remains a reference, not a fixed requirement
or a live-create timing. Do not rearrange the prescribed tree to feed more
initializer producers.

The 5–10-second create Commit planning range, 3–5-second stretch and subsecond
generic delete target remain provisional and unchanged. Stable-candidate evidence
adds one matched seed-1 initialization comparator when compatible evidence is
missing; a large unexplained shared-work gap still requires investigation even
if the broad Commit target passes. The handoff authorizes isolated implementation
when executed, with no merge, publication, release changes or Phase 2 work.
No performance measurements were run as part of this specification update.
