# Handoff: finish R2 after the verified native-consumer checkpoint

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Work in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` on local `main`. The owner asked
this agent to stop after the next verified checkpoint, prepare this handoff, and
pause its active R2 Goal. That stop supersedes the earlier instruction to continue
through component checkpoints. The [forwarded owner request](checks/r2-native-consumers-20261008/38-owner-stop-request.txt)
is retained. **Full R2 is not complete.** Resume implementation only when the owner
dispatches/resumes the next agent; this saved handoff is not itself a new dispatch.
Do not start another chat, automation, remote publication or worktree.

## Exact checkpoint and scope

Latest production checkpoint:
`edeb8b35024823f905252efd8333089ac0610168`, tree
`4d12c9285d21ef242db0ee3933fcae7611974859`.
First parent: `8431b6ba9fd2703b74d92905e3a7b3108586b6bd`.
Current product identities, also applicable to a subsequent documentation-only
handoff commit, are:

| Product | Git tree |
| --- | --- |
| `core/crates` | `d20bfd96156d3f3a873bbf53a25f008aaa7317aa` |
| root reference `crates` | `498dd1917812ae90efb8841f57e22bfc284e96fb` |
| preserved `core/crates/layerfs-fuse-legacy` | `be11d96ec9db31b897109323379fec9f3e009dae` |

[Commit confirmation](checks/r2-native-consumers-20261008/39-commit-confirmation.json),
[final source manifest](checks/r2-native-consumers-20261008/23-final-source-identity.json),
[results](checks/r2-native-consumers-20261008/36-results.md) and
[architecture75](../../architecture/75-native-request-service.md) are the current
component record. Inspect current HEAD/status before editing; a documentation-only
commit may follow the production checkpoint. Neither this source nor the current
application is mounted-FUSE qualified.

There are13 active core members, including the real replacement Fuse. The old
Fuse was moved byte-for-byte to excluded `layerfs-fuse-legacy` at8431b6ba9; do not
build it, import it or delete it. Root reference remains unchanged.

## Binding instructions

Read root/core AGENTS, both root handbooks, the [303 index](../303/README.md) and
relevant primary operation/engine/FUSE/integration contracts. The
[S8 specification](S8-SPECIFICATION-20261008.md),
[implementation plan](S8-IMPLEMENTATION-PLAN-20261008.md),
[proof plan](S8-PROOF-PLAN-20261008.md),
[reviewed ownership](R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md),
[file layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md) and
[original implementation assignment](HANDOFF-R2-R5-IMPLEMENTATION-20261008.md)
retain their limits. This next dispatch targets full R2; R3–R5 are separate
unfinished work, and no R6–R9 execution or early retirement is authorized here.

Global Store execution is explicitly Disposable/WAL/OFF only. Durable remains
NOT_RUN, disabled until explicit owner reauthorization. Overlay is one initialized
MEMORY/OFF/EXCLUSIVE database per daemon. Every daemon opens the same global Store
directly from a shared named Linux VM volume; host Init/seal/install precedes
control-only host activity. No host data server, legacy fallback or command
supervisor/registration/automatic timeout/implicit Commit or unmount.

The user already authorized the narrowly scoped fuser0.18.0 lifecycle extension;
see [the decision](FUSER-LIFECYCLE-DECISION-20261008.md) and
[original interface proposal](checks/r2-native-prerequisite-20261008/05-lifecycle-interface-proposal.md).
Do not ask again for that same scope. Timestamp and lifecycle provenance remain
exact. Run `python3 -B core/tools/check_fuser_integrity.py` before native fuser
builds. No other third-party edit, fork, replacement or registry patch is allowed.

## What is implemented

One fixed `K=read_handles+2` Fuse dispatcher is shared across mounts and survives
zero-mounted intervals. Each mount has16 admitted request slots and2 borrowed
receive guards; owned input is copied only after admission. Runnable steps rotate
across mounts; real completion/credit notifications resume parked work. Weak
notifications avoid request/completion cycles. Panic/error state, original
continuations and join outcomes remain retained. Dispatcher Drop is not detach,
drain or native success; its owner belongs outside its workers.

`service/filesystem_port.rs` adapts actual Owner admission/Pending futures and
Store ReadTicket/ReadLease to Fuse ports. It contains no second dispatcher or
registry. `ServiceReply` retains completion credit while any original shared
payload remains in use. LocalRead is borrowed directly from its original
Completion. Admitted immutable views reuse CanonicalClient/cache; no provider I/O
runs without actual reader admission and no reader is held across a later SQL wait.
Admission currently precedes an immutable step even if some/all demands hit the
cache; this is functional integration, not cache-hit performance qualification.

NativeRead drives the existing Workspace semantic plan for lookup/getattr/open/
opendir. READ/READLINK consume metadata projections and their original Completion
before asking for a data job, retaining independent FileRead/source capabilities.
This fixes a reproduced16-slot SQL-credit cycle without raising limits. Data paths
reuse existing `read_file_window`/`readlink_window`; no encoder is duplicated.

DirectoryStream/DirectoryBatch use backed native directory sources and64-key
pages. Bounded, independent copies consume read/cookie response payloads before
the next ordinary SQL job. Actual accepted prefixes, including zero, are published
once; unused reservation IDs are not valid positions. Empty whiteout windows
continue with bounded yielded turns. Reads survive RELEASEDIR. GETATTR handles
are classified by one indexed file/directory union and atomic source acquisition;
there is no failed-operation fallback between kinds. Original inputs, source,
page/cookie replies and publication-prefix context survive failures.

Linux callbacks wire LOOKUP, GETATTR, OPEN, OPENDIR, READ, READLINK, READDIR,
RELEASEDIR, RELEASE and FORGET. READDIR output is bounded to128KiB using the pinned
24-byte fuse_dirent header/8-byte alignment. Kernel inode1 is the canonical root;
other serials use checked `serial+1`, rejecting the unused root alias. UID/GID are
configured command identities, directory nlink2, times derived from portable
mtime. Negotiation records offered/selected flags separately and requests the
selected128KiB, background1/congestion1 profile. These callbacks have compiled on
Linux; none has yet been accepted through the actual application mount.

## Verification and exact reusable builds

Current consumer suite:4 host and4 Linux cases. It covers the real Owner/Store
pipeline, reader parking/progress, full16 metadata pressure,128KiB inherited/local
bytes, original failed input, directory GETATTR, partial/zero cookie prefixes,
16 offered batches and reads after descriptor close. The expanded host directory
case ran separately after its scoped extension. Locked builds, all-target Clippy
on Fuse/Daemon/Overlay, formatting,804-file guard and49 tooling tests pass.
Earlier dispatcher/attribute/engine evidence remains valid only at its unchanged
component scope; do not call it native acceptance.

| Artifact | Identity / use |
| --- | --- |
| Linux toolchain image | `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6` |
| Host test `core/target/debug/deps/filesystem_port-9a367c3ce80975d4` | SHA256 `c3ae651dd12cadbed4dad97d38b7a3a0a8135ee2bdc54355f025ec744a103ebb`,7709744bytes |
| Linux test `core/target/cluster2-linux/debug/deps/filesystem_port-4fb32d26badddcfb` | SHA256 `111ad36736c77d8eda2cf55172717b309aca87e9324ab4bdec9bbcb53007daa0`,116324816bytes; matches executed suite |
| Host runtime | Darwin25.4.0 ARM64, Rust1.85.1 |
| Docker | Engine29.5.2 ARM64; actual kernel/mount qualification must be recorded on the next native proof |

Linux build environment: repository bound at `/work`, cwd `/work`,
`CARGO_HOME=/work/core/target/cluster2-linux-cargo`,
`CARGO_TARGET_DIR=/work/core/target/cluster2-linux`,
`LAYERFS_CONSTRUCTION_WORKERS=1`. Reuse image/build inputs, not mutable measurement
samples. Serialize Cargo/tests/proofs with nonblocking fcntl lock
`core/target/r2-completion.lock`. Preserve root ARM flags; locked Cargo1.85.1
commands, build with `--no-run` first, explicit runtime wall bound≤120s. Component
invocations here use100s, internal waits3s; native application proofs require
their stricter9s selection. Read the measurement workflow and report template
before any new proof selection. No cold/performance/residency claim exists here.

The application still exposes R1 control/binding behavior. Do not treat an old
application/example/image binary, or a passing `native_application` case whose
`native_fuse_ready` is false, as this native implementation. Build and pin actual
application/image artifacts after wiring it.

## Immediate next implementation and acceptance

1. Complete inline/refusal/opcode accounting in
   `core/crates/layerfs-fuse/src/request/{callbacks,state,failure}.rs` and dispatch
   diagnostics. FLUSH/FSYNC/FSYNCDIR succeed with no SQL/durability work; STATFS has
   declared nonzero free space,4096 block size and255-name limit; xattr ENOSYS is
   sticky. The remaining fuser defaults are not a completed policy. Distinguish
   one BATCH_FORGET opcode from each independently bounded ownership unit. Preserve
   every reply/no-reply disposition and terminal wakeup under full16+2 capacity.
2. Implement first-party `mount/syscalls.rs` and `session/` under Fuse. Use safe
   existing nix, an independently owned /dev/fuse fd, `Session::from_fd` and the
   authorized runner/monitor. No Session::new/spawn/unmounter/helper/lazy fallback.
   Select two shared-fd loops and the exact profile. Current SessionMonitor has
   snapshot/condition waits but no nonblocking event subscription. Integrate
   first-exit admission fencing under the existing authorized lifecycle scope,
   with exact provenance updates if needed; full request capacity must not prevent
   that terminal signal. No busy polling or helper thread per waiter/request.
   Keep every original partial-start/error/panic/join outcome. Reply delivery
   results remain unavailable by the selected pinned API; do not invent them.
3. Add daemon `application/filesystem.rs` assembly using the same Store/Overlay and
   one shared dispatcher. Inspect `application/{owner,config,serve}.rs` and
   `control/{registry,operations,status,types}.rs`. Keep the existing registry as
   sole authority. Compose native serving with core/control readiness; no Ready
   from a spawn or one stat. Status/drain must cover admitted/received/runnable/
   parked work, owner completions, Store consumers, file/dir/lookup ownership and
   control producers. Dynamic payload/library/kernel memory remains a proof gap;
   `future_bytes` measures inline future allocation only, not total RSS.
4. Extend Bridge `control_{request,reply,types}.rs` and SDK
   `workspace/{api,binding,status,unmount}.rs` with additive Attach/Locate/Ready.
   SDK mount has two original acknowledgements: bind then attach. A lost Attach
   stays bound/unknown; Locate observes without replay or guessed resolution.
5. Implement reversible normal unmount. Service kernel requests throughout the
   plain umount probe; EBUSY restores usable Ready without ENOTCONN/EIO injection.
   Cached lookups and idle receive reservations do not count as busy work. Retained
   external cwd/FD/O_PATH/mappings still make the kernel busy. Detach, every loop
   join and all actual consumers must end before revocation, logical Close and
   registry removal. No command-exit fence or process killing. Retain connection
   abort authority safely for later R6; do not claim force teardown complete.
6. Change only required Sandbox Docker access configuration in
   `backend/docker/{container_types,container,topology,inspect}.rs` and application
   setup. SYS_ADMIN and selected /dev/fuse access must preserve Privileged=false,
   no-new-privileges and ordinary nonroot command identity. Reprove backing,
   credentials, /proc aliases and device/control protection after changing powers.
7. Complete the owning R2 native proof portions: FP-1/2/3/4, parked/fair progress
   and background HOL (FP-8/9), read-side permissions/times/opaque symlink limits,
   required inline behavior, read-only FP-20, FP-21, exact ownership/normal-detach
   FP-31/34, lost Attach/Locate FP-25-Routes and actual runtime visibility/access
   successors. Follow the proof plan's C1/C2/C3/C4 scoping: R2 does not close rows
   whose mutation/force portions belong to R3–R6. Use host Init/seal/install into a
   named VM volume, real mounts, independent complete-root oracle, ordinary external
   unregistered filesystem access, retained cwd/FD/O_PATH Busy, continued usability,
   repeated fresh mounts and sustained same-mount work, then full normal drain.
   READLINK boundary/native signed-time limits remain to qualify; the historical
   fractional signed-minimum FAIL is not erased by ordinary attribute tests.

No new immutable miss-coalescing/flights algorithm was selected. Do not turn an
optional optimization or a Content rewrite into a prerequisite for finishing R2.

## Failures, resources and preservation

The original16-slot stall and iterator compile error remain in the checkpoint
receipts. Metadata/source ownership was preserved while fixing progress. Raw
stdout includes terminal blank lines: full whitespace scans flag those original
transcripts; scoped source/docs/tooling checks pass. Do not rewrite raw evidence.

The failed pressure fixture remains at
`/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-56560-full-native-handoff`.
[State33](checks/r2-native-consumers-20261008/33-state-and-runtime.json) pins its
files. The test exited; it created no kernel mount. Retained SQL rows are failure
evidence, not a running daemon or an original operation to replay/adopt.

No owned check container, Cargo process, test process or native mount is left
running at this checkpoint. Removed consumer containers:
`e6ad01c4fdbe3d34229953bcc161c720efef80b39cf7e625ad71b4475cdcb37b` and
`a52cc5659d583e518493c3f140729e5a094e145b2e3a1a14e105849a44fa7ef3`.
The preceding service checkpoint removed its four acknowledged check containers.
Do not touch these unrelated running containers:

- `9cf2fe345496b45d79ff272e6c698552948f1e92ea106926826a770592623b24`
- `ce75ac504df9d0e58cedcdd0df93fe7ef39c3b66ef3c13b0bb6b580def64516c`
- `d2433851ea5990e273b0673fdbd5c180917f1216f3c81bdfb4517da0207e2c2a`
- `d2550144998b4b71de98cb9ecf6448f87c7b99a3c2f69ea778a29413fffa5ffb`

Preserve unrelated untracked files: `HANDOFF-PRE-S8-SERVERLESS-20261007.md`,
`HANDOFF-S7-S9-RESUME-20261006.md`, `S7-S9-SPEED-TEST-PLAN.md` in this directory,
and `checks/multi-workspace-model-confirmation-20261008/04-commit-confirmation.json`.
The first three exact hashes are in state33. Do not stage them as part of this work.

## Source accounting

| Checkpoint | Combined production LOC | Migration |
| --- | --- | --- |
| `8431b6ba9fd2703b74d92905e3a7b3108586b6bd` |173827→176088, delta+2261|1447 unchanged lines moved from excluded integration to excluded predecessors; no retirement|
| `edeb8b35024823f905252efd8333089ac0610168` |176088→176905, delta+817|No relocation/reclassification|
| Documentation-only handoff closure |176905→176905, delta0|Same product trees as above|

Current totals: core111488, active68614, excluded predecessors38878, excluded
integration3996, root reference65417. Use the pinned `tools/production_loc.py`
SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, exact
parent/final staged archives, first-party Rust/runtime SQL and exact active Cargo
member paths. Exclude inline/transitive tests, tooling, docs, third-party and
generated source. Record before/after/signed delta in every commit and handoff,
including unchanged totals for docs-only commits. Required footer remains
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
