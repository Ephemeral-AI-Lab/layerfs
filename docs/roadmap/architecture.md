# LayerFS roadmap planning

> **Status:** Living planning document. This file is not part of the LayerFS
> 0.1.0 product or compatibility contract.

This document explains how to sequence the work in the
[roadmap checklist](README.md). Version labels are working targets, not
promises. Evidence and compatibility determine when work is admitted to a
release.

## Current position

LayerFS 0.1.0 already has the complete local product loop:

```text
one local SQLite Store
  → immutable LayerStack history
  → named Branch
  → ephemeral Workspace
  → fresh-process execution
  → incremental Commit
  → Add Branch head as the next Layer
```

The current source also has content-addressed canonical objects,
content-defined chunking, persistent extent/tree copy-on-write, bounded
admission, host materialization, managed-container FUSE, Docker lifecycle,
monitoring, a public Rust SDK, a CLI, and retained end-to-end benchmark
evidence.

LayerFS 0.1.0 is the immutable source-only Developer Preview baseline. The
annotated `v0.1.0` tag, verification record, source archives, checksums, and
benchmark evidence are published in the
[release record](../../release-notes/0.1.0/README.md).

## North star

LayerFS should remain the filesystem-history and Workspace-state layer for
parallel agents:

> Ephemeral Workspaces. Durable Shared History.

LayerFS owns:

- canonical filesystem identity;
- CAS, CDC, and COW;
- LayerStacks, Layers, Branches, and Commits;
- Workspace filesystem state;
- capture, reconciliation, Commit, and Add;
- projection contracts;
- Store import/export and future synchronization.

LayerFS should not absorb agent orchestration, Git semantics, network policy,
microVM lifecycle, or model-provider integrations.

## Architectural tracks

Future work should attach along four independent axes:

| Axis | Stable responsibility | Implementations |
| --- | --- | --- |
| Store | Immutable history, facts, canonical objects, integrity, and publication | Local SQLite now; verified bundle and synchronized Store endpoint later |
| Projection | Present one Branch frontier as a filesystem | Materialization, Linux FUSE, OverlayFS, reflink/clonefile, WinFsp, in-process VFS |
| Execution | Run and observe commands against a Workspace | Fresh host process, Docker, Firecracker through Ephemeral Sandbox, possible WASM runtime |
| Integration | Translate external products into the public lifecycle | CLI, Rust SDK, DeepSeek Harness plugin, AgentsGit, future TUI and language SDKs |

Content identity must not depend on projection, execution runtime, or
integration choice.

## Dependency direction

The desired ownership direction is:

```text
agent, harness, CLI, or TUI
            │
            ▼
      public LayerFS SDK
            │
            ├── LayerStackStore
            ├── Workspace lifecycle
            ├── projection binding
            └── execution binding

external integration packages
            ├── Ephemeral Sandbox SDK
            └── AgentsGit SDK
```

LayerFS must not depend on a harness plugin. LayerFS, Ephemeral Sandbox, and
AgentsGit must not share one mutable database.

## Release sequence

### 0.1.x: finish and optimize the existing architecture

The 0.1.x line is the benchmark-completion and evidence-driven optimization
phase for the existing host-native Store, Docker runtime, authenticated daemon,
real Linux FUSE projection, fresh-process execution, explicit Commit, and
explicit End path.

Keep the environment and measurement contract stable: public SDK operations,
prepared containers outside the timer, no Store bind mount, matched
acknowledgement, source/container custody, exact oracles, and fresh evidence
resources. Optimize production code only after passive evidence identifies a
shared root cause. Do not add benchmark-only APIs, persistent workers, hidden
caches, skipped integrity checks, weaker durability, or shifted timing
boundaries.

LayerFS-only focused rows drive iteration and stable-candidate regression
checks.

The 0.1.x benchmark registry is append-only. Once a scenario is admitted, its
fixture, public operation sequence, timing boundary, correctness oracle, and
result schema remain frozen through 1.0.0. Later patch releases add rows and
rerun all earlier rows; they do not rewrite them. A benchmark correction uses a
new scenario ID or schema version and retains the superseded evidence.

### 0.1.0: immutable baseline — complete

The released `v0.1.0` source, manual, verification, artifacts, checksums, and
benchmark record provide the exact behavioral and performance reference for
later work.

