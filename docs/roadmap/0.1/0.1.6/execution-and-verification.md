# Execution, evidence and verification contract

This plan inherits [general benchmark rules](../../../general/benchmark_rules.md)
and the [supported benchmark topology](../../../../benchmark/AGENTS.md). Older
release documents do not authorize a container-owned SQLite Store or a weaker
timing boundary. The following settings are prospective qualification settings,
not statements that the current runner already implements them.

## Environment

| Setting | Required configuration |
| --- | --- |
| Host | macOS; host owns SQLite Store, SDK/coordinator, canonical construction/publication and physical spool |
| Product | Ordinary Init/Commit storage supported by the frozen candidate; promoted v0.1.5 ordinary schema10 is starting context, not permission to silently migrate a prepared input |
| Store | One shared host Store per selected invocation; all its development branches use it |
| Container | One fresh Linux daemon/FUSE/workload container, shared by all workers |
| Container CPU | 2 CPUs total, not 2 per worker |
| Container RAM | 2,147,483,648 B total |
| Container swap | Disabled: memory-swap equals memory; nonzero usage/OOM is failure |
| Container process cap | 256 PIDs total |
| Mount roots | `/workspace/a`, `/workspace/b`; four-worker extension adds `/workspace/c`, `/workspace/d`; unique workspace IDs |
| Host coordinator workers | One per live workspace:1/2 regular,4 only explicit extension; single SDK Client binding shared/cloned as supported |
| Host resource caps | No claim that Docker limits constrain host CPU/RSS; record CPU model, cores, RAM, RSS/CPU/I/O, memory pressure and swap independently |
| Transport | Existing authenticated daemon/ProxyHost, loopback-only exposed daemon port, real FUSE; no project/data/socket bind mounts |
| File/dir metadata | Files0640, dirs0750, explicit epoch and mode toggles in fixtures; runtime-supported ownership |
| Cache profile | Prepared pristine input + independent host byte copy; fresh live Workspace; uncontrolled OS page cache, never called cold disk |
| Seeds | Selected default1; explicit qualification seeds1,2,3, exact same seed schedule per source arm |
| Build | Explicit prerequisite; reuse compatible sealed host/helper/image artifacts; resolve immutable image ID and record toolchain/target/product/harness seals |
| Measurement concurrency | One selected campaign owns existing measurement lock; no parallel builds, verifiers, unrelated perf tests or v0.1.5 release run |

Reference host hardware/OS/Docker version and candidate/control revisions must
be recorded and frozen before paired measurement. This roadmap does not invent
a reference-machine model or measured memory ceiling. Existing product resource
limits remain hard gates. Any additional phase-RSS/physical-growth comparison
threshold must be registered before candidate optimization, not after results.

Multiple live mounts are supported by daemon identity/root separation, but the
new multi-session benchmark coordinator needs implementation qualification.
Keep one daemon and live workspace session through the sequence. Launch exactly
one helper per POSIX stage via `Client::exec_workspace_session`, drain its
bounded receipt once and wait for completion; no per-file shell/Docker Exec.
Finish helper executions and writable handles before committing. Do not rely
on a persistent helper crossing Commit or bypassing a Busy execution fence.

## Public operations and timing

- Fixture Init uses `Client::initialize_layerstack` through existing native
  initialization. Pristine Init is explicit preparation for these post-Init
  families, not an Init performance result.
- Fork uses `Client::fork_branch`; all measured trunk/child construction occurs
  inside the selected test. Only a pristine initialized input is cached.
- Workspace lifecycle uses create, full-status Commit, clean/discard End and
  Exec through public SDK. One active lease per branch; concurrent branches
  are distinct.
- SDK-only profiles call `edit_workspace_file_range(s)`; M1 explicitly mixes
  real POSIX FUSE stages with two single-file SDK calls. The public batch API
  accepts only one file; cross-file pairs are ordered, not atomic. SDK-caused FUSE writes
  are zero; POSIX write receipts are separate and expected.
- POSIX operations include unlink/create, pwrite, append/ftruncate, mkdir/rmdir,
  rename, chmod/explicit mtime, link/symlink/readlink, open/close/fsync and declared
  descriptor reads. Unsupported setxattr is a reused negative proof, not a
  persistence feature. Chown, ACL, atime/ctime and BSD flags are not new claims.

