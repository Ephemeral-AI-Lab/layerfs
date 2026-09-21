# Workspace and FUSE contract

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Updated 2026-09-21. This document specifies intended behavior and admission
> requirements. It does not claim an implemented core mount, POSIX conformance,
> measured latency, qualified concurrency, or crash durability.

[Packet index](README.md) · [Overlay and snapshot](02-overlay-snapshot.md) ·
[Commit integration](03-commit-integration.md) ·
[Implementation and verification](04-implementation-and-verification.md)

## 1. Source authority and capability boundaries

The API baseline is **`152b9c3a2e8ec2536a1d63601b681e1f7ef34455`**. Source links
below pin that commit. The destination checkout in which this packet was written
is older and contains unrelated work; the packet does not imply that its local
product sources already contain the baseline APIs. No checkout update is part of
this document. Reference comparisons use exact v0.1.6
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`.

Three capability stages are distinguished throughout:

| Mark | Meaning |
| --- | --- |
| **R** | First proposed Linux mount: an already prepared, immutable filesystem root; read operations and truthful refusal of mutations |
| **W** | A later selected writable operation, enabled only after its local semantics, shared-operation dependencies and mounted tests pass |
| **Deferred / prohibited** | No support claim; either omit capability negotiation and use a documented kernel facility, or explicitly refuse the operation as specified |

R is a useful first capability slice, not completion of #179 or a writable-shell
comparison for #207. Full W targets frozen generation G plus a live successor;
blocking all writes for the duration of Commit is not equivalent acceptance.
The detailed snapshot algorithm belongs to [02](02-overlay-snapshot.md).

Pair 2 provides logical stages, Commits and Layers. Its implementation does not
qualify this projection: #210 remains open for H04, the remaining H06/H08
schedules and full H14. No prior core-only result establishes mounted behavior.

## 2. Component ownership

```text
 shell / editor / compiler / application
                  |
             POSIX syscalls
                  v
       local Linux kernel and VFS
                  |
             FUSE callbacks
                  v
 +------------------- daemon process --------------------+
 | layerfs-fuse library                                 |
 |   request/reply types, mount, kernel IDs, errno        |
 |                  |                                   |
 |                  v                                   |
 | layerfs-workspace library                            |
 |   identity, live namespace/inodes, handles, G/G+1      |
 |   bounded byte segments, piece views, cursors          |
 |   local visibility, lowering, lifecycle result state  |
 |                  |                                   |
 |        existing logical bridge Client                 |
 +------------------|-----------------------------------+
                    | authorized logical operations
                    | bounded frames; terminal result
                    v
 +------------------- service process -------------------+
 | Authorization + operation admission + composition     |
 |             /                         \               |
 |            v                           v              |
 |       C1 content                    C5 history         |
 |            |                  stage/Commit/Layer/CAS   |
 |       C2 storage                       |              |
 |            |                    separate catalog      |
 |       content Store                                   |
 +------------------------------------------------------+
```

| Component | Owns | Must not acquire |
| --- | --- | --- |
| `layerfs-daemon` | Process assembly, configuration, service connection and authorized Workspace control dispatch | Filesystem algorithms, duplicate live state or a second transport |
| `layerfs-fuse` | Kernel session, callback decoding, one-shot replies, inode/handle ID translation, mount policy and kernel cache interactions | SQLite credentials, SQL, canonical-object traversal, pack layouts, Commit algorithms |
| `layerfs-workspace` | Mutable view, semantic handles, snapshot generations, byte ownership, explicit lifecycle orchestration and status | FUSE/daemon dependencies, a second transport, private C1/C2 algorithms, history CAS/token allocation, automatic mutation replay |
| Bridge | Shared operation DTOs, authenticated caller evidence, framing, bounded delivery, connection/error reporting | Pending Workspace overlays, filesystem algorithms, a replaying write cache |
| Service | Authorized logical operation execution, Store lifetime and local C1/C2/C5 composition | FUSE client handles, pending Workspace bytes, kernel directory cookies |
| C1 / C2 | Canonical content/filesystem operations / physical storage | Mount paths, remote per-object algorithms or Workspace generations |
| C5 | Compact history records, exact stages, conditional history transitions and scope reservations | FUSE, byte segments, open descriptors or live G+1 state |

The service may know the opaque producer incarnation carried by a stage. This is
not ownership of the live Workspace. Local and stream delivery must enter the
same service operation handlers. All canonical-object provider/consumer calls
remain local to C1/C2; one object lookup must never become a daemon-to-service
RPC. [Service entry][service-owner] [Logical reads][service-read]
[History composition][service-history]

The owner selects separate `layerfs-fuse` and `layerfs-workspace` libraries under
`core/crates/`, assembled by `layerfs-daemon` in the same process. FUSE depends on
Workspace's public semantic API; Workspace does not depend on FUSE or daemon.
Its implementation is grouped into `runtime`, `filesystem`, `overlay`, `backing`
and `commit`, using ordinary concrete types/functions. The groups remain private
implementation modules behind the narrow crate exports. No universal provider
registry or additional runtime facade is introduced.

Mount code is Linux-gated. Workspace semantic state contains no fuser, native
socket, SQL or physical-pack types; local OS files are owned by `backing`.
Daemon assembly supplies logical operation delivery and the bounded projection
coherence binding. Kernel invalidation remains in `layerfs-fuse`, including
ordering required by SDK mutations. The [file/API plan](04-implementation-and-verification.md#41-local-production-api-and-dependency-direction)
records the cross-crate surface; creating a library does not add a remote hop.

### 2.1 Public control, SDK edits and cluster integration

The first public execution-side API is `layerfs-workspace`, consumed by the
daemon binary and a real native embedding/executor. Its concrete WorkspaceHost
owns bounded Workspace admission and lifecycle; each Workspace owns semantic
state. This is an ordinary finite runtime collection, not a provider/service
registry. The expected declarations, module boundaries and implementation rounds
are owned by [04](04-implementation-and-verification.md).

```text
 native execution controller             external host SDK / cluster launcher
               |                                      |
               |                         daemon-control binding REQUIRED
               |                         contract/auth/framing not yet provided
               +------------------------+-------------+
                                        |
                         layerfs-workspace public API
                           attach / status / close / admission
                                        |
                                    owns Workspace

 local kernel -> FUSE adapter ------> Workspace <------ SDK edit binding
                callbacks/replies    semantic state    explicit edit receipt
                                     versions/G/D1     local or control caller
                                        |
                              existing bridge Client
                                        |
                            service-local C1 + C2 + C5

 cluster launcher: placement, endpoint/identity inputs, container lifetime
 Workspace: local file semantics and state; no scheduler/cloud control logic
```

The daemon at the reviewed pin only forwards bounded headless stdin requests to
the service. It does not already listen for attach/mount/Workspace-Commit
management. A host SDK cannot call a Rust handle inside another process; its
daemon-control binding must be designed and implemented before Docker lifecycle
or SDK visibility qualification. Reuse existing authentication/framing/transport
capabilities where applicable, with an explicit control operation contract;
do not forward FUSE mount controls to the service/C5 or label them as existing
profile-1/profile-2 operations. Container launch, process execution and cluster
scheduling remain executor/orchestrator work, not Workspace algorithms.

The control contract must identify the exact target Workspace/incarnation,
authorized owner, request identity and deadline, input byte/count bounds,
accepted mutation/capture boundary and complete typed outcome. Disconnect is
not rollback; it must preserve uncertainty without mutation replay. Reopening a
connection cannot silently create a new Workspace or adopt a different stage.
Local API and remote binding invoke the same semantic handlers. No second
content/storage implementation, universal registry or provider lookup is needed.

SDK edit families require a distinct binding to Workspace range-edit semantics.
The existing service `EditFile` builds/saves immutable content; by itself it
does not edit a mounted Workspace. Conversely, writing through FUSE is not a
substitute for a declared SDK edit call. The SDK path must preserve exact
range/member ordering, atomicity and accepted-byte/version results, and perform
the required kernel visibility/invalidation without edit-caused FUSE writes.
Those SDK/control receipts acknowledge the selected live edit; they do not
silently Commit or promise durability. [Benchmark operation mapping](06-benchmark-qualification-map.md)

Extensibility comes from these real boundaries: future macFUSE/Windows adapters
translate kernel behavior into the same semantic operations; a future carrier
binds the same logical operation contract; alternative compatible C1/C2 providers
remain service-local. Public data types must not expose fuser, sockets, SQLite,
pack layout or native backing handles. Use concrete internal algorithms and
small capability interfaces only at these actual boundaries, not a trait or
factory per file or optional algorithm.

### 2.2 SRP/SOLID boundaries and extension rules

| Principle | Concrete rule in Pair 1 | Review consequence |
| --- | --- | --- |
| Single responsibility | Kernel conversion/session code, semantic Workspace state, local backing/index I/O, logical service operations and external execution control have separate owners | A callback does not hash canonical objects, interpret SQL, allocate history tokens or schedule a container; backing cleanup does not decide Commit success |
| Open for extensions at real boundaries | Add a platform projection or transport/control binding around the same semantic methods and operation contracts | A new deployment must not fork write/capture/history algorithms or require edits throughout the kernel-independent state model |
| Substitutable behavior | Compatible direct/stream handlers and later projections preserve bytes, identity, bounds, visibility, typed failures and acknowledgement semantics for the same advertised capability | Run contract-parity cases; unsupported semantics fail explicitly rather than silently degrade or use a fallback algorithm |
| Narrow interfaces | Keep lifecycle/status, semantic filesystem/edit operations, projection notifications and bounded service byte delivery separate | Do not make every integration implement a giant provider interface or expose its file handles/transport just to use one operation |
| Depend on component contracts | Workspace calls the public logical service surface; the service composes public C1/C2 capabilities locally; control supplies validated identity/endpoint/policy | No private content/storage code import, pack-layout assumption, global service locator or arbitrary callback that can replace history rules |

These principles constrain implementation, not mandate a class/trait per row.
Prefer concrete functions/types internally and the existing bridge client,
Source and result contracts. The public library is justified by real daemon
assembly and embedding use; tests consume that same API without special hooks.
Keep OS-specific mount and backing calls in their owners with explicit supported
capabilities. A future macFUSE/Windows build does not inherit Linux permission,
cache, mmap or syscall guarantees merely by compiling the shared state code.

Cluster integration selects placement, endpoints, identities and lifecycle
requests from outside Pair 1. It may enforce a computer-wide Workspace/resource
budget above the per-daemon cap. It cannot bypass Workspace authorization,
generation checks or backing ownership, and Pair 1 does not gain cloud APIs,
node discovery, consensus, a provider registry or automatic failover/replay.
The [file/LOC plan](04-implementation-and-verification.md#3-proposed-production-layout)
maps these boundaries to focused files and explicit implementation rounds.

## 3. Mount topology and deployment

Choose **one mount per Workspace**. A daemon may own several mounts for one
consumer. Sessions and Workspace locks are separate, while the consumer's
allocation allowance is shared. This costs N kernel sessions for N Workspaces.
It provides independent mount lifecycle and failure handling, not guaranteed
neighbour fairness through the shared service.

```text
 Native Linux                 Docker caller / host service
 -----------------------      ---------------------------------
 host callers                 callers in Linux container
      |                               |
 host kernel mount            container mount namespace
      |                               |
 host daemon Workspace        container daemon Workspace
      |                               |
 local service                configured host-reachable endpoint
                                      |
                                 host service

 Remote Store/service         Remote execution node
 -----------------------      ---------------------------------
 local callers + kernel       callers + kernel on execution node
      |                               |
 local daemon Workspace       daemon Workspace on that node
      |                               |
 remote authenticated         configured service, local or remote
 service beside C1/C2/Store
