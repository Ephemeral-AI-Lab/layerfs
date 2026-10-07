# workspace_api.exec — ordinary runtime execution against a ready Workspace

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Reconciled 2026-10-08 at R0 input `1a6bb53ef14e1860d8f222df11394e5a654bb34d`.
> Earlier baseline `f96d97651be5299f153ccde2bc8d921dd58807ad` and design
> `334fc743751b9a181e670d0601a24fb3169208f9` remain historical source identities.
> No implementation, native execution, measurement or qualification is claimed.

This document owns the optional SDK execution convenience and ordinary runtime
boundary. [Mount](mount.md) supplies a complete ready filesystem;
[daemon/SQLite](../daemon-sqlite.md) owns local data, [FUSE](../fuse.md) kernel
requests, and [Commit](commit.md) explicit captured history. Sandbox/external
executor owns commands. Current [S8 specification](../../307/S8-SPECIFICATION-20261008.md)
and [rollout](../../307/ROLLOUT-LEDGER-20261008.md) govern implementation/proofs.

## 1. Ordinary execution contract

[owner requirement, 2026-10-08]

Commands execute as ordinary Bash or ordinary Sandbox/runtime commands, including
commands launched by an external executor. Optional WorkspaceApi.exec selects the
Ready mounted directory and delegates to Sandbox execution. It creates no daemon
command registration or filesystem admission identity. A process with mount
visibility and permissions accesses the same filesystem through ordinary syscalls.

SandboxApi owns actual runtime lifecycle/execution, standard I/O, exit status
and explicit caller-requested cancellation. The filesystem daemon supervises no
commands; no launcher mode, per-Exec cgroups, execution sessions, custom streams,
status or cancellation protocol is added to its control channel.

There is no automatic runtime timeout, command count, lifetime output cap,
command classifier, hidden preparation, change-at-exit hook or implicit Commit,
reset/install/unmount. A build/server/logger can run until actual exit or caller
cancellation. OS/platform bounds and explicit runtime resource admission remain.
A control deadline cannot silently become a shell lifetime limit.

The selected root already contains .git/index, ignored files, dependencies,
symlinks, caches and outputs. Runtime executes the caller's command; it does not
restore/install/reconstruct paths to make a filtered Workspace usable. One
Workspace can serve many sequential/concurrent calls and incremental Commits;
command duration, Workspace lifetime and Commit cadence are independent.

## 2. Inputs, results and independent lifetimes

[proposed public organization; real facades/backend remain R1 work]

| Subject | Contract and actual owner |
| --- | --- |
| Workspace selector/cwd | WorkspaceApi resolves exact Ready incarnation/mounted directory; Sandbox or caller selects authorized cwd |
| Program/environment | Ordinary runtime invocation; private Store/Overlay/daemon credentials excluded by access setup |
| Streams | Existing ordinary runtime stdin/stdout/stderr or caller sinks; bounded buffering and ordinary backpressure |
| Exit/status | Actual runtime exit/signal result, independent of stream EOF/delivery and filesystem owners |
| Lost result | Original runtime process/result custody; never automatic command replay |
| Cancellation | Explicit caller request to runtime/executor; accepted filesystem effects remain published |
| Filesystem access | Permission/visibility based, with no command/parent/Exec identity requirement |

Root-shell exit, pipe EOF, delivered/disposed buffered output, descendant lifetime,
cwd/O_PATH/open descriptors, dirty mappings and FUSE work are distinct events.
No runtime exit/status or zero-command count authorizes filesystem reclamation.
A descendant can retain a pipe or file after its shell exits; stream delivery must
remain exact or explicitly failed/disposed under the runtime owner.

## 3. Launch, I/O and filesystem workflow

[proposed composition]

```text
caller / optional WorkspaceApi.exec -> Sandbox or external executor
                                         ordinary command + mounted cwd
                                         standard streams / actual status
                                                      |
                                         ordinary filesystem syscalls
                                                      |
                                              kernel -> FUSE
                                                      |
                                         Workspace -> Overlay / Store
                                                      |
                                        atomic local publication -> reply
```

Runtime streams use the runtime's standard route, separate from authenticated
filesystem mount/Commit/status/unmount controls. Redirected stdout written to a
Workspace file is ordinary filesystem state and participates in capture; an
output stream is not implicitly inserted into history. No registry/Workspace
lock spans command runtime or stream backpressure.

## 4. Bounded buffers without total-flow limits

