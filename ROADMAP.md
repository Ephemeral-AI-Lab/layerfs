# LayerFS roadmap

This roadmap defines the sequence from the current local Linux/FUSE product to
OCI distribution, OverlayFS compatibility, Firecracker execution, runtime
integration, remote publication, and storage lifecycle management.

It defines milestones and acceptance boundaries, not delivery dates.

## Current baseline

~~~text
local Linux/Docker
+ real LayerFS FUSE mount
+ SQLite-backed authoritative Store
+ bounded dirty memory and spool
+ restart-durable accepted roots
+ exact Verified reopen
+ source-bound live fs-bench qualification
= PASS_LOCAL_ONLY
~~~

Current boundaries:

- direct FUSE is the primary workspace path;
- the LayerFS Store is authoritative;
- <code>/workspace</code> has no materialized backing tree;
- local SQLite is the current durable authority;
- OCI, OverlayFS, Firecracker, and containerd are not yet product paths;
- execution-environment names and handles never enter LayerFS identities; and
- Kubernetes is not a near-term priority.

## Product principles

### One canonical filesystem model

LayerFS Core, Engine, and VFS remain authoritative. Adapters may expose,
transport, or attach accepted roots, but they may not create a second canonical
namespace or content model.

### Direct FUSE remains primary

~~~text
applications
→ Linux VFS
→ LayerFS FUSE
→ MountedWorkspace
→ Engine
→ Store
~~~

OverlayFS and container-runtime integrations are compatibility surfaces around
that path.

### Runtime separation

LayerFS owns portable filesystem state and projection contracts. A Sandbox or
runtime adapter owns process, container, microVM, networking, attachment, and
quiescence lifecycle.

Runtime names, VM IDs, container IDs, mount paths, and attachment state never
enter canonical LayerFS bytes or identities.

### Immutable objects and guarded publication

Objects and roots are immutable. A small guarded reference selects an accepted
root and generation. No adapter may publish partially transferred, partially
validated, or unacknowledged state.

### Explicit durability

An OCI manifest, OverlayFS upperdir, container snapshot, or VM snapshot is not
automatically a LayerFS checkpoint.

~~~text
workspace mutation
→ LayerFS checkpoint
→ accepted-root acknowledgement
→ platform-specific snapshot or export
~~~

### Evidence before promotion

Every product path must retain exact source and platform identity, correctness,
restart behavior, bounded resources, cleanup, workload custody, and an honest
<code>PASS</code>, <code>REVISE</code>, or <code>NO_GO</code> disposition.

A faster invalid population remains invalid.

## Milestones

| Milestone | Priority | Outcome |
|---|---:|---|
| R0 — Canonical release | Immediate | Public repository, CI, image and stable entry points |
| R1 — OCI interoperability | High | Deterministic accepted-root import/export |
| R2 — OverlayFS compatibility | High | LayerFS lowerdir plus explicit upperdir commit |
| R3 — Firecracker workspace profile | High | LayerFS mounted and durable inside a microVM |
| R4 — Firecracker snapshot and branching | High | Snapshot-safe restart and isolated children |
| R5 — Container runtime integration | Medium | Snapshot lifecycle and containerd adapter |
| R6 — Remote object/ref transport | Medium | Authenticated fetch and guarded publication |
| R7 — Retention and GC | Medium | Authenticated reachability and bounded reclaim |
| R8 — Additional platform adapters | Later | AMD64 closure, FSKit and WinFsp evaluation |
| Kubernetes | Deferred | Optional orchestration after runtime and remote contracts |

## R0 — Canonical release

### Deliverables

- canonical public repository;
- README, roadmap and MIT license;
- Rust formatting, test, and Clippy CI;
- protected main branch;
- reproducible source and image identities;
- versioned Linux/Docker image;
- ARM64 release artifact;
- AMD64 build artifact;
- stable daemon CLI;
- release notes describing the exact supported scope; and
- preserved benchmark and durability evidence.

### Acceptance

- clean source tree;
- CI passes;
- canonical repository URLs are embedded in release surfaces;
- release image binds its source commit and tree;
- historical evidence remains unchanged;
- no stale temporary repository name in current-facing release surfaces; and
- no benchmark rerun unless product bytes or the measured path change.

## R1 — OCI interoperability

### Goal

Import and export accepted LayerFS roots through OCI without making OCI the
canonical internal representation.

~~~text
OCI image or layer
→ streamed decode
→ canonical LayerFS objects
→ integrity admission
→ accepted root

accepted LayerFS root
→ deterministic traversal
→ OCI blobs and manifest
~~~

### Initial scope