```

The mount follows the calling kernel. A remote service does not move the mount;
a daemon on another machine cannot directly answer this kernel without a local
projection endpoint. A container's `localhost` is not automatically the host
endpoint. Docker requires the selected Linux mount namespace, `/dev/fuse` and
mount permissions; no privileged configuration is silently presumed.

Later macFUSE and Windows adapters should call the same Workspace operations,
with explicit mappings for their own kernel semantics. No macOS/Windows mount,
cloud/serverless carrier, shared-volume scheme or writable restart capability is
claimed. A Linux VM mount serves its Linux callers; it does not automatically
become a native macOS or Windows path.

### 3.1 `/workspace-id` is an immutable managed root

```text
 managed parent namespace                 Workspace contents
 ------------------------                 ------------------
 /workspace-id  [mount + lifecycle ID] ---> /src /data /bin ...
      X rename / unlink / rmdir                  |
      |                                  selected ordinary
 explicit lifecycle only                  descendant operations
```

Neither rename, unlink nor rmdir may move/delete the Workspace entity. The parent
of the mount lies outside its inner FUSE namespace, so inner callbacks alone
cannot enforce this. Managed-parent permissions, mountpoint ownership and
lifecycle controls must enforce the rule as well. An administrator's ability to
unmount is not a capability of an ordinary Workspace mutation.

Renaming across two Workspace mounts is cross-filesystem; LayerFS does not add a
private copy/delete fallback. A shell utility may perform its own fallback after
`EXDEV` for ordinary descendants; that does not authorize moving the managed
root. Keeping the root immovable does not remove descendant rename or cycle
validation requirements. [Linux rename][linux-rename]

### 3.2 Storage locations and configuration

One configurable local root contains two fixed sibling areas: `workspace/` for
FUSE mounts and `private-backing/` for daemon-owned pending state. Their ownership
and access remain separate. The service Store/catalog paths are configured
independently and do not move when this local root changes.

The current writable target uses explicit daemon-local disk backing for pending
bytes and bounded resident metadata/buffers. This carries the owner's large/tiny
file and `npm install` workload with a low-memory target into the main design.
The earlier RAM-only candidate is retained only in the comparison in §13; it is
not a second selected runtime mode or an implemented fallback.

```text
 EXECUTION MACHINE / CONTAINER                  SERVICE MACHINE

 <root>/workspace/<workspace-id>                LAYERFS_STORE
 FUSE mount path                               configured content Store
             |                                          ^
       kernel FUSE view                                  |
             |                                   C1/C2 file/tree save
 daemon Workspace:                                      |
   bounded RAM state/buffers ----- logical requests -----+
   G + G+1 roots, handles, status                        |
             |                                          v
 <root>/private-backing/<workspace-id>          LAYERFS_HISTORY_CATALOG
   immutable payload extents                   separate C5 catalog:
   indexed overlay metadata with               compact stages, Commits,
   bounded resident pages/indexes              Layers and Branch records

 capture shares backing roots/ranges; it does not copy a directory image
```

| Item | Location and configuration | Status / boundary |
| --- | --- | --- |
| FUSE mount | A path in the execution machine's mount namespace; inside the container for container callers | Derived as `<LAYERFS_WORKSPACE_ROOT>/workspace/<validated-workspace-id>`; no second mount-path setting or per-Workspace path override in the initial interface |
| Live state, handles and snapshot descriptors | Bounded daemon RAM on the execution machine | Proposed 8 MiB aggregate accounted allocation target includes resident indexes, views, buffers and reservations; no claim of total RSS |
| Pending payload and backed overlay metadata | Derived `<LAYERFS_WORKSPACE_ROOT>/private-backing/<validated-workspace-id>`, outside the FUSE mount | Current proposed disk-backed target; explicit disk quota/path/ownership and bounded indexes/FDs are required. Frozen roots/extents share this backing; it is not a copied Workspace image or a restart archive |
| Canonical file/filesystem objects | Service-side C2 Store named by existing `LAYERFS_STORE` | Current native service opens that configured Store. FUSE receives logical identities, never its physical path or internal pack layout |
| Compact stage/Commit/Layer/Branch metadata | Separate service-side C5 file named by existing `LAYERFS_HISTORY_CATALOG` | Not the Workspace byte backing and not a second file containing its mutable tree |
| Service endpoint | Existing daemon `LAYERFS_ENDPOINT`, interpreted from the daemon's execution environment | No presumed default, loopback-to-host translation or mount-path implication |
| Mount owner, Workspace incarnation, Branch/root selection | Explicit validated proposed attach configuration | No PID/path-derived authority, guessed root serial or automatic Branch creation |

At the source pin, native daemon startup configures the endpoint, authenticated
connection and headless route; it does not parse the proposed mount fields.
Mount support must add its explicit configuration/validation in its own round.
A configured mount path belongs to the caller's namespace, even when C1/C2 and
their Store are remote. Validate parent ownership, conflicting/overlapping mount
paths and the immutable managed-root rule; never clear an existing path as an
implicit setup action. [Current daemon assembly][daemon-run]

Writable attach derives its backing location from the same validated local root
and receives the selected backing policy. The root setting is proposed; quota
values remain design inputs, not existing runtime defaults.
Keep backing outside all projected mount paths and separate from another
Workspace's backing and service Store/catalog. Creating/claiming it must verify
ownership and a fresh incarnation; a stale directory is not silently adopted or
deleted. Backing paths and segment locators never cross the bridge. The physical
backing module owns OS files; semantic Workspace records use scoped identities
and immutable ranges, not SQL, pack locators or another storage-provider API.

The existing service has no implicit Store-path default. History is optional
when no catalog is configured. Once `LAYERFS_HISTORY_CATALOG` is supplied, native
history configuration requires the authority binding and cursor capability,
explicit create/read-only mode, and an incarnation when creating:

| Existing service setting | Meaning |
| --- | --- |
| `LAYERFS_STORE` | Content Store path opened by the native service |
| `LAYERFS_HISTORY_CATALOG` | Separate C5 catalog path |
| `LAYERFS_HISTORY_BINDING` | Stable binding between that catalog and its content authority |
| `LAYERFS_HISTORY_CURSOR_KEY` | Authority-supplied cursor authentication capability; keep it secret and out of Workspace state/replies |
| `LAYERFS_HISTORY_CREATE` | `1` creates a fresh catalog in this owning process; `0` opens an existing catalog read-only |
| `LAYERFS_HISTORY_INCARNATION` | Required explicit catalog incarnation for creation; not the Workspace producer incarnation |

These names identify configuration, not values to copy into a document or a
Workspace request. The daemon still uses its existing connection authentication;
catalog paths/binding configuration and cursor secrets remain service/authority
concerns. Creating is not reopening or repairing an existing catalog. A service
restart does not regain writable continuity through the same path: mutations and
new reservations on a read-only reopen return `ContinuityUnavailable`.
[Service Store assembly][service-startup] [History configuration][service-config]
[Catalog continuity][catalog-open]

Writing an object to the Store or a stage row to the catalog is not a crash
durability guarantee. MEMORY journal, synchronous OFF and the no-sync/no-WAL
posture remain. Loss of the daemon can lose its live context even if private
backing files and a compact stage record remain. Their presence does not prove
a resumable Workspace, known Commit outcome or writable service continuity.

Unchanged content stays referenced in the service Store. Accepted replacement
bytes go to owned local extents through bounded buffers before their pieces
become visible. Large writes must not expand the daemon heap with their payload;
many tiny files also require bounded resident metadata, indexes and descriptors.
The [overlay ownership and resource rules](02-overlay-snapshot.md) define capture,
allocation/retention accounting and streaming. Numerical disk/segment/index
settings and their qualification remain open; this packet does not silently
choose the old spool constants or increase the memory target.

#### 3.2.1 One configurable root: Linux example

Configure one absolute parent directory. The child names `workspace` and
`private-backing` are fixed; each uses the same validated Workspace child ID.
For example, selecting `/data/layerfs` produces the following layout. Paths and
backing filenames are illustrative, not an implemented configuration or a frozen
backing format. `ws-001` is a display label, not an authority/incarnation value.

```text
 /data/layerfs/                         configured common root
 |-- workspace/
 |   |-- ws-001/                        FUSE mount for Workspace 1
 |   |   |-- package.json
 |   |   |-- src/
 |   |   `-- node_modules/
 |   `-- ws-002/                        FUSE mount for Workspace 2
 `-- private-backing/
     |-- ws-001/                        private backing for Workspace 1
     |   |-- payload/
     |   |   `-- segment-000001.bin
     |   `-- metadata/
     |       `-- ... versioned records and bounded indexes
     `-- ws-002/                        isolated backing for Workspace 2
```

| Derived location or policy | Meaning |
| --- | --- |
| `<root>/workspace/<workspace-id>` | Application-visible FUSE view; mount only this Workspace's child directory |
| `<root>/private-backing/<workspace-id>` | Daemon-owned pending payload and metadata; outside all FUSE mounts |
| Disk quota | Separate explicit resource setting; changing the common root does not choose or enlarge capacity |
| RAM working allowance | Separate proposed 8 MiB aggregate consumer allocation target |

