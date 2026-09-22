# Pair 1 continuation handoff: daemon management through npm and R6

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Production base: `2e7f42ea22778e0119d5e3d0da806130bcd50330`.
> Product input seal: `375c2076db5299d601c7b8e18042a7831eeaa51e54911b1b11fb5f18610ead3d`.
> Prepared 2026-09-22 for a separate Codex implementation task.

**Continuation update:** [Mount32](32-control-mount.md), [native failed-Attach35](35-failed-attachment-ownership.md)
and [authenticated Attach36](36-control-attach.md) now have their source-pinned
checks and actual Linux proofs. The earlier baseline/ordered steps below remain
traceable planning context. [Workspace Commit37](37-control-commit.md) now verifies its writable startup and
dirty-shutdown custody prerequisites. [Shared prepared directories38](38-prepared-directories.md)
now verifies new directories and existing-directory portable patches through the
existing Service save owner. [Native mkdir39](39-native-mkdir.md) now verifies the
maintained namespace index and Commit integration. [Mounted mkdir40](40-mounted-mkdir.md)
now verifies kernel/SDK creation and checked entry coherence. [Portable metadata41](41-construct-portable-metadata.md)
now verifies no-base construction through the existing save owner. [Prepared files42](42-prepared-files.md)
now verifies fresh regular-file declarations on direct and C5 routes. [Native file
creation43](43-native-create.md) is implemented with13 registered functional
selections passing; its capacity gate remains failed/open despite a successful
labelled diagnostic. [Mounted CREATE44](44-mounted-create.md) verifies its
selected kernel/SDK, custody and G/D1 paths. [Symlink-content constructor45](45-construct-symlink.md)
verifies exact target-object saves. [Prepared symlinks46](46-prepared-symlinks.md)
verifies shared direct/C5 fresh declarations. [Native symlink47](47-native-symlink.md)
verifies ten selected functional routes, including typed backing failure custody.
Kernel symlinks and the full prepared upload remain open. The prepared DSH workload in [34](34-preinstalled-dsh-workload.md)
remains unchanged and will be uploaded in full before one explicit Commit.

## Continuation recorded after this checkpoint

Authenticated remote Mount and the shared cancelled-pipe correction are committed
at`8639c6bda9e5911c3d434e659380d99b0bcf8d88`; see[32](32-control-mount.md) and[33](33-cancelled-pipe.md).
The current source inventory is[31](31-source-map-and-loc.md), core43364/reference65417,
combined108781 production LOC. This note does not relabel this handoff's original
counts or evidence. The next native prerequisite is retained failed-Attach ownership,
observation and explicit cleanup, followed by remote Attach.

The owner now selects `npx @deepseek-ai/dsh web` with dependencies preinstalled
outside the workload. The actual test uploads/writes that prepared tree through
the mount and explicitly Commits it; network npm installation is excluded from
the workload. Platform-native dependencies must match Linux execution. Earlier
requirements to pin the project/lock/runtime now apply to that prepared artifact.

## Mission and starting state

Continue the remaining Pair 1 work for #179 and its separately qualified matched
comparison #207. The complete objective remains bounded private disk COW, frozen G
with live G+1, incremental repeated Commits, required shell/npm behavior and R6.
Do not restart the original R0-R/R1 study, repeat completed component investigations,
or stop after another general plan. Implement one public operation per round,
closing its concrete prerequisites first. The current next operation is authenticated
remote Mount of an already attached Workspace; its native failure-owner prerequisite
is now implemented and verified in 29.

The source checkout is
`/Users/yifanxu/.codex/worktrees/pair1-readable-mount/layerfs`, on
`codex/pair1-readable-mount`. Its production base above is clean and committed.
It began at synchronized main `0749180db34d1cdc57f905806a17e3f3f48ec2bc` and includes
the subsequent Pair 1 implementation. The new task must use this committed
continuation, including this documentation checkpoint, rather than accidentally
implementing against a default-main checkout that lacks the work. Inspect the new
task's HEAD/status first, then select the exact handoff revision in its own clean
worktree and use its own codex/ branch. Preserve the original checkout/worktree.
Do not push, merge, close issues or replace another task's working changes.