- OCI image layout and registry transport;
- deterministic layer import and export;
- regular files, directories, symlinks, and hard links;
- modes, ownership, and timestamps;
- OCI whiteouts and opaque-directory semantics;
- architecture and platform metadata;
- exact content digests and accepted-root provenance;
- bounded streaming;
- restart-safe partial transfer; and
- exact cleanup after failure.

OCI is an interchange and distribution format. LayerFS objects remain
canonical and need not be byte-identical to OCI layer blobs.

### Acceptance

~~~text
OCI fixture
→ import
→ accepted root
→ real LayerFS FUSE mount
→ exact recursive inventory
→ export
→ clean re-import
→ equivalent logical inventory
~~~

The campaign must include large files, many small files, hard links, symlinks,
whiteouts, opaque directories, replacements, malformed archives, interrupted
transfers, and restart/resume.

Memory must remain independent of complete image size. Partial roots are never
accepted.

## R2 — OverlayFS compatibility

### Goal

Allow accepted LayerFS roots to participate in conventional OverlayFS workflows
without replacing the direct FUSE product path.

### Initial profile

~~~text
LayerFS accepted root
→ read-only LayerFS FUSE lowerdir

owned upperdir + workdir
→ OverlayFS merged workspace
→ explicit commit
→ new LayerFS accepted root
~~~

Before commit, the LayerFS root is the authoritative base, the upperdir is
mutable compatibility state, and the merged mount is a derived execution view.
Upperdir changes are not accepted LayerFS state until commit succeeds.

### Required semantics

- whiteouts and opaque directories;
- rename and replacement;
- copy-up;
- hard links and symlinks;
- modes, timestamps, and admitted xattrs;
- expected-base conflict detection;
- interrupted commit recovery;
- exact ownership and cleanup; and
- an explicit commit receipt.

OverlayFS upper-directory inspection proves changed paths, not exact changed
byte ranges. Integration must pair upperdir semantics with exact mutation
custody or deterministic bounded conservative ranges. Uncertainty may not
silently become a whole-file or through-EOF update.

### Performance disclosure

OverlayFS copy-up can copy a complete lower file for a small modification.
LayerFS must expose copied-up files and bytes, upperdir scan work, commit I/O,
and LayerFS object creation/reuse.

No benchmark may present the OverlayFS route as direct LayerFS FUSE.

LayerFS as an OverlayFS upperdir is a later evaluation, not an initial
requirement.

## R3 — Firecracker workspace profile

### Goal

Run LayerFS as a first-class persistent workspace inside a Firecracker microVM
while keeping VM lifecycle outside canonical LayerFS state.

### Architecture

~~~text
host runtime adapter
├── Firecracker process
├── guest kernel
├── OCI-derived root image
├── LayerFS Store block image
└── vsock lifecycle channel

guest
├── root filesystem
├── layerfs-fuse daemon
├── /workspace
└── workload
~~~

The execution adapter owns VM creation, networking, attachment, quiescence, and
teardown. LayerFS owns the Store and mounted filesystem semantics.

The first Store transport should use an ordinary file-backed virtio-block
device. The Store disk remains separate from the guest root filesystem.

Vsock carries small lifecycle requests and receipts:

~~~text
health
open accepted root
checkpoint
report accepted generation/root
prepare snapshot
verify reopen
graceful shutdown
~~~

It is not the workspace payload path.

### Acceptance

- cold boot with empty and retained Stores;
- create, build, test, and serve a project;
- kill and restart the LayerFS daemon;
- stop and boot a fresh microVM with the same Store;
- exact accepted-root and mounted-inventory verification;
- substituted or corrupt Store rejection;
- bounded host and guest CPU/memory;
- bounded Store and spool growth;
- no leaked VM, socket, block, or network resources; and
- separate ARM64 and AMD64 evidence.

## R4 — Firecracker snapshot and fast branching

### Durability ordering

A Firecracker snapshot is not a LayerFS checkpoint.

~~~text
application operations
→ LayerFS checkpoint
→ accepted-root acknowledgement
→ flush guest Store block device
→ pause microVM
→ create Firecracker snapshot
→ retain VM-state, memory, and Store-disk identities
~~~

Restore:

~~~text
fresh Firecracker process
→ load snapshot
→ attach exact Store disk
→ resume guest
→ reconnect lifecycle channel
→ verify accepted generation/root
→ verify /workspace inventory
~~~

Firecracker snapshot state and disk lifecycle are separate responsibilities:

- <https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/snapshot-support.md>
- <https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/versioning.md>
- <https://github.com/firecracker-microvm/firecracker/blob/main/docs/vsock.md>

### Branching model

~~~text
immutable guest root image
+ reusable Firecracker memory snapshot
+ immutable LayerFS accepted root
+ isolated writable child
= fast disposable coding environment
~~~