Every runtime stream includes queued, executing and blocked-producer bytes in
its own accounting. Slow sinks apply ordinary pipe backpressure; they cannot
cause unlimited collection, silent total truncation or complete-output success
on failed delivery. Filesystem request/payload windows remain independently
bounded without a file/edit/Commit cap. Runtime stream correctness is covered by
prospective FP-6/7/30-Runtime; no daemon stream protocol is built for those tests.

## 5. Concurrency and Commit independence

Several commands can share one Workspace or run in different Workspaces.
Computation/runtime execution is independent; filesystem requests meet at exact
inode ordering, bounded request service and the single fair Overlay SQL owner.
Store writes attempt once; contention returns typed Busy before effects with no
busy handler/wait/retry. No writer serializes whole commands or Commits.

Commit captures the shared locally published frontier regardless of launch route.
Later active writes remain over the new base after known install. An ordinary
syscall split across FUSE requests can straddle capture; mapped stores not yet
published are outside it. Shell exit is not an automatic flush or capture hook.
Nonzero exit does not imply no filesystem changes. The caller chooses explicit
Commit/unmount cadence or keeps a Workspace across calls.

## 6. Failure, cancellation and terminal unmount

Runtime launch/delivery/exit failures retain their original custody. Neither a
lost runtime result nor a lost filesystem reply permits re-execution or guessed
rollback. Explicit process cancellation is requested from Sandbox/executor.

[Normal unmount](unmount.md) probes the kernel while ordinary service continues.
Kernel Busy withdraws the control probe and leaves the filesystem usable. Force
refuses active namespace control producers before effects; otherwise it owns
connection abort, attempted filesystem/daemon-work drain and one plain detach.
It does not kill caller processes or wait for their streams/output/status.
If a caller wants process cancellation and filesystem teardown it explicitly
requests both, preserving their separate results and filesystem effects.

Sandbox setup/external executor owns command identity, actual mount visibility,
Store/Overlay/credential protection and no inherited protected descriptors.
Namespace propagation/isolation needs a proof at actual topology; a working
directory or shared uid is not an adversarial isolation boundary.

## 7. Workloads and acceptance

[prospective validation; no frozen sample or run]

| Case | Workload | Required observation |
| --- | --- | --- |
| E1 | Immediate git/build access on full root | Complete .git, ignored/dependency/cache/output state; no hidden preparation |
| E2 | Many tiny files/wide directories | No dirty frontier cap; indexed bounded work attributed |
| E3 | 100000 scattered edits then Commit | Exact final bytes, no edit-count refusal |
| E4 | Large and sparse files | Windowed flow; no implementation file-size cap; >4GiB actual execution retains owner waiver |
| E5 | Logger/tail/rotation/truncate/unlink-open | Exact lifetime/content, bounded reclaim/read depth |
| E6 | Long command and large standard output | Runtime stream order/backpressure/completion, no automatic timeout/truncation |
| E7 | Several processes/Workspaces/Commits | Finite fair filesystem progress; no daemon command slot |
| E8 | Shell exit with descendants, pipes and open/cwd/mapping references | Independent runtime and kernel/filesystem custody; truthful normal Busy |
| E9 | Explicit cancellation/lost result during mutation/output | Original runtime result and output disposal, retained accepted filesystem effects |
| E10 | Optional SDK execution and external executor on same mount | Identical permissions and filesystem semantics without registration |
| E11 | Same mount across repeated calls/incremental Commits | Coherent live view, later writes and live/idle cleanup; no lifetime/call cap |

Full historical fixture is 130045 entries/3475776149 regular-file bytes; replay
95021 entries/2126509110 bytes. [Mount](mount.md) retains exact manifest/source
pins and limitations. Full-root/mounted survival remains real proof work; source
organization or a small smoke fixture does not close it. Timing follows current
measurement/family/cache rules and prospective registration with distinct runtime
and filesystem observations; harness stops are never product command timeouts.

## 8. Current source versus replacement

At R0 input, active SDK exports qualified Init/install and low-level Control;
it has no real ProjectApi/WorkspaceApi/SandboxApi facades. Active daemon has no
command supervisor or native FUSE executable. Excluded SDK/daemon/Sandbox source
contains old host Server/data paths, capped execution and legacy dependencies;
its organization may be inspected, never source-included or restored.
The [destination layout](../../307/FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md)
lists replacement homes/origins, all proposed until implemented and proved.
