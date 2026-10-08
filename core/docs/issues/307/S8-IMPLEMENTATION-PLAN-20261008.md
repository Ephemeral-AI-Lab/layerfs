# S8 implementation plan: deepest files, activation and checkpoints

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-08 on local `main` at `32d969776` (product source pin
> `f0797c646`). Source/membership refreshed at `5be93f6d7`; R1 is verified.
> The native additions remain proposed and unproved. The [S8 specification](S8-SPECIFICATION-20261008.md) owns every
> decision; the [proof plan](S8-PROOF-PLAN-20261008.md) owns every proof row
> named here. File names for new source are proposals an implementer may adjust
> within the stated responsibility and line ceilings; the reuse and ownership
> columns are not adjustable without a specification change.

Later owner authorization2026-10-08 permits the [scoped fuser lifecycle extension](FUSER-LIFECYCLE-DECISION-20261008.md).
It supersedes this plan's timestamp-only dependency restriction for that exact
entry/exit/startup/join API. The original archive/timestamp pins remain unchanged;
the lifecycle delta has separate exact provenance. Product R2–R5 proofs remain.

Review correction after `77cf51686`, 2026-10-08: follow the revised
[specification](S8-SPECIFICATION-20261008.md) and
[correction ledger](checks/s8-spec-review-fixes-20261008/02-correction-ledger.md).
Original reviews and receipts remain unchanged. These are corrected proposals
and prospective oracles, not new implementation or runtime evidence.

Owner supersession 2026-10-08 at R0 input `1a6bb53ef`: follow the current
[specification](S8-SPECIFICATION-20261008.md), [full destination layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md)
and [R0–R9 rollout ledger](ROLLOUT-LEDGER-20261008.md). SDK organization is
ProjectApi/WorkspaceApi/SandboxApi; actual Sandbox or external runtime owns
execution, standard streams/status and explicit cancellation. All old daemon
Exec/supervisor/launcher/cgroup/registration/wire tasks are withdrawn before
implementation. The [withdrawal ledger](checks/r0-owner-reconciliation-20261008/03-owner-and-proof-ledger.md)
retains original IDs/scope and historical evidence unchanged.

Reviewed source-ownership update2026-10-08 at `5be93f6d7`: follow the
[ownership review](R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md) and
[reviewed file layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md). FUSE owns the
connection and kernel request service; daemon assembles it with the existing
shared SQL/Store owners and composes overall Ready/unmount/Commit. Planned daemon
`native/` and kernel `request/steps/` homes are superseded. This is proposed source
organization, not implementation or relaxed proof requirements. R1 is complete;
only optional admin cancellation/client provenance is deferred. R2–R5 execution
awaits owner dispatch; forced teardown/concurrency/frozen acceptance remain later.

## 1. Starting point and rules

Starting state [implemented and source-verified at `5be93f6d7`]:

- Twelve active members: Content, Storage, Persistence, Project, History,
  Telemetry, Overlay, Workspace, Daemon, SDK, Bridge and Sandbox. The actual
  daemon executable/application and ordinary Sandbox/SDK foundation are verified
  by [R1](R1-COMPLETE-FUSE-HANDOFF-20261008.md).
- Excluded: incompatible old FUSE, Server and five `*-legacy` predecessors.
  Replacement FUSE is the remaining activation, yielding13 active crates.
- Patched fuser0.18.0 remains unused in the active lock graph. Existing nix0.31.3
  features must be reviewed at actual owning activation; no broad lock update.
- Daemon, Workspace, Overlay and Bridge forbid unsafe code. Source counts,
  current paths and members are pinned in the [inventory](checks/r2-r5-ownership-review-20261008/01-current-source-inventory.json).

Rules that bind every checkpoint, from the [root guide](../../../../AGENTS.md)
and the [core guide](../../../AGENTS.md):

- One engine, one registry, one Store read set, one immutable cache. No host
  data path, no `layerfs-server`, no second SQL scheduler.
- Product source only under `crates/<package>/src/`; proofs are external tests,
  examples and harness code. No test hook, fault switch or benchmark-selected
  behaviour in product source.
- Production files at most 999 physical lines; `lib.rs`/`mod.rs` at most 200 and
  declarations only. Split by responsibility before the ceiling.