Each child requires a unique writable authority, independent accepted
reference, exact parent root, unique VM/resource ownership, bounded resources,
and complete cleanup.

### Metrics

- cold boot to mounted workspace;
- snapshot restore to mounted workspace;
- first-command latency;
- branch creation latency;
- host and guest RSS;
- private memory growth;
- Store amplification;
- snapshot size;
- concurrent child density; and
- teardown latency and residue.

## R5 — Container runtime integration

### Goal

Expose LayerFS through a runtime-facing snapshot lifecycle independent of
Kubernetes.

~~~text
prepare
mount
commit
view
remove
~~~

Map read-only snapshots to accepted roots, active snapshots to writable
workspaces, and committed snapshots to newly accepted roots.

The initial target is a containerd-compatible adapter usable by conventional
containers, OverlayFS compatibility mode, and Firecracker preparation.

Required properties include immutable views, expected-parent commit,
restart-safe metadata, exact ownership, and no orphaned mount or Store
resources.

## R6 — Remote immutable object and reference transport

### Read-only phase

~~~text
remote immutable objects
→ authenticated local cache
→ accepted local root
→ FUSE or Firecracker workspace
~~~

Deliver object/root fetch, digest verification, bounded cache, resumable
download, exact missing-object behavior, and offline reopen from a complete
cache.

### Guarded publication phase

~~~text
local candidate objects
→ upload session
→ remote object admission
→ expected-head publication
→ accepted remote root
~~~

Deliver expected-head updates, idempotent upload sessions, immutable
deduplication, quotas, leases, partial-transfer recovery, exact conflicts, and
accepted publication receipts.

The first remote release is single-writer. Mutable path-revision logs do not
become canonical storage.

## R7 — Retention and garbage collection

### Goal

Reclaim unreachable data without weakening immutable history or accepted-root
safety.

~~~text
accepted and retained refs
→ authenticated reachability
→ stable retention boundary
→ reclaimable candidate set
→ guarded deletion
→ post-delete verification
~~~

Deliver explicit root pinning, retention policies, authenticated reachability,
epochs or certificates, crash-safe collection, bounded work, Store compaction,
physical-space accounting, and interrupted-GC recovery.

GC remains separate from normal publication and is never added as a hidden
background mutation.

## R8 — Additional platform adapters

### AMD64 closure

Run the same source-bound correctness, durability, resource, unchanged
<code>fs-bench.sh</code>, and cleanup campaigns as ARM64.

### Apple FSKit evaluation

Evaluate only after Linux, OCI, and Firecracker contracts stabilize. Any
adapter must preserve portable Core/Engine/VFS ownership.

### Windows WinFsp evaluation

Evaluate path/case behavior, links, metadata, deletion/open handles, restart,
cleanup, installer, and service lifecycle when a real consumer requires it.

## Deferred — Kubernetes

Kubernetes is intentionally not a near-term product priority.

LayerFS will not currently build:

- a CSI driver;
- an operator;
- custom resource definitions;
- admission webhooks;
- cluster-wide scheduling policy; or
- multi-node writable-volume semantics.

Those surfaces would force orchestration and distributed-storage policy into
the project before the local storage, OCI, OverlayFS, Firecracker, runtime, and
remote-publication contracts are stable.

A future Kubernetes integration should compose completed layers:

~~~text
OCI
+ containerd adapter
+ Firecracker profile
+ remote immutable objects
+ guarded publication
→ optional Kubernetes integration
~~~

Kubernetes remains an adapter and deployment environment, not an architectural
dependency.

## Promotion policy

A milestone advances from planned to implemented only when:

1. source is frozen;
2. exact identities are recorded;
3. correctness passes;
4. restart and fault evidence passes;
5. resource gates pass;
6. cleanup reaches zero owned residue;
7. performance populations satisfy preregistered controls;
8. no benchmark-specific production behavior exists;
9. evidence is independently verified; and
10. the final manifest is generated and verified.

Allowed dispositions are <code>PASS</code>, <code>REVISE</code>,
<code>NO_GO</code>, <code>BLOCKED_EXTERNAL</code>, and
<code>OUT_OF_SCOPE</code>.

No threshold may be weakened after observing a result. No diagnostic population
may be promoted as an authoritative performance result.

## Priority summary

~~~text
Now
    canonical release
    CI and packaging
    AMD64 build hygiene

Next
    OCI import/export
    OverlayFS compatibility
    Firecracker guest workspace

Then
    Firecracker snapshot and fast branching
    containerd snapshot lifecycle
    remote immutable object/ref transport

Afterward
    retention and garbage collection
    additional platform adapters

Deferred
    Kubernetes
~~~