### 0.1.1: reliable current path

The first patch release should make one existing public path boringly
reliable, not absorb the complete future roadmap. The selected lifecycle is:

```text
existing directory
  -> LayerStack initialize
  -> Branch fork
  -> Workspace create
  -> localized edit
  -> Commit
  -> End
  -> reopen and verify
```

The [versioned 0.1.1 checklist](0.1/0.1.1/README.md) is the canonical
working state. It first measures initialization and localized Commit separately
across increasing namespace sizes. A defect becomes release work only after
that public-path measurement, a failing check, a compatibility analysis, and a
bounded resource model exist.

Accepted fixes must preserve the frozen schema, identities, canonical bytes,
CDC profile, SDK, CLI, daemon protocol, acknowledgement behavior, and resource
bounds. Use existing builders and mutation state before adding code. Do not
raise a bound merely to postpone whole-tree work.

One focused logical case must also pass through materialization and real Linux
FUSE, followed by one managed Docker lifecycle and one attachment-failure
cleanup proof. This establishes a documented subset rather than claiming
universal POSIX compatibility.

Patch-compatible large/mixed-edit and `copy_file_range` follow-up is tracked in
the [0.1.2 proposals](0.1/0.1.2/README.md). Any item that requires an
incompatible wire, public API, schema, canonical, or identity change moves to
0.2.0 with the broader projection and platform work.

### 0.1.2: payload, mixed-edit, and Store-footprint optimization

After v0.1.1 completes namespace admission, v0.1.2 may accept measured prepend,
range-copy, fragmented-write, sparse-growth, mixed-edit, and
[Store-footprint](0.1/0.1.2/store-footprint-efficiency.md) work. Each item must
use the same public SDK, real-FUSE, Docker, fresh-process, timing, integrity, and
acknowledgement boundaries established by v0.1.0.

Keep the existing registered payload and namespace rows stable. Add a new row
only when it answers a concrete product question, iterate LayerFS-only on the
smallest failing case, and rerun registered LayerFS payload evidence only after
candidate stability. Preserve exact bytes, canonical roots, fresh reopen
results, Store compatibility, and resource bounds. Storage evidence counts the
complete durable Store, not only the SQLite file, and must not trade footprint
for hidden packs, background compaction, CPU, RSS, page cache, or I/O.

### 0.1.3: single-history filesystem workload completion

After v0.1.2 closes the known prepend, capture, and Store-footprint paths,
v0.1.3 benchmarks the
filesystem workload families in the smallest useful history topology: one
LayerStack, one genesis Layer, and one Branch. The topology is a control
variable, not the benchmark subject. New scenarios run a deterministic
filesystem workload through real FUSE, one final Commit or proved `UpToDate`,
End, and exact fresh reopen. Previously registered rows such as `edit16` retain
their frozen multi-Commit sequence; v0.1.3 does not add Commit depth as a scale
axis. The
[0.1.3 README](0.1/0.1.3/README.md) indexes one draft document per family.

Families use geometric 1/10/100 load tiers where applicable, nested seeded
schedules, random unaligned positions, explicit same-count and count-changing
classification, and separate path/inode/byte deltas. Supporting SDK lifecycle
phases remain measured, but standalone control-plane microbenchmarks are not the
v0.1.3 goal. Commit-history depth, Add, multi-Layer Diff, conflict handling, and
Branch fan-out wait for v0.1.4.

### 0.1.4: multi-Layer and multi-Branch history

v0.1.4 extends the same public-path contract to bounded history depth and
Branch fan-out. The [0.1.4 README](0.1/0.1.4/README.md) owns Fork from Layer and
Commit, repeated Commit history, Add outcomes, Layer/Branch Diff, paged Query,
conflict and resolution, head movement, historical reads, reopen, and storage
reuse.

Use a few preregistered geometric profiles rather than a Cartesian matrix.
Every operation receives an evidence-backed disposition; code changes are
required only for measured defects or material opportunities. Move only
incompatible mechanisms to v0.2.0.

### 0.2.0: agent Branch integration and portable projection foundation

A minor release may deliberately extend public contracts. The
[0.2 roadmap](0.2/README.md) has two release-defining objectives: make a Branch
a rapidly iterating multi-agent node or pod while the LayerStack remains main,
and prove that LayerFS semantics survive multiple projection strategies.