Record monotonic intervals for every public Commit, stage/helper execution,
SDK call, Fork and lifecycle call. `commit_api_ns` ends after full-status
acknowledgement, including publication/presentation outcome. Mixed stage and
whole-workspace wall include their declared orchestration. Complete invocation
wall also includes authentication, copying, runtime readiness, receipts and
cleanup. Do not replace an API interval with external process wall or subtract
real publication work. Concurrent interval sums may exceed elapsed wall; report
the enclosing wall and overlap, never subtract summed workers as serial time.

Performance receipts must contain zero *added benchmark* digest/oracle/reopen/
materialization/fault-injection work. Reads integral to the inode/POSIX workload
remain included, named and counted. Expensive equality/oracle assertions run
only in verification; minimal result/outcome validation still rejects bad perf.

## Deadlines and preparation

Regular perf and regular verify have independent **15,000,000,000 ns complete
deadlines** from entry. A common monotonic absolute deadline stops worker work
at t+12s, reserving3s for cancellation, owned teardown and receipt publication.
Nested readiness/work calls inherit remaining time; they do not restart15s.
Completion evidence must include its own write and cleanup. If cleanup cannot
finish, record failure and remaining owners; do not report a pass or leave an
unbounded successful background teardown. A later emergency cleanup is tracked
as remediation, not deducted from failed invocation time.

The measurement lock is nonblocking: return NOT_READY when occupied. Missing
or incompatible prepared input/binary/image likewise fails promptly. Explicit
first-use fixture preparation is separately timed and selected: planned
watchdogs120s for S/B/BA/L100 and300s for L500. These are caps, not target
durations; builds use the existing explicit build path and report actual wall.
Preparation cannot cache a post-edit history, produced commits, live sessions,
or measured reader cache. Historical-access/exhaustive proof cases are the
explicit exception that consume externally supplied sealed history artifacts;
they are read claims, not history-construction claims.

Each run authenticates the master, byte-copies to an independent writable
Store, validates hashes/distinct inode/sidecar rules, starts fresh runtime and
removes only its owned sample artifacts. Acquisition and both identity checks
count inside15s. Do not run old states through an unsupported format simply
to reuse a cache. Prepared master reuse never supplies verification success.

A K100 timing miss is a retained FAIL/TIMEOUT, not an automatic smaller workload,
extra retry, longer deadline, or promotion to extended. All33 regular cases
remain *unqualified* until their complete operation and proof fit. First diagnose
the smallest failing selected row; never hide full verification costs behind
a millisecond operation metric.

## Regular verification

Run independently from a fresh copy of the same pristine input and replay the
same selected schedule, outside all perf distributions. Bind exact source,
fixture, seed, schedule, environment and corresponding perf selection identity;
logical branch roles map to that replay's IDs, not assumed reused random IDs.
Oracles derive from initial byte recipes and operation algebra, not the mutated
Store or the same mutation helper. Flat expected bytes for changed small files
avoid recursively expanding history recipes.

1. Check every attempted/Created count, every retained parent edge and every
   branch head. Verify historical fork ancestry, longest depth and complete
   graph cardinality. Discarded work adds no committed state.
2. S/B/BA focused cases: all declared retained states, complete contents and
   namespace/metadata/alias relations. History access: exactly the named reads
   and selected state, not a claim of complete producer qualification.
3. Load-bearing intermediate commits: all affected paths, types, lengths,
   explicit modes/mtimes, deleted old names, newly created names, moved
   descendants and persisted link targets. For rename moves, check every one
   of the64 descendants and source absence.
4. Fully verify newly created/replaced small files and changed small/medium/
   threshold targets. For large targets verify all inserted/changed ranges,
   4 KiB adjacent ranges (clamped at boundaries) and first/middle/last4KiB
   witnesses. Offsets for SDK insert/delete are translated by the independent
   edit algebra. This is **not full-large-payload verification**.
5. Check the full namespace inventory/count/type/length at initial and final
   states against the sealed manifest. Reuse qualified unchanged payload
   identity where permitted, but do not substitute cached certificates for
   actual changed-path proof. At intermediate states use affected-set checks.