Read root AGENTS.md and core/AGENTS.md first. Then read this handoff and
[the actual file/LOC map](31-source-map-and-loc.md), README,04,01,02/03, and05/06.
The original07 handoff's mission/policies still apply; its instruction to start at
R0-R/R1 is superseded by the completed checkpoints here. Historical source pins,
old proposed signatures and planning LOC ranges are not current implementations.
Use the current source and operation records08–29 for actual behavior. In particular,
read 25–29 before extending mounted operations or daemon lifecycle control.

## Folder layout, ownership and LOC

The [complete source map](31-source-map-and-loc.md) lists every core production
file with exact production LOC and separate physical lines, plus recursive folder
and crate totals. Its [JSON](evidence/continuation-handoff/source-loc.json) also
lists every reference production file. Core has 241 files/43276 production LOC;
reference has 193 files/65417 LOC; combined 108693. These are counts of exact source,
not estimates or performance evidence.

```text
repository/
|-- crates/                         reference only; never a product dependency
|-- core/
|   |-- Cargo.toml / Cargo.lock     independent replacement workspace
|   |-- crates/
|   |   |-- layerfs-daemon/         856 production LOC; 6 source files
|   |   |-- layerfs-fuse/           1062 production LOC; 4 source files
|   |   |-- layerfs-workspace/      9783 production LOC; 37 source files
|   |   |-- layerfs-bridge/         4340 production LOC
|   |   |-- layerfs-service/        2031 production LOC
|   |   |-- layerfs-content/        12549 production LOC; C1
|   |   |-- layerfs-storage/        7676 production LOC; C2
|   |   |-- layerfs-history/        2722 production LOC; C5
|   |   `-- layerfs-telemetry/      2257 production LOC
|   |-- docs/architecture/         current component descriptions
|   |   `-- proposal/fuse-workspace-snapshot-overlay/
|   |       |-- 01..31*.md         contracts, chronological rounds and this handoff
|   |       `-- evidence/          committed append-only receipts/caller identities
|   |-- tools/                     boundary guard and external guard self-tests
|   |-- benchmark/                 separate benchmark workspace/harness
|   |-- target/                    this worktree's host builds/local artifacts
|   `-- target-linux/              this worktree's Linux builds
`-- docs/general/, benchmark/      measurement/release policy and runner guides
```

In every owning crate, src/ and required sql/ are production; tests/ contains
external public-API and mounted/control tests and support; examples/ are external
runnable callers. Tests, fixtures, docs, tools and manifests do not count as
production LOC. Keep production files <=999 physical lines; lib.rs/mod.rs <=200
and declaration/reexport/thin-delegation only. Add real modules when a responsibility
needs one; do not scaffold planned groups or split every function into a facade.

Workspace's existing private groups have these production totals:

| Group | LOC | Responsibility |
| --- | ---: | --- |
| runtime/ | 1268 | Host registry, per-Workspace state/lifecycle, bounded projection coherence |
| filesystem/ | 1663 | Lookup/directory/read, portable handles and existing-file mutations |
| overlay/ | 654 | Piece recipes, maintained generation capture and immutable source pins |
| backing/ | 4414 | RAM/disk accounting, payload segments, metadata pages/index, ownership/reclaim |
| commit/ | 1304 | Lowering, captured Source, submission, completion and own-result reconciliation |
| root lib/types/commit_types files | 480 | Narrow exports and portable production options/results/errors |

Some files are already substantial: filesystem/write.rs811 physical lines,
backing/ownership.rs827, backing/metadata.rs807, Bridge metadata codec796 and
response codec817. Split along a real responsibility before reaching a ceiling.
The per-file map includes C1/C2 files near their ceilings; never minify or hide
implementation in entry modules to fit.

## Loosely coupled design to preserve

Compile-time ownership and runtime delivery are different:

```text
same execution process:
  daemon ------> fuse ------> workspace
     |                         |
     +-------> bridge <--------+   Workspace uses portable contract/Source only

runtime logical delivery supplied by daemon:
  Workspace -- OperationDelivery --> existing native bridge --> service
                                                        service --> C1/C2/C5