#### Agent Branch reconciliation

The [reconciliation main task](0.2/agent-branch-reconciliation/README.md) replaces the
single-writer Workspace assumption with an agent-native integration boundary:

```text
many concurrent Workspaces on one Branch
  -> immutable tool-call Proposals
  -> one ordered Branch integration stream
  -> automatic cumulative reconciliation or structured resolution
  -> validated linear Branch Commit history
  -> explicit Branch checkpoint into the LayerStack
```

Workspace execution remains concurrent. Only candidate acceptance into one
Branch is ordered. A stale head that reconciles cleanly is an internal retry,
not a Git-style manual rebase. Genuine conflicts retain bounded Base,
Incoming, Current, and WorkingTree evidence and remain resumable independently
of the originating process.

The first implementation should reuse existing Workspace mutation state and
the three-root filesystem reconciler. Add schema, operation logs, range-aware
merge, or format-specific adapters only when the frozen correctness matrix
shows the smaller mechanism is insufficient.

#### Projection conformance

Turn the materialization/FUSE behavior matrix into an adapter contract. A
projection must preserve:

- visible filesystem results;
- inode and hard-link semantics within the declared platform surface;
- dirty frontier and capture results;
- canonical root and object identity;
- Commit, reconciliation, and End behavior;
- cleanup and resource bounds.

#### Reflink and APFS acceleration

Treat copy-on-write file cloning as a materialization accelerator, not a new
Store format:

- Linux capability detection for `FICLONE` or filesystem reflink;
- macOS capability detection for `clonefile`/APFS cloning;
- safe streamed fallback;
- identical canonical capture regardless of clone support;
- no hard dependency on a particular host filesystem.

This is the lowest-risk new path because it changes projection cost without
changing durable identity.

#### OverlayFS

OverlayFS should be an alternative Workspace projection whose upper directory
feeds the same canonical capture and Commit path. It must correctly translate:

- whiteouts and opaque directories;
- file and directory rename;
- metadata-only changes;
- hard links and symlinks;
- open-unlink;
- sparse files;
- copy-up behavior;
- extended attributes when supported.

FUSE and OverlayFS must create the same canonical Commit for the same logical
result.

#### Public tooling

Stabilize typed CLI JSON before external tools depend on it. The Rust SDK
remains authoritative; CLI JSON should expose typed IDs, operation outcomes,
pagination, and structured errors without embedding preview strings.

#### 0.2.0 exit gates

- concurrent Workspaces on one Branch with deterministic linear accepted
  history;
- automatic cumulative reconciliation of independent stale candidates;
- changed-read revalidation and structured resumable write conflicts;
- exact post-reconciliation validation before Branch publication;
- explicit Branch/pod checkpoint into LayerStack/main history;
- one projection conformance suite shared by materialization, FUSE, and
  OverlayFS;
- capability-detected clone acceleration with verified fallback;
- stable typed CLI JSON contract;
- unchanged content identity across projection types;
- per-adapter performance, CPU, memory, and cleanup evidence.

### 0.3.0: platform and runtime expansion

This phase expands where Workspaces run without changing what a LayerFS Store
means.

#### In-process VFS

Define a platform-neutral filesystem surface only after the projection
contract is proven. The minimum operations are lookup, directory iteration,
stat, read, write, truncate, create, mkdir, rename, remove, symlink/hard-link
where supported, and explicit Commit.

The in-process VFS can serve:

- browser or serverless execution;
- language interpreters;
- environments without a kernel mount;
- deterministic adapter tests;
- future NFS or virtio-fs bridges.

#### Windows

Use WinFsp rather than calling the effort “Windows FUSE.” Freeze an explicit
mapping for case sensitivity, path separators, reserved names, attributes,
ACLs versus mode bits, symlinks, rename/delete sharing modes, and open handles.
Platform behavior must not silently alter canonical identity.

#### WASM

WASM uses the in-process VFS, not FUSE. Select persistence only after a real
consumer exists. Candidate backends include SQLite WASM, OPFS, IndexedDB,
in-memory Store with verified export, or a remote Store endpoint.

#### Firecracker

