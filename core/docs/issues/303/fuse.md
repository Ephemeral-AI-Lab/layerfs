# FUSE: complete filesystem execution and kernel coherence

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-05 against product baseline `f96d97651` and design
> `334fc743751b9a181e670d0601a24fb3169208f9`, with the owner's subsequent
> per-tool-call, shared-database and terminal-unmount directions. Nothing here
> is implemented or measured. This is the primary kernel-facing design;
> [05](05-fuse-assessment.md) retains source/evidence assessment.

Owner supersession 2026-10-08 at R0 input `1a6bb53ef`: the SDK exposes
ProjectApi, WorkspaceApi and SandboxApi. Ordinary Sandbox/runtime or an external
executor owns command launch, standard streams, exit status and explicit
cancellation; the filesystem daemon owns no command supervisor, launcher,
per-Exec cgroup, command registration or custom Exec wire. FUSE serves every
permitted visible process. Shell exit/zero registered commands proves no
filesystem drain, and forced filesystem teardown never implicitly kills caller
processes. Current [S8 specification](../307/S8-SPECIFICATION-20261008.md) and
[R0–R9 rollout](../307/ROLLOUT-LEDGER-20261008.md) govern prospective work;
historical baseline pins, receipts and verdicts retain their original scope.

Source-ownership revision2026-10-08 [proposed design, reviewed against verified
R1 at `5be93f6d7`]: [reviewed ownership](../307/R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md)
assigns the complete native connection/request service to `layerfs-fuse`.
Daemon assembles it with the existing shared SQL/Store services and retains
registry/control, overall Ready/terminal unmount and the existing Commit driver.
Dependency is daemon -> fuse -> workspace; Fuse imports no daemon. This changes
proposed homes, not filesystem guarantees or proof outcomes. Native implementation
remains R2–R5; optional admin is deferred and R1 is verified.

## 1. Load-bearing contract

One tool call is the smallest supported orchestration granularity. A Workspace
can also serve many sequential or concurrent calls over a long lifetime with
incremental Commits. Fresh mount must expose the complete committed filesystem;
further calls on the same mount use its current live view. This includes source, `.git` including
index/objects, ignored files, dependencies, symlinks, caches and build output.
Git ignore rules affect Git, not LayerFS membership. There are no special local
paths, temporary dependency projections or Exec-time restore/install hooks.

Both per-tool-call and per-task orchestration are required; per-tool-call is
the expected common mode. Short and long-lived Execs occur in either. Cache,
dispatch and ownership policy must not assume a command finishes quickly or
turn mode selection into automatic Commit/unmount or an elapsed-time limit.

```text
 caller selects full root R
          |
     mount Workspace ----------------------------------------------+
          |                                                       |
 ordinary Bash / any authorized process                            |
          | normal open/stat/read/write/rename/unlink syscalls       |
          v                                                       |
 kernel mount: dentries + attrs + cached file pages                  |
          | cache miss / write-through request                     |
          v                                                       |
 FUSE adapter -> Workspace semantics -> shared overlay SQL owner   |
                        | uncovered immutable base ranges          |
                        +--> content read -> base cache/runtime    |
          |                                                       |
          +---------- coherent reply ------------------------------+
          |
 caller explicitly Commit; mount can continue -> eventual terminal unmount
```

Immediate execution readiness means no namespace reconstruction or dependency
preparation at mount. Cold lookup/data faults still cost authenticated demand I/O;
a whole-root scan or payload prefetch is not hidden in setup. Historical Init symlink refusal is superseded by current backed faithful import.
Acquisition of the historical full fixture into an installed Store and its native
proof remain required; all 10070 symlinks stay in scope.

## 2. Profile and inherited behavior

| Concern | Current dormant implementation | Target / proof obligation |
| --- | --- | --- |
| Library | `fuser =0.18.0` | Retain Linux-gated adapter |
| Entry/attribute TTL | Zero | Owner-promoted 60 s candidate, coherent replies required |
| File reads | Writable opens use DIRECT_IO | KEEP_CACHE candidate; bounded attributable residency |
| Kernel writeback | Off | Retain; accepted writes reach daemon before return |
| Requests | 128 KiB | Initial window, not file or total-flow limit |
| Threads/background | Two / background 1 / congestion 1 | Initial settings; deferred waits and fair dispatch required |
| Permissions | default_permissions plus guards | Retain enforcement; proper non-root command identity |
| Serial identity | Canonical inode serial | Stable across rename, Commit, install and remount |
| Enumeration | Name-based cookies, charged per-entry maps | Preserve semantics with bounded reply/cursor storage |
| fsync/fsyncdir | Unsupported | Promoted no-op compatibility; no durability claim |
| xattrs/ownership | Unsupported | Explicit supported-format boundary, never hidden omission |
| Contention | Reply permits can return EBUSY | No contention EBUSY; park waiters without holding workers |

