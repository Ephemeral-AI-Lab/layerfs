# Source organization for remaining milestones

> **Status:** Archived; retained for historical source-organization context only.
> Owner requested this recommendation on 2026-10-06. It extends the applied
> active-package organization at local `1775fdf98` and the S6 custody checkpoint
> `cae3d43ed`. Future paths below are recommended destinations, not evidence that
> the modules or their capabilities exist. S6 remains in progress.

Keep implementation grouped by responsibility as cluster two grows. The current
crate boundaries already separate mutable storage, filesystem meaning, native
adaptation, daemon orchestration, transport and host runtime composition. Extend
those boundaries with focused folders for S7 through S13, while finishing S6 in
its existing lifetime, database and maintenance domains.

This is a source-organization design document. It defines recommended locations
and explains ownership; it contains no test campaign or benchmark procedures.
It does not authorize later milestone execution, select new algorithms, change
public contracts, activate excluded packages or retire reference source.

The [original folder proposal](SOURCE-ORGANIZATION.md) and its
[implementation receipt](SOURCE-ORGANIZATION-RECEIPT.md) describe the applied
11-package structure. The [milestone plan](../303/07-implementation-validation.md)
owns deliverables and dependencies; [progress](PROGRESS.md) and
[tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307) own completion.

Current routing2026-10-08: this older source-organization proposal is superseded
by the [reviewed layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md) and
[ownership review](R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md). Its old daemon native/
request-step, host-runtime and API-core trees are historical; do not instantiate
them. Current R1 has12 active members; full native serving remains unimplemented.

## Rules for extending the structure

Keep `src/` roots small: library entry declarations in `lib.rs`, thin executable
entries in `bin/`, and behavior in responsibility folders. Every Rust module
folder needs a thin `mod.rs`; abbreviated trees below omit some unchanged entry
modules and existing files for readability.

`lib.rs` and `mod.rs` retain the 200-line declaration/delegation ceiling. Other
new/replacement production files retain the 999-line ceiling. Split real
responsibilities before reaching those ceilings; a folder is not a reason to add
a wrapper, factory, trait or one file for each small function.

The trees provide default destinations. Add a module when its milestone has real
implementation to place there. Adjacent small responsibilities may remain together
until separation improves ownership or meets the file ceiling; document a
justified naming adjustment. Do not create empty future folder scaffolds.

Preserve public paths through reexports where appropriate. Reuse owning library
types and contracts; moving a source file does not change its public identity.
First-party runtime SQL remains production source. Tests, examples, fixtures,
benchmarks and development tools stay outside product `src/`.

Authoritative mutable state remains in Overlay's SQLite rows and indexes. FUSE,
Workspace and Daemon must not grow competing inode tables, custom mutable trees
or namespace mirrors. Bounded caches and native token bindings retain their own
identity, capacity and release contracts.

Apply the [repository guide](../../../../AGENTS.md), [core guide](../../../AGENTS.md)
and [optimization guide](../../../../docs/general/optimization-guide.md). A
structural move changes neither supported semantics nor measured efficiency.

## Milestone ownership map

| Milestone | Recommended homes | Boundary |
| --- | --- | --- |
| Remaining S0 prerequisites | Existing owning content, provider, runtime, transport and native modules | Resolve contracts in their owners; no new generic coordinator package |
| Remaining S6 | Overlay `database/`, `lifetime/`, `maintenance/`; existing Workspace/Daemon ports | Physical admission, exact ownership and eligible cleanup stay distinct |
| S7 engine cost gate | Existing Overlay `diagnostics/`, Daemon service observations and shared Telemetry | Product diagnostics observe the operational mechanism rather than duplicate it |
| S8 native filesystem and explicit operations | FUSE `mount/`, `requests/`, `dispatch/`, `ownership/`, `coherence/`; Daemon `service/`, `registry/`, `lifecycle/`, `execution/`, `control/` | Kernel conversion stays in FUSE; filesystem rules stay in Workspace |
| S9 host runtime and complete roots | SDK `client/` and `runtime/`; Bridge `contract/`, `codec/`, `native/`; Project `import/`; Sandbox and API-core grouping | Separate application API, wire representation, host authority and library execution |
| S10 incremental Commit | Workspace `commit/` and `ports/`; existing Content construction/edit/filesystem domains | Workspace owns Commit semantics; existing libraries own canonical algorithms, Save and history |
| S11 integration cleanup | Current owning crates, manifests and source/API documentation | Remove superseded routes after replacement coverage; keep the grouped destination |
| S12 integrated qualification | Existing `core/benchmark/` families, registry, diagnostics and retained receipts | Qualification machinery stays outside product implementation |
| S13 reference retirement | Root reference source and obsolete wiring, after qualification | Retain shared build inputs and historical evidence; no new product folder needed |

