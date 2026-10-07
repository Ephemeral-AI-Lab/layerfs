# 01 — Cluster-two architecture and ownership

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-05 against product `f96d97651`, reviewed design `334fc7437`
> and the owner's subsequent directions. No new implementation or performance
> qualification is claimed. [README](README.md) indexes the primary contracts.

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

## 1. What the owner requires

Implement both current workstreams in `core/`. Root `crates/` is the v0.1.6
reference and will be removed once cluster two is complete; do not extend it as
the new implementation or import it as a runtime dependency/fallback. Excluded
integration directories under core are not proved current product merely by
their presence. Use the current contracts and actual workspace membership.

The smallest generally supported orchestration granularity is a tool call.
One Workspace can serve many sequential or concurrent calls over a long lifetime,
with repeated incremental Commits; it is not tied to one invocation or task.
New mount binds a complete committed root. Further calls on an existing mount
observe its current live filesystem, including uncommitted and later active
changes. Both include `.git`, ignored files, dependencies, symlinks, caches and
output. No per-call install/restore or filtered ephemeral path substitutes.

Support both per-tool-call and per-task orchestration, with per-tool-call the
expected common case. Either can contain short or long-lived Execs. Granularity
selects caller ownership/reuse policy, not command duration or a timeout; no
command-class heuristic changes Commit or teardown behavior.

```text
 application/controller                           sandbox daemon
 ----------------------                           --------------
 complete committed root R0  -- mount -----------> Workspace W1 + FUSE
 normal command              -- exec -----------> ordinary Bash
 explicit publication        -- commit ---------> capture + content construction
         ^                                             |
         |                  runtime adapters <---------+ bounded canonical objects
         |                  embedded cluster-one libraries
         |                  immutable storage + typed history transition
 known root/head R1 <-------+                          |
 terminal teardown           -- unmount ---------> detach + logical close + reclaim

 next call: mount complete R1 -> exec -> commit R2 -> terminal unmount
 persistent W: mount R0 -> exec A/B -> commit C1 -> exec C/D -> commit C2 -> unmount
               same mount; calls/Commits do not close W; later writes stay live
```

Exec itself never interprets commands, commits, filters paths or changes lifecycle.
It has no automatic Bash runtime timeout. A controller explicitly sequences calls
and must preserve exact failed/uncertain publication outcomes. A command exiting
nonzero may have valid acknowledged filesystem changes; no implicit rollback is
invented. No-change Commit is UpToDate, not automatically a new invocation record.
Calls sharing W share ordinary filesystem visibility; Commit records the whole
captured Workspace frontier rather than an isolated delta for its invoking call.
Known install advances W's base without remounting or discarding later changes.

## 2. The shape in one picture

```text
host Project Init -> seal -> install once -> named in-VM shared Store
host filesystem controls -------------> Linux daemon A / B / C
                                          in-process Content / Storage / History
                                          one Store + fixed readers/cache each
                                          one local Overlay SQL owner each
                                          Workspace / FUSE mounted directories
                                                       ^
ordinary Sandbox/runtime or external executor ---------+
commands, standard streams/status and explicit cancellation
```

Host retains no installed Store/data service. Every daemon opens shared Store
directly; no legacy layerfs-server or host runtime adapter returns. Filesystem
controls and ordinary runtime execution have separate owners. Immutable IDs
permit reuse/distribution but imply no distributed provider, authority, exact
history completion or safe collection.

## 3. Ownership, execution location, database and transport

| Concern | Owner / location | Required boundary |
| --- | --- | --- |
| Mutable metadata and physical payload | daemon overlay engine / shared SQLite | Every key/query constrained by Workspace namespace/incarnation |
| Filesystem semantics and stable views | Workspace / daemon | No SQL text, kernel protocol or pack format knowledge |
| Kernel protocol/coherence | FUSE adapter / daemon | Ordinary syscall path for every authorized process |
| Bash and streaming process I/O | ordinary Sandbox/runtime or external executor | No automatic runtime cap or command-specific filesystem hooks |
| Registry/lifecycle/admission | daemon composition | Short routing references; no whole-Exec/Commit slot lock |
| Logical content reads/construction | layerfs-content / daemon | Stable sources, Store policy, bounded synchronous output |
| Saved objects and physical encoding | cluster-one Storage/provider / daemon | Immutable identities, semantic admission, reference closure |
| Branch/stage/Commit/serial authority | HistoryCatalog/provider / daemon | Atomic overwrite Branch publication with captured parent; exact outcomes |
| Cross-process delivery | authenticated filesystem control and ordinary runtime routes | Bounded correlated calls, authority binding, cancellation fences |

One overlay database is opened/schema-initialized before daemon readiness.
Workspace mount creates only small logical state and a base binding; no database
creation, namespace scan or eager payload acquisition. Indexed demand I/O remains
real, especially after cold mount. Fast bootstrap is a requirement, not a result.