- No new third-party crate where a locked one supplies the capability; no
  third-party edit beyond the recorded fuser timestamp correction.
- One attempt per operation; readiness waits happen before an attempt.
- Disposable/WAL/OFF for the Store, selected explicitly; overlay stays
  MEMORY/OFF/EXCLUSIVE. No sync call on disposable backing.
- Every test command has an explicit wall stop of at most 120 s and is built
  first with `--no-run`.
- Every commit records production LOC as section 7 describes.

## 2. Cargo activation

Activation follows the actual owning rollout checkpoint, never an empty scaffold.
R1 activates real Sandbox; R2 activates real FUSE/native serving. The old C1
label remains a filesystem coverage group, not a daemon execution assignment.

| Step | Change | Notes |
| --- | --- | --- |
| A-1 | Run `python3 -B core/tools/check_fuser_integrity.py` before the first native build and record its output | Provenance check only; not a capability proof |
| A-2 | Move the excluded predecessor `crates/layerfs-fuse` to `crates/layerfs-fuse-legacy`, unchanged, and keep it excluded | Same treatment the Workspace, daemon and Bridge predecessors received. A relocation: production LOC delta 0, reported as relocation in its own commit |
| A-2b | COMPLETE in R1: ordinary Sandbox replacement active, predecessor retained as layerfs-sandbox-legacy | Reuse current lifecycle/stdio/control; admin stays deferred |
| A-3 | Create the active member `crates/layerfs-fuse` with new content (section 3.3). Linux-only dependency on `fuser = "=0.18.0"` with default features off; `nix` `=0.31.3` | The predecessor is not ported: its request model is the blocking legacy one the specification rejects (D-3, D-4) |
| A-4 | Lockfile growth, reviewed as its own diff: fuser moves from `patch.unused` to a patched package, and its dependencies not yet locked enter (`log`, `num_enum`, `page_size`, `parking_lot`, `ref-cast`, `zerocopy` and whatever they pull in transitively) | Authorized transitively by the pinned fuser. Prefer the versions the reference lockfile already resolves (`log` 0.4.34, `num_enum` 0.7.6, `page_size` 0.6.0, `parking_lot` 0.12.5, `ref-cast` 1.0.27, `zerocopy` 0.8.56). The exact added set is recorded in the commit; `--locked` builds after it |
| A-5 | `nix` feature additions only in actual owners: FUSE mount controls; Sandbox/runtime access setup if required by its ordinary backend | Features of an already locked crate; exact list reviewed at activation. No daemon process supervision features |
| A-6 | COMPLETE in R1: daemon executable/application exists; extend application/filesystem.rs only for Fuse assembly | No new executable/process supervisor or second request engine |
| A-7 | Extend the [boundary guard](../../../tools/check_product_boundary.py) coverage and review the [LOC counter](../../../../tools/production_loc.py) classification for the new and relocated paths | A counter change changes its hash and is stated in that commit; classification is never silently altered |

The `[patch.crates-io]` entry already exists in both owning roots and is not
changed. No direct `libc` use and no `unsafe` is planned in first-party code;
if a required kernel interface turns out to have no safe wrapper in the locked
crates, that is reported with source evidence as an incompatible required
dependency before anything is worked around.

## 3. Deepest-file matrix

`R` reuse unchanged; `C` change; `N` new. Line counts are physical lines at the
pin.

### 3.1 Bridge and SDK

| File | Lines | Action | S8 responsibility |
| --- | --- | --- | --- |
| [control_types.rs](../../../crates/layerfs-bridge/src/control_types.rs) | 171 | C | `WorkspaceStatus` gains the bounded `native` block (§4.3) |
| `layerfs-bridge/src/control_native.rs` | — | N | NativeMount, connection/negotiation identity, native phase, separate live/reserved/drain gauges, TeardownCustody and forced terminal outcome |
| [control_request.rs](../../../crates/layerfs-bridge/src/control_request.rs) | 132 | C | Additive `Attach`/`Locate` in R2; `ForceUnmount` belongs to R6 |
| [control_reply.rs](../../../crates/layerfs-bridge/src/control_reply.rs) | 248 | C | Ready/Located/Retained in R2; ForceUnmounted and its exact publication/work dispositions in R6 |
| [control.rs](../../../crates/layerfs-bridge/src/control.rs), [wire.rs](../../../crates/layerfs-bridge/src/wire.rs), [native/](../../../crates/layerfs-bridge/src/native/channel.rs) | — | R | Channel, record limits and existing encodings are unchanged; retained F13 receipts keep their meaning |
| [SDK control connection](../../../crates/layerfs-api/sdk/src/control/connection.rs) | 208 | C | Shared control connection beneath Project/Workspace facades; attach/locate/force_unmount and typed mount composing two separately acknowledged attempts |


