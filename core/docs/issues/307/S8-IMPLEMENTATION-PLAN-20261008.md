# S8 implementation plan: deepest files, activation and checkpoints

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-08 on local `main` at `32d969776` (product source pin
> `f0797c646`). This is a plan. Nothing in it is implemented, built, tested or
> measured. The [S8 specification](S8-SPECIFICATION-20261008.md) owns every
> decision; the [proof plan](S8-PROOF-PLAN-20261008.md) owns every proof row
> named here. File names for new source are proposals an implementer may adjust
> within the stated responsibility and line ceilings; the reuse and ownership
> columns are not adjustable without a specification change.

## 1. Starting point and rules

Starting state [implemented and source-verified at the pin]:

- Active members ([core/Cargo.toml](../../../Cargo.toml)): content, storage,
  persistence, project, history, telemetry, overlay, workspace, daemon, SDK,
  bridge. `layerfs-daemon` is a library with no executable target.
- Excluded: `crates/layerfs-fuse` (1,577 physical lines; imports symbols that
  exist only in the excluded legacy Workspace and cannot compile against the
  active one), `crates/layerfs-sandbox`, the `*-legacy` predecessors and
  `layerfs-server`.
- fuser 0.18.0 is `[[patch.unused]]` in [core/Cargo.lock](../../../Cargo.lock):
  no active member depends on it yet.
- `nix` `=0.31.3` is already locked: Persistence and Overlay use its `fs`
  feature, and telemetry uses it optionally. `libc` is locked at 0.2.189.
- Daemon, Workspace, Overlay and Bridge are `#![forbid(unsafe_code)]`.

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

All of this lands with the first real native code in checkpoint C1, never as an
empty scaffold.

| Step | Change | Notes |
| --- | --- | --- |
| A-1 | Run `python3 -B core/tools/check_fuser_integrity.py` before the first native build and record its output | Provenance check only; not a capability proof |
| A-2 | Move the excluded predecessor `crates/layerfs-fuse` to `crates/layerfs-fuse-legacy`, unchanged, and keep it excluded | Same treatment the Workspace, daemon and Bridge predecessors received. A relocation: production LOC delta 0, reported as relocation in its own commit |
| A-3 | Create the active member `crates/layerfs-fuse` with new content (section 3.3). Linux-only dependency on `fuser = "=0.18.0"` with default features off; `nix` `=0.31.3` | The predecessor is not ported: its request model is the blocking legacy one the specification rejects (D-3, D-4) |
| A-4 | Lockfile growth, reviewed as its own diff: fuser moves from `patch.unused` to a patched package, and its dependencies not yet locked enter (`log`, `num_enum`, `page_size`, `parking_lot`, `ref-cast`, `zerocopy` and whatever they pull in transitively) | Authorized transitively by the pinned fuser. Prefer the versions the reference lockfile already resolves (`log` 0.4.34, `num_enum` 0.7.6, `page_size` 0.6.0, `parking_lot` 0.12.5, `ref-cast` 1.0.27, `zerocopy` 0.8.56). The exact added set is recorded in the commit; `--locked` builds after it |
| A-5 | `nix` feature additions in the crates that need them: mount and namespace calls, identity change, process and signal handling, polling | Features of an already locked crate, not a new dependency. The exact list is recorded at activation |
| A-6 | An executable target for `layerfs-daemon` with a thin entry that selects serve mode or launcher mode and delegates | The entry file is declarations and delegation only |
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
| [control_types.rs](../../../crates/layerfs-bridge/src/control_types.rs) | 163 | C | `WorkspaceStatus` gains the bounded `native` block (§4.3) |
| `layerfs-bridge/src/control_native.rs` | — | N | `NativeMount`, negotiation receipt, `TeardownCustody`, native state and gauge types |
| [control_request.rs](../../../crates/layerfs-bridge/src/control_request.rs) | 125 | C | Additive tags `Attach`, `Locate`, `ForceUnmount`, `ExecStatus`, `ExecCancel` |
| [control_reply.rs](../../../crates/layerfs-bridge/src/control_reply.rs) | 241 | C | Additive tags `Ready`, `Located`, `Retained`, `Exec`; new refusal phases |
| `layerfs-bridge/src/exec_types.rs`, `exec_wire.rs` | — | N | Exec identity, observation, and the stream records of §4.1 with the 32768-byte chunk bound |
| [control.rs](../../../crates/layerfs-bridge/src/control.rs), [wire.rs](../../../crates/layerfs-bridge/src/wire.rs), [native/](../../../crates/layerfs-bridge/src/native/channel.rs) | — | R | Channel, record limits and existing encodings are unchanged; retained F13 receipts keep their meaning |
| [sdk control.rs](../../../crates/layerfs-api/sdk/src/control.rs) | 164 | C | `attach`, `locate`, `force_unmount`, `exec_status`, `exec_cancel`; a typed `mount` helper performing `Mount` then `Attach` as two attempts with separate custody |
| `layerfs-api/sdk/src/exec.rs` | — | N | Exec client on a dedicated connection: start, command chunks, stdin, event stream |

