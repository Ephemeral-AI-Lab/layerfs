# FUSE investigation — independent Workspace per tool call

> **Status:** Research; informative and not a product contract.
> Reviewed 2026-10-05. LayerFS product baseline
> `f96d97651be5299f153ccde2bc8d921dd58807ad` and inspected checkout
> `334fc743751b9a181e670d0601a24fb3169208f9` share product tree
> `05c00c5d62889ae316bec9ea09dba16e93ba888e`. The owner-revised operation
> documents are uncommitted proposals. No source, dependency, workload, build,
> benchmark, history record or external application was changed for this report.

This investigation covers the complete mount → ordinary Bash → Commit → terminal
unmount path, rather than FUSE callback time alone. It complements the primary
[FUSE contract](../fuse.md), [daemon/SQLite engine](../daemon-sqlite.md),
[runtime integration](../06-cluster-one-integration.md), and the
[mount](../workspace-api/mount.md), [Exec](../workspace-api/exec.md),
[Commit](../workspace-api/commit.md) and [unmount](../workspace-api/unmount.md)
operation contracts. Root/Core rules and both handbooks were read. Their profile,
one-attempt, accepted-write, stable-input and exact-outcome rules apply.

Owner clarification 2026-10-05: both per-tool-call and per-task modes are required;
per-tool-call is the expected common case. One Workspace can serve many calls
for a long lifetime and Commit incrementally. Commands may be short or long-lived
in either mode; no assumed duration or command class determines lifecycle.
The fresh-mount paths analyzed here are one supported orchestration choice.
Retaining the same Workspace preserves its native connection and valid caches,
with repeated base advancement, later active writes and automatic reclamation
before eventual explicit unmount. Commit covers the shared Workspace frontier,
not mutation isolation for its invoking call.

## 1. Findings that determine the architecture

The load-bearing target is plausible only after several mechanisms change. A
fresh logical overlay over an immutable full root avoids base-sized bootstrap,
but a fresh native FUSE connection has real session/cache/teardown work. Huge
mutation bursts also expose current cluster-one whole-set and whole-tree work;
cached FUSE alone cannot repair that Commit path.

| Priority | Source-qualified finding | Required correction / consequence |
| --- | --- | --- |
| P1 | Existing namespace validation and reduction materialize dirty sets and derive refusal/work ceilings from ordering memory; directory rebind validation can walk the whole base | Back validation/reference state with bounded storage and develop authenticated incremental topology validation; do not merely increase budgets |
| P1 | Bash leader exit and stream EOF do not establish descendant, FD/mapping, callback or history quiescence | Keep independent custody; capture the published frontier behind prior reply-send attempts and prove the generic completion boundary; no command-specific exit hook |
| P1 | Old FORGET performs whole-node collection; default fuser batch processing repeats it | Indexed lookup ownership and bounded batch handling; bulk retirement only after actual session/request/handle fences |
| P1 | A path disappearing through lazy detach does not end retained connection/FD ownership | Distinguish normal privileged unmount from helper/lazy paths; never treat path absence as terminal cleanup proof |
| P1 | Repeated successful orphan captures and failed payload folds violate version/progress requirements | Complete bounded orphan/failure composition before qualification |
| P1 | Shared SQL/runtime queues can starve bootstrap, Commit finish or teardown | Per-Workspace fair bounded jobs, demand capacity and cancellation-aware drain |
| P2 | Pinned fuser allocates a 16 MiB + 4 KiB receive buffer per loop even at 128 KiB negotiation; current mount has two loops plus an outer supervisor | Count allocation/residency/thread startup; no small-buffer claim based solely on negotiated request size |
| P2 | New mounts cannot inherit old kernel dentries/inodes/pages merely by stable inode numbers | Preserve bounded authenticated userspace caches and committed stat/index state; price fresh kernel requests |

No measured throughput, constant total memory, constant unmount time or sub-100 ms
full pipeline is established. The few-millisecond logical open target is distinct
from first useful cold access and from construction of a genuinely large delta.

## 2. Source identities and evidence scope

The dormant LayerFS FUSE package pins `fuser =0.18.0`, `default-features=false`
in [Cargo.toml](../../../../crates/layerfs-fuse/Cargo.toml). It remains excluded
reference source at the product pin. The locally available published package
reports upstream VCS `9c957f74efe715112049298cdf1d601781829c8d` in
`.cargo_vcs_info.json`. Exact inspected package file hashes are:

| Published package source | SHA-256 |
| --- | --- |
| `src/session.rs` | `1311a037e94d8c874b0df47e1a268ded85a7c57872235e730791ece253b6005f` |
| `src/lib.rs` | `1d61b8ac9709047acadba0126ca655d0e38f5a836b3a1a95fd7c6d7361abc3c1` |
| `src/read_buf.rs` | `33e48ad1782ad81d6e87cb6d2b86920b2e578a9d3266c9bf08f24feb3a22d049` |