| SDK/Sandbox owning home | Action | Actual starting point and extension |
| --- | --- | --- |
| SDK project/, control/, sandbox/ | R/C | R1 implemented; preserve Init/seal/install and ordinary runtime control |
| SDK workspace/ | C/N | Existing binding/status/commit/unmount; add mount/types for actual native Attach/Locate/Ready |
| Sandbox backend/docker/container.rs, container_types.rs | C | Existing owned Create/lifecycle gains selected FUSE device/capability configuration |
| Sandbox endpoint.rs, topology.rs | C | Verify actual changed deployment and protected backing/mount visibility |

No second facade implementation, admin subsystem or daemon execution owner.
Current owning paths supersede the earlier planned relocations.

### 3.2 Daemon assembly and engine services

| Existing or proposed home | Action | Responsibility |
| --- | --- | --- |
| application/{config,owner,serve}.rs; new application/filesystem.rs | C/N | Assemble one shared Fuse request service using the existing Store/Overlay and configured identity/limits; no session/request engine |
| control/{registry,operations,status,serve,types,failure}.rs | C | Sole registry, service admission and aggregate Ready/normal terminal disposition |
| control/{attach,unmount}.rs | N as needed | Compose Fuse serving/drained receipts with engine/Store/control predicates; preserve Busy usability and exact retained failures |
| control/commit.rs | N as needed in R5 | Control admission/delegation to existing store/commit.rs, never another orchestrator |
| service/filesystem_port.rs | N | Narrow nonblocking implementations of Fuse-consumed service contracts; no daemon types imported by Fuse |
| service/completion.rs; overlay/credits.rs | C | Race-safe completion/loss/credit notification for original pending native demands; preserve synchronous callers and original result custody |
| overlay/{owner,queue,commands}.rs | R/C | Existing sole fair SQL service across filesystem, Commit and cleanup; bounded job extensions only |
| overlay/native_ownership_commands.rs | N as needed | Atomic consistent answer/acquire, checked indexed lookup decrements and lifetime jobs |
| overlay/captured_namespace_port.rs | N as needed in R4 | Existing captured page/point jobs adapted to Workspace's provider-neutral port |
| store/{open,ports,bind}.rs | C | Idle/healthy reader admission, per-request original failure scope without locks spanning provider I/O, read-session snapshot |
| store/read_service.rs | N only for missing service | Bounded immutable demands at read-set concurrency, not a kernel request scheduler |
| store/{commit,operation,commit_types,settle}.rs | R/C | Reuse actual Store driver and failure custody; R4 constructor integration in R5 |

Do not create daemon native/ or kernel request/steps/. Configuring and retaining a
Fuse service in daemon composition does not transfer its session, parked request
or reply machinery back into daemon. No new adapter crate or duplicated SQL owner.

### 3.3 FUSE native connection and request service

Only this crate names fuser types. Its dependency is toward Workspace and actual
reused domain contracts, never daemon. It consumes narrow engine service ports
implemented by daemon. The [reviewed layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md)
collapses optional splits and preserves existing engine owners.

| Proposed Fuse home | Responsibility |
| --- | --- |
| ports.rs | Real missing nonblocking service/pending-result interfaces; no generic service locator or copied token hierarchy |
| mount/{config,profile,syscalls}.rs | Native settings/negotiation; safe owned device, direct mount and normal plain detach; force additions remain R6 |
| session/{startup,readiness,state,drain}.rs | Exact connection and loop custody, all-loop serving evidence and normal connection/request drain |
| request/{callbacks,decode,types,reply}.rs | fuser entry, borrowed receive slots, bounded post-admission owned inputs, one reply attempt/disposal |
| dispatch/{admission,queue,workers,pending,completion}.rs | R+N accounting; one fixed K pool shared across mounts; fair resumable steps, parked owners and exact completion/terminal wakeups |
| operations/{lookup,attributes,directory,open,read,readlink}.rs | R2 kernel handlers over Workspace decisions and atomic backed ownership; lookup/forget and open/release share homes |
| operations/{write,create,link,rename,remove,flush,unsupported}.rs | R3 native mutation/metadata/refusal composition; split truncate only when needed |
| coherence/{reply_order,attributes,pages}.rs | R3/R5 exact cached attributes/pages and install continuity, no independent mmap engine |
| attributes.rs; diagnostics.rs | Checked identity/time/attribute conversion and actual request/credit/drain observations |