## 4. The three paths

| Path | Required sequence | Resource boundary |
| --- | --- | --- |
| Read/lookup | Bounded consistent overlay plan, immutable references, cached/base demand read | Release locks before fetch; metadata queries exclude payload BLOBs |
| Mutation | Fair admission, consistent validation, bounded SQL data/metadata transaction, reply | No base-payload copy-up, construction, network or arbitrary cleanup in transaction |
| Commit | Fixed capture domain, construction, Save finish, atomic stage-and-publish, install | Bounded SQL windows; canonical work/transport outside overlay owner |

State is disk-backed rows, not arrays sized to dirty file/name/edit counts. Paging
limits memory windows, not total accepted files/bytes. Cluster-one deferred-node,
directory Vec/new-parent map, sparse and initial-import limits remain required
changes. No cap increase or whole-file error fallback substitutes for those changes.

## 5. Inside the daemon

```text
 Registry: WorkspaceId/incarnation -> Workspace reference
             | W1 queues            | W2 queues            | W3 queues
             +----------------------+----------------------+
                                    v
                 fair bounded read/write/capture/operation records/reclaim jobs
                                    |
                      overlay owner / shared pager
                                    |
                         overlay.sqlite

 independent daemon work: content construction per Commit, Store demands,
                          runnable filesystem service; commands remain runtime-owned
 parked requests: inode/resource waiters retain bounded cancellable replies
```

Overlay uses the selected one-owner connection MEMORY/OFF/EXCLUSIVE profile;
shared Store is separate, Disposable/WAL/OFF, opened directly in each daemon
with a fixed read set/cache and one writer session. No upstream exists. Neither
single-writer database serializes whole commands or Commits. SQL jobs retain
short transaction ownership and bounded fair service; actual device throughput
and whole-system residency remain qualification work.

Fairness spans Workspaces and service classes, including live/idle cleanup.
Waiters park without occupying all native workers or holding Workspace locks
across owner/Store waits. Each Workspace admits one Commit lifecycle, with one
construction producer and its local borrowed Save. Different Workspaces may
construct concurrently, paying explicit shared resources and one-attempt Store
Busy outcomes; no whole-Commit writer gate or extra construction helper.

## 6. Trust boundary

Commands use configured non-root identity/environment without daemon credentials.
The shared overlay stays daemon-private outside the FUSE mount. Namespace checks
protect logical ownership; database/daemon failures remain shared physical domains.
Authenticated controls bind peer, Workspace incarnation and Branch/scope.
Constructed objects come from Content in-process; Store derives policy and
reference closure. Sandbox/executor protects whole-Store authority and credentials;
actual access setup/native proof remains required. No Save wire capability exists.

Immutable objects are suitable for reuse/replication and prevent in-place version
corruption. They do not establish durability, safe GC, distributed publication or
mutable history isolation. [06 §5](06-cluster-one-integration.md#7-authority)
separates content and authority planes.

## 7. Decisions that shape the architecture

| Decision | Current disposition |
| --- | --- |
| One overlay DB per daemon | Owner decision; initialized once, all WS keys prefixed |
| Complete per-tool-call root | Owner requirement; no ignore/dependency/cache filtering or per-call restore |
| Ordinary Bash without timeout | Owner requirement; explicit cancellation, streaming I/O |
| Terminal unmount includes close/cleanup | Owner decision; no separate public close or implicit Commit |
| Daemon construction / embedded runtime storage | Proposed boundary; Linux/API/admission prerequisites |
| Retired layerfs-server | Owner direction; do not restore old package/coordinator |
| Bounded payload and failure/orphan representation | Required algorithms; not completed by this topology |
| Cached FUSE profile | Promoted candidate; coherence/residency proofs required |

Detailed contracts: [engine](daemon-sqlite.md), [FUSE](fuse.md),
[mount](workspace-api/mount.md), [Exec](workspace-api/exec.md),
[Commit](workspace-api/commit.md), [terminal unmount](workspace-api/unmount.md),
[status](workspace-api/status.md).

## 8. What is removed

Phase-4.5 private page files/index revisions, charged resident dirty/name/edit
frontiers, 128-handle arrays and 256 MiB prepared-stream total-upload restriction
are retirement targets. The new path must complete full affected state with bounded
processing windows and backpressure. Source/evidence inventory remains in
[02](02-base-overlay.md); claims of removal require implementation proof.

## 9. Exec is an ordinary shell process

The [Exec contract](workspace-api/exec.md) is authoritative. Shell launch, process
identity, streaming I/O and cancellation are runner concerns. File access goes
through the same FUSE callbacks as any other authorized process; no Git/build/
installer special treatment. Lifecycle remains explicit, including capture,
publication and terminal unmount. PTY mode is distinct from noninteractive Bash.
