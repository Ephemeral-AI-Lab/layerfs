# 100k / 500 MiB bulk-create: 3–5-second feasibility investigation

2026-09-04. Isolated investigation; no release requirement or production change.

**Verdict: five seconds is a conditional research target, not a demonstrated
capability. There is no credible five-second path that retains one synchronous
macOS-host metadata acknowledgement per file. A local authoritative mutation
owner near FUSE, plus final-state Commit construction and bounded private
compilation, is the smallest credible architectural path. Three seconds is not
yet supported by the evidence; it needs a materially faster local FUSE path and
less serial admission/cleanup. Do not authorize a production performance claim
from these diagnostics.** Neither result proves a universal hardware limit.

## 1. Identity and scope

Dedicated worktree: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-bulk-create-feasibility`;
branch: `codex/bulk-create-feasibility`.
Starting commit: `7a6e119acba8e5a7ecd22d96c24a42a4413b10af`, inspected on the
implementation branch `codex/v013-phase1`, not `main`. It contains capacity repair
`fbf32e84662d00993c033515e113437965395494`; subsequent changes up to the pin were
benchmark/report changes. During this investigation the original branch advanced
to `b8c2ad4bf4fa0415fd49d57abea15729b33a4284` (resource sampler, history accounting,
verifier/report fixes). Its product crates and ordinary workload are unchanged;
we retained our explicit pin and did not relabel old measurements.

Read the eight requested roadmap/architecture documents. The untracked starting
optimization notes were copied, with their digest, into evidence. The original
checkout, Phase 1 ledger and existing results were not edited. Every diagnostic
held the **original checkout's** `phase1-v013/measurement.lock` exclusively from
build/preparation through verification/cleanup. No baseline rerun, competing
measurement, new benchmark crate, production integration or publication occurred.

Measured lifecycle means the sum of public Create, complete managed Exec
(including planning, payload generation, POSIX operations, normalization and root
sync), Commit, visibility query and End. Nested attribution timers are not added
again. CLI/process/runtime preparation, observation overhead and independent
verification remain separate. Diagnostics below do not replace this lifecycle.

[Retained summary](evidence/retained-summary.json) pins raw hashes and three
corrected sample paths from `phase1-v013/slots.json`. Median lifecycle is
**188.072 s**, Exec **95.260 s**, normalization **65.044 s**, Commit **93.181 s**,
refresh **44.215 s**. Independent phase medians need not sum to the total median.
The three performance slots say product pass; they are not complete independent
verification/release admission. The later sampler repair reinforces that limit.

[Representative raw sample](evidence/corrected-s1-raw.jsonl): lifecycle 188.072 s;
Create 0.0101, Exec 94.7117, Commit 93.1811, query 0.000214, End 0.1692 s.
External process wall 188.942 s; preparation 0.523 s includes runtime preparation
0.422 s—do not add both. Host orchestration 188.612 s includes 0.539 s of in-loop
observations/output outside the pure call sum. The 0.870-s process-minus-call
residual is **not** a pure CLI-startup timer; it includes observation and teardown.

[Historical initializer](evidence/historical-100k-result.json): 2.766280 s for
100,000 **prepared** files / 500,000,000 bytes, different topology/distribution,
eight host producers and an eligible empty Store. Source creation/normalization
and verification are excluded. Its final inode/root build was 0.132914 s;
SQLite bind/step 0.886396 s and commits 0.409148 s. These are nested in its
pipeline, not additional to 2.766 s. It used 13.0 host CPU-seconds. It is evidence
that construction can be efficient, not a live-creation result or a reusable
empty-Store cleanup policy.

## 2. Confirmed critical paths

### POSIX → FUSE → proxy → Workspace

The actual route is `ordinary_workloads::Ops::write_content` → exclusive open,
`write_at` retry loop, close; `Ops::finish` → `set_metadata` (chmod then
utimensat for each changed survivor), root `sync_all`, close. See
[ordinary_workloads.rs](../../benchmark/fs-bench-pro/ordinary_workloads.rs#L607)
and [workspace_common.rs](../../benchmark/fs-bench-pro/workspace_common.rs#L598).
The 100,634 normalization records imply **201,268 metadata syscalls**, in addition
to setup directory chmods. The older `workload_chmod_call_count=633` does not count
normalization calls routed directly through `set_metadata`.

[FUSE callbacks](../../crates/layerfs-fuse/src/filesystem.rs#L92) perform
truncate/chmod/mtime then return attributes. `create` pins a handle; `write`
forwards it; flush currently returns success without a barrier; release unpins;
fsyncdir calls the all-dirty barrier. The sample has approximately 906k callbacks:
203,216 lookup, 100,217 getattr, 201,901 setattr, and 100k each create/write/flush/
release. These are **not** 906k network exchanges.
[host_mount.rs](../../crates/layerfs-fuse/src/host_mount.rs#L88) configures one
FUSE worker; [adapter.rs](../../crates/layerfs-fuse/src/adapter.rs#L7) uses a
one-second kernel TTL.

[ProxyClient](../../crates/layerfs-fuse/src/proxy_client.rs#L11) has one TCP
connection. Complete cached directories answer positive and negative lookup;
attribute cache answers getattr and the post-setattr attribute request locally.
New directories and reserved NodeIds already avoid some host calls. Sequential
chmod/mtime still use `exchange_at` and wait for a host response. More connections
or workers alone cannot pipeline this workload's dependent syscalls.

Reductions with the existing ownership model:

- Exact same-mode chmod can avoid host mutation work only after proving the
  cached inode, normalized mode, active-state and error semantics. Kernel
  `DefaultPermissions` and proxy pause/invalidation already provide part of the
  machinery; a plain cache comparison does not prove all of it. Host chmod also
  records mutation generations. Keep actual chmod syscalls in the workload.
- Longer kernel cache lifetimes can eliminate expiration-driven lookup/getattr
  callbacks if every owner mutation/pause/resume invalidates affected bindings,
  attributes and pages. Do not call all observed lookups redundant or promise
  that caching removes the two required metadata syscalls per inode.
- Reads/reopen/links need the current inode state; fsync/Fence must flush and
  surface errors; namespace conflicts and new ownership require validation.
  Existing local caches are not resource reservations or a general write lease.

[protocol.rs](../../crates/layerfs-fuse/src/protocol.rs#L49) is especially
important: Write, reserved creates, release and several other requests **already
have no immediate reply**; host errors are retained and acknowledged at Fence/
fsync. Thus the earlier description “synchronous per-operation replies” applies
to metadata exchanges, not every operation. We did not extend deferred success.
Chmod/mtime are currently synchronously validated by
[Workspace](../../crates/layerfs-workspace/src/cow_tree.rs#L970).

To remove the remaining per-file metadata round trips correctly, the FUSE-side
owner must be authoritative for namespace bindings, inode metadata, resource
reservations, handle lifetime and operation errors during its ownership epoch.
The host must not independently mutate/read stale state: SDK operations,
Commit/Discard, pause and recovery need an explicit handoff/fence and cache
invalidation protocol. Validate active ownership, ranges, modes, timestamps,
existence, quotas and ordering **before success**; later transport failure cannot
be disguised as successful validation. The lowest-risk model to investigate is
one mutation owner at a time, not two asynchronously reconciled owners.

### Why closed-create batching is ineffective—and not a one-line fix

`write(nonzero)` buffers bytes; `unpin` first calls `flush_write_locked`;
`send_buffer_locked` removes `PendingCreate`, emits reserved create + Write, then
release emits Unpin. Nothing survives for `pending_closed`.
[proxy_client.rs](../../crates/layerfs-fuse/src/proxy_client.rs#L237).

The existing wire entry already carries offset/payload writes and optional mtime.
Bounds: 128 files, 128 writes/file, 16,384 aggregate writes, 16 MiB payload and
17 MiB frame; the client closes batches at 128 files / 1 MiB. All prescribed
1/8/48-KiB files fit. No new bulk SDK API is necessary.

However, the [host batch handler](../../crates/layerfs-workspace/src/projection.rs#L604)
creates/pins/writes/unpins sequentially with `?`: a failed write/mtime can leave
that inode pinned, with only a prefix of the batch applied. The generic port
handler also ignores a short successful write's returned length. The current
Workspace writer is all-or-error with physical rollback, but that property must
be explicit at the batch boundary. Reserved IDs do not reserve storage capacity.

Before enabling nonzero retention, prove these cases:

| Operation | Required behavior |
|---|---|
| Read, read-only or writable reopen | Flush the matching pending inode/closed batch before host read/pin; preserve read-your-writes and ordering. |
| Rename | Preserve alias identity and validate both parents/destination. The pending-create shortcut currently treats an uncached destination as absence; extending its reach is unsafe without validation. |
| Hard link | Materialize the pending inode before linking; aliases share versions and link accounting. |
| flush / fsync | Do not move failures beyond their existing acknowledgement point. File fsync and root fsync must drain all applicable buffers and errors. |
| Partial write / ENOSPC | Retain the valid prefix or documented all-or-error outcome; always release batch-owned pins and preserve error/retry state. Bound pending bytes and write counts. |
| Truncate / delete / open-unlinked | Zero-length shortcuts must not resurrect data; deleted-but-open data survives through final close. Closed-batch draining must precede dependent operations. |

Batching cuts protocol dispatch/locking, not the 100k spool creations or the
later all-file normalization pass. Do not retain all files until normalization
in a nominally 1-MiB queue. The safe batch state machine remains unimplemented.

### Staging, final construction, refresh

[File I/O](../../crates/layerfs-workspace/src/file_io.rs#L194) creates one physical
spool per new inode, keeps its descriptor, checks linked/descriptor identity and
high-water, appends, observes allocation, rolls partial append back, then installs
the new piece tree. The sample: **100,000 opens**, 13.25 s spool work, 500,000
allocation observations, 720.896 MB physical allocation for 524.288 MB payload.

Existing `Piece::Spool` is an offset/length within a per-file spool;
`Piece::Inline` is already limited to **8 MiB per Workspace**. An all-RAM 500-MiB
inline conversion is disallowed. Bounded shared segments are a credible backing
change: append immutable slices, let per-inode piece trees reference them, count
physical allocation once per segment, and retain segment ownership until all
versions, read plans and open-unlinked handles release it. Never truncate a shared
segment to an individual file's length. Rollback may truncate only an exclusively
owned uncommitted tail; subsequent appends require dead-range accounting.
Existing read plans, checkpoints, fsync, reclaim, rebase and Discard all assume
per-node spool paths/descriptors and must change together. The prototype below
measures the primitive, not those lifecycle guarantees.

[Current frontier Commit](../../crates/layerfs-workspace/src/changes.rs#L359)
already avoids full base/final manifests. Preserve that small-delta path. It still
flushes directory changes in 128-entry groups, recomputes metadata, creates new
records before their final reference counts, then traverses edges again and
updates records. The sample has 411,614 candidates / 566.844 MB; admission takes
3,234 transactions capped at 127 objects. The historical initializer has similar
cardinality with approximately 131 transactions.

Reuse bottom-up final directories, count final inode references before encoding,
exact bounded metadata interning from
[NativeImport](../../crates/layerfs-layerstack-store/src/layerstack.rs#L1735),
and [rope::build_bytes](../../crates/layerfs-content/src/file/rope/build.rs#L39).
Its canonical shortcut applies only below 8,192 bytes: 64k of this workload's
files; 8-KiB/48-KiB files retain normal CDC. Keep canonical/collision validation.
Move bounded owned slabs and carry a <4-MiB / ≤8,191-object admission batch across
directory boundaries, adapting existing **nonempty-Store candidate** planning,
checks and conditional publication. Never invoke the empty-Store initializer or
its clear-all-objects failure cleanup.

The historical `InsertNode` capacity alone was **9,124,352 bytes**, above the
current 8-MiB final-delta budget. A simple call to its iterator builder is not a
bounded solution. Use charged chunks/spilled compact pairs and a bounded ordered
builder (or the existing incremental fallback); count maps, vector capacity and
simultaneous buffers. Rebuild whole final state only when a bounded density plan
justifies scanning survivors; retain incremental behavior in large mostly-clean
trees, including external aliases.

[Refresh](../../crates/layerfs-workspace/src/lifecycle.rs#L145) clones current
nodes, resolves each live path again, validates aliases/attributes, then closes
and deletes obsolete spools. Representative Commit reads 703,315 snapshot DB rows
(~2.20 GB) and spends 47.34 s in rebase. Carry candidate NodeId → canonical inode/
record/content/metadata identities into refresh, authenticate/bind them to the
successfully published root, and retain the existing attribute/alias checks.
Sparse changed-record maps plus batched authenticated reads avoid a second whole
tree. Do not simply delete checks or throw away pinned unlinked nodes.

### Safe overlap

[CaptureState](../../crates/layerfs-workspace/src/capture.rs#L8) contains one
Running/Ready file. Switching files invalidates it; it is not a multi-file
pipeline. Replace that limitation only with a fixed bounded worker set and queue.
Compile **private immutable versions** after completed writes while later files
are created; return canonical slabs into private charged storage. Queue entries
need stable inode identity, content generation and retained slices. Rewrites,
truncate, failed writes and deletion invalidate versions; aliases share the same
inode; metadata changes can reuse content but not stale inode records. Discard
cancels/drains workers and deletes private artifacts before returning.

Admit only the final reachable generations after quiescence, using normal
candidate accounting. Private preparation can overlap Exec; speculative persistent
CAS admission cannot silently accumulate overwritten/deleted/Discard data.
Raw + compiled payload is roughly 1.1 GB here, but capacity, superseded data and
buffers must be charged within existing limits, with backpressure/fallback.
A final fsync that merely runs all compilation serially gives no lifecycle win.

## 3. Experiments and decisions

All experiments are one selected seed, without favorable rerolls. Resource profile
and commands are retained per attempt. The witness manifest equals the qualified
Phase 1 input, and the Linux helper SHA-256 equals the corrected sealed binary;
see [identity check](evidence/identity-check.json).

| Experiment / hypothesis | Evidence and outcome | Decision |
|---|---|---|
| Native Linux: the prescribed workload itself may fit below five seconds. Same image/helper, seed 1, 2 CPUs, 2 GiB memory+swap, 256 pids; fresh Linux disk volume, no FUSE/Workspace/CAS. | [First attempt](evidence/native-500-s1/commands.json), source `c243cbe5`: preparation qualification failed because Docker copy did not preserve destination root mode/mtime. No workload timing occurred. Container removed. | Fix preparation only; retain failure. |
| Same hypothesis, root preparation corrected; source `4fb02d46`. | [Receipt](evidence/native-500-s1-r2/performance.stdout): workload **2.397016 s**, planning **0.169217 s**; total helper work **2.566234 s**, external Docker exec **2.631917 s**. Normalization **0.376810 s**, root sync **0.776943 s**, 100k files / 524,288,000 bytes. | Five-second research remains worthwhile. This is not a LayerFS result; no FUSE callback floor was measured. |
| Shared staging: physical per-inode spools are avoidable cost. Source `78f254da`; existing generator, same seed/500 tier; macOS host, serial 4-MiB segments. Predict opens ≈126 instead of 100k. | [Receipt](evidence/segments-500-s1/performance.stdout): planning **0.190304 s**, generation+staging **0.833903 s**, **126** segments, **524,288,000** allocated bytes, cleanup **0.571631 s**. Plan+stage+cleanup **1.595838 s**. Peak process RSS **24.9 MB**. | Pursue backing integration; do not infer an end-to-end speedup from a primitive without Workspace/transport/piece-tree work. |
| Transport: removing host service work alone cannot make 100k sequential replies cheap. Source `8fce40e8`; 10k checked 32-byte echo exchanges on one TCP_NODELAY connection. | [Cross-boundary](evidence/rtt-10k/host-boundary.stdout): mean **167.765 µs**, p50 149.625, p95 233.125. [Linux loopback](evidence/rtt-10k/linux-loopback.stdout): mean **46.307 µs**. | Remove the boundary from sequential metadata success. Python adds overhead; this is a directional screen, not a product RTT or a hardware lower bound. Even loopback RPC is not equivalent to in-process ownership. |

Native preparation reused the first attempt's generated witness (3.251 s);
corrected container creation/copy/root metadata/qualification were separately
recorded. Independent final verification took **5.639 s** and passed 100,968 paths,
100,200 regular files, 525,336,576 bytes including witness. Boundary resource
observations show about **1.805 container CPU-seconds**, 878.4-MB memory peak,
no throttling/OOM/swap before verification. Sampling/exec overhead is included in
that CPU delta. Native container+volume cleanup took **2.333 s** after verification;
this diagnostic therefore does **not** claim a 2.57-s complete lifecycle or CLI.

Segment verification regenerated and compared every stored file separately
(**0.316482 s**). Its focused check covers partial append rollback, retained old
versions, replacement and a truncated slice. No directory, hard-link, quota
handoff or open-unlinked Workspace integration is claimed. Cleanup followed
verification and may benefit from its cache state; 0.572 s is a planning reference,
not an independently established cleanup floor. The retained 1-MiB witness fixture
is diagnostic preparation, not leaked target data; segment target scratch and
all owned containers/volumes were removed.

Rejected approaches: blindly enabling pending nonzero writes; no-reply metadata
with deferred validation; larger thread/connection counts as the sole fix;
500-MiB inline buffering; empty-Store import in Commit; persistent speculative CAS
admission without reclamation; omitting normalization, witness, integrity checks
or End cleanup. None was timed or presented as a successful product candidate.

## 4. Five-second budget and ownership boundary

This is a **falsifiable engineering allocation**, not a sum of measured component
wins. It requires a single authoritative FUSE-local mutation owner, host-side
bounded private compilation overlapping Exec, and normal final Store admission.
The public SDK and POSIX route remain; ownership/handoff internals change.

| Serial lifecycle phase | Budget | Required condition / evidence gap |
|---|---:|---|
| Create | 0.020 s | Current ≈0.010 s gives room; retain real attach. |
| Exec, including planning, all POSIX calls, metadata and root barrier | **2.600 s** | Local authoritative operations and kernel-cache improvements; full local FUSE performance **unmeasured**. Native 2.566 s supports investigation, not this FUSE prediction. Host compilation/transfer must keep up inside this interval. |
| Commit: private pipeline tail | 0.100 s | Content generations mostly compiled during Exec; no unbounded tail at fsync. |
| Commit: final directory/inode state | 0.180 s | Historical 0.133 s supports scale only; nonempty Store, topology and 8-MiB structural budget still unproven. |
| Commit: final candidate admission | **1.200 s** | Historical SQL stepping+commits ≈1.296 s; requires modest improvement plus bounded nonempty-Store membership/collision checks. Not yet measured. |
| Commit: conditional publication + checked refresh | 0.120 s | Reuse authenticated candidate identities; publication already milliseconds. Refresh target unmeasured. |
| Commit: obsolete private staging cleanup | **0.570 s** | Include raw/compiled segments and descriptors; primitive cleanup ≈0.572 s for raw payload alone. Extra compiled artifacts may break this allocation. |
| Visibility | 0.010 s | Single query, no polling. |
| End | 0.200 s | Current ≈0.16–0.17 s; all workers joined and owned product resources cleaned. |
| **Total** | **5.000 s** | **No headroom: a stretch plan, not a commitment.** |

Only the critical-path tail of overlapping compilation is added to Commit. Private
compilation, transport and FUSE must fit simultaneously within Exec's wall and
resource budgets. This is why optimizing Commit alone cannot suffice.

At the observed cross-boundary mean, 100k replies would take ≈16.78 s; five seconds
allows only 50 µs/file even if everything else were free (three seconds: 30 µs).
Current normalization averages ≈323 µs per prescribed metadata syscall. These
measurements reject “retain synchronous remote metadata and optimize CAS” as a
credible target plan; they do not mathematically rule out a redesigned transport.

The earlier ≈36 container CPU-seconds and measured ≈103 host CPU-seconds are work,
not wall-time lower bounds for a new algorithm. At two runtime CPUs, five seconds
permits at most ten container CPU-seconds; the 2.6-s Exec allocation permits about
5.2. That needs substantial CPU elimination, not just more concurrency. Historical
construction alone used ≈13 host CPU-seconds: moving that unchanged work inside
the two-CPU container costs at least ≈6.5 seconds of CPU capacity. Keep host
construction outside that cgroup or demonstrate a large reduction; do not silently
raise CPU limits. Exact new worker count and total memory must be measured.

**Within current architecture:** safe batching, shared staging, exact metadata
reuse, final structural construction, carried nonempty-Store admission and checked
refresh can remove major costs. They are worthwhile, but no integrated timing
here proves their combined speedup or a five-second lifecycle.

**Changed ownership/deployment:** synchronous metadata becomes local only by
moving authority to FUSE or colocating the Workspace owner. A macOS SDK/Store with
a Linux mutation owner preserves the broad deployment topology but introduces a
new ownership protocol. Moving the complete Workspace/Store into Linux is a
separate profile/comparator and must account for the two-CPU restriction and
retained host roots. Native volume results do not qualify under the frozen profile.

## 5. Three seconds and full CLI wall

A concrete three-second screen would require Create 0.02 + Exec **1.80** + Commit
**0.99** + query 0.01 + End 0.18 s. Commit's 0.99 would allow roughly 0.02 pipeline
tail + 0.10 structure + **0.55 admission** + 0.06 publication/refresh + **0.26
cleanup**. No measurement demonstrates these numbers. Admission must more than
halve the historical SQL path; cleanup must more than halve the raw-segment
observation; local POSIX/FUSE work must beat the native 2.566-s helper total.
Do not subtract native root-sync time as though the required LayerFS barrier were
free. Three seconds is therefore **unsupported/high risk**, not a production
recommendation and not proven impossible.

A five-second lifecycle also is not a five-second command. The retained command
has roughly 0.523-s cache-hit preparation and 0.870-s process-minus-call residual;
these scopes are not identical to a production CLI but already exceed a nominal
zero-overhead assumption. A five-second CLI needs a materially shorter lifecycle
(roughly ≤3.6 s if similar overhead persists), or measured reductions in fixture
acquisition, runtime setup, observation and teardown. First-use builds, image pulls
and initialization cannot be assumed free. A persistent ready runtime might reduce
startup, but reusing it changes the frozen fresh-container deployment profile and
must be reported separately. It must not leave task workers running after End.
If the command includes independent full verification, this native verifier's
5.639 s alone exceeds five seconds; verification stays separately reported.

## 6. Ranked implementation sequence and next experiment

Recommend staged prototypes before authorizing production integration:

1. **Falsify the ownership budget first.** The smallest decisive next experiment
   is the same sealed `tiny-bulk-create-500`, seed 1, through public SDK + real
   FUSE with a colocated/authoritative mutation owner, preserving witness and all
   calls. Start with one small diagnostic for lifecycle/error behavior, then one
   100k sample. Record callback/RPC counts and metadata/CPU time. If Exec cannot
   approach 2.6 s, replan the five-second architecture before polishing Commit.
   A colocated profile is a diagnostic, not frozen-profile admission.
2. **Repair then activate bounded closed-create batching.** One focused regression
   should cover nonzero read/reopen, rename into an uncached parent, hard-link
   aliasing, fsync errors, partial batch failure and pin cleanup. Require actual
   batch counters to change without shifting validation to a later operation.
3. **Integrate shared staging at the backing abstraction.** Update read plans,
   checkpoints, append rollback, allocation charging, fsync, unlinked lifetime and
   cleanup together. Keep a bounded fallback for churn/large files; do not grow
   the inline limit. Measure small correctness first, then one 100k sample only
   if needed to establish scaling beyond the primitive evidence.
4. **Build final candidate state and refresh once.** Reuse exact metadata and small
   content builders; compute final references once; reuse final identities.
   Preserve sparse changes in large trees. Prove canonical equivalence, aliases,
   historical roots and memory ceilings before enabling the dense path.
5. **Adapt carried admission to existing-Store Commit**, retaining collision and
   membership checks, expected-head validation and failure/retry accounting.
   Use focused injected batch/publication failures. The historical empty-Store
   cleanup cannot be reused. Measure admission/tail against the 1.2-s allocation.
6. **Add versioned private compilation overlap** only after the above mechanisms
   are stable. Bounded workers, slices and artifacts; cancel/reclaim superseded
   generations; drain on Discard/End. Measure total lifecycle, not just Commit.
   Run broader affected verification once stable, followed by source-bound
   three-seed performance/independent proofs if production work is authorized.

Key unresolved gates: local FUSE dispatch/CPU floor; authoritative owner handoff
and failure semantics; nonempty-Store admission speed; final-builder memory;
combined raw+compiled cleanup; contention while compilation overlaps creation;
and full CLI setup. These are explicit experiment gates, not postponed cleanup or
permission to weaken the contract. The native and staging evidence support
continuing a five-second research path; they do not justify claiming success.

Prototype commits (all on this investigation branch): `c243cbe5` native diagnostic;
`4fb02d46` root-preparation correction; `78f254da` segment primitive;
`8fce40e8` transport screen. `evidence/*/commands.json` records exact commands,
source commits, timings and exit status; SHA-256 manifests retain successes and
failures. No production crate was changed. Reproduction scripts intentionally
refuse existing output directories; use a new attempt directory and record a new
source identity rather than overwriting retained results.