### 3.2 Daemon

| File | Lines | Action | S8 responsibility |
| --- | --- | --- | --- |
| [control/registry.rs](../../../crates/layerfs-daemon/src/control/registry.rs) | 131 | C | `Bound` gains native state and the `execs`, `handles`, `requests` gauges; uncertain `Open` becomes an observable `Retained { bind }` entry instead of a silent placeholder |
| `control/native_state.rs` | — | N | The §5.1 state enum and its guarded transitions, called only under the registry mutex |
| [control/operations.rs](../../../crates/layerfs-daemon/src/control/operations.rs) | 240 | C | Mount admission gains the debt check (D-13); `Locate` |
| `control/attach.rs`, `control/unmount.rs` | — | N | `Attach` conversation; normal and forced terminal sequence of §11.2 with its outcomes |
| [control/status.rs](../../../crates/layerfs-daemon/src/control/status.rs) | 60 | C | Native block from maintained counters; still zero Store SQL |
| [control/serve.rs](../../../crates/layerfs-daemon/src/control/serve.rs) | 93 | C | Route new tags; hand an Exec conversation to its own owner |
| [control/failure.rs](../../../crates/layerfs-daemon/src/control/failure.rs) | 202 | C | New phases; no message parsing |
| [bootstrap.rs](../../../crates/layerfs-daemon/src/bootstrap.rs) | 91 | C | Native configuration and readiness are composed here; concrete Store opening stays here |
| `native/config.rs`, `native/readiness.rs` | — | N | Bash identity, `R`, `K`, mount root, P-2 policy; explicit readiness failure when `/dev/fuse`, required capabilities or a delegated cgroup v2 subtree are missing |
| `native/session.rs` | — | N | Mount session owner: owns one `layerfs-fuse` session for its life, drives Attaching → Ready and Detaching → Detached, joins every loop |
| `request/types.rs`, `request/credits.rs`, `request/queue.rs`, `request/workers.rs` | — | N | Request states and prerequisites (§5.2); per-mount request and byte credits; per-Workspace runnable FIFOs with round-robin; `K` fixed workers |
| `request/steps/` (`lookup`, `attributes`, `read`, `directory`, `open`, `mutation`, `refused`) | — | N | One resumable step function per request shape of §6.4; each returns a reply or exactly one prerequisite |
| [service/completion.rs](../../../crates/layerfs-daemon/src/service/completion.rs) | 189 | C | Optional notifier on `Pending` that enqueues the owning request; synchronous `wait` stays for control and tests |
| [overlay/credits.rs](../../../crates/layerfs-daemon/src/overlay/credits.rs) | 60 | C | Release notification for admission waiters |
| [overlay/owner.rs](../../../crates/layerfs-daemon/src/overlay/owner.rs) | 492 | C | `lifecycle_jobs_per_namespace` chosen explicitly for native serving at readiness; closed-namespace debt and stopped-maintenance state exposed as counters |
| [overlay/commands.rs](../../../crates/layerfs-daemon/src/overlay/commands.rs) | 763 | C | Kept under the ceiling by adding the new commands in a sibling file |
| `overlay/read_commands.rs` | — | N | Read-only compound commands for LOOKUP, GETATTR, READLINK and a directory page (D-4); the group-retirement command (D-8) |
| [overlay/queue.rs](../../../crates/layerfs-daemon/src/overlay/queue.rs) | 376 | R | Lanes and rotation unchanged; H-12 measures them |
| [store/open.rs](../../../crates/layerfs-daemon/src/store/open.rs) | 103 | C | Idle and healthy reader selection; quarantined readers leave the rotation and are counted (R-2) |
| `store/read_service.rs` | — | N | Per-Workspace FIFOs of bounded demand batches served round-robin at read-set concurrency; a demand runs on a worker that already holds a reader (R-1) |
| [store/ports.rs](../../../crates/layerfs-daemon/src/store/ports.rs) | 151 | C | One failure scope per kernel request, built from the session's `Arc<BoundWorkspace>`; no scope mutex across provider I/O (R-3) |
| [store/bind.rs](../../../crates/layerfs-daemon/src/store/bind.rs) | 117 | C | Branch snapshot through a read-only session (R-4) |
| [store/commit.rs](../../../crates/layerfs-daemon/src/store/commit.rs), [store/operation.rs](../../../crates/layerfs-daemon/src/store/operation.rs) | 175, 67 | R | Store half of Commit unchanged; admitted in `Bound` and `Ready` |
| `exec/session.rs`, `exec/supervisor.rs`, `exec/custody.rs`, `exec/streams.rs`, `exec/serve.rs` | — | N | Exec state machine (§5.4); one supervisor thread on process, pipe and group events; one cgroup v2 leaf per session; one unsent chunk per stream; the dedicated-connection conversation |
| `exec/spawn.rs`, `exec/launcher.rs` | — | N | Spawn of the daemon's own executable with exactly three pipes; the seven launcher steps of §10.1 with the descriptor-table assertion |
| executable entry | — | N | Mode selection and delegation only |

