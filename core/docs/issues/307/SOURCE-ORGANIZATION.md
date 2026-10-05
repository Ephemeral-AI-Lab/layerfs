# Proposed active core source structure

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Owner requested this organization on 2026-10-05. It defines the selected
> destination shape for the active packages; it does not certify that relocation,
> verification or S6 acceptance is complete.

Group implementation by responsibility so that a reader can find mutation
execution, filesystem rules, payload storage, lifetime transitions and service
ownership without searching a flat source directory. Preserve the existing crate
boundaries, public APIs and product behavior. The main implementation chat should
align its structural refactor and subsequent S6 work with this document.

The reviewed committed baseline is local `main` at S6 foundation checkpoint
`32bd3bec0`, following S5 completion `a0dc7da9b`. Responsibility-folder moves were
already underway in the shared working tree when this document was written.
That observation is not a committed relocation or a verification receipt.
[Progress](PROGRESS.md) and [tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307)
retain the actual implementation state and source/evidence identities.

Follow the [repository guide](../../../../AGENTS.md),
[core guide](../../../AGENTS.md),
[optimization guide](../../../../docs/general/optimization-guide.md) and
[implementation ownership plan](../303/07-implementation-validation.md#44-file-ownership).
This organizational direction adds no new algorithm, dependency, database profile,
canonical format or workload limit.

## Scope and organizing rules

Apply the layout to the 11 active packages listed by [core/Cargo.toml](../../../Cargo.toml).
Root `crates/` remains the retained v0.1.6 reference. Excluded legacy/FUSE/sandbox/
server packages retain their current migration status; this refactor does not
activate, reorganize or retire them. Preserve unrelated owner work.

Each `src/` root should expose its entry module, with behavior in responsibility
folders. `lib.rs` and every `mod.rs` contain declarations, imports/reexports,
attributes, API documentation and thin forwarding only, within the 200-line
ceiling. Other new/replacement production files retain the 999-line ceiling.
Required runtime SQL remains first-party production source and stays counted.

Use the destination names shown below. Names in comments identify the original
flat paths. Existing grouped implementations stay in their current domains;
"existing modules" means retain them, not delete or replace them. Every Rust
module folder needs a thin `mod.rs`, including where an abbreviated tree omits it.

Create additional files/folders when real implementation needs them. Keep the
domain boundaries stable as S6 and later milestones grow; do not scaffold empty
future directories or add wrappers/interfaces merely to populate a hierarchy.

## Workspace

Workspace owns filesystem meaning and the effective view. `mutation/` drives
evaluation and publication; `operations/` decides the rules and final effects.
`base/` owns immutable committed access; `workspace/` owns binding/view/install
state; `ports/` describes independently usable owning services.

```text
core/crates/layerfs-workspace/src/
├── lib.rs
├── base/
│   ├── mod.rs
│   ├── view.rs                 formerly base.rs
│   ├── client.rs
│   └── cache.rs
├── workspace/
│   ├── mod.rs
│   ├── state.rs                formerly workspace.rs
│   ├── view.rs                 SourceView and ViewStat
│   ├── install.rs
│   └── serials.rs
├── mutation/
│   ├── mod.rs
│   ├── driver.rs               formerly mutate.rs
│   ├── job.rs
│   ├── eval.rs
│   └── facts.rs
├── operations/
│   ├── mod.rs
│   ├── types.rs                formerly operation.rs
│   ├── attributes.rs
│   ├── namespace/
│   │   ├── mod.rs
│   │   ├── create.rs
│   │   ├── remove.rs
│   │   ├── rename.rs
│   │   └── list.rs
│   └── file/
│       ├── mod.rs
│       ├── read.rs
│       └── write.rs
└── ports/
    ├── mod.rs
    ├── overlay.rs              OverlayRead and OverlayJobs from port.rs
    ├── lengths.rs              FileLengths from port.rs
    └── files.rs                independent file/captured read jobs
```

`driver.rs` retains `Workspace::mutate`: reserve a serial when needed, submit an
owner round, fetch missing immutable facts outside the SQL owner, and return the
one attempted publication's result. `job.rs` evaluates current local rows and
dispatches the operation. `eval.rs` combines those rows with supplied immutable
facts; `facts.rs` performs the base acquisition. Readiness rounds remain distinct
from replay of a failed or uncertain mutation.

The name `NamespaceJob` does not restrict the implementation to namespace changes:
it also serves writes and resize. The structural refactor preserves that public
type and behavior. `attributes.rs` stays directly in `operations/` because mode
and timestamp changes apply to more than regular files.

## Overlay

Overlay owns mutable SQL state, indexed access, transaction atomicity and payload
storage. Separate those responsibilities from exact lifetime custody and bounded
maintenance. `maintenance/garbage.rs` includes the live garbage work introduced by
the S6 foundation checkpoint.

```text
core/crates/layerfs-overlay/
├── sql/
│   └── schema.sql
└── src/
    ├── lib.rs
    ├── database/
    │   ├── mod.rs
    │   ├── connection.rs       formerly db.rs
    │   ├── profile.rs
    │   └── statements.rs       formerly sql.rs
    ├── namespace/
    │   ├── mod.rs
    │   ├── inode.rs
    │   ├── compound.rs
    │   └── names.rs            formerly source_names.rs
    ├── payload/
    │   ├── mod.rs
    │   ├── cells.rs
    │   ├── layers.rs
    │   ├── access.rs           formerly payload.rs
    │   └── stream.rs
    ├── lifetime/
    │   ├── mod.rs
    │   ├── workspace.rs
    │   ├── generation.rs
    │   ├── frontier.rs
    │   ├── source.rs
    │   ├── scratch.rs
    │   ├── close.rs
    │   ├── composition.rs
    │   ├── file_owners.rs
    │   ├── captured_reader.rs
    │   ├── operation.rs
    │   └── orphan.rs
    ├── maintenance/
    │   ├── mod.rs
    │   ├── ready.rs            formerly maintenance.rs
    │   ├── garbage.rs
    │   ├── reclaim.rs
    │   └── orphan.rs
    ├── diagnostics/
    │   ├── mod.rs
    │   ├── metrics.rs
    │   ├── access_plan.rs
    │   ├── source_plan.rs
    │   └── lifetime_plan.rs
    └── contract/
        ├── mod.rs
        ├── types.rs
        ├── custody.rs
        └── error.rs
```

`payload/` retains the effective/raw-read distinction, trimmed cells, validity,
cutoffs and shrink staircase. `lifetime/` owns route/generation/source/capture/
operation transitions and their exact retained knowledge. `maintenance/` owns
ready-work selection and bounded garbage/retirement progress. Diagnostics observe
the actual production queries and executions.

As S6 adds independent orphan custody and physical reservations, place those
implementations according to their responsibilities: custody in `lifetime/`,
physical allocation/headroom in `database/`, and deletion/serviceable debt in
`maintenance/`. This placement does not select their algorithms or qualify them.

## Daemon

Group the current SQL owner library under `overlay/`. These service modules comprise
one service boundary: startup/ownership, scheduling, credits, typed commands and
Workspace adaptation.

```text
core/crates/layerfs-daemon/src/
├── lib.rs
└── overlay/
    ├── mod.rs
    ├── owner.rs
    ├── queue.rs
    ├── credits.rs
    ├── commands.rs
    ├── read_port.rs
    └── file_port.rs
```

Future real registry, lifecycle, execution, upstream and control implementations
can occupy sibling folders as their milestones implement them. Do not create
those directories during a relocation of the current service.

## SDK host runtime

Keep initialized host-library ownership, session state and binding under the
existing `runtime/` boundary. Group the two library-port adapters under `ports/`.

```text
core/crates/layerfs-api/sdk/src/
├── lib.rs
└── runtime/
    ├── mod.rs
    ├── owner.rs
    ├── sessions.rs
    ├── binding.rs
    ├── types.rs
    ├── error.rs
    └── ports/
        ├── mod.rs
        ├── lengths.rs          formerly length_port.rs
        └── serials.rs          formerly serial_port.rs
```

Full Save/history/transport handlers can be grouped further when their real
implementation requires it. Keep the current scoped runtime capabilities and
public exports intact.

## Content

Preserve canonical object, file and filesystem boundaries. Group complete-file
construction while retaining the existing CDC, mapping and localized-edit
domains. `construction/bytes.rs` carries all of the former `file/content.rs`,
including ordinary stream construction, classification and framing helpers.

```text
core/crates/layerfs-content/src/
├── lib.rs
├── contract/
│   ├── mod.rs
│   ├── error.rs
│   └── policy.rs
├── object/
│   └── existing identity, codec, admission, access and output modules
├── file/
│   ├── mod.rs
│   ├── construction/
│   │   ├── mod.rs
│   │   ├── bytes.rs            formerly file/content.rs
│   │   └── runs.rs
│   ├── cdc/
│   │   └── existing gear.rs, zeros.rs and mod.rs
│   ├── mapping/
│   │   └── existing builder, codec, read and repetition modules
│   ├── edit/
│   │   └── existing localized-edit modules
│   ├── read.rs
│   └── view.rs
└── filesystem/
    ├── mod.rs
    ├── attributes/
    ├── directory/
    ├── inode/
    ├── references/
    ├── rows/
    ├── sorted/
    ├── validate/
    └── existing filesystem orchestration and public types
```

Existing public paths such as `file::cdc`, `file::mapping`, `file::edit`,
`filesystem`, `object`, `error` and `policy` must retain their supported surface.
Do not silently break callers because the private source file moved. Frozen
canonical codecs, CDC profiles and hole-root compatibility remain unchanged.

## Storage

Collect the Store handle and shared physical contracts while preserving the
already separated encoding, placement, port, read and Save domains.

```text
core/crates/layerfs-storage/src/
├── lib.rs
├── store/
│   ├── mod.rs
│   ├── handle.rs               formerly storage.rs
│   ├── policy.rs
│   ├── location.rs
│   ├── source.rs
│   └── error.rs
├── encoding/
│   ├── existing encoding modules
│   ├── delta/
│   └── pool/
├── pack/
├── port/
├── read/
└── save/
```

Keep Storage engine-independent. Moving the contracts does not move physical
provider implementation into this package or change saved-object publication.

## Persistence

Separate opening/owning the Store from storage-provider publication. Preserve
backend-specific SQLite, semantic history, metadata and object domains, and the
current shipped SQL directory.

```text
core/crates/layerfs-persistence/
├── sql/
│   └── sqlite/
│       ├── existing schema SQL
│       └── queries/
│           └── history/
└── src/
    ├── lib.rs
    ├── store/
    │   ├── mod.rs
    │   ├── config.rs
    │   ├── handles.rs
    │   └── open.rs
    ├── storage/
    │   ├── mod.rs
    │   ├── provider.rs          formerly storage_provider.rs
    │   └── publication.rs
    ├── backend/
    │   └── sqlite/
    ├── history/
    ├── metadata/
    └── objects/
```

Keep exact profile/platform guards and global Store guarantees. Update relative
SQL include paths without editing query semantics or dropping guard coverage.

## Project

The current Project implementation is a native import/Init workflow. Keep its
orchestration, scan, bounded constructor messages and namespace/attribute
construction together under `import/`.

```text
core/crates/layerfs-project/src/
├── lib.rs
└── import/
    ├── mod.rs
    ├── init.rs
    ├── scan.rs
    ├── batch.rs
    ├── namespace.rs
    ├── metadata.rs
    ├── work.rs                 formerly namespace_work.rs
    └── error.rs
```

This move does not remove existing import limitations or qualify faithful bounded
initial root acquisition. Those remain their owning implementation prerequisites.

## History

The active History package owns portable semantic contracts, identities, requests,
outcomes and cursors. SQL implementation remains in Persistence.

```text
core/crates/layerfs-history/src/
├── lib.rs
└── contract/
    ├── mod.rs
    ├── catalog.rs
    ├── identity.rs
    ├── records.rs
    ├── query.rs
    └── error.rs
```

## Telemetry

Use the existing timing, output, runtime and platform boundaries. Move the loose
loss counters to output and process-window/operation recording to runtime.

```text
core/crates/layerfs-telemetry/src/
├── lib.rs
├── timer/
│   └── existing scope, recording, formatting and report modules
├── output/
│   ├── health.rs               formerly root health.rs
│   └── existing collector, encode, queue and retention modules
├── runtime/
│   ├── operation.rs            formerly root operation.rs
│   ├── observation.rs          formerly root observation.rs
│   ├── monitor.rs
│   └── session.rs
└── platform/
    ├── linux.rs
    ├── macos.rs
    └── mod.rs
```

Telemetry remains observational. Its move creates no new fake clocks, test-only
product hooks, resource enforcement or evidence claims.

## Bridge

The current Bridge already follows the selected shape. Preserve its independently
usable native channel/authentication boundary.

```text
core/crates/layerfs-bridge/src/
├── lib.rs
└── native/
    ├── mod.rs
    ├── channel.rs
    ├── handshake.rs
    ├── io.rs
    ├── profile.rs
    ├── types.rs
    └── error.rs
```

Future logical framing, multiplexing and operation contracts may introduce real
sibling modules. The folder guide does not implement or qualify those capabilities.

## S4 to S6 ownership and the FUSE boundary

| Area | Milestone responsibility |
| --- | --- |
| Workspace `operations/namespace/` | S4 namespace semantics and enumeration |
| Workspace `operations/attributes.rs` | S4 mode/time plus S5 size/truncate |
| Workspace `operations/file/` | S5 composed reads and atomic append/overwrite |
| Workspace `mutation/` | Shared S4/S5 evaluation/publication machinery; later ownership integration preserves it |
| Overlay `namespace/` and `payload/` | Indexed S4 effects and S5 byte representation |
| Content `file/construction/runs.rs`, `cdc/`, `mapping/` | S5 canonical zero-run construction and compatibility |
| Overlay `lifetime/` and `maintenance/` | S6 composition, exact custody, last-owner gates and bounded live/terminal work |
| Overlay `database/` | S6 physical reservation/headroom, in addition to existing connection/profile work |
| Daemon `overlay/` | Fair service, maintenance progress, admission and retained outcomes |
| Workspace `workspace/` and ports | Binding/source/install foundations and S6 ownership integration |

FUSE owns kernel callbacks, argument/flag conversion, native inode/handle/reply
custody and cache coherence in `layerfs-fuse`. Workspace owns create/remove/rename/
write rules so direct library callers and future mounted callers share one
semantic implementation. The current excluded FUSE adapter retains predecessor
bindings; S8 owns its real replacement integration.

## Tests documentation and physical source

Keep production source under `src/` and required runtime SQL in the declared
shipped-source scope. External tests, fixtures, helpers, examples, harnesses and
development tools retain their existing ownership outside product source.

Cargo discovers top-level integration-test entry files. Preserve target names
and paths during this source reorganization; the request does not require nesting
every test file. Any later test move must explicitly maintain Cargo discovery,
all target coverage and public-library execution. Do not introduce source includes,
test-only public APIs or wrapper crates to compensate for a file move.

Update current first-party imports, examples, include paths, guard/tool path
assumptions and architecture/API/handbook links affected by the moves. Historical
receipts retain their original source identity and paths; they are evidence of
that snapshot and must not be relabeled as current verification. Document moved
paths in current guides rather than rewriting immutable raw evidence.

## Implementation and verification checkpoint

The main implementation agent should finish a coherent in-flight checkpoint,
then apply and verify relocation as a separate source-organization checkpoint.
After that checkpoint, continue the original remaining S6 assignment and stopping
boundary. Relocation does not complete S6 or authorize later milestones.

Preserve public module paths through thin reexports where needed. Check field,
type and trait identities and visibility, not just whether the crate builds.
Public root interfaces must not change merely because private files were grouped.

For the code refactor, build first with `--no-run`, run covering package/all-target
tests under explicit wall deadlines no greater than 120 seconds, and run required
warning-denying Clippy, formatting, product-boundary/tool checks and owning
platform checks. Diagnose any failure before a justified repair/rerun. Source
relocation alone requires no new performance campaign; retain prior measurements
at their actual unchanged mechanism scope without relabeling them.

Every local commit must include the exact first-parent/final-staged production
LOC comparison with core/reference/combined subtotals and committed-tree
confirmation. Use the existing counter/method and review classification of moved
source/SQL. Module declarations and reexports count; test/docs/tool lines do not.
Report relocation honestly, including any small declaration growth. Preserve
root-reference and excluded-source accounting until their authorized retirement.

## Alignment checklist

- [x] All 11 active packages follow the destination folders above, with any justified adjustment recorded explicitly.
- [x] Workspace filesystem rules and FUSE/kernel adaptation remain in their owning crates.
- [x] New S6 modules have appropriate lifetime, payload, database or maintenance homes; no empty future scaffolds exist.
- [x] Public API/module paths, trait/type identities, platform behavior and production algorithms are preserved.
- [x] Entry modules obey the declaration/delegation and size rules; SQL remains covered and counted.
- [x] Tests remain discoverable, examples compile, and current source/documentation links match the relocated tree.
- [x] Required covering checks and actual platform scope are recorded, with failures/gaps retained.
- [x] The standalone organization commit records exact production LOC and the counted committed tree.
- [x] The main agent then continues the remaining S6 exits without claiming completion from organization alone.

These boxes belong to the owning implementation checkpoint. Creating this guide
does not mark them passed, modify product source or create a completion receipt.

Alignment evidence: local-only organization commit `1775fdf98f2207563ea6335bdd2af1cb4e2e8704`, tree `bb389a65a0cf5da26332ce0b288c4199d535d432`. [Receipt](SOURCE-ORGANIZATION-RECEIPT.md) and retained checks support the boxes above. S6 continues; no S6 or release acceptance follows from relocation.