Local package line references below identify that exact inspected source. Public
upstream links use its reported VCS commit; formatting/line numbering need not be
identical. No registry file was edited. Linux v6.12 source is a pinned primary
mechanism reference, not a claim of binary equality with historical
`6.12.76-linuxkit` arm64 or an unknown future runtime kernel. Current upstream
documentation was accessed 2026-10-05.

Retained evidence is branch `codex/phase7-experiment-305` at
`1451b68a720bbe2175a103dd9b35693ad05e2be1`, read with `git show`. A2 is ext4
passthrough, not product overlay/cluster-one Commit. No numeric row has residency
proof; historical observations remain INELIGIBLE diagnostics. Prior fixed-cost
Exec/unmount records also have polling resolution error, and no historical
receipt was corrected or rerun. See [05](../05-fuse-assessment.md).

## 3. The efficient complete-call architecture

[proposed mechanisms; no new endpoint or retired server]

```text
ONE-TIME DAEMON / HOST-APPLICATION READINESS
------------------------------------------
host application: cluster-one runtime
  provider handles + policy + authenticated adapters
  mutable HistoryCatalog authority + immutable object acquisition
daemon:
  one initialized overlay.sqlite + statements + fair engine service
  bounded immutable object/attribute caches + process/transport supervisors
  credential/identity routing, not a namespace-wide materialized base

TOOL CALL n: independent Workspace incarnation Wn
------------------------------------------------
select authorized exact root Rn / history expectations
      |
      v
create bounded namespace state in EXISTING overlay database
      |
      v
attach fresh native FUSE session + INIT + readiness
      |
      v
ordinary Bash / normal authorized processes
  .git + ignored deps + caches + outputs are already present
      |
      v
explicit admission/frontier barrier         later requests -> active An
  drain prior admitted attempts + reply-send attempts
  stable capture Cn; barrier wait counted
      |
      v
one producer: captured SQL windows -> localized edit/stream construction
      |                                       SQL/transport waits yield
      v
bounded finalized-object windows -> runtime-owned Save
      |
Save.finish -> exact stage -> conditional history transition
      |
      v
known install: Rn+1 = Cn over Rn; preserve An
      |
      v
terminal unmount: activity/native fences -> logical close
      |                                 |
      |                                 +-> bounded automatic local reclaim
      v
next call mounts COMPLETE Rn+1; no dependency restoration
```

The host application embeds cluster-one libraries and owns required runtime
adapters. `layerfs-server` is not revived or renamed. Existing Rust APIs are
`Storage::begin_save`, `Save::accept/finish`, `HistoryCatalog::stage_changes` and
`commit_staged`; names such as SaveBegin describe proposed adapter calls, not
deployed endpoints. Current global persistence opens only on macOS. Distributed
object identity is useful, but a distributed provider's consistency, authority,
reference closure and history coordination are not implemented by this diagram.

Fast readiness binds a checked root/scope/profile and root inode, not every path.
Retain initialized policy/provider and bounded exact-identity caches where valid.
Cached bindings must include authority/catalog incarnation and selected history
version; a previously checked root is not authorization for another peer/session.
Latest-head selection still requires coherent history knowledge. Do not cache a
mutable Branch response indefinitely or silently select an older base.

## 4. What crosses a tool-call boundary

| State | Survives logically? | Required rule / actual cost |
| --- | --- | --- |
| Saved canonical objects | Yes, immutable by identity | Authentication/reference closure; caches bounded and authority scoped |
| Committed root, inode serials, mode/mtime and `.git/index` | Yes, if included in exact Commit | Next mount presents stable stat identity; changed objects still processed |
| Base/attribute userspace caches | May remain in persistent daemon | Identity/authority keys, eviction, declared warm/cold state |
| Shared SQLite pager/prepared SQL | Daemon lifetime | Hot pages may be reused; no per-call schema/open; actual queue/page work counted |
| Overlay namespace and orphan/scratch state | Until terminal custody release/reclaim | No reuse of namespace keys while stale owners/jobs remain |
| Old mount kernel dentries/attrs/file pages | No transfer to a new independent connection | New path accesses require fresh kernel population; retained old references stay old |
| Exec FD/cwd/mapping/descendant | Only if explicitly retained on old Workspace | Cannot silently redirect to next Workspace or treat retained descriptor as new-root access |
| Branch/stage/inode allocator | Mutable runtime authority | Exact conditional operations; old tokens do not authorize new state |