## Overlay for S6 and S7

Extend the existing Overlay groups. Physical allocation and admission belong in
`database/`. Exact retained owners belong in `lifetime/`. Reclamation eligibility,
serviceable work and progress belong in `maintenance/`. Diagnostics observe those
owners without becoming authoritative state.

```text
core/crates/layerfs-overlay/
├── sql/
│   └── schema.sql
└── src/
    ├── lib.rs
    ├── contract/
    │   ├── mod.rs
    │   ├── types.rs
    │   ├── custody.rs
    │   └── error.rs
    ├── database/
    │   ├── mod.rs
    │   ├── connection.rs
    │   ├── profile.rs
    │   ├── statements.rs
    │   ├── capacity.rs          proposed allocation observations
    │   └── reservation.rs       proposed admission and cleanup headroom
    ├── namespace/
    │   └── existing inode, binding and compound-operation modules
    ├── payload/
    │   └── existing cells, layers, access and composed streams
    ├── lifetime/
    │   └── existing capture, source, file, lookup and operation custody
    ├── maintenance/
    │   ├── mod.rs
    │   ├── ready.rs
    │   ├── garbage.rs
    │   ├── reclaim.rs
    │   ├── orphan.rs
    │   └── debt.rs              proposed cleanup progress and attribution
    └── diagnostics/
        ├── mod.rs
        ├── metrics.rs
        ├── access_plan.rs
        ├── source_plan.rs
        ├── lifetime_plan.rs
        └── allocation.rs        proposed page/allocation diagnostics
```

Capacity and reservation modules supply the selected physical accounting and
one-attempt admission behavior when implemented. Their names do not select a
quota algorithm or imply device headroom already exists. `debt.rs` should contain
real cleanup accounting if it needs a separate implementation home, rather than
a second resident registry of every reclaimable object.

S7 consolidates the operational cost model and existing observations. Native
request and host transport costs have their own owners. Keep their diagnostics
there or in shared Telemetry instead of turning Overlay into a system-wide
metrics service. See [live composition](../../architecture/32-live-composition.md)
and [independent custody](../../architecture/33-independent-custody.md).

## FUSE for S8

Organize the adapter around native attach/detach, request conversion, deferred
reply custody, kernel references and cache coherence. The current FUSE package
is excluded integration source; this tree is its proposed replacement shape.
Actual activation remains an implementation change governed by the milestone.

```text
core/crates/layerfs-fuse/src/
├── lib.rs
├── mount/
│   ├── mod.rs
│   ├── attach.rs
│   ├── detach.rs
│   ├── handle.rs
│   └── options.rs
├── requests/
│   ├── mod.rs
│   ├── adapter.rs               Filesystem callbacks and delegation
│   ├── namespace.rs             checked namespace request conversion
│   ├── file.rs                  checked open/read/write/resize conversion
│   ├── directory.rs             enumeration replies and resume handling
│   ├── attributes.rs            native attribute conversion
│   └── convert.rs               common checked arguments and flags
├── dispatch/
│   ├── mod.rs
│   ├── jobs.rs                  bounded owned native requests
│   ├── replies.rs               exact native reply ownership
│   ├── credits.rs               request/result buffer custody
│   └── disposition.rs           queued/attempted/published/replied state
├── ownership/
│   ├── mod.rs
│   ├── identity.rs              native and canonical inode identity
│   ├── opens.rs                 open/release capability bindings
│   └── lookups.rs               lookup/forget capability bindings
├── coherence/
│   ├── mod.rs
│   ├── attributes.rs
│   ├── pages.rs
│   └── notifications.rs
└── diagnostics/
    ├── mod.rs
    ├── requests.rs
    ├── copies.rs
    └── lifecycle.rs
```

`requests/` validates native arguments and delegates to Workspace. It does not
reimplement rename, append or truncate rules. `ownership/` translates kernel
open/lookup/reference transitions into exact engine custody; it is not another
authoritative mutable filesystem. `coherence/` owns the native cache/reply-ordering
rules, including aliases, EOF, mappings and installation equivalence.

`dispatch/credits.rs` accounts for native buffers and retained replies before
required ownership is allocated. The daemon remains the SQL service scheduler.
`disposition.rs` records exact attempt/outcome states; its existence does not
promise an INTERRUPT callback the pinned dependency does not expose.

