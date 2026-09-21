# Pair 1 implementation handoff — FUSE, Workspace and snapshot overlay

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Prepared 2026-09-21 after the owner selected separate `layerfs-fuse` and
> grouped `layerfs-workspace` crates. Reviewed source basis:
> `152b9c3a2e8ec2536a1d63601b681e1f7ef34455`; exact v0.1.6 reference:
> `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`. This handoff records no new
> implementation, mounted verification, measurement or durability guarantee.

Use this document as the prompt for the continuing implementation task. The
packet's [implementation plan](04-implementation-and-verification.md) owns the
detailed rounds, file map, proposed APIs and verification IDs; this handoff
directs execution without replacing those contracts.

## 1. Mission and first deliverable

Continue Pair 1 for [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179)
and its matched mounted comparison in
[#207](https://github.com/Ephemeral-AI-Lab/layerfs/issues/207). Implement the
selected architecture, one public operation per round, using the existing
Pair 3 and C1/C2/C5 foundations. Do not restart a general architecture study or
stop after proposing another plan. Close the concrete decisions needed by the
next operation, record them, implement that operation, and verify its real route.

**Start with R0-R, then deliver R1: an actual readable Linux FUSE mount.**
The writable backing/index decisions do not all have to be closed before R1.
Shared read metadata needed by the advertised callbacks does have to exist.
A local library test or headless service request is not a mounted result.

The complete objective remains the coherent writable pipeline: bounded private
disk COW, frozen G with a live successor, incremental repeated Commits, required
shell/npm semantics, and the separately declared matched comparison. R1 is the
first milestone, not completion of Pair 1. Continue in the ordered rounds below;
do not bundle the entire pipeline into one change.

Use subagents for bounded independent work. Suitable initial reviews are the
current shared read/API gaps, Linux mount and lifecycle requirements, and the
Workspace public boundary/resource arithmetic. Give each implementation worker
exclusive file responsibility, tell it that others share the checkout, and
preserve their edits. Keep the parent integrating the selected operation; do not
delegate overlapping copies of the same investigation or implementation.

## 2. Read first and establish the actual source tree

Read the repository [AGENTS.md](../../../../../AGENTS.md),
[core/AGENTS.md](../../../../AGENTS.md), and this packet in this order:

1. [README](README.md): current decisions and source basis.
2. [04 — Implementation and verification](04-implementation-and-verification.md):
   R0 outputs, selected layout, public API and operation-specific acceptance.
3. [01 — Workspace/FUSE contract](01-workspace-fuse-contract.md): complete
   callback inventory, errors, identities, deployment and configuration.
4. [02 — Overlay/snapshot](02-overlay-snapshot.md) and
   [03 — Commit integration](03-commit-integration.md): capture, ownership,
   incremental lowering, exact stages and failure observations.
5. [05 — v0.1.6 source comparison](05-v016-source-comparison.md) and
   [06 — Benchmark qualification map](06-benchmark-qualification-map.md): reuse
   the completed audit and historical evidence; do not re-derive them wholesale.

The requested document destination is
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/docs/architecture/proposal/fuse-workspace-snapshot-overlay`.
**Synchronization update, 2026-09-21:** the packet was committed in local
checkpoint `81ace2778201036e9b1ca3040c63949e595f5971`. Main integration combines
that checkpoint with upstream `b0260df3a2ffc371773cd062feafd4b5e435bf1e`, which
contains bridge, daemon, service and history and later C1/C2 changes. The older
three-crate checkout is no longer the intended implementation starting point.
The source audit remains pinned to `152b9c3a2`; review its delta to the selected
integrated tree, including [configured concurrency and schema 8](../../../../../docs/roadmap/0.1/0.1.7/concurrency-controls.md),
before treating any fixed limit or missing operation in this packet as current.

Before editing product code, inspect HEAD, working changes and the actual
relevant source. An implementation worktree already created at `152b9c3a2`
does not move with main. Inspect and preserve its work before explicitly
integrating the newer main there; do not reset it or overwrite another owner's
changes. Select an isolated checkout when needed and record its actual base.

Carry the committed current packet into the selected implementation tree and
record its identity. A worktree created from an older commit does not gain these
documents automatically. Keep the requested packet updated with design/round
status. Historical source pins and receipts retain their original identities;
do not relabel them as current measurements or restart completed investigations.

## 3. Settled architecture and source ownership

```text
core/crates/
|-- layerfs-daemon/                 existing process and connection assembly
|-- layerfs-fuse/                   selected new projection library
|   `-- src/                       lib, mount, adapter, replies
`-- layerfs-workspace/              selected new semantic library
    `-- src/
        |-- lib.rs                 narrow production API exports
        |-- types.rs               logical options/results/errors
        |-- runtime/               host, state, lifecycle
        |-- filesystem/            namespace, read, write, changes, directory
        |-- overlay/               pieces and snapshot capture
        |-- backing/               budget, segments, metadata pages/index, reclaim
        `-- commit/                lower, source and save
```

Use 04's full per-file map. Add real workspace members/files with their first
implementation; do not scaffold unused writable groups or future platforms.
Root `crates/` remains reference source, including its same-named packages. No
dependency, source include or fallback may silently use the reference product.

```text
                        layerfs-daemon
                       /              \
                      v                v
               layerfs-fuse ----> layerfs-workspace
                kernel types       semantic operations
                mount/replies      live/frozen state + private backing
                                           |
                                  logical operation delivery
                                           |
                                     layerfs-bridge
                                           |
                                     layerfs-service
                                           |
                                      C1 / C2 / C5
```

Both libraries run in the daemon process. FUSE depends on the public Workspace
semantic API. Workspace depends on neither FUSE nor daemon, and does not link
service/storage/history implementations to perform private algorithms. Daemon
assembly binds the existing bridge delivery capability. Reuse authorization,
framing, Source delivery, admission and failure reporting; no second client
protocol or per-canonical-object network requests.

Keep Workspace groups private behind real production exports. Finalize the
portable read/attribute/directory/handle/mutation types needed by FUSE as part
of R0-R. MountHandle and kernel adaptation belong to `layerfs-fuse`. Define the
required bounded coherence binding so SDK edits and mounted reads share ordered
visibility without Workspace importing fuser or an unbounded event bus.

Linux is first. macFUSE and Windows adapters are later work. One mount per
Workspace is selected in the plan; a managed Workspace mount ID cannot be
renamed or removed by ordinary Workspace operations. Descendant rename retains
all type, identity, emptiness, atomicity and cycle checks.

## 4. Ordered implementation rounds

| Round | Deliverable and principal owner |
| --- | --- |
| R0-R | Close the readable public API, required complete attributes/directory/readlink results, kernel capabilities, admission, deadlines, cleanup and daemon-control contract |
| R1 | Workspace read/lifecycle implementation, Linux FUSE binding and minimal daemon assembly; actual mounted read/metadata/handle/failure proofs |
| R1-C | Authenticated daemon-targeted control through existing transport owners, one control operation at a time; required before host-SDK/container-Workspace claims |
| R2 | One portable metadata construction/update operation through the shared service; writes must eventually save their changed timestamps |
| R0-W / R3a | Close concrete writable resource/backing inputs, then implement bounded private payload extents and failure-safe ownership |
| R3b | Bounded metadata pages/indexes and maintained dirty frontier; no full resident namespace mirror |
| R3c | Coherent snapshot G and live G+1 with immutable version/source pins |
| R3d | Bounded lowering, streamed file inputs, exact Commit/stage results and own-result reconciliation |
| R4 | Mounted existing-file write/append/truncate/extend through the complete pipeline, including progress during actual service save |
| R5a | Required new-inode, namespace, metadata, symlink and larger-input operations, each in its own prerequisite/implementation round |
| R5b | Complete declared npm installation, explicit Commit and later incremental Commits through the real route |
| R6 | Separately declared, eligible matched mounted comparison against v0.1.6 |

Move a missing shared prerequisite ahead of the operation that needs it. R5a is
a group of responsibilities, not permission to postpone a dependency needed by
R1/R3/R4. R1-C initially exposes supported read/lifecycle controls; edit/Commit
controls follow their underlying implementation. Local writable work can proceed
independently of network management, but cannot qualify that management route.

At the reviewed pin, Inspect is not by itself a complete FUSE attribute/batching
surface. Prepared updates also have 128 changed names, 128 inode updates,
128 directory records and 32 KiB request metadata; EditFile has 256 edits and
8 MiB replacement input. Confirm the current equivalents. Private disk backing
does not remove these limits. Agree any needed shared extension's inputs,
results, identity and completion semantics in its existing owners. Do not
invent streamed StageChanges support, attach new inodes through an unrelated
bootstrap path, or divide one user Commit into hidden smaller logical Commits.

## 5. Storage, resources and concurrent Commit invariants

One configurable common parent derives both local areas:

```text
LAYERFS_WORKSPACE_ROOT=/layerfs
|-- workspace/
|   `-- <workspace-id>/             application-visible FUSE mount
`-- private-backing/
    `-- <workspace-id>/             owned payload and metadata backing
```

The execution host/container resolves this path. The mount is where the callers'
kernel is; a remote service does not make its local paths available to the
daemon. Never mount the common parent or expose private backing inside the user
tree. Service Store/catalog paths and credentials stay at the service. A
container's localhost is not automatically the service host endpoint.

Proposed startup inputs are `LAYERFS_WORKSPACE_ROOT`,
`LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES`, `LAYERFS_WORKSPACE_DISK_BUDGET_BYTES`
and `LAYERFS_WORKSPACE_MAX_COUNT`. The proposed memory default is 8388608 bytes
of aggregate accounted Workspace working allocation per consumer, not RSS or
cgroup memory. Disk quota for W and Workspace count require explicit positive
values; no numeric defaults are selected. Reuse existing Pair 3 connection/key
settings. Do not add a public knob for every internal segment/page constant.

Before writable backing, record the actual disk quota/reserve equation, segment
and metadata formats, I/O/page/FD limits, progress headroom and last-reference
reclamation rules. Count live, frozen, reader-pinned, reserved, partial, dead
and failed-cleanup resources. No automatic cache growth, compaction, spill
fallback or implicit Commit on close/pressure is selected.

Keep these invariants through every implementation round:

- Opening, first writing, renaming or capturing never copies up a whole file.
  Preserve immutable base references, changed extents and zero ranges. Accepted
  bytes must be owned before local success; failed/partial allocation stays
  accounted until release is established.
- Maintain bounded metadata and dirty indexes during mutations. Capture rotates
  already-maintained roots with reserved descriptors, without payload copying,
  full namespace/dirty-set scans, paging I/O or recursive destruction under the
  short state lock.
- A per-Workspace submission slot serializes its Commit lifecycle. Do not hold
  a state/registry lock across backing I/O or remote save. Supported operations
  continue in live G+1 during actual save, within declared admission bounds.
- Resolve the tighter proposed one-retained-G-per-consumer policy against the
  reference's multi-Workspace schedules before qualifying them. Do not silently
  add workers, queues, retries or consumers to avoid a required refusal.
- Save each captured changed inode/version once, sharing hard-link aliases.
  Metadata-only changes preserve content roots. Lowering uses exact captured
  versions and coordinates; changing a base-root label is not a rebase.
- A local snapshot is not a database stage. C1/C2 first construct and save the
  candidate; C5 stages its root and frozen context. The final C5 transaction
  validates the exact stage, inserts/verifies a Commit, conditionally advances
  the Branch head and removes that stage. C2 and C5 are separate boundaries.
- After known own success, replace G-over-B with the acknowledged R1 while
  retaining live D1. Repeated Commits start there. Preserve known remote success
  even if local reconciliation fails. Unknown results do not authorize replay,
  token substitution, guessed cleanup or discarding later edits.
- AddLayer remains an explicit separate operation. Saving objects or staging
  does not publish a Branch Commit or Layer. Reuse C5 semantics; do not design
  a second history catalog or claim daemon restart/resume recovery.

## 6. Reference lessons and deferred work

Preserve v0.1.6's useful mechanisms: lazy base reads, disk-backed range COW,
immutable retained ranges and live continuation during save. The proposed
improvements remove duplicated transport/behavior, repeated frontier scans,
complete frozen-record materialization, redundant payload copies and duplicate
filesystem construction. They remain requirements to prove, not measured wins.

The exact release has a shared 32 MiB immutable range cache. Its 8 KiB prefetch
cutoff does not make every larger-file read a cache miss. Keep this correction;
repeated exec is useful future evidence, not a guaranteed eager/lazy winner.
No extra userspace content cache/prefetch is selected initially. Treat omitted
optional operations and tighter admission as compatibility/performance tradeoffs.

Preserve authentication, input/budget checks, error/visibility distinctions and
the single construction producer. `init_namespace` alone keeps its multi-worker
exception and its standing 2.7 s cold Init target. No extra save worker or helper
lane is added to pass a benchmark.

Keep MEMORY journal and synchronous OFF. Do not add fsync/fdatasync/sync_data/
sync_all, WAL or crash-durability claims. A write acknowledgement means the
declared local visibility, not durable Commit. Unsupported required behavior
fails explicitly; no third-party patches, vendoring or reference fallback.

Do not close #179, #180, #181, #190, #205, #207 or #210. Do not restart closed
component work or infer that a Pair 1 pass closes outstanding Pair 2 schedules.

## 7. Verification and completion reporting

Use 04's operation-specific verification IDs, with external tests in the owning
crate. Test production APIs and actual mounted/control routes; no inline product
tests, private-source includes, test-only public methods or benchmark-specific
algorithms. Keep production files at most 999 physical lines and declaration/
delegation-only lib.rs/mod.rs at most 200. Apply ordinary concrete modules;
no facade, trait or factory per file.

Run relevant locked Rust 1.85.1 checks from the implementation worktree's core
workspace, plus the required boundary guard/self-tests, as specified in 04 §6
and core/AGENTS.md. Actual Linux mounted proofs are required for mounted claims.
If the necessary Linux/FUSE runtime is unavailable, finish independent source/
API checks, retain a precise NOT_RUN blocker and reproduction instructions,
and do not call R1 or a mounted acceptance row complete.

There is no CI or aggregate preflight gate. Never run or restore permanently
retired `tools/preflight.sh`. No measured performance claim precedes a real
mount. R6 additionally needs its declared matched semantics and eligible
measurement route. Before builds/measurements, read the current
[measurement contract](../../../../../docs/general/benchmark_rules.md),
[benchmark rules](../../../../../benchmark/AGENTS.md),
[runner guide](../../../../../benchmark/fs-bench-pro/QUICKSTART.md),
[isolation policy](../../../../../docs/roadmap/0.1/0.1.7/measurement-isolation.md)
and [release policy](../../../../../docs/general/release-policy.md).

Use one sample per case per arm unless an explicit campaign says otherwise,
fresh output paths and append-only receipts. Keep failures and NOT_RUN rows.
Reuse setup only outside timers; no pre-touching, invented cold state, shrunk
selection or cost shifted into setup. Respect current per-worktree isolation,
keep Cargo targets inside the owning worktree and record interference. Older
global-lock wording does not override the current owner policy. Export
`LAYERFS_CONSTRUCTION_WORKERS=1` for ordinary construction cases.

After each round, update the packet with the actual source identity, changed
files, operation/API scope, exact checks, results, retained failures and next
dependency. For each Git commit, compute and record exact parent-to-commit
production LOC before/after/signed delta, reference/core subtotals and method;
planning ranges or Git diff line counts are not a substitute.

End each handoff with a clear distinction between implemented behavior, verified
routes, resource/performance evidence and still-open prerequisites. Keep moving
on independent authorized work when one proof is unavailable. Do not represent
documentation, compilation, direct API parity or a narrower prototype as the
full writable mounted result.
