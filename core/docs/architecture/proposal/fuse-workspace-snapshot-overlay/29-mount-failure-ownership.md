# Retained native mount failure ownership

> **Status: implemented and verified native prerequisite; remote Mount remains separate.**
> Parent: `3c227266b6740cb9c50f63f0ba27a38d1b8b9a00`; target v0.1.7, not released.
> Product input seal: `375c2076db5299d601c7b8e18042a7831eeaa51e54911b1b11fb5f18610ead3d`.
> Selected 2026-09-22 before exposing the next authenticated Mount operation.

## Concrete failure and selected correction

The previous native mount path returned only MountError after Session construction,
invalidation binding, worker creation or final deadline failures. Some branches kept
Workspace.mounted charged when absence could not be established, but discarded the
lease/session/worker owner required for inspection and explicit cleanup. Passing a
Session directly into thread::Builder::spawn also lets a failed spawn destroy the
closure's Session. The final deadline path called unmount with a new callback budget.
Those source-level failure paths cannot qualify bounded remote Mount. Earlier
successful mount/WRITE/SETATTR receipts retain their original identities and scopes.

The existing read-only and writable mount functions now return
Result<MountHandle, Box<MountFailure>>. The concrete failure holds a phase, primary
MountError and optional retained MountHandle. Admission refusal precedes lease
reservation and has no owner. Every failure after reservation returns the exact
owner and stops new projection admission. It never replaces the primary error with
a cleanup error. LayerFS performs no automatic cleanup in that constructor path:
a later explicit owner.unmount has its own checked result and uses its caller's
deadline. A final expired deadline returns the owner immediately, without invoking
cleanup or extending the request budget.

One temporary Arc<Mutex<Option<Session<Adapter>>>> transfers a successful Session
to the existing mount worker. The worker takes Session, releases the mutex and
empty cell before run. On failed spawn, the original owner still holds Session.
There is no queue, helper worker, second device descriptor or alternate backend.
Boxed failure/transfer storage is reserved before entering native Session creation.
The existing unmount path also handles an owned Session without a started worker,
using its actual unmounter and checking absence before finishing the lease.

fuser 0.18 Session::new already performs its INIT handshake before returning. Its
reply sender attempts the INIT reply but logs send errors instead of returning
them; no new readiness or hard delivery guarantee is inferred. If that constructor
itself fails, its internal Session destructor may already attempt native unmount.
The no-automatic-cleanup rule above governs LayerFS, not that upstream destructor.
When no native capability is returned, explicit cleanup can verify absence or
retain failed/unknown ownership; it never adopts or detaches another mount by path.
The existing conservative terminal handling for failed detach/join remains.

## Daemon startup custody

Daemon startup retains the returned owner before attempting cleanup and closes its
prebound control listener when mount creation fails. Its first checked cleanup uses
the original mount deadline. If incomplete, the process, Workspace and mount owner
remain alive, with a retained diagnostic and without a ready/control advertisement.
Only another explicit termination signal admits a new 10-second cleanup attempt.
After checked cleanup the process still exits with its original startup error.
Control-start failure after a successful mount uses the same retained cleanup path.
Ordinary running control/signal shutdown keeps its prior semantics.

This is a native prerequisite, not authenticated WorkspaceMount or remote Attach.
The next Mount operation may target the existing CLI-attached exact incarnation;
remote Attach, writable startup/edit/Commit and the remaining namespace/npm/R6
requirements remain separate. No issue closure, performance or durability claim.

## Declared verification

External Linux tests hold the real FUSE INIT reply beyond the original deadline,
then continue that syscall; force worker creation to fail after an actual completed
Session constructor; deny native mount entry; and check pre-admission refusal.
They use thread-local seccomp in external callers, real public APIs and checked
mountinfo/descriptor identity. Product source gains no test hooks. An actual daemon
startup selection holds its real INIT past its original startup deadline and checks
retained process/listener behavior, then explicit-signal cleanup and original error.
Success-path RO/RW and lifecycle regressions use their existing route callers.
Raw outcomes, fixed allocation arithmetic and exact LOC follow after verification.

## Fixed allocation arithmetic