Keep platform guards and the selected writeback policy. The folder proposal does
not resolve the signed-timestamp dependency blocker or promote research cache
alternatives. See the [FUSE contract](../303/fuse.md) and
[recorded dependency gate](FUSE-TIME-BLOCKER-20261005.md).

## Daemon for S8 and S9

Keep the current fair SQL service under `overlay/`. Add process composition,
Workspace routing, lifecycle, command execution, explicit control operations and
host demands as separate domains.

```text
core/crates/layerfs-daemon/src/
├── lib.rs
├── bin/
│   └── layerfs-daemon.rs        thin executable entry
├── service/
│   ├── mod.rs
│   ├── run.rs
│   ├── config.rs
│   └── readiness.rs
├── overlay/
│   └── existing SQL owner, queue, credits, commands and ports
├── registry/
│   ├── mod.rs
│   ├── workspaces.rs
│   └── incarnations.rs
├── lifecycle/
│   ├── mod.rs
│   ├── mount.rs
│   ├── unmount.rs
│   └── shutdown.rs
├── execution/
│   ├── mod.rs
│   ├── process.rs              ordinary Bash process ownership
│   ├── streams.rs              stdin/stdout/stderr ownership
│   └── disposition.rs           process exit and retained activity
├── control/
│   ├── mod.rs
│   ├── mount.rs
│   ├── exec.rs
│   ├── commit.rs               routing and operation orchestration
│   ├── status.rs
│   └── unmount.rs
└── upstream/
    ├── mod.rs
    ├── objects.rs
    ├── saves.rs
    ├── history.rs
    └── sessions.rs
```

`control/` handles an explicit request. `lifecycle/` owns native attach/stop/detach
and terminal close coordination. `execution/` owns ordinary processes and streamed
I/O without implicit Commit or automatic command timeout. `service/` establishes
initialized owners and readiness; it does not create a database per Workspace.

`registry/` stores backed or bounded service routing according to its actual
ownership contract. `upstream/` uses authenticated host sessions and demand
capacity. Provider/network waits do not occupy the SQL owner. Daemon control
Commit delegates filesystem construction and outcome semantics to Workspace,
then coordinates its native/runtime boundaries.

The executable is thin composition. Cargo binary target naming and discovery
must be made explicit when the executable is introduced; this proposal does not
rename an existing public command. See [daemon ownership](../../architecture/21-daemon-owner.md).

## SDK for S9

Separate the application-facing SDK from the host runtime that serves the daemon.
Keep initialized ownership, session state, authority binding and existing port
adapters at their current `runtime/` boundary.

```text
core/crates/layerfs-api/sdk/src/
├── lib.rs
├── client/
│   ├── mod.rs
│   ├── project.rs
│   ├── sandbox.rs
│   └── workspace.rs
└── runtime/
    ├── mod.rs
    ├── owner.rs
    ├── sessions.rs
    ├── binding.rs
    ├── types.rs
    ├── error.rs
    ├── ports/
    │   └── existing length and serial adapters
    ├── handlers/
    │   ├── mod.rs
    │   ├── objects.rs
    │   ├── saves.rs
    │   ├── history.rs
    │   └── policy.rs
    └── service/
        ├── mod.rs
        ├── admission.rs
        ├── dispatch.rs
        └── delivery.rs
```

`client/` forwards application operations through supported public boundaries.
`runtime/handlers/` binds authenticated typed requests to existing host libraries.
History decisions stay in History/Persistence and Save mechanics stay in Storage.
The runtime reuses initialized `Handles` and Storage owners rather than opening
the global Store for each call.

The host service has its own process/transport admission and delivery obligations.
Those are distinct from the daemon's overlay queue. Do not duplicate a generic
scheduler inside every library. Handler state must retain exact Save/session and
completion custody across connections and calls without taking a global provider
connection for an entire Commit. See [SDK runtime](../../architecture/22-sdk-runtime.md)
and [runtime integration](../303/06-cluster-one-integration.md).

## Bridge and public API contracts for S9

Preserve Bridge's existing `native/` channel/authentication implementation. Extend
it with actual framing, multiplexing and transport flow, and group wire contracts
and codecs independently from semantic execution.

```text
core/crates/layerfs-bridge/src/
├── lib.rs
├── contract/
│   ├── mod.rs
│   ├── objects.rs
│   ├── saves.rs
│   ├── history.rs
│   ├── control.rs
│   └── outcomes.rs
├── codec/
│   ├── mod.rs
│   ├── requests.rs
│   ├── responses.rs
│   └── limits.rs
└── native/
    ├── existing channel, handshake, I/O, profile and error modules
    ├── framing.rs
    ├── multiplex.rs
    └── flow.rs
```