Current sources: [adapter](../../../crates/layerfs-fuse/src/adapter.rs),
[mount](../../../crates/layerfs-fuse/src/mount.rs),
[replies](../../../crates/layerfs-fuse/src/replies.rs). These crates are excluded
from the active core workspace; their presence is not a built replacement.
Portable metadata's ctime=mtime behavior is an explicit compatibility decision,
not full POSIX metadata fidelity.

## 3. Callback ownership and scheduling

```text
 mount A request --+                     daemon overlay.sqlite
 mount A request --+-> Fuse shared dispatch -> existing fair SQL owner
 mount B request --+              |
                                  +-- no request owns SQL connection

 if an original engine/immutable demand is pending:
     retain bounded reply + incarnation + cancellation state
     park request ---------------------------> waiter registry
     return worker to dispatch
     completion/credit notification -> resume original continuation -> exact reply

 two parked requests for A/file-x
     do NOT consume both workers serving A/file-y
```

Per-mount namespace/incarnation routing binds every callback to its owner. One
shared database can serialize SQL writes; it does not serialize whole Execs or
Commits. No worker holds Workspace locks while queued for SQL, base fetch or
transport. Avoid automatic SQL retry/reprepare after an outcome: readiness before
an attempt and retrying a failed attempt are different operations.

Write updates data/validity, size and mtime atomically before reply. Read copies a
consistent bounded plan and retains immutable roots before fetching base data.
The payload algorithm must bound work even after dense one-byte fragmentation;
128 KiB alone does not bound the old number of intersected extents. See the
[engine](daemon-sqlite.md) for data/ownership/service contracts.

Fuse session/dispatch/operations/coherence own this full kernel lifecycle.
Handlers reuse Workspace semantics and atomic backed Overlay lifetime jobs;
operation handlers do not become another filesystem engine. Daemon supplies
concrete services through narrow ports, keeps one SQL owner shared with Commit/
cleanup, and composes overall Workspace Ready/drain. A fixed K Fuse worker pool
is shared across mounts, not recreated for each connection.

Current OwnerClient read/job adapters block on Pending.wait; moving them behind
an interface is insufficient. Missing native service interfaces must transfer an
original pending operation and race-safe completion/loss/credit notifications,
allow Fuse to park without a worker/lock, and preserve exact terminal custody.
No polling, thread-per-waiter workaround or failed-operation replay is selected.
Fuse connection-serving/drained receipts do not alone establish aggregate
Workspace Ready or terminal unmount. Full I-3/I-8/I-9/I-14 remain in force.

## 4. Cache coherence and lifetime transitions

| Event | Required action |
| --- | --- |
| FUSE write/create/remove/rename/truncate | Prove kernel reply ordering, alias/attribute/page coherence; zero per-WRITE invalidation is a liveness requirement under cached I/O; prove coherence through replies |
| Non-FUSE mutation, if supported | SQL commit, then entry/inode invalidation before caller acknowledgement |
| Capture | No visible change; immutable captured domain and current view preserved |
| Known install | No invalidate only if names/bytes/links/attributes/serials are identical before/after |
| Failed/uncertain Commit | Preserve current view and exact custody; replacement composition proof required |
| Retirement/consolidation | Delete only unreachable state; do not change visible bytes or attributes |
| Terminal unmount | Reversible normal kernel Busy probe; force control-producer refusal, one connection-specific abort/one plain detach; full loop + daemon-work drain before indexed ownership revocation/Close/cleanup |

```text
 request plans against [active A -> captured C -> base R]
       | retains exact immutable source roots
       |                         Commit installs R' = C over R
       |                              |
       +-- later base fetch uses R ---+   new requests use [A -> R']
               both return the same planned bytes
```

KEEP_CACHE is not permission to serve stale data. A TTL bounds expiry, not
correctness. Test hard-link aliases, replacement rename, attribute replies racing
writes and partial cached page tails after shrink/regrow. Dirty mmap stores not
yet locally published by the daemon are outside capture; Exec exit alone is not a
flush fence for inherited mappings/descriptors. Capture includes the actual local
publication frontier, ordered with earlier mutation/reply-send attempts. A lost
reply does not remove successfully published state; fuser supplies no kernel
delivery receipt. No whole-Exec pause is introduced.

