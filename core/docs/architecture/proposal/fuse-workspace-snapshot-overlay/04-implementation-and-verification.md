# Implementation and verification plan

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Source basis: `152b9c3a2e8ec2536a1d63601b681e1f7ef34455`, consolidated
> 2026-09-21. All new implementation and verification rows below are planned,
> not executed results. This document creates no production files, benchmark
> campaign, concurrency change or release claim.

**Implementation update, 2026-09-21:** the shared R0-R readable prerequisite is
implemented against synchronized main `0749180db34d1cdc57f905806a17e3f3f48ec2bc`.
`Inspect::Attributes` adds complete checked metadata and logical size without
changing existing Stat/List tags or bounds. The existing Source capability is
exported from the portable bridge contract, and `connect_until` carries one
deadline through connection/authentication/HELLO. The
[shared runtime description](../../14-service-runtime.md) records its exact
surface. This is a prerequisite result, not a mounted R1 result.

Validation for that prerequisite: Rust 1.85.1 locked bridge/service tests and
warning-denying Clippy with all targets passed; the product boundary guard and
its six self-tests passed. The exact commands were
`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-bridge -p layerfs-service`
and `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked -p layerfs-bridge -p layerfs-service --all-targets -- -D warnings`,
from the implementation worktree root with its own `core/target`, no overriding
Rust flags and `LAYERFS_CONSTRUCTION_WORKERS=1`. The new direct/authenticated
attribute parity test covers actual root serial 9, pre-epoch timestamps, file,
directory and symlink sizes, missing paths/objects, grants and profile refusal.
Initial checks retained a Source/Read trait ambiguity and an obsolete unknown
response-tag test; both were corrected and the rerun passed. Whole-core and
Linux mounted checks for the following R1 round are now recorded in
[08](08-readable-implementation.md), with exact scope and retained failures.

Packet [index](README.md); behavior owners:
[Workspace/FUSE](01-workspace-fuse-contract.md),
[overlay/snapshot](02-overlay-snapshot.md),
[Commit integration](03-commit-integration.md).
The [exact v0.1.6 source audit](05-v016-source-comparison.md) owns reference behavior;
the [benchmark qualification map](06-benchmark-qualification-map.md) owns
inherited workload membership and route-specific acceptance.
The [implementation handoff](07-implementation-handoff.md) carries this sequence
into the next implementation task; this document remains the detailed plan.

The current writable target includes `npm install`: large pending files and
many small files, with a bounded daemon working set. Its proposed backing is
explicit daemon-local disk segments plus bounded, file-backed runtime metadata
as specified in [02](02-overlay-snapshot.md). The earlier RAM-only payload plan
is a narrower prototype and does not satisfy this workload. The source-pinned
service still lacks the complete namespace/input surface needed for a large
installation; backing changes do not silently remove those limits. This plan
updates the required work and evidence, not production code or numerical caps.

## 1. Delivery principle

Implement one public operation at a time against the existing service. A
readable mount is the first deliverable. The first claimed writable profile
must include its required metadata semantics and frozen G/live G+1; a
content-only or freeze-and-wait prototype is a narrower bring-up step.
The owner-selected implementation is two libraries, `layerfs-workspace` and
`layerfs-fuse`, assembled by `layerfs-daemon`. Start with R0-R and the real
readable mount; close R0-W inputs before enabling their dependent writable
operations. The file plan is settled, while the listed API/format/resource
decisions still require concrete design and verification.

```text
 REVIEWED SHARED FOUNDATIONS
 GetBranch + Inspect/ReadFile + file saves + history commands
                  |
 [R0] close the selected operation's design/API/profile inputs
      R0-R: public API + complete read metadata + mount/control contract
      R0-W: backing/index/resource + writable shared-input contracts
                  |
                  v
 [R1] actual readable Linux mount
                  |
 [R1-C] authenticated daemon control binding, separate round
        required for host-SDK / container-Workspace route claims
                  |
                  v
 [R2] one shared portable-metadata operation
                  |
                  v
 [R3a] bounded disk payload write/read and ownership
                  |
 [R3b] bounded runtime metadata lookup/update and dirty index
                  |
 [R3c] coherent capture G / live G+1
                  |
 [R3d] bounded lowering + exact own Commit reconciliation
                  |
                  v
 [R4] mounted existing-file writes with non-pausing Commit
                  |
                  v
 [R5a] selected missing namespace/shared-input operation
       -> next independently qualified operation -> ...
                  |
 [R5b] full declared npm workflow and explicit Commit
                  |
                  v
 [R6] separately declared matched mounted comparison
```

Optional prefetch, wider caches, more concurrent saves, alternative transports
and additional operating systems are not bundled into these rounds. Existing
shared-service limits are source facts, not permission to change caller policy.
R3a-R3d and the individual R5a entries are separate operation rounds. They are
listed together to show dependencies, not authorization to combine storage,
metadata, transport and namespace changes into one implementation.

## 2. Round-by-round plan

| Round | Inputs/prerequisites | Concrete implementation | Exit evidence |
| --- | --- | --- | --- |
| R0: operation design closure | This packet, pinned source, complete selected workload/route and current unresolved inputs | Produce the checked format/ownership/profile/API decisions in §2.1 and §4; identify exactly which R/W/control subset is ready | Reviewed concrete input/result/identity/completion rules, layout/format and resource arithmetic; unresolved rows remain blocked for dependent implementation |
| R1: readable mount | Explicit root or coherent GetBranch descriptor; complete shared read metadata for the advertised callbacks; configured reachable service and grants; supported Linux mount capability | Workspace read/lifecycle public API, thin FUSE adapter, bounded identity/handle state, lazy metadata and range reads, permission/error mapping and daemon assembly/cleanup | Actual mounted lookup/stat/list/read/readlink/EOF, executable-read candidate, handle lifecycle and failure behavior; mutation refusal |
| R1-C: daemon control | R1 local API and real mount; agreed daemon-targeted control identity/auth/profile and bounded delivery | Bind mount/lifecycle and supported Workspace controls through existing bridge machinery; introduce each control operation separately | Actual host-SDK/container-Workspace identity, authorization, visible state and cleanup proof; local library or headless service forwarding is not equivalent |
| R2: portable metadata | One agreed bounded shared input/result shape, using existing public C1 metadata algorithms in the service | Existing-inode portable mode/mtime construction/update through existing bridge framing and handler ownership | Valid/invalid mode and timestamp cases, role validation, unchanged content roots, failure visibility and byte/count limits |
| R3a: payload backing | Declared local backing path/capability, disk quota, segment/window/descriptor limits, failure and retention policy | Stream accepted bytes into private disk extents before visible publication; bounded reads and immutable extent ownership | No whole-file copy-up; real partial-write/disk-full behavior; retained/dead/reserved storage and FD/window bounds accounted |
| R3b: runtime metadata | R3a lifetime model; selected bounded on-disk lookup/update/index representation and page/cursor limits | Namespace/inode/piece/segment records and dirty frontier with bounded resident state; no full in-memory mirror | Cardinality covered by the available public path, bounded indexes/FDs and failure visibility; full npm cardinality remains gated on its required R5a inputs and R5b proof |
| R3c: overlay capture | R2 before claiming full file-write metadata semantics; immutable payload/metadata versions and pre-reserved capture descriptors | Coherent G frontier rotation, one mutable G+1 and exact version/source pins | Capture performs no payload copy, demand-loading of metadata backing pages or full namespace/dirty-frontier scan; later operations cannot mutate G |
| R3d: lowering and completion | R3c; supported file and history operations; each selected larger-input capability agreed separately | Read G's dirty records/pages incrementally, stream file inputs, associate exact roots, reconcile known own Commit and preserve failures | One file save per captured changed payload version; one tree build per StageChanges/Commit; G+1 survives; S-11 proves progress during actual service save |
| R4: existing-file writable mount | R3a-R3d, timestamp updates, defined callback flags, enforceable cache/mapping capability and volatile flush policy | Existing regular-file write/append/truncate/extend with explicit Commit, within the actual shared edit envelope | Read-your-writes, successful-open truncation, atomic append positioning, stored metadata, non-pausing snapshot and retained failure state; this alone is not npm acceptance |
| R5a: shared/namespace increments | Each operation's complete API and pre-visibility invariants; current limitations recorded below | Select live file creation, mkdir, symlink, unlink/rmdir, regular link, rename, setattr, or the bounded larger filesystem-input path separately | Public direct/transport parity plus mounted operation tests where applicable; complete input and one explicit Commit meaning preserved |
| R5b: declared npm workflow | Every callback, metadata rule, backing capability and shared input required by the entire chosen package graph is implemented | Run the declared installation and explicit Commit through the real mount and service, retaining full workload membership | Independent complete namespace/content/metadata oracle, large and tiny files, repeat use, quota/failure behavior, G/G+1 progress and bounded resource observations |
| R6: comparison | A real mount and all selected workload semantics verified on both arms; exact prospective declaration | Existing harness integration and matched v0.1.6/core selections | Independent semantics/resources/custody proof; declared mount/read/Commit timing; all failed/ineligible/unrun rows retained |

R0-R/R1 must resolve any missing complete attribute, file-length, directory or
readlink result needed by the advertised callbacks through the shared service
surface. At the reviewed pin, existing Inspect results do not by themselves
constitute a complete FUSE attribute/batching surface. Do not synthesize unknown
attributes, expose canonical-object RPCs or defer a required read contract until
the writable round. Any necessary shared read operation is its own prerequisite
round. R2 is specifically metadata construction/update for mutations.

R2 is needed for implicit write mtime changes, not only explicit `touch` and
`chmod`. An in-memory updated timestamp followed by Commit retaining old metadata
is not the promised writable behavior. A prototype preserving old metadata must
be named as such and cannot pass R4.

R5a has separate dependencies. ReserveInodes supplies identities but current
PreparedChanges cannot attach new serials. Bootstrap's bounded manifest is not
a replacement for a live create/mkdir/symlink operation. Descendant rename needs
normal type/emptiness/flag checks and bounded cycle/alias validation before
success becomes visible. Forbidding root rename does not remove those checks.

If an R1/R3/R4 proof needs a missing shared contract, complete that selected
R5a-type prerequisite in its own earlier round. The labels group responsibilities;
they do not require a circular implementation order or permit a private test
backdoor around an unavailable operation. A bounded foundation proof cannot be
reported as the larger npm-cardinality proof.

R1's first production control entry is the local Rust API specified in §4.1,
used by daemon assembly or an embedding executor. A Docker host SDK cannot call
that in-process handle across a container boundary. The daemon-targeted control
binding in §4.2 is therefore the separate R1-C implementation round before
claiming a host-SDK/container-mounted route; it is not an optional cluster feature
that can be omitted from those benchmark rows. Current headless stdin forwarding
to the host service remains its own mode and is not a Workspace control server.
The main diagram shows the recommended complete-route sequence. Local R2/R3
work does not depend on a network management endpoint solely because R1-C is
drawn first; host-driven Docker/SDK acceptance does require it. R1-C first exposes
the read/lifecycle subset; later write/edit/Commit controls appear only with
their independently implemented and qualified Workspace operations.

### 2.1 R0 closure outputs and operation readiness

