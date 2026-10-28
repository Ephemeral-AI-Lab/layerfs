# LayerFS SDK surface

> **Status:** Implemented on `sdk-refactor`; not a released contract or a
> performance qualification. Source layout and size are pinned to `ce4e214d9`.
> The complete SDK migration awaits its consolidated host/Linux and benchmark
> gate. Owner direction of 2026-10-12 governs the implementation and measurements.
>
> Relocation 2026-10-15 ([#327](https://github.com/Ephemeral-AI-Lab/layerfs/issues/327)):
> the crate is renamed `layerfs-sdk` -> `layerfs-api` at `engine/crates/layerfs-api`
> and regrouped as `project/`, `branch/` and `sandbox/`; links below follow the move.
> The layout table stays pinned to `ce4e214d9`. See `core/docs/issues/327/SPEC.md`.
>
> #352 (2026-10-18 and 2026-10-19): Store methods run in the SDK's process.
> `fs/`, `branch/` and `project/list_branches.rs` are again one file per
> JavaScript method; each reads its arguments and calls the layerfs-store-service
> engine directly. The daemon routes described below for fork, branches and
> commits (typed History requests over Control) are retired. Current layout:
> [the SDK call channel](../../core/docs/architecture/82-sdk-call-channel.md).

The SDK provides three groups with eleven operation methods, plus their existing
constructors. A Project is a sealed initial Store and its history. A Sandbox owns
one acknowledged container and its original authenticated control channel. A
Workspace is the complete mutable filesystem view of a selected Branch.

| Group | Operation methods | Constructor |
| --- | --- | --- |
| ProjectApi | init, fork, branches, commits | new() |
| SandboxApi | create, close | new(Docker) |
| WorkspaceApi | mount, exec, commit, status, unmount | new(&mut ManagedSandbox) |

## Implemented Rust interface

The original typed requests, outcomes, runtime owners and exact failure carriers
remain. The following shapes describe the source; they introduce no builder,
option, dependency, crate feature or additional operation.

| Method | Inputs and returned result |
| --- | --- |
| ProjectApi::init | `InitRequest`, caller `Active` timer; returns `SealedProject` or boxed `InitFailure` |
| ProjectApi::fork | `&mut ManagedSandbox`, `ForkRequest`; returns `BranchSnapshot` or boxed `OperationFailure` |
| ProjectApi::branches | `&mut ManagedSandbox`, `LayerStackId`, `Page`; returns `PageResult<BranchRecord>` or boxed `OperationFailure` |
| ProjectApi::commits | `&mut ManagedSandbox`, `CommitHistoryRequest`; returns `PageResult<CommitRecord>` or boxed `OperationFailure` |
| SandboxApi::create | `SandboxCreate`, `&SealedProject`; returns `ManagedSandbox` or boxed `SandboxFailure` |
| SandboxApi::close | owned `ManagedSandbox`; returns `()` or boxed `SandboxFailure` |
| WorkspaceApi::mount | borrowed facade, `WorkspaceId`, `BranchId`; returns `MountedWorkspace` or boxed `MountFailure` |
| WorkspaceApi::exec | associated call with `&ManagedSandbox`, `&MountedWorkspace`, arguments, environment, stdin selection; returns the original `ExecCreated` or boxed `ExecCreateFailure` |
| WorkspaceApi::commit | borrowed facade, `WorkspaceToken`; returns `CommitStagedOutcome` or boxed `OperationFailure` |
| WorkspaceApi::status | borrowed facade, `WorkspaceToken`; returns `WorkspaceStatus` or boxed `OperationFailure` |
| WorkspaceApi::unmount | borrowed facade, `WorkspaceToken`, `force: bool`; returns `Option<ForcedOutcome>` or boxed `OperationFailure` |

Use temporary Workspace facade borrows for filesystem operations; ordinary Exec
borrows the Sandbox runtime independently. Concurrent control callers keep their
original separate authenticated channels. No operation reconnects, resets its
correlation, retries a failure or silently cleans up an uncertain resource.

## Project

[Init](../../engine/crates/layerfs-project/src/store/init.rs) lives in
`layerfs-project::store`, re-exported by `layerfs-api`. The Node binding calls
that same Project implementation directly. Init imports the complete native
tree, including Git metadata, ignored files, dependencies, symlinks, caches and
output. It retains the original eleven-field InitRequest and caller timer: source,
Store configuration, daemon locator, storage policy, catalog authority, stack and
Branch identities/names, inode scope seed and explicit Init deadline. This refactor
does not select new defaults or change the supported Namespace Init profile.
Every permitted new Store explicitly selects Disposable/WAL/synchronous=OFF.
Init publishes its original initial Layer and Branch, seals the sole Store, and
returns a closed file and the actual original acknowledgements.

[Fork](../../engine/crates/layerfs-api/src/branch/fork.rs) preserves the original
explicit ForkRequest and its Layer or Commit source. It does not invent a latest
head lookup or a hidden refresh. The history trunk is a LayerStack, not a Branch;
the first working Branch is named main today.

[Branches](../../engine/crates/layerfs-api/src/project/list_branches.rs) reads one page of a
selected stack in name order, with id, name, base Layer and head Commit. It uses
one authorized additive Bridge request (tag 14), one reply (tag 17) and one daemon
dispatch arm to the existing HistoryCatalog::branches port. The existing control
window is 32 records. The existing authenticated History cursor covers further
pages; the caller advances it explicitly. The SDK checks stack identity, result
size, ordered names and continuation shape without a second request.

[Commits](../../engine/crates/layerfs-api/src/branch/list_commits.rs) renames the public
history method. It sends the unchanged History request and returns one anchored,
bounded ancestry page, newest first. Cursor authentication, captured-head scope,
publication semantics and original control bytes are unchanged.

## Sandbox

[Create](../../engine/crates/layerfs-api/src/sandbox/create.rs) uses the original
explicit deployment/image/volume, private configuration, executable, authentication
keys and startup wait. For a fresh Store it creates, uploads, starts, authenticates,
sends the original initial Hello, installs the sealed Project once, then waits on
the same incarnation for ControlReady. A nonready result is a failure, with the
original observation, runtime, channel and installation acknowledgement retained.
A matching explicit existing_store selection uses the original one-Hello ready
startup and sends no second install. Its manifest must match the supplied Project;
only the populated daemon SQLite version may differ from the host Init manifest.
A mismatched selection fails at Validate before deployment or control effects.

[Close](../../engine/crates/layerfs-api/src/sandbox/close.rs) consumes the owner. It
attempts the original one-second stop once, then deletes once after acknowledged
stop, with force=false and v=false. Stop failure does not attempt delete. Either
failure retains exact runtime, control, original inputs, handshake, observation
and any installation acknowledgement. The failure implements standard Error so
callers can preserve its typed carrier through their existing error interface.
A successful close deletes this container, while the shared Store volume remains
borrowed. Stop/delete acknowledges container lifecycle, not graceful filesystem
or command/descriptor drain. The caller explicitly unmounts Workspaces first.

## Workspace

[Mount](../../engine/crates/layerfs-api/src/sandbox/mount.rs) takes no Project argument;
the Sandbox has already selected its Store. It sends the original bind and attach
requests once each. The first acknowledgement carries the selected Branch/root;
the second carries the mounted directory and native negotiation receipt. A failed
attach retains the bound token in MountFailure. No bind, attach, unmount or rollback
is guessed from a lost reply or attempted again automatically.

[Exec](../../engine/crates/layerfs-api/src/sandbox/exec.rs) delegates ordinary existing
Sandbox execution with the mount as working directory and the deployment's admitted
identity. It returns the original runtime owner for Start, streams, exit status and
explicit cancellation. It sends no daemon control request, registers no filesystem
Exec, applies no ordinary command deadline, and performs no Commit or unmount.
Any other permitted process can use the same mount through ordinary filesystem
calls; the filesystem daemon supervises no commands.

[Commit](../../engine/crates/layerfs-api/src/sandbox/commit.rs) captures the shared
published frontier and returns Committed or UpToDate. It does not filter mutations
by launch route or command completion. [Status](../../engine/crates/layerfs-api/src/sandbox/status.rs)
retains the existing maintained-state scope and never resolves an unknown operation.

[Unmount](../../engine/crates/layerfs-api/src/sandbox/unmount.rs) folds normal and
forced terminal operations into `unmount(token, force)`. Normal success returns
None; forced success returns Some(ForcedOutcome) with the original dispositions.
Normal Busy keeps the mount usable. Public force does not relinquish unknown
Commit custody: its original explicit relinquishment flag is false. The component
proof/recovery helper retains the original explicit flag internally. No unmount
implicitly kills caller-owned processes or deletes global history.

## Internal boundary and benchmark integration

Shared control, failures, installation and recovery live in base/. The hidden
internal module holds raw original-channel access, low-level bind/attach, locate,
cleanup, explicit forced-custody recovery, observation-scoped borrows, channel
work and component-proof startup/query helpers. These support existing tests and
benchmark evidence. Control is absent from the ordinary public method parameters.
Free initialize/install, public Project install, public bind/attach/force_unmount,
public cleanup/locate/with_observations, ManagedSandbox status and ManagedSandbox
exec are removed from the ordinary operation surface.

Mounted benchmark commands use public Workspace exec. Native outside-mount commands
and concurrent disjoint borrows use the hidden delegate to the same original
launcher. Setup and examples use create/close, and setup independently observes
absence of its exact deleted container. The benchmark retains its separate backend
Stop for samples until required closed Store/overlay export and raw telemetry are
harvested. The receipt driver then removes only that sample's container, copies
and exported Store immediately. This proof-only sequence preserves strict closed
Storage readback and adds no public SDK operation.

## Source layout

Scope: all SDK src/ production code at `ce4e214d9`, counted with pinned
`tools/production_loc.py --files` (counter SHA-256 prefix c0fe7f36a0d4). Values are
production LOC, excluding external tests and examples. Each directory subtotal
covers all listed files beneath it. The shared folder is base/.

```text
src/                                             [1444]
  lib.rs                                            19
  base/                                          [644]
    mod.rs                                           7
    control.rs                                     215
    failure.rs                                      51
    install.rs                                     142
    install_types.rs                                44
    internal.rs                                    133
    recovery.rs                                     52
  project/                                       [242]
    mod.rs                                           7
    branches.rs                                     26
    commits.rs                                      24
    fork.rs                                         24
    init.rs                                        161
  sandbox/                                       [362]
    mod.rs                                           4
    close.rs                                        36
    create.rs                                      322
  workspace/                                     [177]
    mod.rs                                           9
    commit.rs                                       16
    exec.rs                                         39
    mount.rs                                        65
    status.rs                                       15
    unmount.rs                                      33
```

SDK tests live under engine/crates/layerfs-api/tests; affected daemon tests and
benchmark runtime tests remain in their owning external test directories.

## Decisions and deferred work

Owner direction, 2026-10-12: complete the SDK migration and documentation before
one consolidated host/Linux/harness gate, one sealed source identity, twelve cells
and a 23-family refactor sample. Compare each case against its own terminal row;
retain failures, unavailable count fields and eligibility. Then simplify the
benchmark and record the final source baseline. No performance optimization.

Working defaults remain explicit: mount takes no Project argument, shared code is
in base/, and exec delegates the existing Sandbox launcher. Publish and a faster
launcher are deferred. The History catalog's add_layer capability does not create
a public SDK publish method or a daemon request here. No new commit metadata,
merge capability, runtime supervision, dependency, feature or Store schema is built.

Simplification retains all five capped-edit cases because their untouched
payload/source-offset oracle is stricter than their twins, with their distinct
Commit watchdog preserved. The expected simplified harness is 157 cases in two
tiers and a 23-family kept-sibling gate; every replacement uses its own terminal
count row. Durable execution remains NOT_RUN — disabled by owner until explicit
reauthorization. Repository history remains NOT_RUN — set aside by owner.