Firecracker integration belongs to Ephemeral Sandbox. LayerFS supplies a
Workspace projection and Commit lifecycle; Ephemeral Sandbox owns microVM,
kernel, rootfs, networking, resource policy, devices, pooling, and teardown.

Evaluate virtio-fs or another guest bridge behind the same projection
conformance contract.

### 0.4.0: portable Store and synchronization

Remote work should extend the one-Store model, not restore a two-database
runtime architecture.

#### Step 1: verified offline bundle

Prove portability before adding a network:

```text
LayerFS bundle
  schema and format identity
  selected LayerStack/Layer/Branch/Commit facts
  required canonical objects
  complete-root closure proof
  checksums and source identity
```

Import must preserve ObjectId, CommitId, LayerId, canonical bytes, history,
and scoped names. Missing-only admission should be measured independently for
facts and objects.

#### Step 2: Store-to-Store protocol

A future synchronization protocol should:

1. identify the exact requested frontier;
2. exchange bounded fact pages;
3. exchange ObjectIds before payload;
4. send only missing canonical objects;
5. authenticate every object;
6. prove complete root closure;
7. publish destination pointers last;
8. compare-and-swap destination heads;
9. retain resumable receipts;
10. remain interruption-safe;
11. avoid rechunking, re-encoding, or reminting objects.

Do not freeze the public names `Pull` and `Push` until conflict, force-update,
tracking, and acknowledgement semantics are explicit.

#### Portable-model research