Compiler DWARF from the actual Rust 1.85.1 Linux AArch64 daemon records
MountHandle growing from 96 to 104 bytes (+8), MountFailure at 208 bytes and the
Session transfer Arc allocation at 144 bytes (128-byte Mutex/Option/Session plus
16-byte Arc header). The native failure Box plus transfer allocation therefore
requests 352 bytes. The daemon's Arc<Mutex<MountHandle>> allocation is 128 bytes;
when failed startup moves the owner there while retaining its primary error Box,
these three allocations request 480 bytes in total. These sums exclude allocator
overhead and existing Session/Config owned allocations, Workspace tables, transport,
thread stacks and kernel memory. They are object-layout evidence, not total heap,
RSS/cgroup or performance evidence. Existing allocations move without duplication.

On successful mount, the failure Box is released before return and the transfer
cell is released after the worker takes Session; only the extra 8-byte handle field
remains. On failure those owners stay bounded and retained until explicit cleanup.
No Workspace budget, backing quota/window, FD cap, worker count or queue is raised.
[Compiler layout and raw DWARF](evidence/mount-failure-ownership/checks/layout.json).

## Actual verification and retained failure

**21 selections pass; one original admission-oracle failure is retained.** Nineteen
passing selections contain actual Linux FUSE mounts, including the daemon startup
failure route; two are native admission/mount-syscall refusal subsets without a
successful mount. Every selection fits its 60-second complete-command budget.

| Selection | Status | Complete command seconds |
| --- | --- | ---: |
| admission-01 | FAIL | 0.511208542 |
| admission-02 | PASS | 0.694752334 |
| daemon-startup-01 | PASS | 11.134311041 |
| deadline-01 | PASS | 4.330188125 |
| regression-close-close_authority-01 | PASS | 1.969009250 |
| regression-close-close_cleanup_failure-01 | PASS | 1.040749708 |
| regression-close-close_loss-01 | PASS | 1.084868083 |
| regression-close-close_mounted-01 | PASS | 0.956587208 |
| regression-close-close_success-01 | PASS | 0.969737875 |
| regression-coherence-notify_failure-01 | PASS | 0.743851000 |
| regression-coherence-visibility-01 | PASS | 0.898858042 |
| regression-unmount-authority-01 | PASS | 1.943193291 |
| regression-unmount-expiry-01 | PASS | 10.496120041 |
| regression-unmount-loss-01 | PASS | 1.123908791 |
| regression-unmount-partial_input-01 | PASS | 1.188503458 |
| regression-unmount-shutdown_retained-01 | PASS | 11.096920667 |
| regression-unmount-signal-01 | PASS | 1.032839584 |
| regression-unmount-success-01 | PASS | 1.244917708 |
| regression-unmount-timeout-01 | PASS | 1.409457916 |
| session-01 | PASS | 0.720123334 |
| success-01 | PASS | 0.716492375 |
| worker-01 | PASS | 0.720990333 |

The native deadline case validates a real blocked INIT writev, its character-device
identity, reply header and mountinfo before crossing its original one-second
budget. It continues that exact syscall; Mount returns Deadline with its initialized
Session owner retained. A different unfiltered thread explicitly unmounts, checks
absence and clean-closes. Native clone3/clone EAGAIN produces Worker failure after
Session initialization; the same kernel mount remains until checked cleanup.
Native mount EACCES produces Session failure with a retained lease and no successful
mount; explicit absence verification releases that owner. EACCES deliberately
avoids the dependency's EPERM mount-helper path. Neither test fabricates a successful
FUSE response or adds a product fault switch.

The original admission case tried attaching a Workspace with a foreign root UID,
then expected FUSE to refuse it. Current WorkspaceHost instead correctly refuses
that ownership mismatch before reserving a registry entry. The original FAIL,
caller/binary and test output remain intact. The corrected oracle asserts that
actual boundary, unchanged accounting and no foreign directory, alongside ownerless
Mount deadline and read-only capability refusals. It passes in admission02 with
unchanged product inputs. Its retained failed-test container had no FUSE mount;
separate external cleanup records checked absence and removed only that owned
container/volume. Passing deadline/worker/session selections were not rerun.

The corrected caller and the not-yet-run success selection use inputs02. Success
mounts the same attached Workspace read-only, checks bytes and EROFS, explicitly
unmounts, then mounts writable, writes/reads bytes, performs an actual native Commit
and verifies saved content before checked cleanup. The source seal is unchanged;
inputs01 and02 separately pin the exact executed callers/binaries. Artifact
preparation also retained a refused archival attempt: a package-only build used a
different Cargo executable path. An assertion prevented reusing the old binary;
the corrected test was rebuilt with the original whole-workspace feature unification
and selected from Cargo output. No stale executable was run as a corrected proof.