The fuser loop reads before callback entry. Only the pre-admission handoff wait
may occupy one of N fixed receive slots: no payload copy, owner job or mutable
lease before admission; no shared lock, and terminal/release wakeup is mandatory.
Admitted requests retain inputs/replies/credits while parked off all receivers
and service workers. FORGET has no reply but still owns a bounded unit through
disposition. FP-34 remains the R+N/no-self-dependency/wakeup proof.

Current OwnerClient adapters block in Pending.wait. Moving them behind a trait
or adding try_complete polling cannot satisfy I-8. Implement original pending
ownership and race-safe notification in actual engine owners, with Fuse resumption.
Readiness remains I-3: from_fd Ok/one stat/thread creation is insufficient; fuser
run error can leave loops unjoined. Preserve Retained and prove both connection
and aggregate daemon drain; no extension of the authorized timestamp patch.


### 3.4 Workspace and Overlay

| File | Lines | Action | S8 responsibility |
| --- | --- | --- | --- |
| [workspace/view.rs](../../../crates/layerfs-workspace/src/workspace/view.rs) | 221 | C | Compose one consistent answer/plan, with positive-entry ownership acquisition and explicit processing leases where needed; do not claim every read-class request is read-only |
| `operations/read_plan.rs` | — | N | Resumable read-class plans in the `Need`/facts shape mutations already have |
| [mutation/](../../../crates/layerfs-workspace/src/mutation/driver.rs), [operations/](../../../crates/layerfs-workspace/src/operations/types.rs) | — | C | Reuse mutation/publication semantics; native entry-bearing results acquire indexed lookup custody in the same transaction before reply. Do not add a second acknowledgement transaction |
| [base/client.rs](../../../crates/layerfs-workspace/src/base/client.rs), [base/cache.rs](../../../crates/layerfs-workspace/src/base/cache.rs), [base/view.rs](../../../crates/layerfs-workspace/src/base/view.rs) | 174, 112, 184 | R | Unchanged in the mandatory scope. Shared cached bytes and the fact cache are ranked candidates behind their gates |
| [workspace/serials.rs](../../../crates/layerfs-workspace/src/workspace/serials.rs) | 54 | C | Low-water early reservation attempts: at most one per create below an explicit low-water value (P-1 ruling) |
| `layerfs-overlay/src/namespace/read_compound.rs` | — | N | Consistent pure observations and positive-LOOKUP short read/acquire transaction; count required fact/lease rounds explicitly |
| `layerfs-overlay/src/lifetime/lookup.rs`, native ownership SQL, `lifetime/retire.rs` | — | C/N | Reuse existing lookup/open/processing lease semantics. Add indexed mount/serial aggregate counts and checked decrements; fixed native-group revocation only after drain; paged physical retirement/dependent-reference release |
| [lifetime/close.rs](../../../crates/layerfs-overlay/src/lifetime/close.rs), [maintenance/reclaim.rs](../../../crates/layerfs-overlay/src/maintenance/reclaim.rs) | 84, 296 | C | Close may reach `Queued` after drain and logical revocation, while indexed physical retirement remains debt; maintenance-stopped observations |
| overlay runtime SQL/schema | — | C | Index native mount/group and serial ownership. Review schema/version change explicitly; no restart migration is implied. Cover exact checked increments/decrements, underflow, foreign incarnation and revoked-group eligibility; retain EXPLAIN/runtime accounting |

Not touched by S8: Content, Storage, Persistence, History, Project, telemetry
and the root reference tree. The Persistence `try_lock` behaviour stays; R-4
removes the one in-process collision S8 would otherwise create.