Mounting FUSE on the common root would cover its private backing and can create
recursive access; it is prohibited. The sibling structure does not itself make
backing inaccessible: apply the selected daemon ownership/permissions to
`private-backing` independently of application access to `workspace`. Validate
resolved path containment and child IDs, including symlink/path-traversal and
cross-Workspace collisions. Cleanup targets only the exact owned Workspace
backing after its references are released, never the common parent wholesale.

Canonical saved content and history remain at the service Store/catalog paths
from §3.2. Configure the parent before startup/attach; it does not permit moving
an active mount, renaming its managed root, relocating active backing or silently
adopting a stale incarnation.

#### 3.2.2 Docker example and snapshot sharing

The proposed Docker default is `/layerfs`. Supply one disk-backed Docker volume
or bind mount at that common root to contain both derived areas. With a named
host directory the mapping is:

```text
 host storage                         execution container

 /srv/layerfs/container-a/ ---------> /layerfs/
                                     |-- workspace/
                                     |   |-- ws-001/     FUSE mount
                                     |   `-- ws-002/     FUSE mount
                                     `-- private-backing/
                                         |-- ws-001/     payload + metadata
                                         `-- ws-002/     payload + metadata
```

The daemon resolves `/layerfs` inside the execution container; neither its path
nor the host source path crosses the bridge as a Store identity. The common
volume/bind mount supplies native backing storage; the daemon mounts FUSE only
at `/layerfs/workspace/<workspace-id>`. This does not promise that the nested
FUSE mount propagates to the host. Use the selected disk-backed volume, not
tmpfs, for the low-memory target. Disk/kernel residency and no-durability-sync
requirements remain unchanged.

Frozen G and live D1/G+1 share immutable ranges in the same private backing:

```text
 /layerfs/private-backing/ws-001/
   payload/segment-000001.bin <--- G and D1 may both retain older ranges
   payload/segment-000002.bin <--- D1's newer replacement ranges
   metadata/...              <--- G/D1 retain their respective immutable roots

 capture G = retain roots and extent ownership
           = no snapshots/G/full-copy-of-workspace directory
```

Sharing is by exact immutable range/version, not by assigning an entire segment
to one generation. Keep backing until every relevant live, frozen, reader and
open-unlinked owner releases it. There is no per-Commit directory containing a
complete file tree. Leftover private files do not establish restart recovery or
a durable Commit.

#### 3.2.3 Proposed Pair 1 startup variables and attach inputs

Keep the initial process configuration small. The four names below are proposed
Pair 1 environment variables to implement; they are not parsed by the current
daemon, frozen wire fields or already qualified settings. Path defaults are
recommended for the Docker execution profile. Native deployments can configure
other validated absolute paths.
Use an absolute common root so placement does not depend on the daemon working
directory. The two child paths are derived; no independent backing-root variable
or alternate child-name setting is selected.

| New proposed variable | Recommended initial value / selection | Scope and validation |
| --- | --- | --- |
| `LAYERFS_WORKSPACE_ROOT` | Docker profile: `/layerfs` | Common local parent; derive `workspace/<validated-workspace-id>` and `private-backing/<validated-workspace-id>`. Absolute, owned path; validate isolation from Store/catalog, traversal and overlapping mounts. W requires disk-backed storage and a fresh owned incarnation |
| `LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES` | Proposed default `8388608` (8 MiB) | Aggregate accounted Workspace working allocation per consumer, across its Workspaces. Positive byte count; must fit the selected minimum progress reservations and qualified profile. No per-Workspace multiplication, automatic increase or total RSS claim |
| `LAYERFS_WORKSPACE_DISK_BUDGET_BYTES` | Explicit positive byte count required for W; no implicit unlimited/default quota | Recommended aggregate private backing allowance per consumer, covering payload/index storage, reserved/dead space and pinned old generations. Per-Workspace subdivision remains a separate design choice; a missing/invalid quota refuses writable attach |
| `LAYERFS_WORKSPACE_MAX_COUNT` | Explicit positive integer required when enabling the Workspace runtime; no selected numeric default | Per-daemon count of owned Workspace entries, including attach reservations and retained/closing/failed-cleanup state. Applies to R and W; existing headless-only operation does not require it. Zero is not unlimited |

The aggregate disk scope above is the recommended configuration contract; its
numeric default is deliberately not selected. Disk allocation/reservation
granularity and per-Workspace subdivision still need the backing design from
02/04. A memory value different from the 8 MiB candidate is an explicit profile
choice requiring validation/qualification, not permission to enlarge a run's
budget or silently raise a too-small value. Zero must not mean unlimited.
Read-only projection does not require a writable spool or disk quota, and it
must not create backing merely because its root setting exists.

Read these settings at startup and bind their effective policy to the consumer
and attach operation. Changing an environment value does not move an active
mount, resize a live quota or adopt a different backing directory. Hot relocation,
hot quota adjustment and per-Workspace startup-variable overrides are not part
of this initial interface. Do not expose public segment-size, cache-size, worker,
writeback, auto-Commit or compaction toggles before their policies are selected;
those remain bounded implementation/profile decisions.

Reserve a Workspace-count slot atomically before creating/claiming backing or
mount resources. Count attaching, mounted, unmounted-but-retained, closing and
failed-cleanup entries until exact retirement releases their ownership. An
unmount does not release the slot while dirty/snapshot/reader state remains.
The selected count must fit minimum per-entry RAM, mount/session, descriptor and
backing headroom; a higher count does not multiply either consumer budget.
Admission requires count and resource reservations together, not merely a free
integer slot. R exposes the same mount/session cost and therefore also needs
the finite count policy.

With one daemon per sandbox this is also the sandbox Workspace-count cap.
Several daemons/containers on a computer have independent caps; any machine-wide
aggregate belongs to their launcher/orchestrator. Do not add a host-global
Workspace/provider registry to the filesystem or confuse the service's four
connections/two active operations with Workspace counts. v0.1.6's different
descriptor-derived cap combined active executions and mounts; the source
comparison in [05](05-v016-source-comparison.md) records its exact scope.

Use the existing Pair 3 native connection settings, rather than introducing
FUSE-prefixed duplicates:

| Existing variable | Existing meaning |
| --- | --- |
| `LAYERFS_ENDPOINT` | Service address reachable from the daemon's environment |
| `LAYERFS_SELECTOR` | Peer selector used by native authentication; not the logical Store ID |
| `LAYERFS_PRIVATE_KEY` | Daemon's native connection private key; keep secret |
| `LAYERFS_SERVER_KEY` | Expected service public key |

[Existing daemon connection assembly][daemon-run] supplies these four. Existing
telemetry configuration is also reused. `LAYERFS_STORE` and
`LAYERFS_HISTORY_CATALOG` remain service-side storage settings, not Pair 1 paths.
A container's loopback address is not automatically the host service endpoint.

Several Workspaces share one daemon, so use the proposed `attach(config)` input
for per-Workspace state instead of global environment variables:

| Attach input responsibility | Meaning |
| --- | --- |
| Workspace display/mount ID | Validated managed child name and local identity; cannot move through ordinary rename |
| Producer incarnation | Distinct authority-supplied identity for exact stage/generation ownership |
| Logical Store ID | Authorized Store selection carried through the existing shared contract |
| Source selection | An explicit immutable root for R, or a Branch-backed descriptor for W; validate required scope/root serial through the service |
| Access mode | Read-only or a specifically implemented writable capability set; setting W cannot enable missing callbacks/shared operations |
| Owner/permission context | Validated caller identity and projected UID/GID/access policy required by the selected mount profile; independent of content hashes |

Generations, stage tokens and file handles are runtime/result state, not operator
configuration. Commit deadlines remain explicit lifecycle-method inputs; the
ordinary callback deadline stays a separately selected bounded profile policy.
This section adds no new transport or public opcode.

Section 3.2.2 shows the resulting Docker layout and single host-to-container
mount. Both derived children move together when configuring a different parent
for a new daemon/attach lifecycle; active relocation is not supported.

A local Docker volume is an alternative when a named host directory is not
needed. Supply the common root through the selected disk-backed mount rather than
implicitly placing it in the container's writable image layer. Docker documents
the extra storage-driver path for writable-layer I/O; this placement recommendation
is not a LayerFS throughput measurement. Named-volume bytes may outlive a
container, which does not establish LayerFS restart recovery or Commit durability.
[Docker volumes](https://docs.docker.com/engine/storage/volumes/),
[Docker bind mounts](https://docs.docker.com/engine/storage/bind-mounts/).

### 3.3 Proposed local lifecycle API

These are recommended caller-level operations, **not implemented Rust methods,
frozen signatures or new wire opcodes**. Host assembly binds the configured
service endpoint through the logical OperationDelivery capability. Per-Workspace
attach selects the logical Store, managed Workspace/mount identity, producer
incarnation, owner/permission context, the derived backing policy, and either
an explicit R root or a Branch-backed context for history mutation. The authority
supplies identity and grants; the runtime validates them through the existing
service surface.

The operation names below describe the user-facing responsibilities. The local
WorkspaceHost/Workspace and FUSE-binding ownership/signature plan is in 04;
they are not a set of already implemented daemon RPCs. Mount/unmount belongs
to the kernel adapter's lifecycle, and the host coordinator composes it with
the Workspace handle. The remote control binding from §2.1 is a prerequisite
to exposing the same operations to an external SDK/container coordinator.

| Proposed operation | Responsibility / result |
| --- | --- |
| `attach(config)` | Create the local runtime handle and obtain/validate its base descriptor. Does not implicitly initialize a stack, fork a Branch or mount a path |
| `mount(workspace)` | Attach the selected kernel presentation at its validated configured path; does not copy the namespace to disk or create a C5 stage |
| `status(workspace)` | Return bounded local mount/dirty/generation/pending-stage/failure state and the last acknowledged context. Does not silently query, rebase or recover the remote Branch |
| `commit(workspace, deadline)` | Reserve the logical submission slot, capture internally, save prerequisites and use composite Commit. Return the exact G-associated result/observations |
| `stage(workspace, deadline)` | Optional explicit staged workflow. Capture/save and return an opaque local stage selector tied to the exact returned StageWire and G |
| `commit_staged(workspace, stage_selector, deadline)` | Validate the retained incarnation/token/generation/context, then submit the existing exact-stage operation. Does not recapture newer live state |
| `discard_stage(workspace, stage_selector, deadline)` | Remove only the selected exact remote stage through the existing operation. Does not by itself revert G/G+1, delete content or settle their local disposition |
| `unmount(workspace)` | Stop presentation admission and drain/detach under the declared policy. Retain the Workspace's dirty/frozen/status state in daemon ownership; no implicit Commit or DiscardStage |
| `close_clean(workspace)` | End the local runtime only after presentation and outstanding uses are drained and local state is clean/settled. Dirty, staged, retained-failed or uncertain state returns an explicit refusal |

`stage_selector` is a conceptual validated wrapper, not a raw token accepted
without context. It binds the Workspace producer incarnation, frozen generation,
exact token and captured context retained by that runtime. A different stage
observed after a stale-token failure cannot be adopted through this wrapper.
Public callers do not get raw `freeze`, mutable overlay access or
`reconcile(new_root)` methods that bypass these checks.

FUSE uses the internal semantic methods for lookup, attrs, open/read/write,
directory operations and reference release. It does not call the public Commit
operation on each write/flush/release. Fork, AddLayer and history queries remain
explicit uses of the existing client operations; the lifecycle API does not
invent another Branch manager or hide publication inside `commit`.

Local synchronization has two distinct purposes. A short per-Workspace state
mutex protects capture/version publication and slot transitions. A persistent
per-Workspace **logical submission slot** serializes Commit/stage attempts;
the slot remains occupied in Staged, retained-failure or Uncertain states after
the function and mutex guards return. G+1 writes use ordinary state publication
and memory admission, not a Commit mutex held across network/storage work.
[Local Commit serialization](02-overlay-snapshot.md#33-local-commit-serialization)
owns the full state and permit rules.

```text
 same Workspace, illustrative operations (not executable API syntax)

 commit(w) --------> reserve slot, capture G ----> save G --------> known result
                         |                         |                  |
 write(w, ...) ----------+------ accepted into G+1 |                  |
 commit(w) while pending -----------------------> Busy; no new capture
                                                                    |
 next explicit commit(w), after settlement --------------------------+
       captures remaining live work as the next generation