The actual daemon supervisor observes INIT major 7/minor 40 and an 80-byte reply,
holds it for 10.251249089 seconds, and continues the original syscall. The daemon
reports its original Deadline, keeps the exact mount/process alive and closes its
prebound control listener without advertising readiness. One later SIGTERM detaches
and closes it; the process exits 1 with the original startup error. The test declares
SYS_PTRACE and an unconfined container seccomp/AppArmor profile, then installs its
own inherited writev notification filter. It is a lifecycle fault proof under that
profile, not a claim about deployment confinement or syscall preemption.

All eight Unmount and five CloseClean regression selections pass on this product
seal, including retained timeout, lost terminal, actual held handles, signal overlap
and checked cleanup failure. The existing mounted SDK visibility and actual
notifier-send-failure selections also pass. No kernel write/resize algorithm changed;
the RO/RW success case and those binding/lifecycle regressions cover the changed
constructor/ownership route without relabeling the earlier full writable matrix.

Rust 1.85.1 locked host whole-core tests pass 659, zero failures/three ignored;
Linux whole-core tests pass 657, zero failures/120 ignored. The ignored native cases
are selected separately above; the aggregate test count does not claim they ran.
Whole-core host/Linux all-target Clippy with -D warnings and bins/examples builds
pass, as do fmt, the 241-file boundary guard and all six self-tests. The corrected
external test was rebuilt without running already-passing suites and its owning
crate Clippy/fmt passed. No compilation, unit or lint failure occurred.

[Append-only functional index](evidence/mount-failure-ownership/functional-index.json),
[original compiled/executed identities](evidence/mount-failure-ownership/mount-failure-inputs-01.json),
[corrected caller identities](evidence/mount-failure-ownership/mount-failure-inputs-02.json),
[exact commands and check scope](evidence/mount-failure-ownership/checks/commands.json),
and [source review/qualifications](evidence/mount-failure-ownership/source-review.json).
Builds did not overlap these selections in this worktree; every row retains its
other-worktree interference snapshot. Fixtures reuse independent byte copies,
without cold/warm performance claims. Construction workers remain 1. ARM build
config SHA256 remains 3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9.

Reproduce a declared case once at a fresh owned output path:

```sh
python3 core/crates/layerfs-fuse/tests/mount_failure_route.py \
  --fixture core/target/pair1-evidence/large-edit-master-01/result.json \
  --binaries <host_binaries-from-inputs> \
  --test-binary <mount_failure_test_binary-from-inputs> \
  --case deadline --output <fresh-owned-output>
python3 core/crates/layerfs-daemon/tests/mount_startup.py \
  --fixture core/target/pair1-evidence/mounted-07/result.json \
  --binaries <host_binaries-from-inputs> \
  --linux-daemon <linux_daemon_binary-from-inputs> --output <fresh-owned-output>
```

Actual binding failure, constructor-internal cleanup failure/native failed detach,
Control::start thread-creation failure and the distinct post-worker deadline cut
remain NOT_RUN subsets. Their conservative source handling is not promoted to an
executed proof. Remote Mount/Attach, writable daemon management, namespace/new-inode/
larger-input/npm, R6 and RSS/cgroup qualification remain open. No issue is closed.

## Exact production LOC and source files

First parent `3c227266b6740cb9c50f63f0ba27a38d1b8b9a00`; counted staged tree
`2a7fc89dcbc7fc58163f68f070dd0d0bca25f9be`. Production LOC: **108562 → 108693
(delta +131)**. Reference 65417 → 65417 (+0); core 43145 → 43276 (+131).
FUSE 963 → 1062 (+99); daemon 824 → 856 (+32). Workspace, Bridge, Service and
reference algorithms are unchanged. Growth preserves the native failure owner,
its phase/cause and checked startup cleanup; no migration/deletion benefit is claimed.
[Exact changed source/test paths](evidence/mount-failure-ownership/changed-files.json).

Method: exact `git archive <revision> crates core/crates` snapshots followed by
identical `python3 tools/production_loc.py --root <archive> --json`, counter blob
`b5b9617d08204977176302311e0b2c72a811b420`. Count nonblank/noncomment production
Rust/runtime SQL; exclude inline/external tests, fixtures, examples, docs, tools,
manifests and generated output. [Machine-readable comparison](evidence/mount-failure-ownership/production-loc.json).
Final documentation/evidence additions preserve the counted production tree.