Architecture documents updated in the same change as the source they describe:
[native control](../../architecture/68-native-workspace-control.md),
[direct Store adapter](../../architecture/61-direct-store-adapter.md), and new
documents for native mount/request service and ordinary Sandbox execution/access.

## 4. Checkpoints

Coverage groups C1–C6 retain their proof IDs; they are not identical to rollout
R numbers. C1/native read/normal drain maps to R2; C2/mutation coherence to R3.
R4 captured namespace is a separate component track and R5 is their mounted
Commit integration. C3 force and C4/C5 sustained concurrency/churn belong to R6;
C6 frozen numerical qualification belongs to R8. This review starts none of them.

Each checkpoint ends at a compiled, externally proved state with its own
commits, LOC comparisons and a dated record under this directory. A later
checkpoint never relabels an earlier receipt. Proof rows are listed where they
first become runnable; a row that needs several Workspaces waits for C4 even
when its code landed earlier.

### C1 — native floor

The smallest genuine vertical slice: installed sealed Store, the existing
overlay Owner, the public Workspace, a native attach with exact readiness,
reads, stats and permission checks through the mount, `/bin/bash -c true`, and
exact terminal unmount with every loop joined and all namespace consumers
disposed before native-owner revocation and logical Close.

| Order | Work | Why it is in the floor |
| --- | --- | --- |
| 1 | Environment facts recorded from the actual sandbox: kernel, `/dev/fuse`, actual FUSE/mount controls, runtime identity and mount visibility/protection | §10.4: verified, not inferred from an image identity |
| 2 | Activation A-1 to A-7 | First native build |
| 3 | `Attach`/`Ready`, `Locate`, native state and gauges, mount session owner, profile and receipt | I-3; M-1, M-2 |
| 4 | Final request admission/service, consistent read plans, indexed positive-LOOKUP/FORGET custody, OPEN/RELEASE and directory owners; logical native-group revocation and bounded retirement | Required for correct reads and the first real detach; no counter-only lookup shortcut. M-3, M-4, M-8, M-11 |
| 5 | Read service R-1 to R-3 | Without R-3 one cold failure poisons later requests; without R-1/R-2 a loop or worker would block on a reader |
| 6 | Use the actual R1 Sandbox/external executor to launch ordinary Bash independently of daemon controls; qualify identity/access and mount visibility topology | Filesystem access requires no Exec identity. Runtime exit/streams never substitute for native busy/drain evidence |
| 7 | Normal `Unmount`: reversible probe with filesystem service, one plain detach, full native/daemon-work drain, ownership revocation and logical Close | I-14; M-11 normal path |

Proofs: FP-1, FP-2, FP-3, FP-4, FP-17 (read side), FP-20 (read probe), FP-21,
FP-22-FS, FP-31 (lookup/decrement/detach), FP-34 (single-mount admission and normal
detach); H-1, H-2, H-3, H-4,
H-7, H-8, H-10, H-15. FP-22-FS is in this checkpoint because the propagation
design is not established until it passes.

### C2 — mutation and coherence

Mutation steps (WRITE, SETATTR, CREATE, MKDIR, SYMLINK, LINK, UNLINK, RMDIR,
RENAME) on the existing one-transaction jobs; publication ticket release after
the reply attempt; mapped WRITE acceptance; the refused families; the ruled
P-1 behaviour (early single reservation attempts, `EAGAIN` on exhaustion) with
its oracle from proof-plan section 8.1.

Proofs: FP-10 to FP-18, FP-20 (mutation probe), FP-28, FP-29, FP-31
(entry-bearing mutation acquisition); H-6, H-18; H-5 recorded.

### C3 — ordinary runtime streams and forced filesystem teardown

Runtime-owned successor FP-5/6/7/30-Runtime proves actual ordinary execution,
access setup, standard streams/status and explicit caller cancellation. These
observations never gate filesystem unmount. Filesystem force refuses an active
namespace control producer before effects, performs one connection-specific
abort, disposes native waiters, drains original attempted work and performs one
plain detach. Preserve original unknown and known-publication custody. No process
signal, daemon supervisor, output-disposal protocol or command registration.

Proofs: FP-19-FS, FP-20, FP-21, FP-22-FS, FP-23-FS, FP-25-Routes,
FP-31–34 forced/adversarial extensions; reuse unchanged earlier passing scopes.