### 3.3 FUSE adapter (`layerfs-fuse`, new active content)

The only crate that names fuser types. The daemon sees first-party request and
reply types and supplies one sink at mount.

| File | Action | S8 responsibility |
| --- | --- | --- |
| `mount/syscalls.rs` | N | Open `/dev/fuse`; first-party `mount(2)` with the option string; `umount2`; per-Workspace shared peer group (D-2, D-12) |
| `mount/profile.rs` | N | The §8.1 flags and limits requested at init; the negotiation receipt; refusal when a required element is not granted |
| `mount/session.rs` | N | `Session::from_fd`, two loops, `SessionACL::All`; `run` returns when every loop has exited |
| `request/types.rs`, `request/decode.rs` | N | Owned first-party request values with one bounded input copy; the `fuser::Filesystem` implementation that decodes, checks the fence, takes credits and hands the request to the sink |
| `request/reply.rs` | N | A typed wrapper owning the fuser reply object: exactly one attempt, send error recorded, drop-without-reply counted |
| `attributes.rs` | N | Identity and attribute mapping of §9 |
| `diagnostics.rs` | N | Opcode counters and per-mount gauges for H-9, H-15, H-17, H-18 |

Because fuser owns the read loop, "stops reading when no credit is free" is
implemented as acquiring the next request's credit before the callback returns.
That is the only wait a loop has (specification §6.2). It precedes any attempt,
holds no lock, and is released only by reply attempts made on service workers,
never by a future kernel request (I-9).

### 3.4 Workspace and Overlay

| File | Lines | Action | S8 responsibility |
| --- | --- | --- | --- |
| [workspace/view.rs](../../../crates/layerfs-workspace/src/workspace/view.rs) | 221 | C | Read-class answers composed from one compound owner result plus immutable base facts; the multi-job `lookup`/`stat` composition is no longer on the native path |
| `operations/read_plan.rs` | — | N | Resumable read-class plans in the `Need`/facts shape mutations already have |
| [mutation/](../../../crates/layerfs-workspace/src/mutation/driver.rs), [operations/](../../../crates/layerfs-workspace/src/operations/types.rs) | — | R | Mutation jobs, windows and publication tickets unchanged |
| [base/client.rs](../../../crates/layerfs-workspace/src/base/client.rs), [base/cache.rs](../../../crates/layerfs-workspace/src/base/cache.rs), [base/view.rs](../../../crates/layerfs-workspace/src/base/view.rs) | 174, 112, 184 | R | Unchanged in the mandatory scope. Shared cached bytes and the fact cache are ranked candidates behind their gates |
| [workspace/serials.rs](../../../crates/layerfs-workspace/src/workspace/serials.rs) | 54 | C | Low-water early reservation attempts: at most one per create below an explicit low-water value (P-1 ruling) |
| `layerfs-overlay/src/namespace/read_compound.rs` | — | N | The unframed read statements of one read-class job: validate bound root, parent row, local entry, target row |
| `layerfs-overlay/src/lifetime/retire.rs` | — | N | Bounded, paged retirement of a detached namespace's remaining open, request and base-source owners (D-8) |
| [lifetime/close.rs](../../../crates/layerfs-overlay/src/lifetime/close.rs), [maintenance/reclaim.rs](../../../crates/layerfs-overlay/src/maintenance/reclaim.rs) | 84, 296 | C | Close reaches `Queued` after retirement; debt and maintenance-stopped observations |
| overlay runtime SQL | — | C if needed | Any new statement ships with its exact `EXPLAIN QUERY PLAN` and correlated runtime counts before a cost claim |

Not touched by S8: Content, Storage, Persistence, History, Project, telemetry
and the root reference tree. The Persistence `try_lock` behaviour stays; R-4
removes the one in-process collision S8 would otherwise create.

