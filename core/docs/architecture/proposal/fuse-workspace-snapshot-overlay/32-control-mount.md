# Authenticated Mount of the existing attachment

> **Status: implemented and verified through actual authenticated daemon/Linux mounts; target v0.1.7, not released.**
> Implementation parent: `3e5d6a66a9f4e4df4a8a19087e63909a18046c6d`.
> Selected 2026-09-22; continuation worktree `795c/layerfs`.
> Exact implementation commit: `8639c6bda9e5911c3d434e659380d99b0bcf8d88`.

## Selected operation and ownership

This round exposes only Mount of the exact CLI-attached Workspace/incarnation.
`Operation::WorkspaceMount` uses existing profile 3, opcode 12, positive request ID,
zero Store/generation/result-data allowance, existing managed-name/63-byte and
nonzero incarnation bounds, empty authenticated END_INPUT and 1..5000 ms budget.
The independent daemon grant is bit 8; masks 0..15 are valid. Service rejects this
control before Store admission. No Attach, implicit Unmount, replacement identity,
new transport or writable profile is introduced.

`Response::WorkspaceMount(Box<WorkspaceLifecycleWire>)`, tag 14, preserves exact
operation/identity correlation and the existing Completed/Retained outcomes.
Completed means the existing native read-only constructor returned its owner,
including its current INIT semantics; it is not a hard terminal-delivery promise.
Pre-admission failures have ordinary Failure terminals. A constructor failure with
no retained owner remains a refusal; an entered failure first installs its returned
owner in the lifecycle slot and then returns Retained. Missing/malformed/mismatched
terminals remain Unknown and never authorize replay or guessed cleanup.

The existing lifecycle allocation becomes `Arc<Mutex<Option<MountHandle>>>`.
Some means a live or unresolved native owner; only checked successful Unmount clears
it. Mount refuses a nonempty slot and mounted, stopping or closed Workspace state.
It never overwrites an unresolved owner or drops one to make a new attempt succeed.
Status remains a fresh observation, not a substitute for a missing Mount receipt.
Signal shutdown joins control and uses this same slot before clean closure.

The lifecycle try_lock is immediate bounded admission. Workspace locks end before
native I/O; the owner slot spans the native attempt, not ordinary filesystem work.
Authorization, identity, complete input, expiry, deadline and stopping checks retain
the existing ordering. Mount reserves the same 100 ms terminal headroom inside the
original deadline. Native syscalls have observation bounds, not preemption; no
renewed deadline, automatic cleanup, extra worker, queue or retry is added.

Daemon owns configuration/control/native lifecycle assembly. Bridge owns contract,
codec and authenticated exact response matching. Service owns early control refusal.
FUSE and Workspace retain their existing algorithms and dependency boundaries.
Per-file/folder LOC, source identities, actual checks and resource observations are
recorded below and in the current source inventory. No RSS/cgroup/performance or
durability qualification is inferred from functional route completion.

## Declared checks and remaining scope

Verify CLI Attach → actual mounted read → authenticated Unmount → authenticated
Mount of the same incarnation → actual metadata/read → Unmount → CloseClean.
Check independent grants/target/incarnation/service authority; mounted/closed/
stopping/unresolved refusal; entered native Session failure retaining its owner;
explicit cleanup; complete-input/deadline admission; lost terminal with Unknown
and no replay. Run affected Unmount/CloseClean regressions and locked Rust 1.85.1
core test/Clippy/fmt, boundary guard and its external self-tests. Each actual
functional selection has one fresh output and a 60-second complete-command budget.
Fixture copies are setup reuse, not a cold or new-proof claim.

Remote Attach must next close native failed-attachment registry ownership and
observability before exposing a replaceable target. Writable startup must preserve
Commit access after dirty-close refusal. Full R4, R5 namespace/new-inode/shared
bounds, the user-pinned npm workload and R6 remain open. Records 25/26 retain their
RWF_APPEND/RWF_NOAPPEND and concurrent SDK-resize mmap/splice/sendfile distinctions.

## Actual verification and retained failures

Final product input seal:
`41eb35f3c8be9f524570868cef18ebeef020ceac247c5aea4dc56477209cde35`.
**24 selections PASS: 11 Mount, eight Unmount and five CloseClean regressions.**
Each creates a real Linux6.12.76-linuxkit AArch64 mount, checks authenticated
control through the production native Client, completes normal daemon/Workspace
cleanup and removes its owned container/volume. All verify Docker's two-CPU
quota and remain within the unchanged60-second functional budget. These wall
values are functional budget observations, not performance measurements.