The low-memory target remains **8 MiB of accounted Workspace working allocation
per consumer**, proposed and unqualified. Pending on-disk bytes are a separate
finite storage allowance, not an increase to that RAM target. No total daemon
RSS or container/cgroup cap is established by this number.

| Design input | Required R0 output and consequence |
| --- | --- |
| Readable projection API | Complete portable attributes, file length, identity, directory-page/cursor and readlink facts required by 01's R callbacks; map each to existing shared operations or a separately implemented extension. Select request/result bounds and refuse unsupported capabilities explicitly |
| Backing location/capability | Exact derived directory rules, ownership/permission checks and stale-incarnation refusal; private execution-side storage outside the FUSE mount, with no shared host-path protocol or recursive writes through the mount |
| Physical disk quota and reserve | Complete accounting equation covering payload, runtime metadata/index files, reserved extents, partial/dead bytes and retained old versions; selected allocation granularity, refusal point and checked release condition |
| Segment size and I/O windows | One finite implementation profile with aggregate ingress/read/source/page conversion arithmetic; no file-size-sized buffer or indefinite shared append allocation |
| Metadata index/page policy | Concrete local record/page/key encoding, format/version checks, bounded lookup/update traversal, immutable publication roots and failure behavior. Recommended shape: one paged ordered index with COW pages; no pluggable backend or unlimited reconstructed HashMap |
| Descriptor and reference limits | Selected FD/page-cache/handle/cookie/root-pin limits and worst-case admission arithmetic; root capture retains a descriptor without enumerating/refcounting every descendant |
| Progress headroom | Reserve descriptors, source windows, metadata publication, lowering and terminal-result capacity needed by already accepted work |
| Physical reclamation | Exact last-reference/retained-root rules, bounded reclaim work and failed-cleanup accounting. Select no automatic compaction initially; fragmentation can refuse quota until a separately designed compaction operation exists |
| Kernel I/O mode | Actual writable-mmap/writeback/truncate enforcement and page-cache accounting; cache advice is not proof of a resident-memory ceiling |
| Local and SDK/control API | Final typed request/result/receipt shapes from §4.1-§4.2, supported operation inventory, permission mapping, capability negotiation and all failure/unknown states; no implied daemon management endpoint |
| Larger prepared/edit input | Explicit shared service input, ordering/scratch ownership, atomic user-Commit meaning and finalization after full input validation; paging frames into an unlimited vector is not a bounded contract |
| Multi-Workspace capture/admission | Explicit disposition of the tighter one-retained-G-per-consumer proposal against the reference's per-Workspace captures and declared multi-Workspace benchmark schedules. Preserve the single construction producer and resource bounds; do not silently remap consumers, add retries/queues or treat a refused required schedule as supported |