In Linux v6.12, `fuse_iget` resolves an inode inside a superblock and mount setup
creates the connection/superblock root. Equal serials on separate connections do
not share that inode address-space cache. Reusing an already initialized FUSE FD
can select its existing superblock; that is an alias of existing state rather
than a new independent overlay. These are inferences from
[inode.c](https://raw.githubusercontent.com/torvalds/linux/v6.12/fs/fuse/inode.c)
(`fuse_iget`, `fuse_get_tree`), not a measured cache-transfer experiment.

Long TTL and KEEP_CACHE improve repeated access within a mount. They do not remove
the first LOOKUP/GETATTR population on the next native mount. Stable metadata and
persisted Git index can still avoid application-level content rereads; that is
a different benefit from retaining kernel pages.

## 5. Native lifecycle floor and its reducible overhead

### 5.1 Current source path

In [LayerFS mount.rs](../../../../crates/layerfs-fuse/src/mount.rs), admission
checks `/proc/self/status` and root attributes, reserves a mount lease, builds
the adapter/config, calls `Session::new`, binds invalidation and spawns an outer
`layerfs-mount` thread to run the session. `n_threads=2`, `clone_fd=false` are
configured at lines 209–211. fuser's `Session::new` performs native attach and
INIT handshake before returning; `run` creates its configured receive/dispatch
threads and joins them before filesystem destruction. Thus this LayerFS path
creates three session-related threads per mount, including its outer supervisor.
[Pinned fuser session source](https://raw.githubusercontent.com/cberner/fuser/9c957f74efe715112049298cdf1d601781829c8d/src/session.rs).

fuser's buffer size is 16 MiB + 4,096 bytes, initialized as a zeroed `Vec`, not
the negotiated 128 KiB. With two live loops, allocated receive-buffer length is
33,562,624 bytes per mount; handshake allocates another such buffer transiently.
This is source arithmetic, not measured resident/physical memory: allocator and
OS zero-page behavior affect residency. Stack, replies, masks, pager and file
cache are additional. See
[read_buf.rs](https://raw.githubusercontent.com/cberner/fuser/9c957f74efe715112049298cdf1d601781829c8d/src/read_buf.rs).

The baseline drain polls activity every 1 ms, polls worker completion, and scans
up to 1 MiB of `/proc/self/mountinfo` before lease completion (`mount.rs:309–397`,
`:438–460`). `MountLease::finish` loops through visited nodes to clear projection
lookup counts then invokes collection
([runtime/lifecycle.rs](../../../../crates/layerfs-workspace/src/runtime/lifecycle.rs):170–199).
These are application overhead, not inherent FUSE readiness rules.

### 5.2 Reductions compatible with independent per-call Workspaces

1. Keep daemon/container, overlay schema/statements, runtime/provider and
   authorized transport ready across calls. No per-call image/build/database Init.
2. Move invariant configuration/capability checks to daemon startup, retaining
   per-request authority and actual native-call errors. Cache no mutable filesystem
   result as an invariant.
3. Use event-driven request/handle/reply drain and worker completion. Avoid status
   polling as a lifecycle or process progress mechanism.
4. Release registry/Workspace locks before native attach/handshake and queue waits;
   readiness is published only after the correct incarnation's service is usable.
5. Replace whole-node FORGET collection with indexed ownership; do not add one
   foreground loop per visited inode after detach.
6. Use bounded callback work queues so workers receive rather than wait on disk,
   inode guards or Save capacity. Existing fuser reply ownership permits deferred
   response handling; kernel ordering/cancellation needs its separate proof.

A custom long-lived processing pool can service callback work while native
session loops remain per mount. This does not eliminate fuser's internal thread
creation. Its public `Session::from_fd` still handshakes, and `run` still creates
loops; the experimental Tokio wrapper creates its own runtime and covers only
part of the filesystem surface. It is not a ready universal lifecycle pool.

If native/session overhead still misses the prospective target, evaluate a
supported public session API or first-party protocol/session adapter with complete
ABI, ownership, interrupt and teardown proofs. Do not patch/vendor/fork fuser,
copy dependency internals as a disguised fork, or claim negotiated-size buffers
are already possible through its current public configuration.

Preprovisioning a bounded pool of fresh, never-exposed native sessions is only an
investigation candidate. Every session must bind exactly once before first use,
with no external FD/cwd/cache reference; empty-root INIT metadata and later
root binding require a coherence proof. Replenishment, idle buffers/threads and
actual creation cost must be accounted for. It can shift synchronous latency but
does not remove sustainable service work; no target-root prefetch or hidden
measurement preparation is permitted. Rebinding a used mount is not this idea.

There is no evidence for a universal constant native mount/unmount floor. Device
open, native mount, INIT, scheduler/thread readiness and real kernel destruction
remain. Measure their events individually before selecting a more complicated
adapter. Large visited inode sets can make native destruction scale with visited
state even after userspace collection is fixed.

## 6. Ordinary Bash exit, capture and descendants

The current [execution.rs](../../../../crates/layerfs-daemon/src/execution.rs)
launches `/bin/sh -c`, puts it in a process group, supplies null stdin, polls
`try_wait`/pipe reads, consults Workspace revision for progress, retains capped
output and kills the group on its deadline. These are dormant wrapper limitations;
the target is ordinary Bash, streamed output, no automatic timeout and no special
filesystem behavior or implicit APIs.

```text
Bash leader                 child/background worker          FUSE / daemon
-----------                 -----------------------          -------------
fork ----------------------> inherits cwd / file / pipe
exit -> leader exit event      |
                               +--> continues file writes ---> ordinary requests
                               +--> closes stdout/stderr
                               +--> retains mmap / file FD ---> still old Workspace
stdout EOF observed            |
                               +--> last mapping/FD release --> callbacks may remain
                                                               |
caller Commit ------------------------------------------------> finite admission cut
                                                               drain prior attempts
                                                               + reply-send attempts
                                                               capture published state
```

Leader exit, pipe EOF, descendant quiescence, last file release, queued FUSE
request completion and history publication are separate observations. A child can
detach from the original process group, keep a mapping after closing its FD, or
keep mount custody through cwd/root. Pipe EOF cannot prove none of those remain.
The process runner needs generic custody/event tracking consistent with its
declared isolation mechanism, not a command-recognition hook or unbounded `/proc`
scan for every operation. PID/process-group custody alone is not a proof of all
descendants; a controlled cgroup/subreaper design needs its own platform contract.

With write-through cached I/O, ordinary writes are sent as one or more WRITE
requests; kernel writeback would instead permit syscall completion before daemon
delivery. Mapped writes need a separately proved boundary. See
[Linux FUSE I/O modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html).
Capture reflects the actual locally published mutation frontier ordered behind
prior reply-send attempts, not every CPU store to a still-dirty mapping. Admit a
finite boundary, finish/fence earlier admitted mutation attempts and their reply
attempts, then capture through the SQL order. Count this barrier wait separately;
do not chase later traffic or claim a zero-wait generation swap. Waiting for Bash
alone cannot strengthen that definition.

Pinned fuser reply methods return no delivery acknowledgement: a send failure is
logged internally. A locally published mutation therefore remains in the frontier
even if its reply was lost or the caller saw no successful syscall return. Exact
request/custody state distinguishes local publication from attempted kernel
delivery. Capture cannot exclude such bytes by equating absent reply delivery
with rollback, and cannot certify kernel consumption merely because a reply
method returned. See
[reply.rs](https://raw.githubusercontent.com/cberner/fuser/9c957f74efe715112049298cdf1d601781829c8d/src/reply.rs).

The parallel kernel review traced v6.12 `fuse_vma_close` to
`write_inode_now(WB_SYNC_ALL)`, but FUSE writepage ends the original page's writeback
after copying into a temporary buffer and queueing the request. Outstanding FUSE
write counters/buffers live beyond that generic page bit. VMA close does not use
`fuse_sync_writes` as FSYNC/FLUSH do, and RELEASE can await async requests. This
supports keeping waitpid/last mapping close distinct from daemon acknowledgement;
it does not prove an automatic exit fence. See
[file.c](https://raw.githubusercontent.com/torvalds/linux/v6.12/fs/fuse/file.c)
(`fuse_vma_close`, `fuse_writepage_locked`, `fuse_release`, `fuse_fsync`).

Mmap-origin writepage can carry FUSE_WRITE_CACHE even when negotiated writeback is
off. The old adapter refuses that flag (`adapter.rs:565–572`); it also refuses
ctime updates emitted by the mapped-write timestamp path. Therefore simply
changing DIRECT_IO to KEEP_CACHE is not enough for ordinary mmap compatibility.
The kernel-facing adapter must distinguish legitimate mount/handle-owned writeback
from unauthorized requests and map portable timestamp semantics explicitly.
These are compatibility prerequisites, not permission to turn on kernel
writeback or special-case Bash commands. Detailed source tracing belongs to the
kernel investigation.

For a short call with no surviving descendants/mappings, prove actual mounted
ordering of process exit, final WRITE/RELEASE and capture. If a stronger
all-command-effects snapshot is required, derive a generic filesystem/Commit
fence within the profile and no-sync rules; do not silently add command-specific
`msync`, recursive `fsync`, a whole-command pause, or change cached writeback.
Explicit background services can keep task-level Workspace custody; normal
terminal unmount may refuse Busy. No finite automatic Bash lifetime is implied.

fuser's pinned dispatcher currently answers FUSE_INTERRUPT with ENOSYS rather
than exposing a filesystem cancellation callback (`request.rs:117–120`). Ordinary
signals and daemon-owned cancellation are therefore not equivalent to immediate
cancellation of already dispatched SQL/transport work. Drain must retain exact
request ownership until reply/disconnect fencing, with no retry after a guessed
abort. See the separate kernel/callback investigation for reply delivery limits.

## 7. Huge bursts and current Commit blockers

Let N be complete base entries, D captured affected names/inodes, B new/replaced
payload bytes, E normalized edits and V visited kernel identities. Normal mount
should not contain O(N) enumeration or O(base bytes) materialization. Mutation
still performs real indexed metadata/payload work; Commit is not O(1) merely
because capture changes a generation.

```text
huge burst: tiny files + large outputs + scattered edits
                          |
                          v
indexed accepted overlay state (disk grows with actual data)
                          |
                   stable capture D,E,B
                          |
       bounded cursors / replayable scratch -------- later active writes
                          |
       FILE work: new/full streams or localized changed ranges
                          |
       NAMESPACE work: validation + counts + changed trees
                          |
       Save batches / exact history / short install

Required resident shape: windows + admitted concurrency + bounded caches
Current blockers: whole dirty collections and selected whole-base validation
```

### 7.1 More than directory Vec and new parents

The source constraints below are confirmed at the product tree pin. They remain
even if overlay cursors are paged. Increasing `ordering_bytes` exchanges refusal
for resident growth and does not satisfy the required product.

| Source | Work / refusal | Required replacement |
| --- | --- | --- |
| [file/edit/tree.rs](../../../../crates/layerfs-content/src/file/edit/tree.rs):31,358–363 | `EDIT_DEFERRED_LIMIT` bounds deferred nodes and refuses fragmentation | Bounded-backed incremental construction state, no edit-count refusal/fallback |
| [filesystem/input.rs](../../../../crates/layerfs-content/src/filesystem/input.rs):29,86–99 | Directory changes `Vec`; touched serial allowance = ordering bytes / 16 | Stream per-directory changes and backed touched-set interface |
| [filesystem/validate.rs](../../../../crates/layerfs-content/src/filesystem/validate.rs):185–242 | Refuses row/name/demand totals; prefetch demand `Vec`, additions map and parent/name map | Bounded-backed validation/demand/membership and progressive queries |
| `validate.rs:375–510` | Clones parent/name state, creates candidates/changes/bound/seen maps, and walks whole base for stored non-file rebinds | Preserve topology validation with authenticated incremental parent/membership proofs/indexes; bounded backing for any remaining full walk |
| [filesystem/update.rs](../../../../crates/layerfs-content/src/filesystem/update.rs):176–198,429+ | Base-less initial-count and final-row `Vec`; directory contents map | Stream/back counts, root lookup and completed directory state |
| `update.rs:535–552` | New-parent/unreachable map limited by ordering bytes / 1024 | Backed reachability/membership with stable bounded work |
| [references/reduce.rs](../../../../crates/layerfs-content/src/filesystem/references/reduce.rs):177–207; `update.rs:652–667` | `touched_serials` returns entire Vec; only afterward checks the allowance; zero set also resident | Paged/streamed reduction/touched/zero-state contract |
| Current file edit/stream inputs | Sparse holes become zero streams, O(logical length) | Hole-aware canonical/read/edit/stream semantics |

A single existing directory rename can enter parent-alias validation and walk
the whole base even when D is tiny. `walk_limit` is ordering bytes / 1024 and
the pass can refuse with `cycle check work limit`. This is a per-call latency and
base-size constraint, not just a huge dirty-set problem. A bounded full walk
would remove resident growth but still cost O(N); to meet the fast-delta goal,
the contract needs incremental authenticated topology evidence. Do not bypass
cycle/alias checks because input came from FUSE or trusted transport.

Fresh files/full replacements pay O(B). Localized existing edits preserve
unaffected canonical structure but compare and construct replacement bytes in
separate passes; small-file assembly/representation transitions may read more.
Canonical CAS may avoid writing an identical object but still pays identity,
lookup/authentication/reuse work. Storage PREFIX is not a chronological FUSE log.
New 2 GiB build output cannot Commit at metadata-only cost simply because other
parts of the Workspace are unchanged.

### 7.2 Many repeated calls and retention

```text
call n        base Rn -> changes Cn -> saved Rn+1 -> history Hn+1
call n+1      base Rn+1 -> changes Cn+1 -> Rn+2 -> Hn+2
                  |            |
                  |            +--> new/reused immutable objects
                  +----------------> bounded cache may help, not required residency

local overlay namespace n ---------> bounded reclaim (not shared-object deletion)
retained history H0..Hn+2 ----------> intentionally retains old reachable roots
```

Current cluster one has no deletion/collector. Historical versions and abandoned
reference-closed Save waves can consume storage. Exact reuse reduces some growth
but is not a history-size bound. A collector requires live-base/orphan/operation
leases and delta dependency preservation. No terminal unmount guesses global
object garbage. `UpToDate` returns existing head/root for no change, not a new
invocation record for every Bash command.

Failed captures still need bounded view composition with short metadata
resolution. Replaying a large captured/active payload while blocking its inode
is withdrawn. A chain of prior failed generations is also unacceptable. Open-
unlinked content needs an independent owner, not one new pin per successful
Commit. These are prerequisites for repeated loggers/rapid per-call history,
not optimizations that can follow qualification.

## 8. FORGET, unmount and safe automatic cleanup

Lookup reference ownership differs from open-file ownership. fuser documents
that a full set of FORGET messages is not guaranteed at unmount; its default
`batch_forget` loops individual `forget`. Thus a normal terminal path must not
wait for every lookup count to reach zero via messages. See
[pinned Filesystem surface](https://raw.githubusercontent.com/cberner/fuser/9c957f74efe715112049298cdf1d601781829c8d/src/lib.rs).

Current LayerFS `Adapter::forget` calls `Workspace::forget`; that invokes
`State::collect` every time
([namespace.rs](../../../../crates/layerfs-workspace/src/filesystem/namespace.rs):201–207).
Collection scans nodes, follows retained directory ancestry, retains the vector,
rebuilds the node index and filters inherited names
([state.rs](../../../../crates/layerfs-workspace/src/runtime/state.rs):429–465).
K individual forgets over V resident nodes can therefore amplify toward O(KV)
before ancestry costs, with shrinking-set sums still quadratic in a large case.
This is a source cost possibility, not a measured teardown profile or a claim
that every kernel unmount emits V individual messages.

Required: decrement indexed per-inode references and reclaim only directly
eligible ownership; coalesce deferred disposal into bounded background work.
Do not run a whole-node collector per member. The pinned public trait's
`batch_forget` signature names `ForgetOne`, but its import/module are private
(`lib.rs:34,90,429`), with no public reexport found. A LayerFS external impl
cannot name that type to override the method. Treat direct batch override as a
published-API blocker, not already available optimization. Cheap individual
FORGET remains usable through the default loop; supported upstream API resolution
or a proved first-party boundary is required for a custom batch entry point, with
no dependency patch. After actual connection
and request/handle fences, projection lookup ownership can be retired by a short
domain transition and disposed in bounded work. Keep explicit orphan and in-flight
leases; lookup zero alone is not permission to delete their content.

```text
entry fence
   |
   +--> refuse new Exec/Commit/open acquisition
   +--> keep RELEASE / replies / cancellation service runnable
   |
process + descriptor/mapping + accepted request/transport custody fenced
   |
native unmount attempt
   |
   +-- Busy/failure ----------> retain exact owner, no terminal success
   +-- lazy detach ----------> path gone, connection references may still exist
   |                            retain custody until exact destruction/fences
   +-- normal detach known --> session loops exit/join and replies/jobs fenced
                                  |
                                  v
                         retire lookup ownership domain
                         logical namespace closed
                                  |
                         terminal result + owned cleanup debt
                                  |
                         bounded metadata/payload/scratch/orphan reclaim
```

Distinguish native paths precisely. The pinned pure-Rust privileged path calls
ordinary `nix::mount::umount` through `mnt/mod.rs:169–192`; it does not request
MNT_DETACH. The EPERM/helper fallback in `mnt/fuse_pure.rs:143+` attempts
MNT_DETACH and later a fusermount `-z` route. This distinction comes from
[mount dispatch](https://raw.githubusercontent.com/cberner/fuser/9c957f74efe715112049298cdf1d601781829c8d/src/mnt/mod.rs)
and [pure mount implementation](https://raw.githubusercontent.com/cberner/fuser/9c957f74efe715112049298cdf1d601781829c8d/src/mnt/fuse_pure.rs).
Do not label normal privileged unmount as lazy. Still, mount-path absence alone
is not a general descriptor/request/semantic ownership proof. The
[kernel connection contract](https://docs.kernel.org/filesystems/fuse/fuse.html)
explicitly distinguishes lazy detachment from final reference release.

Normal unmount refuses active Exec/handle/Commit/uncertain custody and preserves
the Workspace. It does not silently kill an unlimited Bash. Forced policy must
fence processes, parked replies and transport/constructor jobs; killing the
leader cannot prove queued SQL or host publication stopped. Unknown history
stage/transition/discard retains exact context or returns an explicitly
acknowledged-unknown forced disposition. Known publication survives failed local
install/teardown and is reported as such.

Reclamation is automatic unmount ownership, not another public close. A bounded
logical retirement avoids foreground DELETE of all local rows, but total cleanup
work grows with actual metadata/payload/scratch. Native kernel inode/page disposal
can also grow with V; moving SQLite cleanup to background does not make native
detach O(1). Repeated calls require cleanup service sufficient for admitted discard
rate, reserved headroom and visible debt—not indefinitely deferred work outside
reported memory/disk totals.

## 9. Concurrent Workspaces and publication

```text
Workspace A / mount A      Workspace B / mount B       shared resources
---------------------     ---------------------       ----------------
ordinary writes           ordinary reads/writes       short overlay jobs interleave
capture CA                capture CB                  independent generation domains
construct CA              construct CB                one producer each, outside SQL
accept A1                 accept B1                   fair runtime batches + demand reads
finish SA                 finish SB                   finish/history cannot starve
stage TA                  stage TB                    exact stage custody
transition TA             transition TB               conditional Branch updates
known install A           known install B/conflict    no global whole-Commit lock
unmount A + cleanup       commands continue B          cleanup yields; no global drain lock
```

One daemon SQLite writer still serializes transaction execution; one runtime
provider owner may serialize persistence. Multiplexing makes those bounded jobs
interleave, not physically simultaneous writers. Separate Save capabilities
cannot occupy every transport route for their entire lifetime. The `Save<'_>`
borrows Storage/mutable indexes; sound interleaved lifetime ownership is a current
API integration prerequisite.

Same-Branch captures against the same head can conflict. No scheduler ordering
guarantees both may publish their independently constructed states to that head.
Exact conflict/discard disposition preserves the losing Workspace's writes;
automatic rebase is not introduced. If calls must produce a single sequential
task history, caller orchestration must await the prior exact history outcome
before selecting the next root. Different Branches remain independent within
shared capacity. Immutable objects do not resolve mutable Branch/stage/allocator
transactions or unknown acknowledgements.

## 10. Cost and resource model

The following models are reasoning obligations, not measurements:

```text
T(call) = authorized base binding + logical bootstrap + native attach/INIT
        + ordinary Exec demanded work
        + capture + file/namespace construction + Save + history + install
        + required activity/native fences + logical unmount

separate attributable work/debt: physical reclaim and retained history
do not add overlapping callback/transport/construction wall spans twice

resident memory = fixed daemon state + admitted per-call/session windows
                + fuser loop buffers/stacks + reply/queue bytes
                + constructor/scratch/journal windows + all Save caches
                + attributable kernel inode/dentry/page + backing file cache
```

Kernel inode/dentry state can grow with the tree visited under long TTL; it is
not fixed merely because SQL is paged. Kernel cache eviction/pressure and bounded
backing/FUSE residency require proof and resource accounting. The process heap
and one pager allowance are not a total-memory bound. Do not quietly retain one
resident dirty-frontier entry per changed file under a fixed metadata cap.

For sustainable per-call throughput, required SQL service per second must remain
below shared writer/device capacity and cleanup service must cover discarded debt.
Fairness prevents starvation but cannot fix overload. Slow Save keeps a capture
alive while active distinct writes/appends grow; that is real retained data, not
necessarily a metadata leak. Account it and use explicit resource admission.
No tiny mutation should synchronously pay arbitrary old cleanup before reply.

No total file/Workspace/Commit bytes, name/edit count or runtime cap is accepted
as a consequence of resident data structures. Windows continue; actual finite
device/platform/format/resource conditions remain explicit. Sparse logical
length must not turn into proportional zero processing. A large accepted state
must be constructible rather than rejected after history already published.

## 11. Historical observations useful to this investigation

The complete retained fixture is 130,045 entries and 3,475,776,149 regular-file
bytes: 103,108 files, 16,867 directories, 10,070 symlinks. Dependency replay is
95,021 entries / 2,126,509,110 bytes. Source HEAD
`639ed015397290b3745d163aafe02ffee4aa3f84`; manifest SHA-256
`98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658`.
These are acquisition observations from `PREPARATION-REPORT.md`, not a new scan.
Native Init's current symlink/large-file refusal and scan collections must be
corrected before provisioning this full supported base; repeated mount never
runs that importer.

| Retained observation | Useful inference | Boundary |
| --- | --- | --- |
| A2 E01/F mount 0.028993 s, Exec 0.025510 s, unmount 0.015009 s | Fixed native/process overhead merits event attribution | Passthrough; Exec/unmount inside polling erratum; no Commit |
| A2 E12 replay: 698,292 requests, Exec 42.201692 s, unmount 0.129248 s | Lower requests help, but huge burst and visited state remain significant | One old diagnostic; no overlay/Save; source and cache qualifications retained |
| A2 E13 hard-link replay: 985,870 requests, Exec 58.601360 s, unmount 0.127328 s | Aliases/namespace traffic need complete-call accounting | Same limitations; not a benchmark target or repeat authorization |
| A2SI E18: 38,294 → 21,158 requests and 15,159 → 37 READs versus A2S | Stable stat identity and carried Git index can avoid application content rechecks across native mounts | Permissions disabled, aliases unqualified, index excluded from historical whole-tree proof |

The last row is not proof that `.git/index` survives LayerFS Commit. The actual
product must include and independently prove it; historical exclusions do not
authorize present exclusions. Residual E18 LOOKUP/READDIR work is evidence that
stable identity does not magically preserve a prior mount's kernel cache.

Read exact retained reports with:

```text
git show codex/phase7-experiment-305:core/docs/issues/305/STAGE-A-REPORT.md
git show codex/phase7-experiment-305:core/docs/issues/305/A2-IDENTITY-REPORT.md
git show codex/phase7-experiment-305:core/docs/issues/305/CF-FSBENCH-V2-CONTRACT.md
```

They retain their original experiment identities/caps/verifier qualifications.
None was run again here. Historical 600 s exceptions do not transfer to future
standard selections.

## 12. Tempting shortcuts incompatible with the contract

| Shortcut | Why it does not satisfy the required per-call path |
| --- | --- |
| Rebind a used mount's base and call it a fresh Workspace | Cached dentries/pages and old FD/cwd/mappings can address old state; independent ownership was not established |
| Bind-mount the same initialized FUSE FD/superblock | Creates another projection of existing state, not automatically an independent mutable view |
| Ignore `.git`, deps, caches or output | Next call is not ready and changed full state/history is lost |
| Prefetch the full next root before starting the mount timer | Moves demanded work into preparation and invalidates cold attribution |
| Treat Bash wait/pipe EOF as filesystem quiescence | Descendants/mappings/outstanding requests can remain |
| Lazy detach then delete local payload | Remaining connection/descriptor/request ownership can still require it |
| Wait for every FORGET before terminal close | Full forget delivery is not guaranteed |
| Larger ordering/deferred budgets | Trades refusal for input-proportional resident memory |
| More constructor helpers for the huge burst | Violates one producer per Commit and changes the measured mechanism |
| Whole-Commit transport checkout or strict reads-first service | Blocks demand access or starves finish/history |
| Report only Exec latency | Hides capture/Save/Commit/native drain and cleanup cost |
| Delete old global objects when local unmount succeeds | Shared history/delta dependencies and live roots are not local garbage |
| Patch fuser for smaller buffers or custom run loops | Third-party modifications are prohibited; supported APIs or a first-party boundary need proof |

## 13. Prioritized proof and diagnostic matrix

No tests or diagnostics below were executed. IDs describe proposed coverage, not
already frozen benchmark registrations.

| Priority / ID | Correctness or diagnostic subject | Evidence required before a throughput claim |
| --- | --- | --- |
| P0 L1 | Full root per-call mount → Bash → Commit → terminal unmount → next mount | Exhaustive supported membership/stat and required content/index/alias checks; no filters/reinstall/root scan |
| P0 L2 | Late WRITE/RELEASE, inherited FD/cwd/mmap, detached descendant and lost reply | Finite admission barrier drains prior attempts/send attempts; published state remains included; no command hooks or inferred delivery/quiescence |
| P0 L3 | Lost stage/transition/discard, known publication/local failure | Exact phase/fences and retained custody; no guessed cleanup/retry |
| P0 L4 | Independent mounts with identical serials; stale requests after next call | Correct incarnation/session routing, no stale cache/descriptor redirection |
| P0 L5 | Large fragmented/wide/new-parent/touched/alias/sparse Commit | Corrected cluster-one structures complete without count/length refusal or process-size growth |
| P0 L6 | Repeated orphan successes and failed captures during append | Bounded versions/depth, stable bytes/attrs and no payload-sized hot-inode pause |
| P1 D1 | Daemon readiness and native attach events | DB schema/open count, device/mount/INIT/thread readiness and exact root reads; no hidden target priming |
| P1 D2 | fuser buffer/thread lifecycle | Allocated lengths, actual residency/faults, thread creation/join and retained reply/FD memory; 128 KiB not assumed buffer bound |
| P1 D3 | Full visited tree under FORGET/batch FORGET/terminal detach | Messages/member counts, nodes scanned, ancestry work, foreground disposal; no O(KV) collector |
| P1 D4 | Two blocked inode requests plus unrelated activity | Deferred workers remain runnable; release/cancel drain progresses |
| P1 D5 | Huge mutation burst and small directory rename | Visited changed/base entries by validation site, peak resident collections, scratch IO and emitted/reused objects |
| P1 D6 | Many Workspaces with simultaneous Saves/unmounts | Per-class queue/service wait, demand capacity, finish/history progress, isolation and exact conflicts |
| P1 D7 | Rapid repeated calls near disk pressure | Accepted/discarded/reclaimed bytes/pages, reserves/debt and service convergence; native versus SQL cleanup separated |
| P2 Q1 | Prospective full pipeline performance | Matched identity/cache/workload, one sample, phase-complete wall/resource receipt and separate proof |

Counter-driven diagnostics should identify causes: native syscalls and INIT
events, receive-buffer faults/bytes, projection references, queued replies,
visited validation rows, SQL/BLOB/journal/page work, replacement passes,
construction emitted bytes, Save reuse/encoding/publication and cleanup debt.
Do not turn those into an unchanged-arm speed rerun or claim cumulative/lifetime
counters are phase values.

Before any future benchmark invocation, read `benchmark_agent_report.md` and
applicable benchmark rules. Prepare once, use independent `--setup clone` where
applicable, declare/enforce equal cache states including residency for cold
claims, pin release/locked/source/workload identities, one sample per case/arm
and one construction producer. Keep every failure/unrun/INELIGIBLE row and fresh
append-only outputs. Standard complete commands are ≤15 s, with prospectively
declared exceptions up to 25 s and separate verification under its bound. A
selection that cannot fit is cited from qualifying evidence or retained NOT_RUN;
do not filter the full tree, warm-repeat, shrink input or enlarge timeouts/workers.
These measurement budgets are not product Bash lifetime or Commit-size ceilings.

The implementation sequence should first fix custody and current cluster-one
whole-set/whole-tree constraints, then remove polling/collection overhead and
prove fair bounded service, then determine the real native session floor. A
more complicated mount pool or protocol adapter is justified only by those
source/count results and a concrete independent-view proof.