```

The earlier conservative admission choice remains **one unresolved frozen
submission per consumer**, in addition to each Workspace's correctness slot and
the shared 8 MiB allocation proposal. A staged/uncertain Workspace therefore
retains that consumer reservation and can prevent another Workspace's capture.
This does not hold an active network/client mutex throughout a staged pause.
Active I/O admission and frozen retention are separate; local reads/writes may
continue within their bounds. Service-wide admission 2/Q0 and C2's two private
save slots do not widen this local policy.

Unmount is a presentation operation, not destruction of dirty state. Its retained
state remains charged and can still own the submission reservation. Clean close
is deliberately conservative: a missing stage observation alone does not prove
an uncertain operation settled. Any future destructive abandonment/revert action
must explicitly account for G, G+1, open readers and remote stage disposition;
it is not an automatic destructor, a side effect of unmount, or defined here by
guessing. See [03](03-commit-integration.md) for exact history outcomes.

## 4. Identity and authorization

| Identity | Meaning and ownership |
| --- | --- |
| Display/mount Workspace ID | Managed lifecycle identity naming `/workspace-id`; not a C5 stage token or pathname-derived authorization |
| Workspace producer incarnation | Explicit authority-supplied nonzero 32-byte identity used for C5 stage ownership; distinct from mount name and process PID |
| Branch / LayerStack ID | History identities; changing a Branch association does not rename the mount or silently retarget dirty state |
| Filesystem root | Immutable content identity of a whole namespace; R pins it, W retains its construction context |
| Scope + inode serial | Canonical inode allocation identity; never assume the root serial is 1 |
| Kernel inode number | Mount-local adapter identity; must not alias another mount/incarnation or a recycled exposed inode |
| File/directory handle ID | Bounded opaque local handle; owns semantic lifetime, not a remote SQLite handle |
| Local generation / inode revision | Workspace's capture and per-inode visibility identifiers |
| Exact stage token | Catalog-issued token for one stage; not a request ID or permission grant |
| Catalog incarnation | Catalog/continuation identity; distinct from Workspace incarnation |
| Request ID / outer generation | Transport correlation fields; not durable deduplication or proof of Commit completion |

R can use `(mount incarnation, pinned filesystem root, inode serial)` without
inventing an allocation-scope descriptor absent from plain `Inspect::Stat`.
A Branch-backed Workspace gets the validated scope/profile/root serial from
`GetBranch`. `Fork` returns metadata with no validated root serial; follow it
with GetBranch. Bootstrapping returns the actual consumed root serial.

`Request.profile=1` selects ordinary logical file/read operations. History uses
profile 2 with query opcode 6/grant `0x20` and command opcode 7/grant `0x40`.
The filesystem's canonical 32-byte profile is a different identity. Legacy grant
mask 31 grants neither history operation. Peer authentication, Store/operation
authorization and content authentication remain independent; a hash or stage
token is not authority. Grants at this pin are Store-wide, not per-Branch ACLs.
[History identities and requests][history-contract]

## 5. Existing logical operations consumed by the adapter

| Operation | Existing service-local behavior | What the adapter still supplies |
| --- | --- | --- |
| `Inspect::Stat` | `FilesystemRead::resolve` and `read_portable`; returns serial/kind/reference count/content root/metadata root/mode/mtime | Kernel attr projection; file size is not in this response |
| `Inspect::File` | `FileView::open`; returns logical length/representation | Associate it with the exact content root and retained view |
| `Inspect::List` | `FilesystemRead::list`; bounded names/serials and continuation | Dot entries, cookies and per-entry kind when required; current result has no kind/attrs |
| `Inspect::Readlink` | `FilesystemRead::readlink`; stored target bytes | Symlink size and ordinary kernel traversal behavior |
| `ReadFile` | C1 `read_range` with a local C2 provider; bounded bytes plus terminal length | EOF adaptation, version pins, bounded reply buffering and terminal-success gating |
| `ConstructFile` / `EditFile` | C1 construction/known edits plus C2 save | Stable final input, appropriate operation choice, exact returned-root association |
| `UpdatePreparedFilesystem` | Existing-identity root-only filesystem update | Remains a headless/root-only operation, not arbitrary-root stage registration |
| `Commit` or `StageChanges` / `CommitStaged` | Pair 2 constructs/saves the filesystem tree and performs its stage/history transitions | Explicit Workspace lifecycle, frozen G association and local reconciliation |

C1's `DirectoryUpdate` contains sorted, unique final bindings for **changed
names**. It is not a complete copy of both directories involved in rename.
Unchanged names remain in the base. Writes still need accumulation because
unordered callbacks must become a checked final-state batch. [C1 input][c1-input]

`ReserveInodes` consumes serials but does not add a live-create operation.
Prepared staging at this pin accepts existing serials only. Bootstrap metadata
and symlink constructors do not supply general live chmod/setattr/xattr/symlink
operations. Each missing shared operation must be designed in its own round;
FUSE must not implement it through private object RPCs. [Prepared update][service-fs]

## 6. Lazy lookup, listing and data reads

```text
 lookup(parent, name)                   read(handle, offset, size)
         |                                         |
 pin namespace view                        pin one current inode
         |                                 version + length + spans
 D1 -> G -> immutable base B                         |
         | miss                           split required source spans
         v                                 /         |          \
 Inspect Stat(root, relative path)       local      zero      immutable
         |                              extent     fill       range
 service-local C1 metadata walk                                |
         |                                              ReadFile(root,range)
 File/Readlink inspection if size needed                       |
         |                                           local service C1/C2
 revalidate requesting view before installing                  |
         |                                             terminal success
 bounded entry + projected attrs                              |
                                                   bounded FUSE reply