The initial mount_success01 failed before Unmount/Mount in the initial read
oracle; it remains FAIL with forced cleanup recorded. Its original exact assertion
cannot be recovered because the diagnostic handler lost subprocess stderr when
it treated text as bytes. The corrected oracle accepts the existing supported
fuse/fuse.layerfs mount types with exact source layerfs and preserves subprocess
stdout/stderr for both text and bytes. Success02 passes with unchanged product and
binary identities. No first result was overwritten or promoted.

| Selection | Status | Complete command seconds |
| --- | --- | ---: |
| mount-mount_authority-01 | PASS | 2.943304958 |
| mount-mount_closed-01 | PASS | 1.186046917 |
| mount-mount_deadline-01 | PASS | 1.172641125 |
| mount-mount_expiry-01 | PASS | 10.304438625 |
| mount-mount_loss-01 | PASS | 1.519487209 |
| mount-mount_mounted-01 | PASS | 1.863872792 |
| mount-mount_partial_input-01 | PASS | 1.163643875 |
| mount-mount_session_failure-01 | PASS | 1.530572833 |
| mount-mount_stopping-01 | PASS | 1.232906083 |
| mount-mount_success-01 | FAIL | 4.246197125 |
| mount-mount_success-02 | PASS | 1.761019208 |
| mount-mount_unresolved-01 | PASS | 1.960158209 |
| regression-close-close_authority-01 | PASS | 2.171606208 |
| regression-close-close_cleanup_failure-01 | PASS | 1.148313208 |
| regression-close-close_loss-01 | PASS | 1.224230292 |
| regression-close-close_mounted-01 | PASS | 1.127008917 |
| regression-close-close_success-01 | PASS | 1.072266875 |
| regression-unmount-authority-01 | PASS | 1.976415666 |
| regression-unmount-expiry-01 | PASS | 10.697853167 |
| regression-unmount-loss-01 | PASS | 1.161690209 |
| regression-unmount-partial_input-01 | PASS | 1.459262042 |
| regression-unmount-shutdown_retained-01 | PASS | 11.188673708 |
| regression-unmount-signal-01 | PASS | 1.086820667 |
| regression-unmount-success-01 | PASS | 1.097440625 |
| regression-unmount-timeout-01 | PASS | 1.525365917 |

The success route verifies full file bytes, size/mode/mtime, hard-link inode
identity, symlink target and EROFS before and after same-incarnation remote Mount,
then performs explicit Unmount and CloseClean. Authority tests independently reject
old mask7, individual prior grants, no grant, wrong target/incarnation and the service
endpoint; a Mount-only peer cannot invoke Status/Unmount/CloseClean. Expired
established credentials, incomplete authenticated input and a50ms request with
insufficient terminal headroom do not enter native Mount.

Already mounted, closed, stopping clean-close and retained-Unmount targets refuse
Mount without replacing owners. Temporarily renaming the empty owned mount leaf
forces Session construction failure after lease admission: Retained(Io) is observable,
a second Mount is Busy, explicit Unmount checks absence and releases that retained
lease, then restoring the same leaf permits Mount/read. The lost-terminal proxy
submits exactly one Mount, receives Unknown, and uses only fresh Status/read followed
by explicit cleanup. Subsequent observations never become the missing result.

No build overlapped these selections. Other-worktree interference snapshots remain
in every receipt; no quiet-host claim is made. Closed RO fixture receipt/databases
were independently byte-copied into this worktree, then each case used its own copy.
Reuse is preparation only, with no cold claim or restarted writable C5 authority.
Original inputs01 and corrected test inputs02 retain separate hashes while binding
the same immutable binaries. Original Mount failure subsets in29 remain qualified
only there; this round does not relabel them as authenticated remote proofs.

[All rows and raw receipts](evidence/control-mount/functional-index.json),
[exact commands](evidence/control-mount/functional-commands-02.json),
[compiled identities](evidence/control-mount/control-mount-inputs-01.json),
[corrected caller identities](evidence/control-mount/control-mount-inputs-02.json),
[oracle diagnosis](evidence/control-mount/checks/mount-oracle-diagnosis-01.json).

## Core checks, cancellation prerequisite and resources

Initial whole-core locked Rust1.85.1 tests passed host659/0failed/3ignored and
Linux657/0failed/120ignored. Those runs bind their original source seal in the
check ledger. Clippy then rejected a three-tag OR pattern; changing it to the
identical12..=14 range fixed that lint. The separate user-reported cancellation
busy loop was reproduced and corrected at the shared Pipe boundary, as recorded
in[33](33-cancelled-pipe.md). Final affected Bridge/daemon/service tests pass88
on each platform, including the new cancellation tests. Unrelated already-passing
core tests were not rerun or relabeled as final-source executions.