Architecture documents updated in the same change as the source they describe:
[native control](../../architecture/68-native-workspace-control.md),
[direct Store adapter](../../architecture/61-direct-store-adapter.md), and new
documents for the native mount and request service and for Exec.

## 4. Checkpoints

Each checkpoint ends at a compiled, externally proved state with its own
commits, LOC comparisons and a dated record under this directory. A later
checkpoint never relabels an earlier receipt. Proof rows are listed where they
first become runnable; a row that needs several Workspaces waits for C4 even
when its code landed earlier.

### C1 — native floor

The smallest genuine vertical slice: installed sealed Store, the existing
overlay Owner, the public Workspace, a native attach with exact readiness,
reads, stats and permission checks through the mount, `/bin/bash -c true`, and
exact terminal unmount with every loop joined.

| Order | Work | Why it is in the floor |
| --- | --- | --- |
| 1 | Environment facts recorded from the actual sandbox: kernel, `/dev/fuse`, capabilities for mount, namespace entry and identity change, cgroup v2 delegation | §10.4: verified, not inferred from an image identity |
| 2 | Activation A-1 to A-7 | First native build |
| 3 | `Attach`/`Ready`, `Locate`, native state and gauges, mount session owner, profile and receipt | I-3; M-1, M-2 |
| 4 | Request service in its final shape (credits, parked and runnable states, workers, notifier), read-class steps with the single compound owner job, OPEN/RELEASE, directory cursors, FORGET counters, refused opcodes | A blocking interim adapter would be a second design to delete. M-3, M-4, M-8 |
| 5 | Read service R-1 to R-3 | Without R-3 one cold failure poisons later requests; without R-1/R-2 a loop or worker would block on a reader |
| 6 | Launcher, identity drop, propagation contract, cgroup custody, `Exited` and `Quiescent`, pipes owned with the one-chunk bound | `bash -c true` needs the real launch path; normal unmount needs a truthful `execs` gauge |
| 7 | Normal `Unmount`: gate, `umount2`, join, logical close | I-14; M-11 normal path |

Proofs: FP-1, FP-2, FP-3, FP-4, FP-17 (read side), FP-22; H-1, H-2, H-3, H-4,
H-7, H-8, H-10, H-15. FP-22 is in this checkpoint because the propagation
design is not established until it passes.

### C2 — mutation and coherence

Mutation steps (WRITE, SETATTR, CREATE, MKDIR, SYMLINK, LINK, UNLINK, RMDIR,
RENAME) on the existing one-transaction jobs; publication ticket release after
the reply attempt; mapped WRITE acceptance; the refused families; the ruled
P-1 behaviour (early single reservation attempts, `EAGAIN` on exhaustion) with
its oracle from proof-plan section 8.1.

Proofs: FP-10 to FP-18, FP-28, FP-29; H-6, H-18; H-5 recorded.

### C3 — Exec streams, descendants and forced teardown

Stream backpressure and detachment, `ExecStatus`/`ExecCancel`, reply completion
before waiting for exit, `ForceUnmount`, group retirement of remaining owners,
`Retained` outcomes, lost-reply observation.

Proofs: FP-5, FP-6, FP-7, FP-19, FP-20, FP-21, FP-23, FP-25.

### C4 — concurrent Workspace service

Several Workspaces on one daemon; R-4; reader health reporting; fairness and
head-of-line observations. No new mechanism is selected here: this checkpoint
produces the baselines that gate the ranked candidates.

Proofs: FP-8, FP-9, FP-24, FP-27; H-9, H-11, H-12, H-13 recorded.

### C5 — sustained ownership and churn

Debt and maintenance state in status, debt-coupled mount admission, repeated
per-call lifecycles against a writing peer, resource-domain observations.

Proofs: FP-26; H-16, H-17; the resource domains of proof-plan section 5.

### C6 — speed and storage (not authorized by this plan)

Runs only after C1–C5 are proved at a frozen identity and each selection is
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
| A per-inode lookup table, or SQL work in FORGET | D-6, I-16 |
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

Baseline at `32d969776`: combined 165,813; Core 100,396 (active 57,522;
excluded predecessors 36,325; excluded integration 6,549); reference 65,417.

Expected labels, to be stated rather than netted:

- A-2 is a relocation with delta 0; if the relocated crate moves between the
  "excluded integration" and "excluded predecessor" subtotals, that movement is
  reported as a classification change, not as removed code.
- New `layerfs-fuse`, daemon and Bridge source is growth in the active subtotal.
  Old and replacement FUSE totals are reported side by side while both exist.
- The reference tree is untouched by S8; its retirement follows cluster two.
- Tests, examples, harness code and documents never enter the headline.

No estimate of S8's size is given here; the numbers are computed per commit.