### C4 — concurrent Workspace service

Several Workspaces on one daemon; R-4; reader health reporting; fairness and
head-of-line observations. No new mechanism is selected here: this checkpoint
produces the baselines that gate the ranked candidates.

Proofs: FP-8, FP-9, FP-24, FP-27, FP-34 (cross-Workspace progress);
H-9, H-11, H-12, H-13 recorded.

### C5 — sustained ownership and churn

Debt and maintenance state in status, debt-coupled mount admission, repeated
per-call lifecycles against a writing peer, resource-domain observations.

Proofs: FP-26; H-16, H-17; the resource domains of proof-plan section 5.

### C6 — speed and storage under current R8 assignment

The full rollout retains applicable registered qualification at R8; the current
R2–R5 next-agent prompt does not dispatch that later work. This plan registers no sample. Runs only after C1–C5 are proved at a frozen identity and each selection is
registered prospectively under the P-3 to P-7 rulings, including the numbers
those rulings leave to registration. Candidates of the
[mechanism ledger](S8-MECHANISM-EVIDENCE-20261008.md#4-ranked-candidates) enter
one at a time, each behind its own gate and its own single-mechanism arm.

S10 work (live namespace normalization, install under a live mount, Commit
survival, committed-index fast paths) is outside every checkpoint above.

## 5. Decisions the implementer must not reopen

| Not to be built | Why |
| --- | --- |
| A blocking adapter in which a dispatch loop waits for an owner job or a Store read | I-8; a parked request makes the calling process unkillable |
| A resident map proportional to all visited inodes, counter-only FORGET, or deletion while kernel references remain | D-6 uses indexed backing and bounded windows; owning SQL/decrement costs are required and counted |
| A second routing registry, a private per-mount or per-Workspace immutable cache, a mutable mirror of overlay rows | I-2; #314 |
| fuser's mount helper, lazy detach or its unmount handle | D-2 |
| Kernel notifications, writeback, passthrough bypassing capture, permission removal | D-14; rejected capabilities |
| Any timeout, implicit Commit or automatic unmount around Exec | I-11 |
| A writer gate, wait or retry around the Store session | K30, D-9 |
| A port of the excluded predecessor's request model | D-3, D-4 |
| Any ranked candidate without its gate | Mechanism ledger |

## 6. Verification per checkpoint

Run from the repository root at the checkpoint's final identity, each test
command under an explicit stop of at most 120 s and selected by package when the
whole suite cannot fit:

```sh
python3 -B core/tools/check_fuser_integrity.py
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets --no-run
python3 -B core/tools/check_product_boundary.py
python3 -B -m unittest discover -s core/tools -p 'test_*.py'
```

Native proofs run in Linux containers with the repository ARM64 build inputs
recorded in the build identity. They leave the protected containers, every
unrelated process and worktree, and every historical receipt untouched, and keep
Store files on a named in-VM volume or native container filesystem. A proof that
reaches its ceiling has failed as a hang and is diagnosed from source and
bounded output before any rerun. Unchanged passing checks are not repeated.

## 7. Production LOC accounting

Every commit carries
`Production LOC: <before> -> <after> (delta <signed difference>)` computed by
[tools/production_loc.py](../../../../tools/production_loc.py) over `git archive`
extractions of the exact first parent and the staged tree, confirmed against the
committed tree, with these subtotals: Core, active members, reference, excluded
predecessors, excluded integration.

Current baseline at `5be93f6d7`: combined170673; core105256 (active62382;
excluded predecessors37431; excluded integration5443); reference65417. Original
`32d969776` counts remain historical in its receipts, not current planning input.

Expected labels, to be stated rather than netted:

- A-2 is a relocation with delta 0; if the relocated crate moves between the
  "excluded integration" and "excluded predecessor" subtotals, that movement is
  reported as a classification change, not as removed code.
- New `layerfs-fuse`, daemon and Bridge source is growth in the active subtotal.
  Old and replacement FUSE totals are reported side by side while both exist.
- The reference tree is untouched by S8; its retirement follows cluster two.
- Tests, examples, harness code and documents never enter the headline.

No estimate of S8's size is given here; the numbers are computed per commit.