```

Mount acquires its root descriptor and necessary root metadata. It does not
enumerate descendants or copy every file. A name lookup traverses metadata as
needed; a directory call obtains a bounded page. `open` retains identity/length,
not a materialized host file. Content transfer follows actual kernel reads.

This describes **logical demand**, not exact physical I/O. C1 can read mapping
pages and reconstruct encoded objects/dependencies to authenticate a range. For
a whole-file canonical representation, even length inspection may acquire a root
that includes its payload. A request for 4 KiB therefore does not prove only
4 KiB was read from storage. [C1 range read][c1-read] [File inspection][c1-view]

For each FUSE read, clamp the requested interval to the pinned logical length;
return zero bytes at/beyond EOF. C1 rejects out-of-range endpoints instead of
performing this POSIX adaptation. Retain all selected sources until reply
completion. The bridge writes streamed bytes into its supplied sink before its
terminal result: Workspace must use a bounded private buffer and expose the reply
only after the operation succeeds. [Bridge client][client]

### 6.1 Structural exchange counts, not timing claims

Assume an established authenticated connection. Count logical request/results,
not TCP packets or canonical object reads. Required local metadata may already
be present; kernel caches can suppress callbacks entirely.

| Activity without retained results | Logical exchanges through current APIs |
| --- | --- |
| Complete regular-file lookup/getattr | 2: Stat + File |
| Directory attrs under the projection policy | 1: Stat |
| Complete symlink attrs | 2: Stat + Readlink |
| Names-only page | 1: List |
| Typed readdir over P pages with U uncached entry kinds | P + U; fuser's typed replies cannot invent the missing kinds |
| Future readdirplus | P + U plus missing file-length/symlink-target inspections; richer bounded results would require a shared extension |
| Fitting immutable range with known root/length | 1; 0 for entirely local bytes or EOF |
| Open/release/forget after validated state exists | Local, no necessary service request |
| execve | Actual metadata exchanges plus actual loader/kernel reads; no fixed number |

Do not issue a canonical-object RPC for each internal tree level. Batching or a
richer shared result may reduce exchanges, but neither exists merely because C1
has a public helper. Read/connection/setup/cache phases must remain visible in
any later measurement.

## 7. Complete proposed callback surface

The tables enumerate fuser 0.18's callback vocabulary, plus relevant syscall
activities which have no separate callback. “Required” means required before
claiming the selected capability; no table row claims it already runs in core.
Unexpected flag combinations fail before local mutation. Mutation attempts on R
return `EROFS` after applicable request/identity validation.

### 7.1 Required R operations and local lifecycle

| Callback | Local semantics / shared call | Error, bound and verification obligation |
| --- | --- | --- |
| `init` | Validate root, configuration and implemented capability set; use GetBranch or explicit-root Stat | Wrong root/profile/authority fails mount. No construction hidden in mount. Prove actual root serial mapping |
| `destroy` | Stop admission and drain/join owned work; release references | One-shot cleanup, no implicit Commit or guessed discard. Prove bounded shutdown/failure handling |
| `lookup` | Resolve a name in a pinned view; Stat plus size inspection as needed | No stale reply reinstalls a removed/replaced name; distinguish absent name from unavailable content |
| `getattr` | Current selected inode version and declared projected attributes | Handle-based getattr must work after rename/unlink; no false persisted uid/gid or physical block claim |
| `forget`, `batch_forget` | Release kernel lookup references | Never delete open, frozen or in-flight-read state. Repeated/invalid counts must not underflow or resurrect IDs |
| `access` | Local owner/mode/search checks and kernel `default_permissions`; independent service authorization | Do not return unconditional success. R write access fails; denial is not missing content |
| `open` | Validate access, kind and flags; allocate bounded local handle | Writable/`O_TRUNC` requests fail on R. Refuse unsupported synchronous flags explicitly. Handle admission is bounded |
| `read` | Pin one version, adapt EOF, merge sources; immutable misses use ReadFile | Checked offset arithmetic, bounded reply, no mixed revisions or partial terminal-failed result. Test EOF and short final read |
| `readlink` | Return exact stored target via retained state or Readlink | Wrong kind fails; bounded target; no daemon-side content-object traversal |
| `flush` | Report defined local accepted-visibility/known-error state | No save, logical Commit or durability promise. Multiple flush calls must not drop ownership |
| `release` | Drop one file handle; keep any other semantic/kernel/read references | Closing must not lose dirty bytes or make another alias/descriptor invalid |
| `opendir` | Bind directory handle to a declared immutable view and cursor policy | Validate kind and reserve view/handle state; no unbounded directory snapshot collection |
| `readdir` | Merge bounded ordered entries with tombstones; List plus required kind inspections | Dot entries, byte/count bounds and resumable cookies; avoid duplicate/skipped/resurrected entries across pages |
| `releasedir` | Release cursor/view ownership | Old generations remain charged until the final reference drops |
| `statfs` | Synthetic projection information, not Store physical allocation | Declare zero/unknown backing capacity convention and `name_max=255`; read-only is a mount policy. Prove tools do not receive fabricated free-space data |

### 7.2 Selected W operations and missing shared capabilities

| Callback | Required local behavior | Save mapping / gap and test obligation |
| --- | --- | --- |
| `write` | Reserve, then atomically install accepted bytes, length and timestamp; aliases see the change | Final supported EditFile or legitimate complete-file construction, then history save. General portable-metadata construction is missing. Test append races, overlapping edits and failure before visibility |
| `setattr` size | Truncate removes old tail; extension is logically zero; update metadata coherently | Piece lowering plus metadata operation. Test shrink/extend/shrink and open handles across G completion |
| `setattr` mode/mtime | Validate every requested field before applying any; no partial unsupported update | C1 attribute functionality exists, general shared live constructor/update does not. Test invalid bits/nanoseconds and unchanged content |
| `create` | Atomic name absence check, allocation, metadata/content initialization and open | ReserveInodes alone is insufficient; live new-identity attachment missing. Test `O_EXCL`, umask, failure rollback and allocated-serial nonreuse |
| `mknod` regular file | Same empty regular-file namespace semantics without open | Same new-identity/metadata gap; special types remain unsupported |
| `mkdir` | Validate parent/name, allocate identity, create canonical empty directory and metadata | Live new-directory attachment missing. Test empty-directory representation and no partial parent binding |
| `symlink` | Validate target/name; create one new symlink identity and metadata | Live symlink constructor/attachment missing despite bootstrap support. Test exact target, empty/maximum target according to C1 grammar and invalid bytes |
| `unlink` | Remove name; retain zero-link inode for handles/readers; update parent metadata | Existing binding removal plus missing general metadata path. Test writes through still-open unlinked handle and no resurrection on completion |
| `rmdir` | Validate directory and emptiness, then remove one name | C1 subtree release is not POSIX emptiness checking. Test `ENOTEMPTY`, wrong kind and no recursive deletion |
| `link` | Regular-file names share one inode and observe all later writes | Existing-identity binding; derived canonical reference counts. Directory/symlink hard links unsupported; test aliases and unlink of one alias |
| `rename` | Atomic source removal/destination final binding; type, emptiness, flags, cycle and alias checks | One changed-name batch through history staging. Pre-visibility bounded validation and typed work-limit result are still gaps. Test same-inode aliases, replacement, failed destination preservation and cross-directory capture |

`setattr` of unsupported uid/gid, atime/ctime/birthtime or platform flags must fail
explicitly rather than report a persisted change. Ordinary rename and
`NOREPLACE` may be selected once their preconditions can be enforced; exchange
and whiteout remain unsupported initially. Root moves/removals remain prohibited.

### 7.3 Optional, kernel-owned or unsupported operations

| Callback / activity | Initial decision | Required qualification / distinction |
| --- | --- | --- |
| `readdirplus` | Do not advertise initially; explicitly unsupported if dispatched | Missing complete bounded entry result and pin accounting. This is an optimization/capability deferral, not ordinary readdir failure |
| `fsync`, `fsyncdir` | Explicit unsupported result in the first profile | No invented durability. A later volatile-success contract must be separately defined and tested |
| `getxattr`, `listxattr` | Deferred | Public C1 `read_attribute`/`attribute_keys` do not supply an OS namespace mapping or shared inspection operation; do not falsely return an empty attribute set |
| `setxattr`, `removexattr` | Deferred; R mutations fail | Missing shared construction/update and create/replace/size-probe semantics. Do not infer ACL/capability semantics from opaque attributes |
| `getlk`, `setlk` | Do not add remote lock forwarding/manager initially | Kernel-local advisory locking may still operate without these methods. Qualify same-mount locking/close/ownership behavior; do not claim cross-mount, cross-daemon or distributed locks |
| Special `mknod` types | Unsupported | C1 inode kinds are regular file, directory and symlink; no FIFO/socket/device storage contract |
| `bmap` | Unsupported | No physical-block map is exposed |
| `ioctl` | Unsupported | No arbitrary device/host control forwarding |
| `poll` | Unsupported | No asynchronous poll contract supplied by the logical service |
| `fallocate` | Unsupported | No allocation/reservation/hole-mode guarantee; writing zeros is not every fallocate mode |
| `lseek` for `SEEK_DATA`/`SEEK_HOLE` | Unsupported optional query | Zero bytes do not prove a hole. Ordinary `SEEK_SET/CUR/END` offsets remain kernel/file-description behavior |
| `copy_file_range` | Unsupported optional acceleration | Do not invent private copy RPCs or claim atomic/overlap semantics from read+write helpers |
| `setvolname`, `exchange`, `getxtimes` | Outside the initial Linux adapter | No implicit Apple metadata semantics or future-platform claim |
| `execve`, read-only/executable mappings | Candidate R behavior requiring mounted proof | Not separate FUSE callbacks; require loader reads, execute permission, compatible binary/interpreter/libraries and supported kernel mapping mode |
| Shared writable `mmap` | Outside initial W unless coherently implemented | Must be enforceably refused or supported with correct visibility/capture/resource semantics before W; documentation alone is not refusal |
| `O_SYNC` / `O_DSYNC` | Explicitly refuse unsupported requested guarantee | Do not accept at open and rely on a later unsupported fsync. Audit other synchronous I/O flags delivered by the selected interface |
| `FUSE_INTERRUPT` | No immediate-cancellation claim at the inspected dependency | fuser 0.18's inspected path replies ENOSYS; bounded drain is not immediate interrupt support |
| Explicit Workspace Commit / AddLayer | Lifecycle controls, not POSIX callbacks | Existing Pair 2 operations; preserve their distinct completion boundaries; see [03](03-commit-integration.md) |

The fuser locking-method documentation explicitly permits kernel-local locking
when forwarding methods are absent. This is a useful native facility, not a
reason to create a distributed lock service. Do not assume that two Workspace
mounts backed by the same Branch share lock state. Test the selected POSIX and
Linux lock types rather than treating all advisory locks as interchangeable.
[fuser 0.18 Filesystem API][fuser-api] [Linux record locks][linux-locks]

## 8. Projected metadata, names and handles

### 8.1 Attribute projection

| FUSE field | Proposed value / source |
| --- | --- |
| inode number | Mount-local mapping of retained canonical/logical inode identity; no exposed-ID reuse |
| kind | C1 regular file/directory/symlink; reject unsupported kinds |
| regular-file size | Exact logical length for the selected content version, from File inspection or acknowledged local state |
| symlink size | Target byte length |
| directory size | Explicit synthetic 0; not physical storage accounting |
| mode | C1 portable mode; regular files `0o777` permission mask, directory sticky bit allowed, symlink mode `0o777` |
| mtime | C1 portable seconds/nanoseconds or coherent selected W update |
| uid/gid | Configured mount owner, not persisted C1 owner identities |
| atime/ctime | Explicit synthetic copies of mtime; no promise of independent persisted POSIX timestamps |
| birthtime/platform flags | Not persisted; use a declared unavailable/zero projection where the interface requires fields; reject unsupported setters |
| regular nlink | Namespace reference count, including current local binding effects |
| symlink nlink / directory nlink | 1 / explicit synthetic 2; no implied stored subdirectory-count semantics |
| blocks | Logical size rounded to 512-byte units, not allocated pack/disk bytes |
| rdev / preferred I/O block size | 0 / declared projection value, initially 4096 |

Mode/mtime are the only typed portable metadata values at the source pin. No
claim that chmod/chown/utimens variants are interchangeable follows from this
table. Validate timestamp arithmetic, including pre-epoch times; do not silently
clamp a valid stored timestamp merely to simplify conversion. [Portable values][c1-portable]

Use `nosuid`, `nodev`, owner-only access and `default_permissions`; never turn on
`allow_other` implicitly. A mode bit does not override service grants. Effective
creation mode must respect the kernel/API's umask handling exactly once.

C1 paths are relative with an empty root path. Its current grammar requires
UTF-8, bounds names to 255 bytes, paths to 4096 bytes and depth to 256 components,
and rejects empty components, `.`, `..`, NUL, slash within a name and backslash.
This is narrower than arbitrary Linux filename bytes. Reject incompatible names
explicitly; do not normalize, lossy-decode or silently rename them. Kernel dot
entry/traversal behavior must not be encoded as stored C1 child names.
[C1 path grammar][c1-path]

### 8.2 Handle and per-read lifetime

```text
 name A ---+
           +--> live inode identity I --> current immutable version Vn
 name B ---+            ^                         |
                        |                    read pins Vn
                 open handle H                   |
                        |                    release pin at reply
             unlink A removes one name
             unlink B leaves H alive

 frozen G -----------------------------> captured older version Vg