The [Linux and Docker placement examples](01-workspace-fuse-contract.md#321-one-configurable-root-linux-example)
derive `workspace/` and `private-backing/` from one configured local root, with a
separate disk quota. Verify resolved path containment, isolated child permissions,
per-Workspace FUSE mounts only, correct container-visible path resolution and
refusal of stale/conflicting backing. Never mount FUSE on the common parent or
delete that parent during one Workspace's cleanup; active relocation is not supported.

The [proposed startup variables](01-workspace-fuse-contract.md#323-proposed-pair-1-startup-variables-and-attach-inputs)
define one common root and aggregate consumer RAM/disk budgets. Verify positive byte
parsing, refusal of zero/unlimited and invalid writable quotas, minimum progress
headroom, separate per-Workspace attach identity, and reuse of existing Pair 3
connection settings. These remain proposed names; current daemon startup does
not implement them.

Disk quota, segment/window sizes, metadata page/index ceilings and progress
headroom are required design inputs, not numbers invented by this plan. Record
them with the selected workload and validate their arithmetic before code is
qualified. Do not substitute the reference's spool quota for an owner decision.
Backing writes provide declared volatile read-your-writes; no sync/WAL or restart
recovery is added.

R0 is a design milestone, not an aggregate executable gate or approval to alter
existing source limits. Close only the rows needed by the selected next operation:
R1 can proceed with its read-only subset while R0-W format or large-input decisions
remain open. Do not call disk-backed W/npm implementation ready while its actual
format, quotas, mandatory syscall behavior or shared inputs remain unspecified.
Record the selected decisions in the owning packet document and name every
remaining blocker. Working LOC ranges below reserve implementation space; they
do not resolve those design questions by themselves.

### 2.2 Process configuration and derived storage layout

The following configuration is proposed for Workspace mode, not parsed by the
pinned daemon. Existing headless-only forwarding keeps its current configuration.

| Variable | Selected plan | Validation / owner |
| --- | --- | --- |
| `LAYERFS_WORKSPACE_ROOT` | One absolute common root; `/layerfs` is the proposed Docker-profile default | Derive fixed `workspace/` and `private-backing/` children; validate ownership, containment, mount overlap and separation from service Store/catalog. No independent per-Workspace mount/backing path setting |
| `LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES` | Proposed 8 MiB working-allocation default (`8388608`), unqualified | Positive byte count and selected progress minimum; aggregate per consumer. No zero/unlimited, implicit multiplication by Workspace count or automatic increase |
| `LAYERFS_WORKSPACE_DISK_BUDGET_BYTES` | Explicit positive quota required for W; no invented numeric default | Aggregate owned private backing including reserved, pinned, dead and failed-cleanup allocation. R does not allocate a writable spool merely because the root exists |
| `LAYERFS_WORKSPACE_MAX_COUNT` | Explicit positive count required when enabling Workspace mode; **no default count is selected** | Admission includes attaching, mounted, unmounted-retained, closing and failed-cleanup Workspaces until checked release. Reserve the slot before attach; validate total minimum per-entry headroom against the selected resource profile |

```text
 LAYERFS_WORKSPACE_ROOT=/layerfs          one managed consumer root
 |-- workspace/
 |   |-- ws-a/                          FUSE mount A; managed ID immovable
 |   `-- ws-b/                          FUSE mount B
 `-- private-backing/
     |-- ws-a/                          private payload/index extents + roots
     `-- ws-b/                          private payload/index extents + roots

 host service: LAYERFS_STORE / LAYERFS_HISTORY_CATALOG stay service-local
```

`MAX_COUNT` is a count bound, not a guarantee that that many Workspaces fit or can
all retain G concurrently. One retained frozen submission per consumer still
applies. Failed attach/cleanup ownership cannot vanish from either count or quota
just because a method returned an error. Reject absent/zero/invalid count rather
than assume unlimited. Positive values still require checked size arithmetic and
real resource admission. These values are fixed for the host lifetime; changing
the environment does not relocate active mounts or resize active ownership.

Endpoint/key/telemetry settings reuse the existing Pair 3 names. Per-Workspace
attach inputs supply managed child ID, producer incarnation, logical Store,
explicit read-only root or Branch source, requested capabilities and owner
permission context. No child path override, catalog credential, stage token,
worker count, auto-Commit timer or arbitrary metadata-cache knob belongs in those
inputs. R0's segment/page/FD parameters form one internal qualified profile rather
than a public option for every constant.

### 2.3 Shared-operation gates independent of the spool

At the source pin, a prepared update has at most **128 changed names total,
128 inode updates, 128 directory records and 32 KiB encoded request metadata**.
It accepts existing identities, with no general live new-inode attachment.
EditFile retains its separate **256 edits / 8 MiB replacement-input** envelope;
the 4 GiB file bound does not enlarge replacement replay. [Current request
contract][request-contract] [Prepared filesystem handler][prepared-handler]

| Needed for the declared installation | Current gap / required proof |
| --- | --- |
| New files and directories | ReserveInodes exists; new serial attachment through the shared operation is still missing |
| Modes, timestamps and executable symlinks | General live portable metadata/symlink construction and attachment need their selected shared operations; bootstrap is not a substitute |
| Many changed names/inodes | An agreed bounded input path must cover the complete frozen generation; raising a count alone does not solve the 32 KiB envelope or decoded memory |
| Large existing-file replacements | Disk source availability does not remove service replay/input limits; oversized supported edits need their own agreed stable bounded-input semantics |
| One explicit Workspace Commit | Any streamed/paged internal construction must have declared identity, visibility and completion; no hidden series of 128-name logical Commits |

Do not invent a `StageChanges` streaming body: current history requests carry
their prepared records in bounded metadata. A new bounded shared path uses the
same handler ownership and authorization with a reviewed operation contract;
it cannot expose private canonical objects, SQL or pack layout. Until the needed
path exists, the full installation/Commit row remains unsupported or NOT_RUN.
Do not hide that status with a smaller package graph, ignored required scripts,
periodic auto-Commits or a private root-registration call.

## 3. Proposed production layout

The owner selects two production libraries, `layerfs-fuse` and
`layerfs-workspace`, assembled by the existing `layerfs-daemon` executable.
Workspace is grouped by responsibility. This is the full-target file map;
R/W/control labels below determine when each real file appears. Do not scaffold
empty packages, modules or future-platform directories. These replacement
packages live under `core/crates/`; same-named root `crates/` packages remain
reference source and are not dependencies or source includes.

```text
core/crates/
|-- layerfs-daemon/
|   |-- Cargo.toml
|   |-- src/
|   |   |-- main.rs                  entry
|   |   |-- lib.rs                   small assembly API exports/delegation
|   |   |-- run.rs                   configured assembly and lifecycle
|   |   |-- config.rs                environment parsing and validated settings
|   |   |-- headless.rs              existing headless route, reused
|   |   `-- control.rs               daemon-targeted SDK/lifecycle dispatch
|   `-- tests/                      startup, control and cross-component routes
|
|-- layerfs-fuse/
|   |-- Cargo.toml
|   |-- src/
|   |   |-- lib.rs                   exports and platform gating
|   |   |-- mount.rs                 session, capabilities, invalidation, cleanup
|   |   |-- adapter.rs               ONE Filesystem trait implementation
|   |   `-- replies.rs               native attrs, errno, entries and cookies
|   `-- tests/                      actual Linux mounted behavior and cleanup
|
`-- layerfs-workspace/
    |-- Cargo.toml
    |-- src/
    |   |-- lib.rs                   narrow public exports/delegation
    |   |-- types.rs                 logical API IDs/options/results/errors
    |   |
    |   |-- runtime/
    |   |   |-- mod.rs               declarations only
    |   |   |-- host.rs              finite admission and shared resources
    |   |   |-- state.rs             live state, identity and semantic handles
    |   |   `-- lifecycle.rs         attach/status/clean-close and stage controls
    |   |
    |   |-- filesystem/
    |   |   |-- mod.rs               declarations only
    |   |   |-- namespace.rs         lookup and metadata resolution
    |   |   |-- read.rs              stable views and bounded range delivery
    |   |   |-- write.rs             coherent file mutations and SDK edits
    |   |   |-- changes.rs           coherent namespace/attribute mutations
    |   |   `-- directory.rs         bounded overlay merge and directory views
    |   |
    |   |-- overlay/
    |   |   |-- mod.rs               declarations only
    |   |   |-- pieces.rs            base/replacement/zero/version spans
    |   |   `-- snapshot.rs          capture G and preserve live G+1
    |   |
    |   |-- backing/
    |   |   |-- mod.rs               declarations only
    |   |   |-- budget.rs            RAM/disk reservations and accounting
    |   |   |-- segments.rs          disk extents, bounded I/O/FDs and lifetime
    |   |   |-- metadata_pages.rs    checked local record/page encoding and I/O
    |   |   |-- metadata_index.rs    file-backed records and dirty index
    |   |   `-- reclaim.rs           bounded last-reference physical retirement
    |   |
    |   `-- commit/
    |       |-- mod.rs               declarations only
    |       |-- lower.rs             final state -> valid shared inputs
    |       |-- source.rs            frozen extents -> existing bridge Source
    |       `-- save.rs              submissions and exact completion handling
    `-- tests/                      public semantic, snapshot and failure cases
```

Folder boundaries name responsibilities, not new services or traits. `runtime`
owns identity/lifetime and short state transitions; `filesystem` owns semantic
operations; `overlay` owns versions and capture; `backing` owns local allocation,
I/O and retirement; `commit` owns lowering and shared operation submission.
The `commit` group consumes C5 through the service and never implements its SQL,
stage tokens or Branch CAS. `config.rs` parses daemon environment variables into
Workspace options; the library validates its typed inputs for every caller.

Tests use each package's production public API or actual mounted/control route.
Group related observable behavior rather than mirroring every private function.
Do not include/recompile private source, introduce product test hooks, or widen
an API purely for testing. A direct Workspace Rust call does not qualify a
cross-container SDK route or a kernel callback.

### Ownership and proposed size

Ranges are **physical Rust lines including comments and blanks**, not measured
production LOC, quotas or lower bounds. Smaller cohesive files are desirable.

| File under `core/crates/` | Working range | Ownership details |
| --- | ---: | --- |
| `layerfs-daemon/src/main.rs` | 10–25 | R: thin executable entry calling the production library |
| `layerfs-daemon/src/lib.rs` | 10–30 | R: production exports and delegation only; public types live in their owning files |
| `layerfs-daemon/src/run.rs` | 100–180 | R: existing client/telemetry assembly, headless versus Workspace mode and bounded process shutdown |
| `layerfs-daemon/src/config.rs` | 180–300 | R: root/quotas/MAX_COUNT/attach and connection configuration validation; no filesystem algorithms |
| `layerfs-daemon/src/headless.rs` | 134–134 | R: preserve existing 134-line body at the source pin; not the mount-management route |
| `layerfs-daemon/src/control.rs` | 200–350 | Control: authenticated daemon-targeted request dispatch to public Workspace operations; reuse bridge parsing/delivery, never implement a second codec |
| `layerfs-fuse/src/lib.rs` | 5–20 | R: declarations and Linux gating |
| `layerfs-fuse/src/mount.rs` | 180–300 | R/W: sessions, capability negotiation, derived mount path, invalidation binding, mount/unmount lifecycle |
| `layerfs-fuse/src/adapter.rs` | 400–650 | R then W: one thin Filesystem trait implementation; signatures and supported/refused dispatch only |
| `layerfs-fuse/src/replies.rs` | 180–300 | R: native attr/errno/entry/cookie conversion; no history algorithms |
| `layerfs-workspace/src/lib.rs` | 10–30 | R: declarations/reexports |
| `layerfs-workspace/src/types.rs` | 180–300 | R then W: logical IDs, options, bounded results and failure/publication observations; no fuser/SQLite/native-path schema |
| `layerfs-workspace/src/runtime/mod.rs` | 5–20 | R: group declarations and crate-private reexports only |
| `layerfs-workspace/src/runtime/host.rs` | 200–350 | R: one finite consumer registry, root policy, count admission and shared resource ownership; not a pluggable provider registry |
| `layerfs-workspace/src/runtime/state.rs` | 250–450 | R then W: one per-Workspace visible descriptor, handles, identities/revisions and submission state; no full resident namespace mirror |
| `layerfs-workspace/src/runtime/lifecycle.rs` | 180–300 | R then W: attach/GetBranch, status, exact staged controls and clean-close; Init/Fork/AddLayer remain explicit existing service operations outside implicit attach/Commit |
| `layerfs-workspace/src/filesystem/mod.rs` | 5–20 | R then W: group declarations and crate-private reexports only |
| `layerfs-workspace/src/filesystem/namespace.rs` | 200–350 | R: view-bound lookup/attrs and immutable-base inspection |
| `layerfs-workspace/src/filesystem/read.rs` | 150–250 | R: EOF, one pinned version per read, range queries and terminal gating |
| `layerfs-workspace/src/filesystem/write.rs` | 180–300 | W: POSIX write and semantic SDK range-edit preparation, reserve/write/publish ordering and revision checks |
| `layerfs-workspace/src/filesystem/changes.rs` | 250–450 | W: atomic namespace/attribute effects and preconditions after their shared capabilities exist |
| `layerfs-workspace/src/filesystem/directory.rs` | 200–350 | W: base/G/D1 merge, tombstones and bounded stable views/cursors; extract from R namespace owner when needed |
| `layerfs-workspace/src/overlay/mod.rs` | 5–20 | W: group declarations and crate-private reexports only |
| `layerfs-workspace/src/overlay/pieces.rs` | 250–450 | W: checked base/extent/zero/version spans; long recipes use selected metadata backing |
| `layerfs-workspace/src/overlay/snapshot.rs` | 250–400 | W: capture immutable roots/frontier, preserve successor and maintain exact generation transitions |
| `layerfs-workspace/src/backing/mod.rs` | 5–20 | R/W: group declarations and crate-private reexports only |
| `layerfs-workspace/src/backing/budget.rs` | 180–300 | R/W: consumer RAM/disk/count reservations and retained/reserved/dead ownership arithmetic |
| `layerfs-workspace/src/backing/segments.rs` | 350–600 | W R3a: selected disk extent allocation and bounded read/write/FD operations; immutable accepted ranges |
| `layerfs-workspace/src/backing/metadata_pages.rs` | 300–500 | W R3b: checked local record/page codec, page I/O and immutable page handles; no canonical C1 codec |
| `layerfs-workspace/src/backing/metadata_index.rs` | 400–650 | W R3b: bounded indexed namespace/inode/piece/dirty-frontier lookup/update and page-root publication |
| `layerfs-workspace/src/backing/reclaim.rs` | 200–350 | W R3a/R3b: bounded last-reference retirement using segment/page owners; failed cleanup stays charged; no implicit compactor |
| `layerfs-workspace/src/commit/mod.rs` | 5–20 | W: group declarations and crate-private reexports only |
| `layerfs-workspace/src/commit/lower.rs` | 250–400 | W: paged G traversal to valid edits/prepared input, no complete-frontier vector or root-label substitution |
| `layerfs-workspace/src/commit/source.rs` | 100–180 | W: exact frozen extent stream through existing bridge Source; bounded windows and source lifetime, no transport wrapper |
| `layerfs-workspace/src/commit/save.rs` | 300–500 | W: single-producer file/metadata/history submission, exact outcomes and paged completion associations |

Hard ceilings are **999 physical lines per production implementation file** and
**200 for declaration/delegation-only lib.rs/mod.rs**. Do not make a larger
file fit by minifying, macro expansion, includes, or moving implementation into
an entry module. Split a real responsibility before the hard limit.

The table has **35 production files across three crates**, with a
**5,804–9,849 physical-line planning envelope**, including the existing
headless body. The split relocates the former daemon modules and adds five
small declaration-only group modules; it does not duplicate their algorithms.

| Crate | Files | Physical-line planning range |
| --- | ---: | ---: |
| `layerfs-daemon` | 6 | 634–1,019 |
| `layerfs-fuse` | 4 | 765–1,270 |
| `layerfs-workspace` | 25 | 4,405–7,560 |
| **Total** | **35** | **5,804–9,849** |

The R-only file subset has **20 files and a 2,564–4,329 line range**; adding the
required daemon control dispatcher makes **21 files and 2,764–4,679 lines**.
These sums include runtime/filesystem/backing declarations used by R. Overlay
and Commit groups appear with W. Subset sums describe file scope, not a forecast
of the first R change: shared files gain W behavior later.

Method: sum lower/upper columns once per listed production file, with headless
fixed at its source-pinned 134 physical lines. The range covers the local API,
FUSE projection, disk/index responsibilities and daemon control dispatcher. It
excludes manifests, tests, docs, build artifacts and changes to
bridge/service/C1/C2/C5. It is not measured production LOC, development effort,
proof that 8 MiB fits, or a whole-product reduction relative to v0.1.6.

The disk/index ranges are planning allowances for the listed responsibilities,
not a claim that their format and algorithms are already designed. R0 must close
them before their implementation rounds. Keep small helpers in their owner;
split a real responsibility if implementation approaches a hard ceiling. Do not
create a facade/factory/interface per file, generic backing-provider layer,
unused storage adapter or additional runtime facade crate. Use the two selected
libraries directly and reuse existing dependencies. If a required capability
would need a third-party patch, report the blocker.

Every future commit separately needs exact parent/staged/committed production
LOC, with reference/core totals and runtime source scope. Tests, docs, manifests,
blank lines and comments are excluded from that count. These planning ranges
and Git diff totals cannot replace it. No production LOC saving is claimed by
this document move.

## 4. Reuse and dependency boundaries

| Need | Reuse/owner | Do not add |
| --- | --- | --- |
| Logical requests and bounded delivery | Existing bridge Client, contract types and framing | Another proxy client, history relay, per-object RPC or transport codec |
| Canonical file/tree work | Service-local public C1 algorithms with C2 providers/consumers | Daemon canonical constructors, SQL/pack interpretation or copied algorithms |
| History transitions and cursors | Existing profile-2 service/C5 operations | Another stage registry/catalog, token allocator or cursor-MAC implementation |
| Local state | Workspace version/handle/budget owners | Duplicated LiveOwner and secondary registries with divergent lifetimes |
| Pending payload backing | Explicit daemon-local immutable disk extents and bounded I/O windows | Whole-file copy-up, RAM-only fallback, per-file unbounded open FDs, or automatic Commit on pressure |
| Runtime metadata backing | One selected bounded lookup/update/dirty-index representation | A full resident mirror, a log that is completely rebuilt into RAM, or a new generic database/provider framework |
| Native callbacks | Selected supported FUSE dependency and thin adapter | Framework forks, vendor copies or custom dependency patches |
| Observation | Existing telemetry/report path plus useful operation-owned counters | Duplicate metric serialization or benchmark-only product instrumentation |

Missing metadata/new-inode/symlink/typed-error operations are changes to the
shared contract and owning service handler in their own rounds. Their existence
must be proven by public source/API tests before the FUSE table advertises them.
Do not hide a missing capability in a private FUSE opcode or translate an error
into a fallback implementation.

### 4.1 Local production API and dependency direction

The first selected control route is a **local Rust API in `layerfs-workspace`**.
`layerfs-fuse` exposes mounting and kernel adaptation. Daemon `run.rs` assembles
both libraries; an embedding native executor/SDK can use the same public APIs. The
standalone daemon gains explicit Workspace startup assembly; its existing
headless mode continues to forward bounded content/history requests and is not
silently reinterpreted as mount management.

```text
 deployment / cluster scheduler / host SDK
     | chooses executor, identities, grants, paths and lifecycle
     | local embedding OR required daemon control binding (section 4.2)
     v
 layerfs_workspace::WorkspaceHost      [runtime/host.rs]
     | finite attach registry, configured root, shared budgets/admission
     v
 Workspace public operations          [per-Workspace semantic handle]
     |                               |
     | short state transitions        +--> segments / metadata pages / index
     |                                    private local backing, bounded I/O
     +--> overlay/snapshot / commit/lower / commit/save
                 |
                 +--> existing logical bridge client --> service C1/C2/C5

 local application --> kernel --> layerfs-fuse --> SAME Workspace methods
                                             ^
                                             |
                               ordered cache invalidation / replies
```

The compile-time dependency direction is:

```text
 layerfs-daemon -------> layerfs-fuse
        |                      |
        +-----------> layerfs-workspace
        |                      |
        v                      v
 layerfs-bridge          layerfs-bridge
 native assembly        logical contract / Source

 Runtime delivery supplied by daemon assembly:
 Workspace -- OperationDelivery -- bridge -- service -- C1/C2/C5
 (the service is not a Workspace crate dependency)
```

`layerfs-workspace` never depends on `layerfs-daemon`, `layerfs-fuse`, fuser or
service/storage/history implementations. Its bridge usage is the selected
logical contract and Source capability; `OperationDelivery` is supplied by
assembly, rather than constructing a native Client in the library. `layerfs-fuse`
uses only the Workspace public semantic surface and owns all kernel types.
The daemon owns connection/configuration/control wiring and invokes both crates;
no crate cycle or extra process/transport hop is introduced by this split.

`layerfs-workspace/src/lib.rs` reexports WorkspaceHost, Workspace, typed
options/results and the semantic operations required by both SDK and FUSE.
Read/lookup/attribute/directory/handle/mutation operations that the adapter calls
are a real public production boundary, with portable types defined in their
owning files and reexported as needed. The five groups remain implementation
modules, not public access to locks, extent/index internals or arbitrary capture
and reconcile methods. `layerfs_fuse::mount` and MountHandle are exported by the
FUSE crate. The exact semantic signatures are closed in R0 against 01's callback
inventory; tests do not justify additional public methods.

The projection-coherence binding must also cross this public boundary: Workspace
publishes portable affected-identity/revision information through the bounded
binding selected in R0; FUSE owns kernel invalidation/reply ordering. SDK success
still waits for the required coherence result. Do not expose fuser types in
Workspace, poll an unbounded event log, or silently turn notification into
best-effort success. This is the existing required projection binding, not a
new generic event bus. Platform-specific backing I/O stays within `backing`;
macFUSE/Windows adapters are added only when implemented and qualified.

`WorkspaceHost` is the real consumer owner required by multiple Workspaces,
MAX_COUNT and aggregate budgets. `Workspace` is its per-Workspace semantic handle,
not another copy of the state. Do not add a manager/factory/facade around each
one. `host.rs` owns registry membership; `state.rs` owns each Workspace's visible
descriptor and submission state; `budget.rs` owns reservations; `snapshot.rs`
owns transitions of G/D1, and `save.rs` owns operation orchestration. One state
machine is recorded, even though focused methods live in separate files.

A host-registry lookup retains the selected Workspace handle and releases the
registry lock before doing work. Reserve/validate/publish under short per-
Workspace state locks, then release them before backing or bridge I/O. Do not
hold either registry or Workspace state locks across reads, writes, Source
delivery, C1/C2 work or cleanup traversal. Mutable access to the existing Client
uses bounded explicit admission: competing work is refused under the selected
profile rather than allowing all mounts to queue behind a client/registry mutex.
An admitted network operation owns its client lease, not every Workspace lock.
R0 must specify these admission and reply/invalidation orderings; a finite
MAX_COUNT does not itself establish independent progress or fairness.

The following are **proposed local signatures**, not current APIs or executable
examples. R0 closes their exact exported types and enabled capability set. Types
use existing bridge roots/Branch IDs/results where appropriate; they do not
duplicate C1 canonical codecs or C5 records.

```text
 WorkspaceHost::new(config: WorkspaceConfig, deliver: OperationDelivery)
     -> Result<WorkspaceHost, WorkspaceError>
 WorkspaceHost::attach(options: AttachOptions, deadline: Instant)
     -> Result<Workspace, WorkspaceError>

 layerfs_fuse::mount(workspace: &Workspace, deadline: Instant)
     -> Result<MountHandle, WorkspaceError>             // Linux-gated binding
 MountHandle::unmount(&mut self, deadline: Instant)
     -> Result<(), WorkspaceError>                     // retains Workspace state

 Workspace::status() -> WorkspaceStatus                 // bounded local observation
 Workspace::commit(deadline: Instant) -> Result<CommitReport, WorkspaceError>
 Workspace::stage(deadline: Instant) -> Result<StageSelector, WorkspaceError>
 Workspace::commit_staged(stage: &StageSelector, deadline: Instant)
     -> Result<CommitReport, WorkspaceError>
 Workspace::discard_stage(stage: &StageSelector, deadline: Instant)
     -> Result<StageDispositionReport, WorkspaceError>
 Workspace::close_clean(deadline: Instant) -> Result<(), WorkspaceError>

 Workspace::edit_file_range(path: &WorkspacePath, edit: RangeEdit,
                            deadline: Instant) -> Result<MutationReceipt, WorkspaceError>
 Workspace::edit_file_ranges(path: &WorkspacePath, edits: &[RangeEdit],
                             deadline: Instant) -> Result<MutationReceipt, WorkspaceError>
```

Method receiver borrows (`&self`) are implicit in the notation above except
where unmount explicitly retains a mutable mount handle. This is a semantic
inventory, not a generated trait with an implementation per deployment.

`OperationDelivery` is one narrow logical-call capability, not a new Client or
provider hierarchy. Its conceptual call shape is:

```text
 deliver(request: &existing Request,
         input: existing bounded Source contract,
         output: bounded logical result sink,
         deadline: caller-local Instant)
     -> Result<existing Response, existing Failure>
```

It preserves the existing Source read/length/cancellation semantics and the
Request/Response/Failure meanings. Native `run.rs` binds this call to the
existing `Client::call_until`; a real direct embedding binds it to the same
authorized service handler rather than another content/history implementation.
The capability owns bounded admission, deadline propagation and typed delivery
failure, and exposes no socket, SQLite handle or pack operation. It must refuse
excess work rather than block all Workspaces on an unbounded client mutex queue.
R0 closes the exact function/closure bounds and shared Source type export; no
alternative source protocol is invented. Native binding belongs in `run.rs` and
minimal source delegation in `source.rs`, already included in the LOC ranges.

| Proposed type | Required contents / ownership |
| --- | --- |
| `WorkspaceConfig` | Validated absolute common root, positive MAX_COUNT, accounted RAM budget, optional R/required W disk quota and one selected capability/resource profile; immutable for host lifetime |
| `OperationDelivery` | One logical request/source/result call boundary with bounded admission and deadline/error preservation; native Client or actual direct-handler assembly stays outside Workspace |
| `AttachOptions` | Validated managed child ID, nonzero authority-supplied producer incarnation, authorized logical Store ID, explicit R root or Branch source, requested access and owner/permission context; no path override or guessed root serial |
| `WorkspacePath` | Validated relative logical path bytes under this Workspace; cannot address the managed parent, private backing or native service paths |
| `RangeEdit` | Checked half-open start/end and exact replacement bytes. A batch uses input order and current-result coordinates, validates its entire accepted effect before publication, and is lowered from final state later |
| `MutationReceipt` | Exact Workspace/incarnation, accepted generation, inode/revision and accepted replacement bytes; means local visibility and required cache coherence only, never C2 save/Commit/durability |
| `StageSelector` | Opaque runtime-owned association of Workspace/incarnation, G, exact token and captured context; not a raw token with authority or a public constructor that adopts another observed stage |
| `CommitReport` | Exact request/G association, known remote Commit/UpToDate context and local reconciliation status. It is not a new history receipt schema |
| `WorkspaceError` | Phase, local publication observation, original typed service Failure/cleanup/history context when present, and any already-known remote success. A local failure after known Commit cannot erase that success or become an invented remote abort |
| `WorkspaceStatus` / `StageDispositionReport` | Bounded local counts/context and exact stage observation; no implicit full history query, rebase, replay or local dirty-state deletion |
| `MountHandle` | Native session/lifecycle owner in the FUSE binding; failed unmount/cleanup remains owned. Dropping a caller value does not silently Commit/discard dirty runtime state |

FUSE uses the same semantic lookup/getattr/open/read/write/namespace/directory
methods. Their logical arguments/results contain no fuser reply, socket, SQL or
pack types. Native reply conversion and notification ordering remain in the
projection binding. Ordinary bounded read results are exposed only after the
selected version's complete terminal result; owned result buffers/view pins
retain their accounting until release. Public lifecycle methods do not expose
raw `freeze`, mutable overlay maps or `reconcile(root)` escape hatches.

`namespace.rs`, `read.rs`, `lower.rs` and `save.rs` compose logical requests at
the point of actual work. Native assembly binds a small logical-call function to
the existing `Client`; `state.rs`, `pieces.rs` and metadata-page algorithms do
not acquire native transport dependencies. `source.rs` implements the existing
bounded file-input capability, not a second client or stream protocol. Init,
Fork and AddLayer remain explicit existing bridge operations used by the caller;
attach does not implicitly create a Branch and Commit does not publish a Layer.

Cluster scheduling, container launch/stop, machine selection, tenant routing and
credential provisioning remain in the executor/orchestration caller. The
filesystem knows only the selected local consumer/Workspace and logical service
identity. No cluster protocol, deployment registry or distributed lock service
enters C1/C2/C5 or the file-piece/index algorithms. Another real projection can
consume these methods later; it must supply its own capability/invalidation
binding and qualification rather than select a second storage implementation.

### 4.2 Mandatory SDK and daemon control binding

For a host SDK controlling a container-mounted Workspace, define and implement
an authenticated **daemon-targeted** binding of the local API. This is a required
dependency before claiming the inherited SDK edit families or host-driven
Docker mount lifecycle. It is not present in the pinned core daemon, and local
Rust API tests cannot satisfy it. [Benchmark route ownership](06-benchmark-qualification-map.md)

```text
 host SDK/executor
      | bounded authenticated Workspace control request / exact input
      v
 container daemon endpoint -> control.rs -> layerfs-workspace API
                                                |
                                   local overlay + layerfs-fuse invalidation
                                                |
                            explicit Commit only -> service C1/C2/C5

 service EditFile alone -> saved canonical root
                         does NOT update the mounted Workspace
```

R0 must select the reachable control endpoint and initiation/lifecycle route,
logical request/response profile and version, authentication and per-operation
authorization, count/byte/in-flight/queue bounds, absolute deadline mapping,
input ownership and original/unknown outcomes. Reuse existing bridge framing,
native authentication, bounded delivery and failure mechanisms through a
reviewed extension in their existing owners. Do not copy the reference SDK's
private snapshot/EDIT wire protocol or add a parallel socket codec. No numeric
opcodes or public network defaults are invented by this plan.

| Daemon control operation | Inputs, identity and result responsibility |
| --- | --- |
| Attach | Explicit AttachOptions plus authorized consumer selection; reserve count/resources and return the bound Workspace identity/capabilities. Daemon derives both paths from its own configured root |
| Mount / Unmount | Bound Workspace/incarnation and declared deadline; daemon owns local kernel/session result and retained cleanup. These requests never go to the host content/history service |
| Status | Bound Workspace identity and bounded result allowance; return local observation only, not an original-operation receipt or automatic remote recovery |
| EditFileRange / EditFileRanges | Workspace/incarnation, validated path, ordered checked edits and exact stable replacement input; validate complete batch, acquire backing, publish one semantic effect and make mounted reads coherent before successful MutationReceipt |
| Commit / Stage | Bound Workspace and complete caller budget; capture internally, return exact G-associated outcome/selector; no caller-supplied arbitrary filesystem candidate root |
| CommitStaged / DiscardStage | Validate opaque local selector against retained incarnation/token/G/context before invoking existing C5 operation; stale selectors do not adopt or discard another stage |
| CloseClean | Bound Workspace identity; refuse mounted, dirty, staged, unresolved or still-used state until the declared clean-close conditions hold; no implicit Commit, force deletion or quota refund |

Each request has explicit operation correlation and producer identity, but no
automatic replay key is inferred. A lost reply after a local edit was accepted
may leave the caller uncertain about visibility, just as a lost remote Commit
terminal may leave publication unknown. Retain original state and observations;
do not rerun the mutation, guess rollback or claim that a Status snapshot is its
missing receipt. Control peer authorization is distinct from the daemon's
service-side Store grants; possession of a Workspace ID, path or content hash
does not authorize the control operation. No arbitrary filesystem path, raw
catalog capability or unlimited upload is accepted from the control caller.

The SDK edit contract is a semantic live-Workspace mutation. Its single/batch
methods share the same write/version/backing owner as POSIX writes but enter
through this control path, **not through write(2) on the mounted path**. Mounted
visibility requires ordered data/attr/dentry invalidation and handling of older
kernel replies/dirty pages; use the selected enforceable kernel mode. Success
must mean the complete declared local edit is visible under that contract; an
error after publication carries the publication observation rather than falsely
reporting no change. Source input and local I/O use the full caller deadline;
timeout/cancellation does not prove a published effect was undone.

Before mapping inherited `Client::edit_workspace_file_range(s)` cases, define
the actual SDK binding/version and independently prove zero **edit-caused FUSE
WRITE callbacks**, complete mounted visibility, subsequent explicit Commit and
reopen correctness. A service EditFile call or POSIX substitute cannot pass that
row. Mixed benchmark families that explicitly combine POSIX preparation and SDK
edits retain that mixed contract; do not misclassify every write in the family
as an SDK edit. The exact reference path is documented in
[05](05-v016-source-comparison.md).

### 4.3 Shared changes and SRP/SOLID checks

The combined three-crate LOC envelope excludes shared API changes. The concrete first control
contract may require one `layerfs-bridge/src/contract/workspace.rs` owner
(planning allowance 180–300 physical lines) and a focused native protocol
`workspace.rs` codec owner (250–450), plus bounded changes to existing contract
exports, request/result dispatch and native client/server matching. These are
selected-operation design allowances, not scaffolding or a second client class.
Exact changed-file totals await R0's finalized schema. A thin protocol mod stays
declaration-only and the same 999/200 ceilings apply.

Live metadata/new-inode/symlink/larger-prepared-input extensions separately
change the existing shared contract/codec and service operation handlers. Their
input/results, identity and completion must be agreed before assigning concrete
implementation totals. Do not count this still-open work as zero or hide it
inside the three-crate planning envelope in §3.

| Design property | Review check |
| --- | --- |
| Single responsibility | Native adaptation, state transitions, extent/page I/O, indexing, lowering, submission and history execution each have one owner; no combined LiveOwner module |
| Explicit extensions | Add one real operation/capability at its public boundary; unsupported required behavior fails explicitly instead of selecting an alternate implementation |
| Behavioral consistency | FUSE and SDK/control use the same semantic mutation methods and coherent publication point, while preserving their distinct entry routes/receipts |
| Small interfaces | Only real caller/I/O boundaries have types or call functions; no trait/factory/facade per helper and no universal backend registry |
| Dependency direction | Workspace algorithms depend on logical requests and owned backing operations; transport/native/cluster assembly supplies connections, paths and notifications; C1/C2/C5 stay service-local |
| Testability | External tests call production APIs and actual routes; no private source include, test-only public export, fake mutation path or injected product-only test hook |

The reference allowed separate Workspaces to retain snapshots while a shared
construction gate serialized construction. The proposed one-retained-G-per-
consumer and narrow caller admission are stricter; they are an isolation and
compatibility trade requiring evidence, not an already-proven improvement.
Reference dedicated ordinary/snapshot/lifecycle admission also provided progress
separation. Replacing it with fewer mechanisms still needs no-starvation/deadlock
proof under slow storage/peer conditions. This plan increases neither caller
concurrency nor the single construction producer to solve those gaps.

## 5. External verification matrix

The following IDs are **document requirement identifiers**, not registered
benchmark case IDs or implemented test names. Each result is currently NOT_RUN
for the new Workspace/FUSE implementation. Tests use public behavior and normal
production paths with independent expected contents/identities.

### Public API, configuration and daemon control

| Requirement | Scenario | Required observable result |
| --- | --- | --- |
| C-01 | MAX_COUNT absent, zero, invalid, exactly full, failed attach and failed close | Configuration/admission refuses correctly; attaching and retained/closing/failed-cleanup entries remain counted until checked release. No unlimited/preallocated registry or early slot refund |
| C-02 | Managed child traversal, symlink escape, duplicate incarnation or overlapping mount/backing path | Refusal before unsafe attachment; both children derive from the one root; no caller path override, common-root mount or recursive backing I/O |
| C-03 | Valid control peer without Workspace permission, or valid daemon without service Store grant | Daemon control and service authorization remain independent; identity/hash/token possession alone grants nothing |
| C-04 | Actual SDK single/batch edit against a container-mounted Workspace | Uses the implemented SDK control route and same Workspace semantic mutation; zero edit-caused FUSE WRITEs, correct mounted bytes/attrs and subsequent Commit/reopen oracle |
| C-05 | Control loss/deadline before input complete, after local publication, or after remote Commit | Exact pre/post-publication and known/unknown context preserved; no retry, false rollback, leaked source ownership or fabricated original receipt |
| C-06 | One operation waits on backing/bridge while another Workspace handles a local request | No registry/state lock is held across I/O; other work progresses or gets the declared bounded refusal. No implicit global mutex queue or unsupported fairness claim |
| C-07 | Oversized/malformed/partial control frames, unknown capability/profile and slow sender/receiver | Existing delivery machinery and reviewed control schema enforce count/byte/deadline/backpressure; no unbounded complete-input accumulation or hidden protocol fallback |
| C-08 | SDK/controller drops a handle or requests unmount/close with G retained | Presentation and runtime ownership remain distinct; no implicit Commit/discard, registry count or physical quota refund while state is retained |

R0 identifies the exact SDK/control API version and deployment route used by
C-04; a direct Rust caller or host service EditFile smoke test cannot replace it.

### Readable mount and lifecycle

| Requirement | Scenario | Required observable result |
| --- | --- | --- |
| R-01 | Mount an explicit valid root | Root attrs and declared entries readable; no eager full-tree/payload acquisition attributed as mount setup |
| R-02 | Fork then mount Branch | GetBranch supplies validated scope/profile/root serial; do not assume serial 1 or use Fork's absent serial |
| R-03 | Wrong root role, missing root, unauthorized Store | Explicit correct failure; no half-live mount or fallback source |
| R-04 | Empty, small, paginated and long-name directory | Complete bounded iteration, stable cookies, no skipped/duplicated names, correct dot entries |
| R-05 | Reads at zero, boundaries and EOF | Exact bytes, partial final range, empty EOF response; no overflow or out-of-range service request |
| R-06 | Symlink and path-kind cases | Correct target/path traversal, wrong-kind/absent distinction, no content failure misreported as a missing name |
| R-07 | Open, duplicate descriptors, forget, release | Lookup references and handle lifetime independent; no early release or leaked owned state |
| R-08 | Appropriate executable/script from mount | Correct permissions and bytes, interpreter/library dependencies declared, real loader behavior tested |
| R-09 | Mutating request on R | EROFS with no state/history mutation; no fake success |
| R-10 | Ordinary user attempts to move/remove mount root | Managed identity remains; parent/lifecycle enforcement checked as well as inner callbacks |
| R-11 | Wrong grants or operation profile | Refused before mutation; legacy grants do not grant history |
| R-12 | Unmount during a delayed response | Admission stops, owned operations follow declared deadline/drain policy, no claim of immediate interrupt support or remote rollback |

### Writable operation semantics

| Requirement | Scenario | Required observable result |
| --- | --- | --- |
| W-01 | Overwrite an existing range | Reads through all same-inode handles/aliases see the installed version; unrelated bytes remain |
| W-02 | Multiple appends | EOF selection plus each installed append is atomic under the supported callback semantics |
| W-03 | Successful O_TRUNC open without later write | Length/tail change occurs at open, with correct metadata |
| W-04 | Truncate then extend, or write beyond EOF | Removed tail never reappears; gaps/extended ranges read zero |
| W-05 | Exclusive create against existing/missing name | Correct existence decision and atomic creation once shared new-inode contract exists |
| W-06 | Unlink with open descriptor | Name disappears; descriptor remains usable; completion never resurrects the name |
| W-07 | Regular hard links | Same inode/data/metadata semantics across names; unsupported directory/symlink hard links refused |
| W-08 | rmdir on file/empty/nonempty directory | Correct type/emptiness failure before mutation; no recursive deletion substitute |
| W-09 | Descendant rename replacement and NOREPLACE | Atomic visible transition; supported flags, type and destination-emptiness rules |
| W-10 | Descendant cycle/alias work ceiling | Bound refusal distinguished as far as public contract permits; no successful local rename followed by a hidden known save refusal |
| W-11 | Writes followed by explicit Commit | Stored bytes and portable metadata reflect the captured mutation; no lost mtime update |
| W-12 | Unsupported sync flags/xattrs/ACLs/optional operations | Declared refusal rather than silently ignored flags, fabricated attributes or durability |
| W-13 | Writable mapping attempt | Enforced refusal or a separately implemented coherent capture path; exclusion text alone is insufficient |
| W-14 | Mutation through a non-kernel runtime control | Kernel data/attribute/entry cache visibility remains coherent under the declared profile |

### Frozen G and live successor

Concurrency review must also establish the lock discipline in
[02](02-overlay-snapshot.md#33-local-commit-serialization): no host-registry lock
nested across Workspace work, no encoding/backing I/O while holding state, and
no reversed nested order between control, capture, completion, cancellation and
cleanup. Owned reservations are not held accounting mutexes. Competing public
operations must complete or refuse under their bounded policy without deadlock;
this is separate from a throughput or fairness claim.

```text
 external test/controller        real Workspace            real service path
 ------------------------       --------------            -----------------
 install known edit D  -------> acknowledge D
 explicit Commit       -------> capture G
 observe public state           G pending  --------------> file/history work
 hold a real boundary                                     bounded wait
 write newer bytes     -------> D1 accepts
 read live view        <------- newer bytes
 release boundary                                        completes G
 observe result        <------- G success
 read live view        <------- STILL newer bytes
 inspect acknowledged root -----------------------------> G's bytes only
```

Use controllable external input/connection boundaries or real public state to
establish order. Do not use sleeps as proof of overlap or add test-only hooks to
production. A response held after service completion can prove client-visible
G/G+1 retention, but it is **not** proof of overlapping C2 save lifetimes; H04
requires its stronger schedule separately.
R3d/R4's non-pausing acceptance also needs S-11 below: local successor progress
while G's actual service save is incomplete. A held terminal response alone
cannot satisfy that requirement. If a causal service-work boundary cannot be
established using public production behavior/observations, record this proof as
NOT_RUN rather than adding product test hooks or inferring overlap from sleeps.

| Requirement | Scenario | Required observable result |
| --- | --- | --- |
| S-01 | Capture spans file/length/attrs and a rename | One coherent local mutation boundary; no split namespace/inode effects |
| S-02 | Write same range again after capture | G retains original captured bytes, D1 shows later bytes |
| S-03 | Modify a G file before its file-save root arrives | D1 uses G's logical file coordinates and resolves only the exact acknowledged version |
| S-04 | G completion while D1 overwrites/truncates/deletes | G's completion preserves every newer revision/tombstone |
| S-05 | Read or directory view begins before completion | Its pinned version/cookie remains valid; resources live until last owner releases |
| S-06 | Delayed metadata result arrives after D1 replacement | It cannot reinstall the old name or retarget a handle |
| S-07 | D1 deletion hides a G/B name | Tombstone suppresses inherited entry; missing overlay key still means inheritance |
| S-08 | Repeated generations while old readers persist | Actual retained versions stay charged; no assumption of only two physical versions |
| S-09 | Capture occurs while a multi-syscall shell command runs | Snapshot satisfies the local acceptance boundary, without pretending the whole command was one transaction |
| S-10 | Second explicit Commit while G is unresolved | Bounded explicit refusal; no queued second construction producer |
| S-11 | D1 write and live read complete before G's actual service save completes | Establish a causal observable boundary during real upload/construction/save; later G root contains only captured bytes and live state retains newer bytes. This proves one real save plus successor progress, not #210 H04's two-save overlap/reverse-failure schedule |
| S-12 | Two Commit callers race for one Workspace; first call returns Staged/uncertain | Exactly one capture; returned/dropped request guard does not release its logical slot; local successor work remains governed by ordinary state/budget admission |
| S-13 | Workspace A retains G while Workspace B in the same consumer requests capture | Preserve the one-frozen-submission consumer ceiling with explicit refusal; idle retention owns no active client/network mutex |
| S-14 | Same mounted Workspace commits A, then B, then A again | Each known own result advances root and expected head together; only the next dirty frontier is submitted; old handles stay valid; no new Fork or full Workspace reconstruction between Commits |
| S-15 | Unmount or clean-close requested with dirty/staged/uncertain state | No implicit Commit/discard; unmount retains owned runtime state and clean-close refuses until its explicit policy is satisfied |
| S-16 | Read/mutation starts before capture but completes afterwards | Read keeps its selected immutable version; mutation inclusion follows coherent publication, not syscall start time. Stale preparation cannot install into the wrong generation; no global operation drain |
| S-17 | Large backed namespace and dirty frontier at capture | Capture rotates pre-reserved immutable roots; no demand-loading of backing pages, full dirty-ID/index scan, payload copy or full namespace scan in the critical section. A generation-root reference retains its metadata graph without listing every page or incrementing every descendant refcount at capture; later traversal/release owns its separate bounded work |
| S-18 | G changes A, D1 changes the same A before G's root arrives, then Commit D1 | First saved root equals G's exact bytes; second equals the composed successor bytes based on exact C_G, including truncate/zero/overwritten-replacement cases. No root-label substitution, lost edit or reupload of unrelated clean files |
| S-19 | Metadata page eviction and old directory/read views while G completes | Views remain bound to immutable backing identities and versions; bounded misses use the proper view; no current-path retarget or eager in-memory rebuild |

### History and failure boundaries

| Requirement | Scenario | Required observable result |
| --- | --- | --- |
| H-01 | Save file roots then StageChanges | StageChanges constructs/saves tree exactly once; no preliminary UpdatePreparedFilesystem stage registration |
| H-02 | Stage success with D1 already dirty | Exact token/context retained; Branch/base and local cleanliness not advanced prematurely |
| H-03 | Known Committed or UpToDate | Reconcile only matching G; no fabricated Commit for UpToDate |
| H-04 | Stale captured head before staging | Preserve local state; no token refresh/rebase and no claim a stage must exist |
| H-05 | Branch advances during G's successful save | Preserve the losing stage/context; conditional Commit behavior remains explicit |
| H-06 | Consumed/stale stage token | StageChanged is not replay success; observed replacement stage never attached to G without exact match |
| H-07 | Unobserved/Absent/Retained/AcknowledgedUnknown | Preserve each distinction independently from Failure.unknown/code/cleanup |
| H-08 | Composite Commit stages successfully then fails | Previously acknowledged stage information survives the error |
| H-09 | Connection lost after mutation may have started | Frozen state retained; no resend, guessed rollback, root-presence proof or cleanup |
| H-10 | Explicit query on another connection to same authority | Current observation is available where supported; not mislabelled an original-operation receipt |
| H-11 | C5 unknown outcome quarantines provider | Reads and writes are refused according to the actual source contract; reconnect does not repair authority |
| H-12 | Existing catalog after service restart | Read-only reopen only; mutations/reservations return ContinuityUnavailable |
| H-13 | AddLayer succeeds | Stack advances, Branch base does not; next publication uses deliberate fork/rebase policy |
| H-14 | Exact-source UpToDate after later publication | Returned existing Layer is not blindly installed as current stack head |
| H-15 | DiscardStage | Only exact row removal; no implicit local revert, content deletion or serial refund |
| H-16 | Known remote Commit succeeds but local backed reconciliation fails | Preserve the known remote result separately from the local failure, retain G/D1/root associations and stop new submission until the explicit local disposition succeeds. Do not turn known remote success into Unknown/abort or resend the Commit |

These H-prefixed document IDs are local requirements, distinct from #210's
historical H01–H14 route labels. Evidence must identify which namespace it uses.

### Resource and capability boundaries

| Requirement | Scenario | Required observable result |
| --- | --- | --- |
| B-01 | Payload fits but copy-on-write metadata does not | Refuse before visibility; account allocated capacity, not just logical bytes |
| B-02 | Many Workspaces share one consumer | One aggregate budget, not 8 MiB multiplied per Workspace |
| B-03 | G, D1, old reads and directory views retain bytes | All ownership remains charged in the correct RAM or disk account, including unreachable-but-pinned inode state; one frozen slot is not a two-version physical limit |
| B-04 | Request count fits but encoded metadata exceeds cap | Refuse complete operation before exposure; do not silently split into more saves |
| B-05 | Maximum edit count/replay bytes, names, inode records | Exact boundary handling; current upstream limits remain independent of local memory |
| B-06 | History page/cursor/failure retained | Charge decoded allocations and capacities; consume bounded pages incrementally |
| B-07 | Slow upload/download and service admission refusal | Bounded buffering/deadline behavior; no unbounded queue or added per-callback dispatch threads. Retain and account for the existing bounded bridge upload thread |
| B-08 | Missing selected platform/kernel/dependency capability | Explicit unsupported outcome; no third-party patch or hidden fallback |
| B-09 | First small overwrite of a large supported immutable base | Inherited ranges stay references; new payload occupies disk extents and only bounded active buffers/metadata in RAM. No file-length allocation/read-to-end or eager namespace copy; required service work remains separately accounted |
| B-10 | FUSE ingress buffer reused after acknowledged write, then capture and overwrite | Accepted payload is completely installed in owned backing before publication/acknowledgement; G/D1 pin exact immutable extents, not a borrowed callback slice or reused ingress buffer |
| B-11 | One surviving byte pins a larger segment; old read survives truncate/unlink | Its full allocated disk extent and any owned resident buffers remain charged in their respective accounts until actual release; logical shrink/unlink does not refund retained storage |
| B-12 | Capture/lower/upload under backpressure while D1 accepts writes | No G payload clone or joined complete-file/replacement vector; bounded scratch and overlapping conversion capacities remain charged; saved bytes equal G |
| B-13 | Sparse extension/gap near and beyond shared input limits | Local zero spans do not materialize the gap; delivery uses bounded buffers, and unsupported logical input length is refused before publication |
| B-14 | Repeated small writes exhaust the declared disk quota or RAM working/metadata allowance | Refuse the next non-fitting mutation; preserve earlier writes and capture/lowering/result headroom. Do not switch backing, auto-Commit, silently evict dirty state or grow either allowance |
| B-15 | Local quota refusal or an externally reproducible supported fallible allocation failure | Previous visible state remains intact; release unused reservations and releasable uninstalled allocations, retaining charges for unreclaimed disk state. Do not label process OOM/kill as a gracefully handled allocator error or add product hooks |
| B-16 | Many tiny edits, opens and directory views; repeated generations | Byte and metadata/count limits both apply; no unbounded recipe/buffer growth, whole-map capture clone or ownership cycle retaining completed state |
| B-17 | Pending payload exceeds the proposed RAM working allowance but fits the selected disk quota | Complete supported writes through bounded windows and backed metadata; pending disk bytes do not become resident byte vectors. If the actual selected quota/API cannot cover the full workload, retain its refusal/NOT_RUN; no smaller passing selection or reference cache-hint equivalence |
| B-18 | Full selected npm installation and subsequent explicit Commit, with large and tiny files | Qualify payload and metadata backing separately within the unchanged RAM target and explicit disk quota. Exercise required create/symlink/metadata behavior and inputs exceeding the current 128-name surface only after its shared contract is extended; otherwise retain NOT_RUN/refusal. Record total allocated/retained state and kernel/container memory, not only payload buffer size; no hidden intermediate Commits or reduced selection |
| B-19 | Backing write progresses while a concurrent read/capture runs | No incomplete extent becomes visible. A successful mutation publishes its complete declared accepted range only after backing completion; capture includes either old or complete new state |
| B-20 | Actual disk-full, partial write followed by failure, backing error or failed metadata append | Preserve the previous visible file/namespace. Keep partially allocated/dead bytes owned and disk-charged until checked release; report original and cleanup failures without claiming rollback from file existence |
| B-21 | Many tiny files, many segments and metadata/completion records | Bound resident metadata/index pages, segment/FD caches, handle/cookie state and completion associations separately. No full HashMap/vector of every file, extent or saved root is rebuilt from backing |
| B-22 | Truncate/unlink followed by last-reference release while an FD still names backing | Distinguish logical live bytes, pinned bytes, allocated/reserved extents, dead/unreclaimed bytes and confirmed released space. OS unlink or a zero logical length alone does not return physical quota |
| B-23 | Repeated overwrites and captures create fragmented/dead extents | Bound metadata and physical allocation growth by declared quotas; either reuse/release through the specified lifecycle or refuse. No hidden compactor or background copy that escapes the measured/accounted operation |
| B-24 | Compaction/relocation only if explicitly selected later | Pin and charge old plus replacement disk/RAM resources, copy completely before installing relocation, preserve readers on old extents and retain failure ownership. If no compaction exists, record that limitation and its quota-refusal behavior |
| B-25 | Disk payload plus metadata cache under slow I/O and pressure | Record daemon anonymous memory, kernel/cgroup file/socket domains, backing I/O and actual resident policy. Fixed buffers or ineffective cache advice cannot be reported as a hard RSS/cgroup bound |
| B-26 | All ordinary capacity is occupied when capture, read or terminal reconciliation needs space | Reserved progress headroom remains available, or admission was refused before accepting incompatible dirty work; no recursive full-frontier allocation or OOM-based backpressure |
| B-27 | Current 129th changed binding or over-32-KiB prepared metadata | Current operation refuses explicitly; only a genuinely implemented/qualified larger shared input may accept the full generation with the declared single logical Commit semantics |
| B-28 | Several hard-link names and metadata-only mutations in G | Count/save each captured changed-payload inode version once; aliases do not multiply payload submissions, and metadata-only changes submit no file bytes. Exactly one filesystem build belongs to the selected StageChanges/composite Commit |
| B-29 | Last generation-root reference drops after a large G completes | No recursive full-graph destruction or metadata-page traversal under the capture/completion mutex. Release work executes in bounded owned steps outside the lock, stays charged until actual release and cannot accumulate an unlimited cleanup queue or add a second construction helper |

### Complete application workload boundary

The npm target is a concrete future workflow, not a claim about every package or
an instruction to run it in this documentation turn. Freeze the Node/npm versions,
package/lockfile graph, install options, required scripts/executables/symlinks,
source/network/cache treatment and expected resulting state before qualification.
Use the complete declared graph; large-file and tiny-file cases are both required.
Do not move required installation or backing work into setup, suppress required
scripts, or change options merely to fit a cap or timing selection.

| Requirement | Scenario | Required observable result |
| --- | --- | --- |
| N-01 | Complete declared install on the actual FUSE mount | Every required namespace/content/metadata operation succeeds through the selected public path, with an independent complete output oracle; a headless file-save loop is insufficient |
| N-02 | Install finished locally, explicit Commit still pending, later supported edits occur | Stable G and live D1 coexist, then first and subsequent acknowledged roots match their respective generations; no auto-Commit hid capacity pressure |
| N-03 | Repeat or modify the installation in the same Workspace | Exact new dirty frontier, modes/symlinks/renames/deletions and open-handle behavior; no remount/Fork or full copy-up to conceal lifecycle errors |
| N-04 | Target requires an unsupported shared input, syscall, mmap/sync behavior or script | Keep the whole target unqualified with the exact blocker; do not reinterpret a narrowed workflow as equivalent npm acceptance |

No size or workload count in these rows becomes a registered benchmark selection
without the separate prospective declaration. An implementation can expose a
smaller supported capability while N-01 remains unqualified, but must report that
distinction plainly.

## 6. Source/API and build verification for later implementation

Run only the checks relevant to the tree actually changed, plus the repository's
required core checks. The commands below are prospective; **none ran for this
documentation task**. Execute Cargo from `core/` so the core profile/configuration
is active, and use a clean owned source tree for sealed evidence.

```sh
# Working directory: the selected implementation worktree's core/
# Keep any CARGO_TARGET_DIR override inside that same worktree.
export LAYERFS_CONSTRUCTION_WORKERS=1
cargo +1.85.1 test --manifest-path Cargo.toml --locked --workspace
cargo +1.85.1 build --manifest-path Cargo.toml --locked --workspace --examples --bins
cargo +1.85.1 fmt --manifest-path Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path Cargo.toml --locked --workspace --all-targets -- -D warnings
python3 tools/check_product_boundary.py
python3 -m unittest discover -s tools -p 'test_*.py'
```

Add `layerfs-workspace` and `layerfs-fuse` as core workspace members only with
real implementation. Their external tests own semantic and mounted contracts,
respectively; daemon tests cover configuration, control and assembly. Review the
actual Cargo dependency graph for the one-way edges in §4.1 and absence of
reference-crate dependencies. Existing boundary checks must cover both new
product paths and retain the 999/200 limits.

The formatter does not resolve dependencies; build/test/Clippy remain locked.
Working directory alone is not a profile proof: RUSTFLAGS or
CARGO_ENCODED_RUSTFLAGS can override the core configuration, including its ARM
crypto selection. Run the standard profile without those overrides in the build
child, or preserve an explicitly selected and recorded alternate profile. Record
the effective configuration and verify matching host/image protocol capabilities;
do not add flags to turn a failed route into an apparent pass.
Add real Linux mounted checks for any mounted behavior claimed. Mac host unit
tests or a headless route do not qualify a Linux mount. The guard checks source
boundaries and physical line limits, not filesystem correctness. Do not restore
`tools/preflight.sh`, invent an equivalent aggregate wrapper, or claim CI green.

A selected new metadata/namespace API needs direct/transport parity through the
same service handler and unchanged canonical semantics. Reuse existing published
dependencies; never patch/vendor/fork a third-party package or registry source.
Keep unsupported required capabilities explicit and avoid error-driven alternate
algorithms, backend switches, refresh/reprepare or mutation replay.

## 7. Measurement is a separate future activity

This packet is not a campaign registration or permission to run performance
cases. A real mount must exist, the selected operation must be supported on both
arms, and the exact prospective declaration must define the allowable claim.

| Declaration item | Required treatment |
| --- | --- |
| Operation | Distinguish mount, first read, execution, explicit Commit, and Layer publication; do not compare mount to ingest/construction |
| Semantics | Same supported callbacks, data/metadata, read fraction, history selection and acknowledgement boundary |
| Workload | Keep the full selected corpus; unsupported caps produce an explicit missing selection, not a smaller workload |
| Setup | Reuse qualified immutable inputs and post-initialization independent copies through the applicable clone contract; setup reuse is not a cold claim |
| Cache | Declare/enforce equally; no pre-touching, hidden warm pages or pooled cold/warm rows; cold-ineligible rows remain visible |
| Samples | One sample per case/arm under the standing owner rule; no best-of selection or warmed reruns |
| Output | Fresh paths; append-only receipts, failures and ineligible attempts retained |
| Identities | Exact product/source/build/dependency/image/harness/workload/input identities and scope |
| Resources | Separate daemon, kernel/cgroup, bridge, host-service and Store domains; lifetime peak is not automatically a phase peak |
| Timing | Product and complete-command boundaries explicit; construction, reads and required transfer stay in the phase that pays for them |
| Budgets | Complete performance command normally at most 15 s; prospectively declared small exception list up to 25 s; verification typically under 15 s with 60 s hard budget |
| Isolation | Per-worktree measurement locks; targets/outputs owned by that worktree; other worktrees may run but interference is recorded and not silently clean evidence |
| Workers | One construction producer for ordinary capture/snapshot/Commit. Namespace init alone retains its documented multi-worker exception and 2.7 s cold target |
| Verification | Separate mode and exact identities; reused qualifying proofs explicitly identified; performance success alone is not release admission |

### 7.1 Work and resource observations for each phase

The [incremental work ledger](03-commit-integration.md#73-end-to-end-work-ledger)
owns algorithm/call-count meanings. The observations below are acceptance inputs
for a later declaration, not existing telemetry field names, new benchmark cases
or measurements made here. They make a claim such as "bounded incremental work"
falsifiable without asserting an unmeasured speedup.

```text
 syscall input -> backing write -> coherent metadata publication
                       |                   |
                  disk bytes          immutable view/dirty index
                                           |
                         short capture ----+---- live D1 continues
                                |
                     paged G lowering / bounded source
                                |
                     file saves at service C1/C2
                                |
                     one tree save + C5 stage/Commit
                                |
                     exact own-result reconciliation
                                |
                  last-reference backing reclamation
```

| Phase / owner | Work, bytes and counts to distinguish | Memory/storage and falsification criteria |
| --- | --- | --- |
| Callback input and local admission / daemon | Requested versus accepted bytes; copied ingress bytes; rejection before/after any backing allocation; prerequisite logical queries | Actual RAM capacities and concurrent reservations, bounded ingress/source/reply windows. A small overwrite cannot reserve or copy the file's full length |
| Payload backing write / daemon | Required new payload bytes, attempted/completed physical writes, partial-write bytes and reserved extent capacity; base bytes copied must remain absent from copy-up | Active buffers/FDs and allocated, pinned, dead and reserved disk space. Data becomes visible only after its accepted range is complete; failure cannot disappear from quota accounting |
| Runtime metadata mutation / daemon | Records and index pages read/written, affected inode/name versions, normalization/count checks, metadata I/O and failed candidate records | Resident index/page capacity and transient old/new pages; no full resident namespace mirror. Publish one coherent root/version only after its required backing records are valid |
| Capture / daemon | Root/frontier references rotated, generation/slot transition and bounded descriptor reservations; report any traversal or retirement separately | No payload copy, full frontier/namespace enumeration or demand-loaded metadata pages in capture. Root ownership must not expand into per-page reference enumeration under the lock |
| G lowering / daemon | Dirty index pages and distinct changed inode versions visited, piece/extent records processed, final edits/changed names, source/zero logical lengths, metadata encoding and skipped clean content | Bounded pages/cursors and result associations; no full dirty-frontier or all-file-root vector. Preserve exactly the declared final-state edit policy |
| Frozen source read and upload / daemon + bridge | Local backing bytes read, exact logical BODY bytes, frames, serial request/result dependencies, backpressure and source lifetime | Fixed source/transport windows plus separately charged existing bridge threads. Commit pays its required backing reads/replay acquisition; pages from earlier writes do not establish a cold claim |
| File construction/save / service C1/C2 | Actual predecessor/comparison reads, reconstructed bytes, emitted/reused objects, whole-file versus chunked route, hash/codec/accept/finish work | Service scratch/replay and Store caches are separate from daemon RAM. Whole-file assembly or broad reconstruction legitimately required by the source remains counted, not hidden as only changed-byte cost |
| Filesystem construction/save / service C1/C2 | One tree update per selected StageChanges/Commit; changed records, authenticated child/sibling pages, alias/cycle/release traversal, retained/emitted roots and C2 checks | Actual bounded service state and limit refusals; small caller input is not proof of O(changes) physical work. No redundant pre-StageChanges tree build |
| Stage/Commit / service C5 | Exact acknowledged boundaries, stage token/context, expected-head checks, created Commit versus UpToDate, Busy/stale/unknown and cleanup observations | Bounded history records/failures; no file-byte upload or C2 save in metadata-only CommitStaged. UpToDate is not a free or replayed earlier operation |
| Own-result reconciliation / daemon | Exact G/version associations examined, backed completion records/pages updated, preserved D1 changes and baseline root/head publication | Account transient descriptors and retained old generations/readers; no whole namespace rebuild, global dirty-map clearing or adoption of a competitor's root |
| Backing reclamation / daemon | Last-reference events, metadata pages visited, actual released extents/files, dead/unreclaimed/reserved bytes, cleanup errors; optional compaction copy work only if separately selected | No recursive full-graph destruction under the state mutex. OS unlink is not proof of physical release while an FD/reference retains storage. Charge old/new relocation allocations together; bounded queued cleanup remains owned |
| Kernel/process domains / actual execution host | FUSE reads/writeback behavior, file/page/socket memory, open FDs, process baseline and competing work | Fixed daemon allocations are not a hard RSS/cgroup bound. An ineffective eviction hint or unavailable phase peak cannot be replaced by zero or by a lifetime peak |

Use cumulative-to-phase differences or true operation-owned counters when
available, with explicit start/end ownership. A process-lifetime maximum is not
automatically a phase maximum. Encoded metadata bytes, decoded allocations,
logical live/pinned bytes, reserved capacities and physically allocated extents
are different quantities. One extent may contain both live and dead logical
ranges: count its allocated capacity once while reporting those ranges separately,
and do not sum overlapping owners into fictitious reclaimed space.

Moving reclamation outside the short state mutex is not moving it outside the
operation/resource account. Use the declared owned cleanup path with bounded
steps and bounded retained backlog; old payload/metadata charges remain until
release is established. Refuse admission when that backlog/headroom cannot fit,
rather than spawn an unbounded background collector or a second construction
producer. The later measurement declaration identifies which operation owns
deferred last-reference work and records every still-retained resource; it may
not hide cleanup to make the timed region look fast.

Bounded I/O windows constrain bytes and queued ownership, not the latency of an
arbitrary synchronous filesystem call. Record actual deadline/cancellation
observation points for local backing and service work. A deadline does not by
itself prove that an in-progress write or remote mutation stopped or rolled back;
retain any incomplete ownership/outcome instead of declaring false cleanup.

Count file submissions by captured inode version, not by pathname or by unique
content hash: hard-link aliases name one inode, while equal bytes in different
operations do not transfer a success receipt. No clean-file reupload and no
duplicate filesystem build are independent caller-path properties to prove.
Metadata-only mutations may still require metadata construction and the normal
tree/history path. Use 03's call formula and separately count logical operations,
BODY frames, useful bytes and physical I/O; a streamed operation is not a single
packet RTT.

Prefer existing operation telemetry, normal public status and external OS/I/O
observation with an independent content/namespace oracle. Record which required
observations actually exist. Missing counters or a causal service-work boundary
leave the corresponding assertion UNVERIFIED/NOT_RUN; do not add test-only
product hooks, create an unbounded per-object trace, or turn source reasoning
into an execution result. Any new production diagnostics need a real bounded
operational purpose and are work in the relevant later implementation round.

### 7.2 Matched workflow and attribution

Compare the same selected mounted operation and acknowledgement semantics on
v0.1.6 and core. Include ordinary writes/backing installation when the selection
is install-and-Commit; a Commit-only selection declares its input-generation and
cache state explicitly and pays all reads/construction it still requires.
Neither moving required metadata work into setup nor leaving a previous write's
pages resident may turn that read cost into a free phase.

The full npm installation, its first explicit Commit, a later dirty Commit and
read/executable behavior are distinct potential selections, not one interchangeable
number. Freeze the selected graph and required phases before running them. If
the complete command cannot fit the standing budget, retain the full selection
as NOT_RUN with its reason or use a qualifying existing receipt under the actual
reuse contract; never shrink the graph or silently divide it into many Commits.

Keep source/input/build/harness identities, cleanup and independent verification
for each matched arm. Per-worktree locks isolate artifact ownership; they do not
isolate the machine's CPU/disk/page cache. Report concurrent work and diagnostic
interference according to the current isolation policy. No new CI job, aggregate
preflight, measurement wrapper or construction producer is introduced.

For #207, a useful first candidate is the same executable above the reference
8 KiB prefetch threshold, first and predetermined repeated execution, with identical
interpreter/library requirements. Count actual kernel callbacks, logical bridge
operations, useful/transferred bytes, read fraction and per-side resource work.
Kernel caching may change later invocation demand; never assume a fixed RPC
count or a guaranteed eager/lazy winner. The reference's old thresholds remain
unjustified for the new topology until the selected policy has evidence.

The exact v0.1.6 tag also has a process-shared **32 MiB immutable range cache**
that can retain returned ranges from files above 8 KiB. That threshold describes
prefetch eligibility, **not an uncached band at every layer**. Verify actual
kernel and userspace cache hits/misses and bridge reads; repeated execution is
not guaranteed to pay storage again. The proposed zero additional userspace
payload cache and 8 MiB Workspace working target differ from that reference
cache allowance, so report this resource/performance trade explicitly rather
than assuming equal cache caps. Historical mechanism notes from another source
pin remain historical; [05](05-v016-source-comparison.md) owns this exact-tag
correction. No reference-cache disabling or new cache budget is selected here.

Keep ordinary FUSE workflow claims separate from SDK-edit families; neither
may impersonate the other's operation surface. The current benchmark hosting
and scoped exceptions must be honored by the eventual declaration rather than
inventing a Docker-owned Store or undeclared topology fallback.

Normative references: [benchmark rules](../../../../../docs/general/benchmark_rules.md),
[benchmark-tree rules](../../../../../benchmark/AGENTS.md),
[runner/reuse mechanics](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
[worktree isolation](../../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md),
[release policy](../../../../../docs/general/release-policy.md).
The current owner direction on per-worktree isolation and single samples
supersedes older machine-global-lock or repetition examples.

## 8. Qualification ledger and completion report

Do not restart or rewrite Pair 2 evidence. At the reviewed source pin, its merged
implementation/remediation had source and route verification, while
[#210](https://github.com/Ephemeral-AI-Lab/layerfs/issues/210) retained:

| Existing qualification item | Still required |
| --- | --- |
| #210 H04 | Real overlapping C2-save lifetimes, reverse completion/failure and retained stage |
| #210 H06 | Independent-stack upload overlap |
| #210 H08 | Actual failures at the acknowledged boundaries, including identical-root failed/unknown versus successful schedules |
| #210 H14 | Full unchanged history-consumer substitution across compatible C1-only, C2-only and combined revisions |

Pair 1 testing may contribute relevant evidence, but a normal mounted pass does
not silently satisfy those stronger schedules. Source reasoning, direct parity,
host routes, Docker routes, resource qualification and performance claims remain
separate evidence categories.

Each completed implementation round records its changed files, operation scope,
public contract used, source pin, exact checks, passing/failing/unrun work,
retained evidence links, known limitations and per-commit production LOC. Do not
mark an unsupported dependent operation complete because its prerequisite passed.

For this documentation consolidation, verification is limited to document links,
anchors, ASCII diagrams, source/policy consistency and whitespace. No product
test, mount, build, measurement or issue-state change is part of the task.

[request-contract]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/request.rs
[prepared-handler]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/filesystem.rs

## Current execution checkpoints, 2026-09-21

R0-R/R1 and the first R1-C Status operation are recorded in [08](08-readable-implementation.md)
and [09](09-daemon-status.md). Subsequent [attribute hierarchy](10-attribute-hierarchy.md),
[registry admission](11-registry-admission.md) and [early-refusal](12-early-refusal.md)
corrections retain their own identities and failures. [R2](13-portable-metadata.md)
now supplies one typed portable metadata save through the shared service. Its
actual native route and refreshed mounted Status pass at the exact recorded
product seal. This closes the metadata-save prerequisite, not mounted writes.
[R0-W/R3a](14-owned-payload.md) now acquire immutable bounded disk inputs with
checked Linux failure ownership. R3b must provide the
maintained disk metadata/piece index before any local file mutation is published.
Remaining control lifecycle/edit/Commit, R3b–R5b and R6 rows stay open.

The [R3b operation record](15-local-range-edit.md) records the implemented local
RangeEdit, maintained disk index, 104-inode public route and failure/resource
proofs. Its wider NOT_RUN rows remain explicit. The next real operation is
Workspace::stage: private R3c capture and the minimum R3d lowering must overlap
as prerequisites of that one public operation, with their acceptance rows kept
separate. A raw public freeze hook or unused capture scaffold is not selected.


The next public operation is now implemented in
[16 — Stage capture and shared save](16-stage-capture.md), from exact parent
`788a63950500e6ba79c6a55dc07f7b84ca0fde89`. Its R3c/minimum-R3d subsets have
actual native evidence: immutable G, local successor progress during an observed
C2 write transaction, disk completion associations, one StageChanges and retained
known/unknown failures. It does not complete the full snapshot/Commit acceptance
matrix. The next operation is CommitStaged and known-own reconciliation, followed
by repeated Commit. The local candidate remains 137 pages; the corrected generation
completion reserve is 208 pages, including the explicit interleaving-safe ledger
bound and a 64-page reconciliation reservation to validate in that next operation.
The stage-only lifecycle retains all post-capture failures and refuses resubmission
or clean close; it supplies no implicit retry/discard/recovery policy.


[17 — CommitStaged and reconciliation](17-commit-staged.md), based on exact parent
`0b2c729bdb3f026f12beb667ccdc15c52853280f`, implements the next public operation.
Known own C5 results install the exact acknowledged base while preserving live D1;
subsequent Stage/Commit submissions use only the current frontier. The compact
reconciliation builder reserves 64 existing fund pages, 26 slot credits and charged
ledger capacity before C5. Twelve actual native selections pass, with the original
successor test-oracle failure retained separately. These are SDK subsets of the
listed S/H/B rows, not mounted S-14 or full truncate/zero S-18 qualification.
Composite Workspace::commit is next; actual UpToDate, explicit failed-state
handling and the remaining mounted/control/npm/R6 prerequisites remain open.


[18 — Ordinary composite Commit](18-composite-commit.md), from exact parent
`6702e31e629ada5e78981b6854e36721e619e65b`, implements Workspace::commit with
shared preparation and one existing HistoryCommand::Commit. It makes no hidden
StageChanges/CommitStaged pair. Clean G reserves its empty descriptor and existing
208-page escrow before capture, then sends empty PreparedChanges through the real
service. Actual UpToDate before/after a changed Commit and clean G with late D1
now pass, along with the selected existing-file/failure/resource SDK subsets.
Composite success has no returned stage token; the local report/selector fields
are optional and observed stage data remains separate. These results do not close
full mounted, namespace, failure-disposition, npm or R6 acceptance.


[19 — Existing-inode resize and zero ranges](19-resize-zero-ranges.md), from parent
`573b4bbd35bdcd5d8fe55c8fc107f8c31cd791c5`, supplies set_len before R4's dependent
truncate/extend callbacks. It uses the shared mutation body and strict tag2 Zero
pieces in the existing64-byte codec; all zero bytes still count against the8 MiB
shared replacement bound. Native S-18 subsets now cover shrink/reextend of G and
overwriting captured zeros before the saved root arrives. The original Q0 test-
oracle failure remains FAIL. Writable handle/open/append and kernel coherence,
full mounted schedules, namespace/npm and R6 remain separate open work.


[20 — Portable open and atomic truncation](20-portable-open.md), from parent
`a5bdc9f1e7e0fa4815ae356a7c5d783315fac649`, adds open_file with explicit portable
read/write/append/truncate options. The existing128-slot vector contains pending
and READY entries; pending IDs are unusable, pin their node, and become READY in
the same publication as truncate-to-zero. Eight native cases pass with original
owner-fixture/lookup-cleanup failures retained. Append is intent only until the
next write operation; no writable kernel callback or mapping/coherence claim follows.