6. At every retained load-bearing state, check fixed unchanged witnesses:
   one empty, two tiny in different directories, two medium lengths, the exact
   threshold control, one untouched1MiB file; plus first/middle/last4KiB in
   every untouched anchor. Verify full small witness bytes and large witness
   ranges. Do not assert a boundary-control size class stayed SmallContent.
7. Inode lifetime: within live workload checkpoints validate hardlink classes,
   nlink and held-descriptor semantics; after reopen validate surviving alias
   classes/content independently. An open-unlinked inode is not expected to
   survive process/session closure. In stage5, canonical name disappearance
   is verified live before recreation, and old bytes through the previous
   commit; do not claim final recreated paths should be absent.
8. At the concurrent discard checkpoint, prove A's extra witness write vanished
   and B's uncommitted stage survived, then verify both final branches and
   branch-specific values. In perf, record real overlap; in verify, assert
   isolation and expected results, not a deterministic thread completion order.
9. Close all sessions, reopen Store/client once, recheck all heads, graph and
   selected historical witnesses (initial, local5, local10, final and fork5),
   intersected with states actually present in that case and deduplicated.
   Access-only cases use their explicitly selected producer states instead.
   Do not reopen once per commit unless a separate lifecycle case declares it.
10. Prove clean/discard results, no active executions/leases, no owned mounts,
    no container left, no OOM/swap, protected input unchanged and receipts sealed.

Reuse existing reliability code for supported hardlink/open-file semantics and
bounded same-branch lease/EOPNOTSUPP proofs as implementation regressions.
Failure injection and unsupported-operation proofs must not enter perf timing.
If the declared regular proof cannot finish, retain its miss; never sample fewer
paths after seeing the result. Explicit exhaustive cases check all bytes of
all101 retained sequential states under their separately frozen watchdogs.

## Receipts and reporting

Every row inherits mandatory core receipts from the general rules and adds:

- Exact case/version/fixture/seed, source/product/harness/helper/image/oracle
  seals, host/container identity, per-view path/byte cap and observed high water.
- Branch role, inherited depth, requested/observed local commits, total graph
  commits, parents/heads, Created/UpToDate/Busy/HeadMoved and presentation status.
- Per-stage operation counters from M1, expected/observed helper executions,
  SDK calls/members, session creations/ends/discards, fork calls, worker count.
- Per-worker intervals and overlap duration/count. Shared cgroup CPU/RAM stays
  the same between one/two/four-worker configurations; host totals are separate.
- Logical path bytes, distinct-inode bytes, inserted/reused objects/bytes,
  physical Store allocation/growth and spool high water. Zero unavailable
  counters must never be fabricated; record null/status and scope.
- Host CPU/RSS/I/O and container CPU/memory/file-cache/spool domains with exact
  sampling scope and precision. Do not call sampled RSS an exact phase peak.
- Preparation, workload, verification, cleanup and complete wall; separate
  correctness/resource/cleanup/custody/timing/coverage/admission statuses.

Suggested retained layout reuses existing runner unique output directories:
`performance/<case>/<seed>/<arm>/<run>/` and
`verification/<case>/<seed>/<arm>/<run>/`, each with existing JSONL/result,
bounded failure logs, identity and manifest. These are layouts to implement,
not additional collectors. Raw failures and valid misses remain immutable.

Selected mode is diagnostic (`admission_eligible=false`). Explicit complete
family/campaign option checks exact rows; regular default never runs all cases
or extended proofs. Qualification uses seeds1/2/3 and one fresh control/candidate
pair per seed, alternating order C→N, N→C, C→N (three samples per arm/case).
Harness/fixture/schedule are identical across arms; only product treatment
differs. All attempts retained; an infrastructure-invalid pair reruns together.
The15s ceiling applies to each individual selected invocation, not paired sum.

Report all individual Commit durations and sequence max/median/range with
sample counts; within-sequence percentiles are descriptive, not independent
confidence evidence. No speedup/storage-gain assertion before paired evidence.
Initial target is coverage and bounded complete execution. Inherited stronger
gates still apply to inherited cases; new synthetic cases do not replace them.