```

An open writable descriptor observes later acknowledged writes to the same
inode, including writes through another hard-link alias. The open handle is not
a permanently frozen content view. Each read pins one version/length/span set
for its own duration. Snapshot G independently pins capture-time versions.

Deletion or rename changes names, not existing descriptor identity. Open-unlinked
content remains reachable through its descriptors, including later writes; it
must not regain a namespace name when G finishes. Closing one descriptor must
not free data still held by another descriptor, lookup reference, frozen view or
in-flight read. Ordinary file-offset sharing and close-on-exec behavior belong to
kernel open-file descriptions; do not duplicate a process FD table in Workspace.
[Linux open-file descriptions][linux-open]

### 8.3 Directory cursors

```text
 directory handle Dh -> view identity V
                        |
            base page + frozen changes + live changes at V
                        |
             ordered merge / shadow / tombstone filtering
                        |
          delivered names ... last name K ... continuation
                        |
               cookie owned by (Dh, V, K)
```

R pins an immutable root/path. W must define a stable view or an explicit
mutation-aware cursor policy before exposing its readdir behavior. The preferred
bounded design pins the view, with cookies identifying positions in that view;
it does not copy the entire directory into a handle. Kernel offsets are opaque
cookies, not live-vector indices or untrusted raw path pointers. A byte-bounded
page shorter than its requested count is not necessarily end-of-directory.

Long-lived cursors can retain older object versions after G completes. Charge
them to the same consumer allowance and refuse new admission when necessary;
never evict state required to interpret a still-open handle.

## 9. Mutation visibility, flags and the rename ceiling

Ordinary callbacks first validate identity, kind, flags, offsets and required
metadata. Acquire required resources before changing visible state. Install all
effects of the selected callback coherently, then acknowledge its accepted byte
count or namespace result. Do not hold Workspace state locks while awaiting a
service request; any prerequisite response must be revalidated before mutation.

`O_TRUNC` takes effect at successful open, not at the first later write. Append
chooses current live EOF and installs the write as one local operation. Exclusive
creation atomically rejects an existing name. Zero-byte writes do not fabricate
content. Checked arithmetic and truncation rules prevent an old tail from
reappearing after extension. These rules must hold while G is saving as well as
when the Workspace is otherwise idle. [Linux open flags][linux-open]

Directory rename supplies removal and final destination binding in the same
checked batch. Failed replacement leaves the previous destination valid. Reject
cycles, invalid aliases, wrong-kind replacements and nonempty-directory
replacement according to the selected semantics; do not treat C1's eventual
refusal as permission to acknowledge an impossible local `mv` first.

The current **4096 examined-binding limit** is per validation walk, distinct from
the bridge's 128 changed-name limit. Parent-alias validation can traverse the
whole base namespace for directory/symlink rebinding; even a small moved subtree
in a large namespace can fail. An oversized moved subtree also fails. Current C1
reports an `InvalidRecord` diagnostic and the ordinary service mapping collapses
it to `InvalidInput`; no typed capacity-versus-cycle result is available for that
path. Do not parse diagnostic strings or increase the bound. Before enabling
affected W renames, add an agreed bounded validation/result contract, or keep
those cases explicitly unsupported. [C1 validation][c1-validation]

## 10. Kernel caching, synchronization and cancellation

The first R proposal uses a declared cached-I/O configuration suitable for
executable reads, with writeback-cache disabled. Kernel page caching and readahead
still exist; no userspace prefetch/cache addition is proposed. Cached mode also
permits mappings, so excluding shared writable mmap requires an enforceable
adapter/kernel choice or a coherent implementation before W. Direct-I/O modes
have different mapping/cache behavior and cannot be substituted silently.
[Linux FUSE I/O modes][linux-fuse-io]

Capture covers coherent mutations already accepted by the daemon. Dirty kernel
pages outside those acceptance points are not magically in G. Mutation paths
outside ordinary FUSE writes require appropriate cache invalidation/coherence;
do not return a successful stale page after changing the live view. Negotiating
writeback, stateless-open, prefill or mapping features requires their own proof.

The first proposed `fsync`/`fsyncdir` result is unsupported. This is weaker
application behavior than exact v0.1.6's volatile-success path, which checks local
errors/resources and retires idle backing without an OS durability flush or host
acknowledgement. A later compatible volatile contract can be selected, but must
not claim crash durability. Reject unsupported `O_SYNC`/`O_DSYNC` at open instead
of silently accepting the guarantee. [Reference volatile fsync][v016-fsync]

For bring-up, the scheduling proposal is two fixed fuser event loops per mount,
one admitted remote callback per consumer and no waiting remote queue. Competing
misses are refused; a second loop must not sit behind a client mutex while local
release/error work is starved. This is a proposal requiring actual mounted
progress tests, not an implemented fairness guarantee or another construction
worker. Keep owned replies single-use and make drop/error paths explicit.

Several applications/threads may operate on the same Workspace, and ordinary
supported operations continue while G saves. Only snapshot submissions are
serialized; the short state mutex is not held for construction, network or
history work. Inclusion in G is determined by coherent publication before the
capture cut, not by syscall start time. A running read can finish on its pinned
view and a later write publishes into D1. Short contention, local capacity and
necessary remote work still constrain progress. [Concurrent-operation semantics](02-overlay-snapshot.md#34-multiple-operations-continue-during-commit)
and the real-save overlap proof in 04 govern the full writable target.

The inspected fuser 0.18 interrupt path returns ENOSYS, and the selected bridge
client does not expose an independent caller cancellation handle. Stop admission
and drain bounded in-flight work before joining. Do not claim immediate interrupt
cancellation, interrupt another owner's task, add an unbounded queue, or patch a
third-party dependency to close the gap. A lost client connection does not prove
a submitted mutation rolled back. [fuser source][fuser-source] [Client][client]

## 11. Resource and failure contract

### 11.1 Existing shared bounds versus proposed Workspace bounds

| Scope | Value at the source pin / proposal |
| --- | --- |
| Logical file/input/response maximum | **4 GiB**; EditFile base/final lengths and File result validation obey it |
| Edit replacements / edit count | **8 MiB / 256** |
| Prepared update | **128 total changed names, 128 inode updates, 128 directory records**; existing identities only |
| Native body frame / request metadata | **16 KiB / 32 KiB**; encoded metadata size can bind before record count |
| Frame allowance | Finite `ceil(bytes / 1024) + 257`; not the old fixed frame count |
| List page | At most **128 entries / 16 KiB**, with continuation |
| History result / continuation / failure | **16 KiB / 160 bytes / 482 bytes** for profile 2; profile-1 failure remains three bytes |
| Service admission | **2 complete operations service-wide, Q=0**; not two per Workspace/Store |
| C2 private saves | **2 per Store**, independently arbitrated; this does not authorize a second construction producer |
| Sessions | **4 admitted plus one accept/refusal slot** |
| Shared operation maximum / I/O progress | **600,000 ms / 5,000 ms**; protocol ceilings, not benchmark targets |
| Proposed callback deadline | At most **10 seconds total**, including dependent queries via the remaining caller budget |
| Proposed Workspace allowance | **8 MiB total allocated capacity per consumer across all Workspaces**; not measured or owner-approved resource acceptance |
| Proposed FUSE read reply reservation | At most **128 KiB per admitted callback**, inside that total; enforce the selected request/reply configuration |
| Proposed frozen work | One unresolved frozen submission **per consumer**; each Workspace also has its own logical submission slot. The owning Workspace retains G and live G+1; reader/cursor references stay charged |
| Additional userspace content cache/prefetch | **0 bytes initially**; required metadata/handle state remains bounded and charged |
| Local disk backing | Explicit primary pending-byte backing for W; quota, segment limits, metadata paging and FD/index limits require design/qualification. No implicit spill or tmpfs substitution |

[Request limits][request] [Service admission][service-owner]
[C2 save boundary][c2-store] [History bounds][history-contract]

The Workspace cap counts allocated capacity, not merely live payload length:
resident segment/index state, piece metadata, names, handles, view pins, G/G+1,
saved-root maps, history descriptors/stages/failures, reply buffers and scratch.
Old references stay charged. Physical backing has a separate reservation and
occupancy account, including pinned/dead extents until released. Kernel cache,
bridge allocations/stacks, service/C1/C2 resources and process baseline are
additional scopes; 8 MiB is not an RSS/cgroup claim.

Validate saveability bounds before acknowledging a mutation intended to be
saveable: per-file base/final size, replacement bytes/edits, changed-name/inode
counts and encoded metadata. A small payload does not excuse an unsaveable
129-name batch. Refuse capacity before visibility; do not secretly split an
atomic operation or measured selection into extra saves. A larger envelope needs
an explicit decision and qualification, not a passing-run adjustment.

At the exact v0.1.6 tag, **8 KiB is an acquisition-prefetch cutoff, not an
uncached-file boundary**. The daemon also has a process-shared **32 MiB immutable
range/name/page cache** whose content admission is not limited to files at that
cutoff, and kernel caching can suppress callbacks. The host SnapshotCache's
8 MiB bound is a different component/scope. Earlier mechanism notes inspected a
different branch; their blanket above-8-KiB miss statement must not be applied
to the release tag. [Exact-tag immutable cache][v016-read-cache]
[Read-path cache use][v016-read-path]

No reference cache/prefetch constant is automatically justified for the new
working-set and operation boundaries. The selected zero extra userspace payload
cache may reduce that allocation class and increase remote read work; it is a
resource/performance trade, not a demonstrated overall improvement. No cache
grows or becomes transport-adaptive without an explicit decision and evidence.
Lazy projection proves neither cold data nor free reads. Record actual kernel
callbacks, cache hits/misses/eviction and logical requests for repeated execution.
The measurement protocol belongs to [04](04-implementation-and-verification.md).

### 11.2 Caller-visible failures

| Cause | Proposed filesystem/control behavior |
| --- | --- |
| Confirmed missing namespace name | `ENOENT` |
| Missing canonical bytes, integrity failure or provider failure | `EIO`, retaining the original typed cause; never reinterpret as a missing pathname |
| Known local wrong kind / nonempty / existing-name precondition | Appropriate `ENOTDIR`, `EISDIR`, `ENOTEMPTY`, `EEXIST`; retain atomic old state |
| Invalid name/offset/flags | Declared `EINVAL` or specific locally established path/size errno; no lossy coercion |
| Denied peer/Store/operation or local permission | `EACCES`/applicable local permission error, independently of content hashes |
| Known ownership/admission busy | `EBUSY` where the typed cause establishes it |
| Capacity | Proposed `ENOSPC`, retaining the cause/scope; generic service Capacity does not identify contention versus byte exhaustion |
| Unsupported selected operation | Explicit `EOPNOTSUPP` or documented unimplemented-kernel convention; no successful no-op |
| Explicit Deadline | `ETIMEDOUT` |
| Transport loss after possible mutation | `EIO` with original unknown state in lifecycle status; retain frozen input and forbid automatic replay |
| History conflict / stage disposition / continuity loss | Preserve complete typed lifecycle result; no single errno can describe it adequately |

Some low-level causes still collapse to generic service codes. In particular,
delivery failure is Io for reads and Unknown for mutations; socket timeout kinds
are not universally preserved. Do not promise timeout-versus-I/O distinction
where the current API loses it. C5 typed history context does not automatically
repair ordinary POSIX kind or rename-limit errors.

The explicit lifecycle result retains `Failure.code`, `unknown`, `cleanup`,
typed conflict and exact stage observation. An observed replacement stage is
not automatically G's stage; never discard or associate it with G without exact
incarnation/token/generation/context matching. The deep transition rules and
read-only-versus-writable-restart limitation belong to
[03](03-commit-integration.md).

## 12. Worked shell behavior and acceptance scenarios

These are ordinary shell examples for a future mounted profile, not existing
LayerFS CLI commands or claims they were run. Preconditions and capability costs
are part of each example.

| Example | Required path through the design | Planned acceptance |
| --- | --- | --- |
| `ls /workspace-id/src` | Root/name lookup, bounded List, entry kinds and directory cookies | R: ordinary and multi-page directories, no eager whole-directory collection |
| `head -c 4096 /workspace-id/data.bin` | Attrs/length, open, one or more kernel-requested ranges, EOF adaptation | R: requested logical prefix is correct; do not assume physical reads or kernel readahead equal 4096 |
| `cat /workspace-id/link` | Stored readlink target, kernel traversal, open/read of target | R: valid/dangling links and wrong-kind errors |
| `/workspace-id/bin/tool` | Execute permissions, compatible executable, loader/interpreter/library reads and mappings | R candidate: test a real binary above 8 KiB, with declared dependencies and kernel cache state |
| `printf x >> /workspace-id/existing` | Append against live EOF, accepted byte/mtime update | W: two writers, capacity refusal and G saving concurrently |
| `: > /workspace-id/existing` | Successful open truncation with no subsequent write | W: size/content change appears at open; creation is a separate dependency if absent |
| `cp /workspace-id/a /tmp/a` | Source read; destination belongs to another filesystem | R source capability only; says nothing about copy-in support |
| `mv /workspace-id/a /workspace-id/b` | Local atomic namespace change with type/flag/validation preconditions | Selected W rename only; normal and failed replacement cases |
| `rm /workspace-id/a` while another process holds it open | Remove binding, retain inode/read-write descriptor state | Selected W unlink; old handle remains valid and name stays absent after Commit |
| `mv /workspace-id /other` | Attempts to change managed mount/lifecycle identity | Prohibited; inner callback and parent/lifecycle enforcement checked |
| Editor / `sed -i` / Git mutation | Often temporary/lock creation, writes, metadata, rename and sync | No blanket support claim; qualify the complete application workflow |

`git status` may update an index; an operation described informally as inspection
is not automatically read-only. Redirects and compiler output generally need
creation as well as writing. A working `cat` and executable read do not establish
a general writable shell or full POSIX filesystem.

Minimum verification groups for this contract are:

1. Root/authority/profile/serial validation, immutable managed-root enforcement
   and separate-mount identity/cross-mount behavior.
2. Every enabled callback's normal, invalid-input, missing-content, wrong-kind,
   capacity and teardown path, with exact single reply ownership.
3. EOF/overflow/truncate/zero-gap/append behavior; repeated writes and reads
   through aliases and open-unlinked descriptors.
4. Multi-page directories, byte-short pages with continuation, dot entries,
   view-bound cookies and old views spanning completion.
5. Kernel page/executable behavior, enforceable unsupported mmap/sync flags,
   selected kernel-local locking and bounded interrupt/drain limitations.
6. Late remote replies, peer failure, incomplete streamed responses and separate
   syscall errno versus lifecycle failure context.
7. The G/G+1, exact-stage and retained-resource scenarios specified by
   [02](02-overlay-snapshot.md) and [03](03-commit-integration.md).

Tests stay outside product `src/` and exercise public production operations and
the mounted path where claimed. No build, mount, test or benchmark ran while
writing this contract.

## 13. What becomes smaller than v0.1.6, and what merely becomes narrower

The legacy FUSE crate contains 18 source files and 14,669 physical lines including
comments, blanks and inline tests. This is not the complete old Workspace stack
and is not a production LOC savings claim. The new adapter can avoid duplicate
transport and concentrate semantic state in one Workspace owner.

| Treatment | Concrete responsibility | Honest classification |
| --- | --- | --- |
| Reuse Pair 3 | Framing, peer/operation authorization, native connection lifetime, bounded logical delivery and service handlers | Reuse; no second filesystem proxy transport |
| Keep one Workspace owner | Inodes, names, pieces, handles, generations and visibility | Preserve semantics; moving them out of a monolithic LiveOwner is relocation |
| Keep Linux adapter thin | Kernel callbacks/attrs/replies/mounts | Retain required functionality; avoid broad duplicate synchronous/asynchronous port APIs |
| Reuse telemetry | Actual callback/byte/exchange/failure observations | Remove duplicate metric wire serializers, not useful evidence |
| Keep bounded private backing | Packed pending extents and metadata with bounded resident state | Required for large/tiny files; simplification must preserve capacity/ownership rather than delete backing and shrink the workload |
| Defer cache/prefill/readdirplus | Demand-only baseline and ordinary readdir | Changed optimization/capability scope, possibly more requests |
| Keep frozen G + live G+1 | Commands continue while one save runs, within bounds | Required full snapshot behavior; freeze-and-wait is only a weaker prototype |
| First fsync unsupported | Reference offered volatile success | Application-visible reduction requiring explicit compatibility treatment |
| Linux first | Later macFUSE/Windows adapters reuse Workspace | Platform scope, not implemented portability |

At the exact tag, `local_spool.rs` and `live_owner.rs` describe
**sandbox/execution-side local packed segments** and host construction consuming
a frozen generation. The current disk-backed target preserves that separation
while consuming the new shared service surface. Preserve accepted-byte ownership
until its readers/generations release it. [Reference spool][v016-spool]
[Reference live owner][v016-owner]

### 13.1 What the reference actually stores on disk

This comparison uses the exact v0.1.6 release
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`. Earlier mechanism notes inspected
`codex/190-pooled-scope`; their generic host-spool description must not override
the release's explicit sandbox-owned backing placement.