Final whole-core bins/examples builds, all-target warning-denying Clippy on host
and Linux, fmt, the241-file boundary guard and six guard self-tests pass. The
initial lint failure and two intentionally failing pre-fix cancellation probes
remain in the[check ledger](evidence/control-mount/checks/commands.json). All builds
are locked, use owned targets, preserve root.cargo/config.toml's ARM AEAD flags,
and export LAYERFS_CONSTRUCTION_WORKERS=1. No CI or aggregate preflight was run.
After the CPU report, heavy checks were serialized with two Cargo jobs and two
Docker CPUs; no timeout or product worker bound was relaxed.

Compiler DWARF of the actual final Linux daemon reports the existing lifecycle
Arc allocation at128bytes, unchanged from29 despite its optional handle. There is
one live/retained mount owner and no additional worker/queue/FD capacity. The native
constructor reuses its existing bounded failure/Session-transfer allocations and
100-byte result envelope. These are object-layout/source-capacity observations,
excluding allocator overhead and existing Workspace/transport/kernel allocations;
RSS/cgroup and performance remain NOT_RUN.
[Raw layout](evidence/control-mount/checks/layout.json) and
[source review](evidence/control-mount/source-review.json).

## Current continuation

Remote Attach next requires explicit failed-attachment ownership, observation and
cleanup in the existing WorkspaceHost registry. Its current failed Entry can remain
charged without an observable/disposable target; the local owned-mount-leaf flag
and post-arena allocation failure must be closed before remote exposure. This is
an identified prerequisite, not implemented by Mount. Writable controls must select
shutdown ordering that preserves Commit access on dirty-close refusal.

The owner has replaced the network-dependent npm-install selection with a prepared
`@deepseek-ai/dsh` dependency-directory upload: prepare and pin locally outside test,
then perform real mounted filesystem writes and explicit/repeated Commits. Linux
platform-native dependencies must match the execution host. Preparation does not
credit measured writes or Commit; network acquisition will not enter that workload.
Full R4, namespace/new-inode/shared-limit work and matched R6 remain open. No issue
is closed and Pair2 is not inferred complete.

## Exact production LOC and file ownership

First parent `3e5d6a66a9f4e4df4a8a19087e63909a18046c6d`.
Production LOC: **108693 →108781 (delta +88)**; reference65417 →65417 (+0),
core43276 →43364 (+88). Bridge4340 →4390 (+50), daemon856 →892 (+36),
service2031 →2033 (+2); Workspace/FUSE and other crates are unchanged.
The cancellation correction contributes zero production LOC. Growth implements
one authenticated lifecycle operation; no relocation/reference retirement or
algorithmic simplification is claimed.

| Changed production file | Before P | After P | Signed delta | After physical lines |
| --- | ---: | ---: | ---: | ---: |
| `core/crates/layerfs-bridge/src/adapters/native/client.rs` | 426 | 433 | +7 | 449 |
| `core/crates/layerfs-bridge/src/adapters/native/pipe.rs` | 112 | 112 | +0 | 132 |
| `core/crates/layerfs-bridge/src/adapters/native/protocol/metadata.rs` | 759 | 771 | +12 | 808 |
| `core/crates/layerfs-bridge/src/adapters/native/protocol/response.rs` | 800 | 810 | +10 | 827 |
| `core/crates/layerfs-bridge/src/contract/control.rs` | 81 | 85 | +4 | 96 |
| `core/crates/layerfs-bridge/src/contract/outcome.rs` | 159 | 160 | +1 | 177 |
| `core/crates/layerfs-bridge/src/contract/request.rs` | 651 | 667 | +16 | 713 |
| `core/crates/layerfs-daemon/src/config.rs` | 234 | 234 | +0 | 248 |
| `core/crates/layerfs-daemon/src/control.rs` | 274 | 304 | +30 | 325 |
| `core/crates/layerfs-daemon/src/run.rs` | 199 | 205 | +6 | 221 |
| `core/crates/layerfs-service/src/operation/dispatch.rs` | 55 | 56 | +1 | 65 |
| `core/crates/layerfs-service/src/owner.rs` | 147 | 148 | +1 | 184 |

[31](31-source-map-and-loc.md) and its[complete inventory](evidence/control-mount/source-loc.json)
record every file and recursive folder P/L/file-count and each crate's ownership.
[Exact snapshot comparison](evidence/control-mount/production-loc.json) uses the
unchanged production counter blob`b5b9617d08204977176302311e0b2c72a811b420`
on git archives of the first parent and staged tree, with identical source scope,
inline-test removal and comment/blank exclusions. Tests/docs/tooling/manifests do
not enter these totals. Final staged/committed product trees are checked against
that counted snapshot; later evidence additions do not change production source.