Learn from [AgentFS](https://github.com/tursodatabase/agentfs) where its model
is useful: one understandable SQLite artifact, direct SDK access, simple CLI
onboarding, and explicit portability. Do not copy unrelated key-value or tool
history surfaces without a concrete LayerFS requirement.

## Continuous performance program

Performance is a gate on every phase, not a final cleanup project.

### Benchmark principles

- time public SDK or CLI operations;
- use a fresh process per command;
- prepare containers and fixtures outside the timed region for every product;
- use identical acknowledgement boundaries;
- retain every valid sample;
- record source and runtime identity;
- record cache policy explicitly;
- do not use prewarmed Workspace pools or unfair caches;
- do not add a multi-agent public comparison;
- retain raw evidence and current-source reports.

### Workload expansion

Extend `fs-bench-pro` rather than creating many overlapping harnesses:

| Family | Planned cases |
| --- | --- |
| Create | 1 MiB, 32 MiB, 256 MiB; large file, many small files, mixed tree |
| Edit | overwrite, insert, delete, append, prepend, large replacement |
| Edit density | 1, 16, 256, and 4,096 deterministic edits |
| Trees | directory rename, subtree move, recursive delete, metadata-only updates |
| Identity | hard link, symlink, permission, timestamp, inode-sensitive cases |
| Materialization | cold, warm, and incremental |
| Commit | no-op, one-byte, many-small, and large replacement |
| Cleanup | clean End, discard, cancellation, execution failure, projection failure |
| Resources | CPU, maximum RSS, bytes read/written, transaction duration |
| Storage | physical growth, semantic bytes, object reuse, metadata amplification |

Each adapter must run the applicable same workloads and prove final
canonical-root equality. Sequential read remains a projection diagnostic when
fresh-shell startup dominates the complete user-visible row.

### CAS, CDC, and COW targets

The foundational invariant is described in
[CAS + CDC + COW from first principles](https://learn.layerfs.ai/zh/chapters/01-cas-and-cdc/):
logical states are complete while physical storage remains incremental.

#### CAS

- check IDs before reading payload;
- avoid re-encoding known objects;
- page membership and closure;
- reuse prepared statements;
- report candidate, inserted, and reused counts exactly.

#### CDC

- start from a safe prior anchor;
- scan until a verified resynchronization point;
- reuse the unchanged suffix;
- test insertion and deletion, not only fixed-size overwrite.

#### COW and prepend

Use a persistent rope/extent tree with subtree lengths so a prefix insertion
becomes a splice and path copy rather than a rewrite of every later offset.
The target for a small prepend to a large file is:

```text
new payload       inserted bytes plus bounded resynchronization region
reused payload    nearly all prior chunks
new structure     bounded extent path, file manifest, directory path, root
```

Every optimization must remain deterministic, authenticated, memory-bounded,
transaction-bounded, and independent of hidden warm state.

## Ecosystem plan

### DeepSeek Harness plugin

The DSH plugin is the first integration to dogfood because it exercises the
real public lifecycle. Keep it outside the LayerFS core.

```text
harness run
  → select or create Branch
  → create Workspace
  → bind execution runtime
  → execute fresh process
  → read bounded output
  → Commit or discard
  → report typed IDs and receipts
  → End on every path
```

The plugin must not use private SQL, internal canonical-object APIs, direct
daemon calls, or a persistent shell.

### Ephemeral Sandbox

Ephemeral Sandbox owns Docker and Firecracker lifecycle, CPU/memory/PID
limits, network and egress policy, kernel/rootfs, device attachment, runtime
images, and cleanup. LayerFS supplies filesystem state and receives final
capture/Commit operations.

Runtime choice must not change canonical LayerFS identity.

### AgentsGit

AgentsGit owns Git repository state, Git diff/merge/rebase/review, agent
workflow coordination, and promotion to branches or pull requests. A LayerFS
Commit is not automatically a Git Commit; the integration must map them
explicitly.

### Shared ecosystem context

The projects may exchange correlation IDs without sharing storage authority:

```text
RunId
LayerStackId
LayerId
BranchId
CommitId
WorkspaceId
ExecutionId
SandboxId
GitRepositoryId
GitRef
```

Each project remains authoritative for its own IDs and data.

### TUI

The TUI remains later optional work and stays outside the 0.1.x core. It should
depend only on the public SDK or stable typed CLI JSON.

Start read-only:

- Store and LayerStack browser;
- Branch and Commit history;
- active Workspaces and executions;
- output, operation timings, and deduplication;
- container status.

Add Create, Exec, Commit, End, Add, conflict resolution, and container mutation
only after the underlying APIs are stable. Keep Ratatui/Crossterm or any other
UI dependency out of storage, Workspace, and SDK crates.

## Prioritization rules

When two roadmap items compete, prefer the one that:

1. fixes correctness or cleanup risk in the current public path;
2. proves an invariant needed by several later adapters;
3. improves a measured public operation without weakening semantics;
4. dogfoods the public SDK under a real integration;
5. reduces future platform work without adding speculative abstraction;
6. has bounded CPU, memory, transaction, and operational cost;
7. can be verified with retained evidence.

Defer work that primarily adds surface area without stabilizing a shared
boundary.

## Immediate execution order

The recommended order after the 0.1.0 freeze is:

1. freeze FUSE semantics and conformance cases;
2. refactor Workspace/FUSE/Docker behind those tests;
3. harden cleanup, cancellation, mount teardown, and lease release;
4. keep the full suite below two minutes;
5. optimize dirty planning, CDC resynchronization, persistent extent splice,
   membership, and Commit transactions;
6. extend `fs-bench-pro` and retain current-source evidence;
7. prototype the DSH plugin through the public SDK;
8. add reflink/APFS acceleration;
9. add and prove OverlayFS;
10. define the in-process VFS and expand platforms;
11. prove verified bundles;
12. design Store synchronization;
13. add TUI polish after typed interfaces are stable.

## Explicit non-goals

- No second durable database in one LayerFS context.
- No BranchStore, active Store selector, Reference/Replica placement mode, or
  hidden per-scope object cache.
- No implicit Commit on End.
- No persistent execution shell introduced for benchmark numbers.
- No canonical identity that changes by projection or runtime.
- No Firecracker lifecycle inside LayerFS core.
- No Git merge/rebase semantics inside LayerFS core.
- No TUI dependency in production storage or SDK crates.
- No compatibility promise based only on compiling code or passing unit tests.

## Planning and review process

Before moving an item from the planning document into an active release:

1. identify the measured defect or user outcome;
2. state the public operation boundary;
3. identify compatibility and ownership effects;
4. define CPU, memory, I/O, transaction, and cleanup bounds;
5. add a focused failing correctness test;
6. define real-adapter evidence;
7. define benchmark treatment and anti-cheat controls;
8. record non-goals;
9. assign a target release;
10. update the [roadmap checklist](README.md).

An item is complete only when its code, public API, tests, benchmarks,
documentation, limitations, and retained evidence agree.