Mounting FUSE at a native path does not materialize every projected file in the
underlying directory. The release already keeps namespace/file-piece state in
execution-side memory, keeps unchanged data in the canonical Store, and stores
pending replacement payload in packed local files. The migration preserves this
range-based backing model while replacing private legacy integration with the
shared C1/C2/history operation surface. The backing is not a materialized copy
of the projected namespace.

```text
 exact v0.1.6                                     current disk-backed target

 application -> local FUSE mount                  application -> local FUSE mount
                    |                                               |
 execution-side RAM                              daemon RAM
   namespace + file pieces                         namespace + file pieces
   Base(root, range) ----------+                    Base(root, range) ------+
   Spool(segment, range)       |                    Extent(segment,range) |
            |                  |                           |               |
 local disk backing            |                    private local backing  |
 /snapshots/<WorkspaceId>/      |                    G + D1 + readers pin   |
   payload-00000001.bin        |                                           |
   payload-00000002.bin        |                    ranges/metadata roots  |
            |                  |                           |               |
 frozen records + ranges       |                           v               |
   -> legacy snapshot lane     +--> host Store      Pair 3 -> C1/C2 + C5  <-+
   -> host construction
```

The release daemon builds the private path from WorkspaceId, refuses an already
existing backing directory, and separately attaches FUSE to the requested mount
root. These are separate locations. The path is disposable backing, not a
restartable Commit archive. [Reference mount/backing assembly](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-daemon/src/main.rs#L1384)

Ordinary reference writes already preserve base ranges and replace only their
written intervals. The owner reserves spool space, prepares the piece, copies
the callback payload into owned transfer memory, completes the positioned write,
then applies the visible edit. The small write does not copy the whole base file.
The new backing must preserve this ordering and account for the temporary owned
input and the installed extents separately. [Reference piece preparation](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/file_edit.rs#L72),
[write-before-apply](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L1048).

### 13.2 Preserve the ownership rules; change the byte backing

| Reference mechanism | Lesson for the current migration |
| --- | --- |
| Base ranges plus replacement pieces | Keep inherited content as references; do not replace the file representation with a whole-file vector. No-copy-up is preserved behavior, not a newly invented optimization |
| Bytes installed before their piece becomes visible | Reserve, acquire owned bytes, validate, then publish. No borrowed callback slice or partially initialized segment may survive as acknowledged content |
| Bounded shared segment backing | Bound the physical allocation and resident bookkeeping a tiny live slice can pin. The old 1 MiB segment size is a source fact, not automatically the selected replacement size |
| Live/frozen/read references retain segments | Retire backing only after its last actual owner; truncate/unlink/Commit completion cannot discard bytes still needed by an older view |
| Requested-range read plans outside the state mutex | Retain a coherent view and read only demanded spans; neither local backing nor remote reads may require holding the state lock for I/O |
| Snapshot identity and exact completion association | Preserve generation, incarnation and revision guards; a stale completion cannot clear newer edits. Consume Pair 3/Pair 2 identities rather than porting the old private wire protocol |
| Separate spool and construction work | Keep daemon pending state separate from host C1/C2 construction; backing layout never becomes a canonical-object RPC |
| Disk physical-I/O permits, file descriptors and page-cache hints | Audit bounded I/O and FD/index capacity in the new runtime. Existing hints and worker arrangements are not inherited proof of RAM, fairness or throughput |
| Reference policy/telemetry counters | Do not treat logical spool or current-view counters as actual retained RAM capacity; use allocation lifetime accounting from 02 |

The old spool caps each segment at **1 MiB** and maintains a queue of up to
**four sealed segments** offered for `POSIX_FADV_DONTNEED`, also offering ranges
after serving them. This is best-effort page-cache management, not a hard 4 MiB
resident guarantee, a sync barrier or proof that later reads are cold. The active
segment and dirty/kernel/process state are additional considerations.
[Reference segment/window constants](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/local_spool.rs#L71),
[hint and served-range behavior](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/local_spool.rs#L252).

Disk bytes can remain in their backing file after a cached page is reclaimed.
Dropping a cache entry must therefore preserve the owned extent and its lookup
information. The abandoned RAM-only proposal could not use this property to
evict dirty bytes because its segment might be their only copy. In either case,
physical backing cannot be retired while live/frozen/read state still needs it.

The release's `spool_bytes` tracks accepted logical append charge and later
completion/reclaim adjustments. Its inline/piece accounting is also distinct
from every allocation retained by old views. A selected subrange can retain an
entire shared allocation. These source counters are useful observations but
cannot be reused as proof of the new total RAM envelope; this comparison does
not allege an independently demonstrated legacy leak.
[Reference edit accounting](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/file_edit.rs#L284)

Preserve the reference's snapshot semantics without inferring a constant-time
capture from its comments. Its actual `capture_frontier` clones dirty IDs and
collects children of dirty directory deltas. The new proposal's maintained
frontier/descriptor rotation is an additional implementation requirement; neither
version receives an unmeasured latency claim. Likewise, the reference already
omits durability sync: changing its volatile-success fsync to unsupported is a
separate compatibility choice, not an inevitable consequence of a backing change.
[Reference capture](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/frozen.rs#L36),
[reference fsync][v016-fsync].

### 13.3 RAM-only has a smaller pending-payload envelope

The exact-tag default ResourcePolicy has a **1 GiB spool-byte ceiling per
Workspace** and a separate **8 MiB final-delta memory ceiling**. These are policy
defaults, not measured peaks, disk occupancy guarantees or proof of every large
workload. The earlier RAM-only candidate was **8 MiB total Workspace-owned
allocated capacity per consumer**, including pending bytes, metadata, G/D1,
readers and scratch. The current disk-backed target retains this RAM working
allocation target while requiring a separate explicit backing quota.
The identical 8 MiB numeral does not make the policies equivalent.
[Reference ResourcePolicy](https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-workspace-core/src/limits.rs#L3)

An existing large file with a small dirty interval can use either backing without
copy-up. A large new or fully overwritten uncommitted file needs backing for all
its accepted new bytes. Removing disk backing therefore removes capacity as
well as file I/O machinery. RAM-only is no longer the selected general writable
target. A small RAM-only demonstration would not establish a compatible v0.1.6
replacement or a competitive result under #207's matched semantics.

The chosen workload retains the requirement for explicitly bounded local disk
backing. An alternative staged-content backing would need its own agreed shared
operation and is not selected in this packet. Keep the
Workspace semantics and ownership local; any service extension must obey the
existing Pair 3 contract and may not hide pending overlays or replay in the
bridge/service. No automatic spill, extra backend abstraction, memory increase,
new stage durability or backing implementation is performed by this comparison.
The backing choice and its qualification remain an explicit implementation input.

### 13.4 npm install and low-memory backing

The selected workload mixes large and tiny files, including `npm install`, with
a low-memory target. Pending data therefore uses explicit local disk backing.
The absence of whole-file copy-up bounds inherited bytes, but does not bound
the total new bytes, names and inode state produced before an explicit Commit.
npm installs dependencies into `node_modules` and can create executable symlinks;
actual workload scope must preserve its selected dependency tree and behavior.
[npm install documentation](https://docs.npmjs.com/cli/v11/commands/npm-install/)

Use daemon-local file-backed payload segments from the beginning, with a small
bounded RAM working set and an independent disk quota. This is the selected
backing direction, not transparent pressure spill from a RAM mode.
Retain the proposed 8 MiB accounted Workspace-allocation target while qualifying
whether all required working state fits it. Do not invent a measured peak or
raise memory to make the workload pass.

```text
 npm / shell -> local kernel -> FUSE -> daemon Workspace
                                           |
                  +------------------------+-----------------------+
                  |                                                |
          bounded RAM working set                       private local disk
          active metadata / view pins                   packed pending bytes
          bounded read/write/transfer buffers            indexed overlay state
          capture/result reservations                   if metadata cannot fit
                  |                                                |
                  +------------ frozen generation references -------+
                                           |
                           existing logical Pair 3 operations
                                           |
                             service C1/C2 + C5 history
```

The disk box is a required design direction, not existing core functionality.
It stays on the execution machine/container beside the daemon, has its own
explicit quota/path/lifetime, and uses ordinary no-durability-sync backing writes.
Install accepted bytes before publishing their pieces. G and D1 retain immutable
segment ranges; snapshot capture does not duplicate files or the spool. Local
backing I/O can impose bounded backpressure without holding the Workspace state
mutex across that I/O. Other operations continue within admission/resource limits.

| Workload pressure | Required treatment |
| --- | --- |
| Large inherited file, small edit | Reference unchanged base ranges; store only replacement bytes in local segments |
| Large new/overwritten file | Stream incoming bytes into quota-controlled disk backing through bounded buffers; no payload-size growth in daemon-owned heap |
| Many tiny files | Pack payloads; also bound names, inode/piece records, dirty indexes, segment descriptors, open backing-file descriptors, handles and directory views. Keeping every dirty inode indefinitely in RAM defeats the target even with disk payloads |
| Metadata exceeds working set | Design bounded indexed local metadata backing/paging, or report the workload limit. Evict only state reconstructible from owned backing or immutable Store roots; never drop dirty state. No new database/provider framework is selected here |
| Slow storage or peer | Bounded requests/buffers and declared deadline/refusal behavior; no unbounded callback or write-behind queue |
| Full disk / full RAM reservation | Refuse the unaccepted operation explicitly while preserving earlier acknowledged writes; do not inject a logical Commit or silently discard dirty data |
| Kernel page-cache growth | Measure/account residency separately and qualify backing I/O policy. tmpfs and best-effort eviction hints do not establish the requested memory bound |

An append-only metadata file followed by rebuilding an unlimited RAM map is
not bounded metadata backing. Its resident lookup/index pages, open backing
files and capture frontier must obey the same working-set limits.

An 8 MiB Workspace allocation budget is neither an 8 MiB process RSS guarantee
nor an 8 MiB sandbox limit including npm. FUSE/kernel page cache, bridge/thread
resources, allocator overhead and the npm process retain their own accounting.
If the desired bound is total container memory, that is a different qualification
target and cannot be established by the overlay quota alone.

There are independent shared-surface blockers at the reviewed source pin:
PreparedChanges permits 128 changed names/inodes/directories with bounded request
metadata, general live new-inode/symlink/portable-metadata construction is not
complete, and the existing edit operation has its own replacement/count bounds.
A package tree that exceeds these limits is not made committable by a spool.
Pair 1 and the shared-operation owners need an explicit bounded construction and
history contract for the complete captured input. Do not turn one user Commit
into hidden intermediate Branch Commits, split an atomic rename, increase a
bound, or exclude packages/scripts/paths to claim success. [Shared bounds](#111-existing-shared-bounds-versus-proposed-workspace-bounds)

Acceptance needs the full selected installation and its subsequent explicit
Commit, with captured G/live D1 correctness, payload and inode counts, actual
daemon allocations, container/kernel residency, spool footprint and all refusals
recorded under the existing protocol. This paragraph specifies evidence needed;
no install or measurement has been run. The RAM-only candidate is retained in
§13.3 to explain its rejected workload scope; this paragraph describes the
current target and the work required before it can be claimed as supported.

The file plan and exact per-commit production LOC obligations belong to
[04](04-implementation-and-verification.md). Neither projected file ranges nor
legacy physical-line counts are substitutes for a matched production-source
comparison or a mounted performance result.

[request]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/request.rs
[client]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/adapters/native/client.rs
[service-owner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/owner.rs
[service-read]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/read.rs
[service-fs]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/filesystem.rs
[service-history]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs
[service-startup]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/native/startup.rs
[service-config]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/native/config.rs
[daemon-run]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-daemon/src/run.rs
[catalog-open]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-history/src/sqlite/open.rs
[history-contract]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/history.rs
[c1-input]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/input.rs
[c1-read]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/file/read.rs
[c1-view]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/file/view.rs
[c1-portable]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/attributes/portable.rs
[c1-path]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/path.rs
[c1-validation]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/validate.rs
[c2-store]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-storage/src/cas/store.rs
[v016-fsync]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L1908
[v016-spool]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/local_spool.rs
[v016-owner]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs
[v016-read-cache]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/immutable_read_cache.rs#L8
[v016-read-path]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src/live_owner.rs#L1203
[fuser-api]: https://docs.rs/fuser/0.18.0/fuser/trait.Filesystem.html
[fuser-source]: https://docs.rs/crate/fuser/0.18.0/source/src/request.rs
[linux-fuse-io]: https://docs.kernel.org/filesystems/fuse/fuse-io.html
[linux-open]: https://man7.org/linux/man-pages/man2/open.2.html
[linux-rename]: https://man7.org/linux/man-pages/man2/rename.2.html
[linux-locks]: https://man7.org/linux/man-pages/man2/fcntl_locking.2.html