```

- **Daemon** owns process/configuration, authenticated control, connection assembly,
  signals and the shared native lifecycle handle. It binds the existing native
  Client to OperationDelivery. It does not implement file/tree/COW/history algorithms.
- **FUSE** owns fuser/kernel types, callback translation, errno/replies and native
  mount/session ownership. It calls only Workspace's public semantic API. It does
  not construct canonical objects, interpret Store formats or maintain a second
  filesystem state.
- **Workspace** owns portable filesystem semantics, one live/frozen state machine,
  private backing and submission lifecycle. Production dependencies are the Bridge
  contract with native disabled, sha2, and Linux platform support. It must not import
  fuser, daemon, native Client, service, C1 storage providers or C5 implementations.
- **Bridge** owns logical Request/Response/Failure/Source and authenticated framing,
  identity, deadlines and transport uncertainty. Reuse its existing native path;
  no second protocol, per-canonical-object RPCs or duplicate client.
- **Service** authorizes and composes actual C1/C2/C5 operations. Store/catalog paths
  and credentials remain there. Workspace requests logical operations; it does not
  reach into those implementations to perform private algorithms.

OperationDelivery is the existing narrow production closure capability, not a new
provider hierarchy. ProjectionInvalidation carries portable MutationReceipt and
deadline; FUSE binds kernel notification. Workspace never imports kernel types.
ProjectionReplyPermit/ProjectionMutationPermit implement bounded ordering; do not
replace them with an unbounded event bus or best-effort cache invalidation.

Keep the five Workspace groups private behind real production exports. Reuse
WorkspaceHost/Workspace instead of creating another manager/registry/state mirror.
Retain a selected handle and release the registry lock before work. Never hold
Workspace/registry state locks across backing I/O or remote save. The daemon's
lifecycle mutex owns native mount exclusion, not filesystem algorithms; ordinary
reads/writes do not queue behind it. Use immediate bounded admission, no hidden
workers, queues, automatic retries or extra consumers.

## Completed checkpoints and honest limits

R0-R/R1 readable Linux mount is implemented and mounted-tested. R2 portable metadata
save and the core R3 existing-file pipeline are implemented: bounded private COW,
maintained disk metadata/frontier, capture G/live G+1, Stage/CommitStaged/composite
Commit, known-own reconciliation and repeated incremental Commits. Selected actual
save-progress and failure schedules pass. This does not close the full matrix.

R4's ordinary existing-file WRITE/append/size SETATTR/O_TRUNC paths are implemented
and mounted-tested, including selected activity during actual C2 save. They use
an explicit direct-I/O writable projection. Production daemon CLI startup remains
--mount-readonly; the writable pipeline was exercised with the actual mount and
embedded Workspace caller, not a remotely managed writable daemon.

Authenticated Status, Unmount and CloseClean are implemented. Current control is
one fixed CLI-attached target, one live/closing session, Q=0, exact Workspace ID and
incarnation, independent daemon grants (Status 1 / Unmount 2 / CloseClean 4). Unmount/Close
use distinct response variants with WorkspaceLifecycleWire with WorkspaceLifecycleOutcome::{Completed,Retained};
pre-admission failures differ from entered retained outcomes, and uncertain delivery
is Unknown without replay. Service explicitly refuses daemon controls before Store
admission. Do not treat all Store permissions as daemon authority.

The latest native API is
Result<MountHandle, Box<MountFailure>> for mount and mount_writable. Failure exposes
phase/cause/retained owner. Every failure after lease reservation returns that owner
and stops admission; no LayerFS constructor cleanup or renewed deadline occurs.
A temporary Arc<Mutex<Option<Session>>> prevents failed worker creation from dropping
the only Session. Explicit unmount owns cleanup. Daemon failed startup uses the
original deadline, then retains process/owner until an explicit signal; cleanup
preserves the original startup error exit. See29 for dependency-owned destructor
cleanup and unexecuted failure subsets. Session::new already performs INIT; do not
repeat the earlier mistaken claim that a new INIT readiness barrier is required.

Latest native prerequisite: 21 functional PASS selections, one original fixture-
oracle FAIL retained; corrected oracle passed without product changes.19 passing
selections contain real Linux mounts; 2 are native refusal subsets. Host tests 659
PASS/3 ignored; Linux 657 PASS/120 ignored; actual ignored routes are selected
separately. Rust 1.85.1 locked builds/Clippy/fmt/guard and 6 guard self-tests passed.
These results are source-pinned, not automatically proofs of a future edited tree.
No performance, RSS/cgroup or durability qualification has been claimed.

## Ordered remaining work

1. **Authenticated Mount of the existing attachment.** Use exact target/incarnation,
   empty authenticated input, an independent grant, the current RO profile and
   existing lifecycle try_lock. Refuse mounted, closed, stopping or unresolved prior
   ownership; no implicit Unmount/Attach. Keep original deadline/terminal headroom.
   Install any returned partial owner in the same lifecycle slot before reporting
   Retained; match operation-specific response/identity and preserve Unknown.
   Prove CLI Attach -> mounted read -> authenticated Unmount -> authenticated Mount
   of the same incarnation -> actual metadata/read -> Unmount -> CloseClean, plus
   authorization, entered failure/retention and lost-terminal cases. The native
   prerequisite is done; implement this operation next.
2. **Remote Attach and remaining daemon management**, one operation at a time.
   Existing control has neither an empty/replaceable target nor WorkspaceHost.
   Reuse native attach; close its retained-failed-attempt observability/ownership
   before exposing it. Attach does not mount. Authorize Store/base/owner/access and
   producer incarnation; derive paths from configured root. Do not invent a second
   registry. Preserve the headless CLI route.
3. **Writable startup/edit/Commit controls and complete R4 qualification.** Select
   shutdown admission so dirty-close refusal does not strand the Workspace after
   stopping its only Commit control. Reuse implemented native operations and exact
   C5 outcomes. Resolve required remaining kernel/coherence cases; do not hide them
   behind the narrower passing profile.
4. **R5a namespace/new-inode/metadata/symlink/larger-input operations**, each with its
   actual shared prerequisite first. HistoryCommand::ReserveInodes already exists;
   inspect it instead of assuming the old audit's missing-operation list is current.
   Current limits include 128 prepared changed names/inodes/directory records,
   32 KiB metadata, 256 file edits/8 MiB replacement, 128 dirty inodes, 1024 pieces,
   256 resident nodes and 128 handles. Confirm current equivalents and extend their
   owning shared APIs where necessary. No hidden smaller Commits, unrelated inode
   bootstrap, full resident namespace mirror or automatic capacity growth.
5. **R5b declared npm install + explicit Commit + later incremental Commits.** The
   exact project/lockfile and Node/npm versions still need pinning with the user;
   this does not block Mount. Never shrink the selection to fit existing bounds.
6. **R6 separately declared matched mounted comparison against exact v0.1.6**
   `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`. No performance claim before eligible
   matched semantics, cache states, source/binary/harness identities and real mounts.

Still-open kernel limits include unobservable RWF_APPEND/RWF_NOAPPEND variants and
concurrent SDK resize versus cached-size mmap/splice/sendfile consumers. Ordinary
supported O_APPEND is separate. Preserve25/26's exact distinctions. Explicit failed
Stage/Commit disposition and broader schedules also remain open: Unknown never
permits replay, token substitution, guessed cleanup or dropping later edits.

## Persistent storage, capture and Commit invariants

One execution-host root derives workspace/<id> and private-backing/<id>; never
mount the common parent or expose backing in the user tree. Default accounted
Workspace working memory is 8388608 bytes per consumer, not RSS. W disk quota and
max-count require explicit positive inputs; no new numeric defaults. Service paths
and credentials stay at the service; container localhost is not the service host.

Preserve immutable base references, owned changed extents and Zero ranges; opening,
first writing, renaming and capture never copy whole files. Failed/partial allocations
stay charged. Maintain bounded metadata/dirty indexes while mutating; capture rotates
maintained roots without scanning/copying/paging I/O under the short state lock.
One submission slot serializes a Workspace's Commit, with one retained G per consumer;
live G+1 continues within admission bounds during actual service save. Save each
captured inode/version once across hard-link aliases. Metadata-only changes preserve
content roots; lowering uses captured coordinates and exact versions.

C1/C2 save the candidate; C5 stages exact root/context, then validates the stage,
inserts/verifies Commit, conditionally advances Branch head and removes the stage.
Known own success installs the acknowledged base while retaining later D1, even if
local reconciliation subsequently fails. AddLayer remains separate. No second
history catalog, implicit pressure/close Commit, restart recovery or crash-durability
claim. Keep MEMORY journal/synchronous OFF; no fsync/fdatasync/sync_data/sync_all/WAL.

## Verification, artifacts and worktree isolation

Before builds/measurements read current docs/general/benchmark_rules.md,
benchmark/AGENTS.md, benchmark/fs-bench-pro/QUICKSTART.md,
docs/roadmap/0.1/0.1.7/measurement-isolation.md and docs/general/release-policy.md.
No CI, aggregate preflight gate or tools/preflight.sh: it is permanently retired.
No third-party patches, vendoring, registry edits or reference-product fallback.
Do not close #179/#180/#181/#190/#205/#207/#210 or reopen closed component work.

Use subagents for bounded independent work with exclusive files. Tell every worker
that others share the checkout and preserve their edits. Parent integrates the
selected operation; do not duplicate overlapping investigations. Do not create
extra user-visible tasks to delegate ordinary subtasks.

Run locked Rust 1.85.1 checks with --manifest-path core/Cargo.toml from the new
worktree root, preserving root .cargo/config.toml's ARM AEAD flags. Host target is
that worktree's core/target and Linux target its core/target-linux. Run relevant
public/mounted/control tests, whole-core checks as required, warning-denying Clippy,
fmt, core/tools/check_product_boundary.py and its6 self-tests. Freeze product source
through builds/proofs. Keep failures and NOT_RUN rows; no mounted claim from a
headless/library-only route. Do not rerun passing selections without a relevant
change or unresolved concern. One sample per case/arm, fresh output, append-only
receipts; no warm/cold mixing, pre-touching, timeout inflation or shifted timed work.
Export LAYERFS_CONSTRUCTION_WORKERS=1; init_namespace alone retains its exception.

The available host is Darwin arm64; Docker runs Linux 6.12.76-linuxkit AArch64 with
/dev/fuse. Reuse tool image layerfs-pair1-rust-tools:c331f3815ef3cfb5c760 (ID
sha256:afa02fb391d3dec196c399131c3d3fe4f34b0e8d7352339220f664bf91eae408) and runtime
rust:1.85.1-bookworm (ID
sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4), after verifying
identities. Shared registry may be mounted read-only; Cargo targets may never point
outside the new worktree. Builds in separate worktrees may overlap; record interference
instead of interrupting another owner. Preserve each worktree's measurement lock.

The old ignored artifacts remain under the source worktree's core/target/pair1-evidence:
mounted-07/result.json plus service/store.sqlite and history.sqlite are the closed
RO fixture; large-edit-master-01/result.json plus its service/store.sqlite are the
64 MiB canonical input fixture. Copy needed fixtures as independent bytes into the
new worktree outside timers, retain original receipt/database SHA/provenance in a
new reuse manifest, and pass only new owned paths to drivers. Do not rewrite the
historical receipts or call fixture reuse a new verification/cold result. Native
writable drivers create a fresh live C5 producer; never reuse the RO history as
restarted write authority. Immutable binaries may only be reused with matching
seals/path assumptions; otherwise build once in the new owned targets. Old inputs
and binary archives are identified in 29/evidence; they are not part of Git.

External route entry points include daemon/tests/control_status.py,
control_unmount.py, control_close.py, mounted_read.py and mount_startup.py;
fuse/tests/mount_failure_route.py, kernel_write_route.py and kernel_resize_route.py;
workspace/tests/stage_route.py, coherence_route.py and support/native_workspace.rs.
Use these existing setup/identity/cleanup owners. Do not fork their algorithms into
a benchmark-specific product path. Read failure notes before changing their oracles.

After every operation update the packet with source identity, files/API, exact checks,
passing/failing/unrun routes, resource evidence and next dependency. For every Git
commit compare exact first parent with final staged/committed production snapshots
using tools/production_loc.py; record before/after/signed delta and separate reference/
core subtotals in both message and handoff. Recompute after changes to staged source.
A docs-only commit has unchanged product totals, not fictitious zero-sized product.

## Documentation checkpoint LOC

This handoff changes documentation only. Exact first parent `2e7f42ea22778e0119d5e3d0da806130bcd50330`;
counted staged tree `f5d5640ec1cd5f2dc8a3e3c100a5cced638b3f60`. Production LOC:
**108693 ->108693 (delta +0)**; reference65417 ->65417 (+0), core43276 ->43276 (+0).
Method: exact parent/staged `git archive` snapshots and the unchanged production
counter `b5b9617d08204977176302311e0b2c72a811b420`, with the same source scope/exclusions.
[Comparison](evidence/continuation-handoff/production-loc.json) and
[documentation validation](evidence/continuation-handoff/validation.json).
No Rust build or mounted proof is relabeled as a new result by this handoff.