`contract/` owns wire-level capability and message representation. `codec/` encodes
and validates that representation; native transport delivers it. Reuse semantic
public request/result types where their owner already defines them instead of
adding a second contradictory schema. Host authority decisions remain in the
SDK runtime, filesystem construction in Content/Workspace, and SQL in its engine.

When API-core gains its actual replacement integration, keep public operation
contracts together:

```text
core/crates/layerfs-api/core/src/
├── lib.rs
└── contract/
    ├── mod.rs
    ├── identity.rs
    ├── project.rs
    ├── sandbox.rs
    ├── mount.rs
    ├── exec.rs
    ├── commit.rs
    ├── status.rs
    ├── unmount.rs
    └── error.rs
```

API-core contains public request/result contracts rather than provider or daemon
implementation. Its future membership and dependency direction must follow the
actual types required by the integrated path; these folders do not create a new
dependency cycle or activate the currently excluded package. See
[native channels](../../architecture/23-native-bridge.md).

## Sandbox and complete root acquisition for S9

Group Sandbox's container backend, session ownership and lifecycle. Keep its
deployment responsibilities distinct from the daemon's process execution and
FUSE's native mount adapter.

```text
core/crates/layerfs-sandbox/src/
├── lib.rs
├── owner/
│   ├── mod.rs
│   ├── config.rs
│   └── routing.rs
├── backend/
│   ├── mod.rs
│   └── docker/
│       ├── mod.rs
│       ├── container.rs
│       └── logs.rs
├── session/
│   ├── mod.rs
│   ├── binding.rs
│   └── attachment.rs
└── lifecycle/
    ├── mod.rs
    ├── readiness.rs
    └── teardown.rs
```

These are destinations for real replacement integration, not another backend
abstraction. Keep backend-specific behavior concrete unless a second supported
backend supplies a real requirement.

Faithful initial root acquisition remains in `layerfs-project/src/import/`.
Extend its current scan, namespace, metadata and batch responsibilities as bounded
import, symlink, large-file and raw-name contracts are corrected. Repeated
Workspace bind must not hide import, dependency restoration or full-tree copying.
No second Init implementation belongs in Daemon, Sandbox or the SDK facade.

## Workspace Commit for S10

Add one dedicated `commit/` domain beside the existing Workspace groups. It owns
stable-input construction and the exact phase/outcome semantics of a filesystem
Commit. Existing operation rules and paired base installation keep their owners.

```text
core/crates/layerfs-workspace/src/
├── lib.rs
├── base/
│   └── existing immutable access and cache
├── workspace/
│   ├── mod.rs
│   ├── state.rs
│   ├── view.rs
│   ├── install.rs              existing paired local base transition
│   └── serials.rs
├── mutation/
│   └── existing evaluation, fact acquisition and publication
├── operations/
│   └── existing namespace, attributes and file semantics
├── ports/
│   ├── existing overlay, file and length ports
│   ├── objects.rs              actual immutable runtime boundary
│   ├── saves.rs                actual Save capability boundary
│   └── history.rs              actual stage/publication boundary
└── commit/
    ├── mod.rs
    ├── driver.rs               phase orchestration
    ├── slot.rs                 one pending Commit per Workspace
    ├── capture.rs              stable input and exact custody
    ├── files.rs                file construction through content APIs
    ├── namespace.rs            namespace construction through content APIs
    ├── scratch.rs              operation-backed preparation
    ├── save.rs                 acceptance/finish disposition
    ├── publish.rs              stage and conditional history transition
    ├── resolve.rs              exact failure/uncertainty disposition
    └── outcome.rs              typed operation results
```

Add independently usable runtime port contracts only where the actual boundary
requires them. Reuse existing owning library contracts and concrete types; do
not create a trait for every construction helper. Workspace ports contain no
SQLite or transport-framing implementation.

```text
Daemon control/commit.rs
          |
Workspace commit/driver.rs
          |
commit/capture.rs
          |
commit/files.rs + commit/namespace.rs
          |       public canonical constructors
commit/save.rs ------------------------> host SDK Save handlers
          |
commit/publish.rs ---------------------> host SDK history handlers
          |
existing workspace/install.rs
          |
typed outcome and exact owner release
```