Open/RELEASE counts and kernel lookup/FORGET references have different lifetimes.
Orphan ownership must survive repeated successful Commits without accumulating
pinned generations. Directory enumeration merges base and overlay in name order,
keeps bounded resume state and accounts for partially consumed replies. Arbitrary
seek behavior under mutation must be specified; positional cookies are not assumed
stable. No array size limits simultaneous files or directory population.

Persistent Workspace support is an owner requirement, not an open lifetime
decision. Calls and incremental Commits retain the same native connection and
valid dentries/attributes/pages. Known install must preserve the live view without
a full cache sweep or remount; later writes remain active. Repeated Commit,
failure, orphan retention and automatic reclamation need sustained bounded
ownership proofs over that long lifetime. No call-count or automatic lifetime
limit is introduced.

## 5. Optimization disposition

Apply long TTL, cached reads, stable identity, bounded immutable base caching,
event-driven lifecycle and removal of unnecessary per-entry bookkeeping only
under their corresponding correctness/resource proofs.

Investigate negative entries, adaptive READDIRPLUS, directory cache, FLUSH elision,
larger requests and more dispatch/background concurrency prospectively. Handle-free
opens require a replacement exact orphan/reference lifetime model. Reject kernel
writeback for the acknowledged-write contract, mutable-file passthrough bypassing
capture, permission removal and fixed CPU affinity as product policy. Removing
avoidable copies is useful but no measured zero-copy claim exists.

Historical A2 is ext4 passthrough, not this overlay. Its promotion is a candidate
selection; #305 B survival proof failed and concurrency Stage C was NOT_RUN.
See [evidence assessment](05-fuse-assessment.md) for identities/qualifications.

The [per-call optimization investigation](fuse-optimization-investigation.md)
records additional source prerequisites: pinned fuser supports deferred replies,
but receive buffers/session workers are independent of negotiated request size;
individual FORGET must avoid repeated whole-state collection. Cached dirty mmap
can emit FUSE_WRITE_CACHE without negotiated mount-wide writeback, while today's
adapter refuses that request flag. The target therefore needs exact kernel-origin
identity/handle semantics and capture ordering, not just an open-flag change.
Research alternatives remain subject to the profile and proofs above.

## 6. Workloads and admission proof

| Workload | Required observation |
| --- | --- |
| Full repository walk and git status across two tool calls | All ignored/dependency/cache entries visible; stable stat identity; persisted git index; no mount-wide scan |
| Copy and hard-link dependency replay | Full 95,021 entries / 2,126,509,110 bytes, aliases correct; no hidden staging/preparation |
| Many tiny files and churn | Bounded replies/lookup ownership; no per-entry resident quota or count cap |
| Large reads/writes and sparse files | Request windows/backpressure/residency; holes correct; sparse Commit prerequisite separately proved |
| Overlapping writers, O_APPEND and tail | Acknowledgement ordering and correct content; syscall/request split semantics explicit |
| Log rotation, retained descriptor, repeated Commit | Correct orphan bytes with bounded read depth/versions |
| Dense scattered edits then full-window overwrite | Bounded payload SQL/BLOB/journal work, no historical-fragment pause |
| Two guarded-inode waiters plus unrelated access | Dispatch remains runnable and service fair |
| Simultaneous Workspaces/Commits | Correct namespace routing; cached reads/writes proceed, shared resource scope reported |
| One persistent Workspace, repeated overlapping calls and incremental Commits | Same-mount caches coherent across calls/install; later writes survive; no Commit-count growth in versions/orphan depth; reclamation progresses before final unmount |
| Terminal unmount/disconnect during requests | No callback use-after-retire or guessed Commit outcome |

Full retained fixture: 130,045 entries, 3,475,776,149 regular-file bytes, copied
HEAD `639ed015397290b3745d163aafe02ffee4aa3f84`, manifest
`98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658`.
This is historical acquisition evidence on experiment branch `1451b68a7`, not a
new scan or performance claim. Workload rows here are design coverage, not a frozen
benchmark selection. No workload was run to write this document.

Record request counts, visited entries, worker occupancy, queued bytes, reply
copies, pager/journal/guest cache residency and phase walls. Per-tool-call total
includes mount, Exec, Commit and terminal unmount; background reclaim debt and
eventual completion stay visible. Follow [validation](07-implementation-validation.md)
and repository one-sample/cold-cache/budget rules before any future measurement.