`commit/files.rs` and `commit/namespace.rs` adapt captured final state to public
Content APIs. They do not copy CDC, canonical mapping/tree algorithms or storage
packing into Workspace. `scratch.rs` owns operation preparation through backed
contracts, not a new private page/index engine.

`workspace/install.rs` remains the owner of the paired engine/BaseView transition.
Commit orchestration prepares and invokes it after exact known Save/history
outcomes. This preserves the live view `A over C over R0` as `A over R1`, where
R1 faithfully represents C over R0. No separate installation algorithm is added.
See [continuing Workspace analysis](WORKSPACE-EFFICIENCY-ANALYSIS.md).

`resolve.rs` retains exact refused, conflicted, failed and uncertain custody. A
filename does not supply an unknown-history resolver or authorize replay: any
resolution needs its owning completion fence and policy. `outcome.rs` keeps known
global publication distinct from successful local installation. The
[Commit contract](../303/workspace-api/commit.md) remains authoritative.

## Owning library corrections alongside S9 and S10

Keep canonical changes in the existing library domains. The locations below are
ownership directions; new filenames depend on the real backed interfaces and
algorithms selected through those public contracts.

| Required correction | Owning source domain |
| --- | --- |
| Back deferred file-edit draft/reference/release state | Content `file/edit/` |
| Sparse construction and replacement integration | Content `file/construction/`, `file/cdc/`, `file/mapping/` and owning edit path |
| Stream directory changes and back parent/touched membership | Content `filesystem/rows/`, `filesystem/validate/`, `filesystem/references/` and owning update orchestration |
| Faithful bounded initial acquisition | Project `import/` |
| Save completion, encoding and publication mechanics | Storage `save/`, `encoding/`, `pack/`; provider publication in Persistence `storage/` and `backend/` |
| Stage and conditional history semantics | History `contract/`; provider implementation in Persistence `history/` |

Cluster-one canonical formats remain immutable public representations. Mutable
scratch/backing adapters must respect dependency direction and the SQLite rule;
Content need not gain an engine dependency simply because its inputs become backed.
See the [integration prerequisite map](../303/06-cluster-one-integration.md).

## Integration cleanup qualification and retirement

S11 completes wiring and removes superseded core routes only after their authentic
replacement exists. It does not move legacy algorithms into new folders and call
them the replacement. The grouped destination remains stable while excluded
predecessor packages and retired server integration are removed in their assigned
scope. Root `crates/` stays available for selected S12 reference comparisons.

S12 uses the existing external harness and its registry rather than putting
qualification logic in production or creating another aggregate preflight wrapper.
This layout names locations, not new campaigns, commands or workload selections:

```text
core/
├── crates/
│   └── owning-package/
│       ├── src/                product only
│       ├── tests/              public integration targets and helpers
│       ├── examples/           executable examples
│       └── benches/            package-owned benchmarks where needed
├── benchmark/
│   ├── fs-bench-pro/
│   │   ├── families/
│   │   ├── registry/
│   │   ├── shared/
│   │   └── diagnostics/
│   └── other existing explicitly owning harnesses
├── docs/
│   ├── architecture/           maintained source and API descriptions
│   └── issues/307/
│       └── checks/             milestone-owned append-only receipts
└── tools/                      development checks and migration tooling
```

Preserve Cargo's integration-target discovery when organizing external cases.
Do not move helpers into product source or add source includes to compensate for
test relocation. Dated receipts keep their original paths and source identities;
maintained architecture/API links follow actual source moves.

S13 retires the root reference implementation and obsolete wiring after integrated
qualification. Preserve shared ARM64 build inputs, immutable receipts and
reproducible Git identities. Every implementation/migration commit still records
exact first-parent/committed production LOC, with core/reference subtotals and
honest relocation or retirement classification. This document creates no commit
or source-size claim.

## Final ownership shape

```text
Public API and SDK client       explicit requests and results
             |
Bridge                         wire contracts and authenticated delivery
             |
Daemon                         routing, processes and service lifecycle
             |
FUSE                           kernel request and reply adaptation
             |
Workspace                      filesystem and Commit semantics
             |
Overlay                        authoritative mutable SQL and custody

Workspace construction ------> Content canonical algorithms
Runtime Save/history --------> Storage and History public libraries
Host provider ---------------> Persistence global SQLite
```

The existing active-package organization remains the base. Add the remaining
responsibilities in their owning crates, keep entries thin, and preserve one
authoritative implementation for each filesystem, storage and lifecycle fact.
Folder alignment supports review and maintenance; milestone completion still
depends on its actual implementation and owning exits.
