# #179 implementation completion: bounded Linux profile

> **Status: Current planning checklist; no release candidate exists.**
> Owner direction: finish implementation quickly; use simple, fast correctness
> verification; defer DSH, difficult load workloads and speed qualification.
> Source basis: `83b4dbf498ed95ca823a154037ad63f48d290905`.
> This document changes the current phase's scope and completion gate. It does
> not claim that the remaining code is implemented or that any new check passed.

## 1. Authority and concrete deliverable

Complete the initial **bounded Linux Workspace/FUSE implementation** for
[#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179), and demonstrate
that its required operations work together through the real daemon, authenticated
bridge, Service, C1/C2 and C5 Commit path. Correctness of a small ordinary workflow
is the integration gate. Large-workspace capacity and performance are later work.

This owner-directed plan supersedes the DSH-first sequence and completion gates
in [30](30-continuation-handoff.md), [04](04-implementation-and-verification.md),
and [50](50-budget-stop-handoff.md). In particular, **do not resume the DSH runner
as the next task**. Historical receipts and failures remain unchanged. Product
boundaries, failure ownership, canonical compatibility, explicit Commit semantics,
source-size rules and repository safety rules still apply.

The phase is complete when every required operation in section 3 is implemented
through all relevant layers, the small correctness checks pass, and the final
integration in section 6 passes. An unsupported stub for a required operation is
unfinished implementation. A documented capacity limit or explicitly deferred
operation in section 2 is an accepted limitation of this phase.

## 2. Freeze the scope; avoid another scalability project

Keep the existing limits and refuse out-of-profile work explicitly and safely:

| Existing bound or policy | Decision for this phase |
| --- | --- |
| Prepared request: 128-entry admission rules and 32 KiB metadata | Keep; update exact accounting for new mutation shapes, without increasing limits |
| Resident Nodes 256, handles 128, payload records 4096 | Keep; preserve real reference ownership and checked admission |
| Existing/captured-file replay 8 MiB and 256 edits; 1024 pieces | Keep; fresh complete-file streaming remains implemented under its existing MAX_FILE and quota rules |
| Default accounted Workspace memory 8 MiB; explicit disk quota and Workspace count | Keep; no unbounded tables or silent budget growth |
| Existing topology work bound, including the 4096-entry limit | Keep and document; work-limit refusal must not be mislabeled as an actual cycle |
| One submission, one construction worker; current callback deadlines | Keep; no background/automatic Commit, retry or pressure flush |
| Owner identity, current name grammar, Linux mount and direct-I/O profiles | Keep; no multi-user, arbitrary-name or cross-platform expansion |

Deferred and **not prerequisites for implementation-phase completion**:

- DSH or other package installations/uploads, large repositories, stress suites,
  maximum-size campaigns, throughput targets, tail latency, RSS/cgroup campaigns,
  cold-cache qualification, and matched R6/#207.
- Larger prepared-input protocols, external indexed C1 input, payload-catalog
  scaling/packing, and enlarged resident-node capacity solely to fit large trees.
- xattr APIs, ACL/capability mapping, arbitrary uid/gid changes, atime/ctime/birthtime
  setters, special files, rename exchange/whiteout, readdirplus, fallocate,
  copy-file-range acceleration, distributed locks, crash recovery, and durability.
  Preserve explicit unsupported behavior from [01, section 7.3](01-workspace-fuse-contract.md#73-optional-kernel-owned-or-unsupported-operations).
- New remote controls for every filesystem operation or a new failed-submission
  recovery protocol. Existing native APIs and authenticated daemon lifecycle/Commit
  controls are the integration surface. AddLayer remains separate from Commit.

Do not silently omit a required operation because it is difficult. Conversely,
do not promote an optional operation or a large-workload limitation into a new
blocking task. State completion as **bounded Linux implementation complete**,
without claiming full POSIX, arbitrary workload, performance or release readiness.

## 3. Required implementation surface

The existing implementation is the starting point, not a rewrite:

| Capability | Current state at the source basis | Required action |
| --- | --- | --- |
| Attach, Status, Mount, Unmount, CloseClean, authenticated explicit Commit | Implemented | Preserve and use in final real-daemon integration |
| Lookup/getattr/access, open/read/release, directory handles/listing, readlink, flush/statfs | Implemented | Preserve; make identity and pinned views survive the new namespace mutations |
| Create/open, write/append, truncate/extend, mkdir, symlink | Implemented | Integrate with aliases, removal, rename and metadata changes |
| Capture G/live G+1, Stage/CommitStaged, composite Commit, known-own reconciliation | Implemented | Extend correctly to every newly supported mutation |
| Portable `setattr` mode and mtime, including valid combinations with size | Missing in the mounted/Workspace mutation path | Implement atomic validation/publication, save and reconciliation |
| Regular-file `mknod` without opening a handle | Missing | Share regular-file creation and identity reservation; expose native and FUSE paths |
| Regular-file hard link | Missing | One inode shared by names, with correct link counts and one captured save per version |
| `unlink` | Missing | Name removal plus retained open/unlinked lifetime |
| `rmdir` | Missing | Empty-directory removal with type/root/emptiness checks |
| Ordinary rename and `RENAME_NOREPLACE` | Missing | Atomic same/cross-directory rename and replacement, including directory semantics |

Each missing operation must include the public Workspace operation, FUSE
translation/errno behavior, overlay representation, admission accounting,
Commit lowering, G/G+1 reconciliation and cleanup. A native-only API or an
adapter that returns success without changing the committed tree is incomplete.

Metadata means the portable profile already selected: regular-file permissions,
directory permissions/sticky bit, symlink mode fixed to 0777, and valid mtime.
Support metadata-only changes on the applicable inode kinds without reconstructing
unchanged contents. Validate every field of a combined SETATTR before publication;
an invalid or unsupported field must not leave a successful size/mode sub-update.
Use existing metadata constructors/updates and directory metadata declarations.
Preserve the access rights of already-open handles after chmod; later opens use
the current mode under the selected owner profile. Regular mknod must not consume
a file-handle slot through a hidden create/open/release sequence.

## 4. Shared prerequisites and invariants

Implement these inside the first operation that needs them; do not create a
generic mutation framework or an independent rewrite project.

### 4.1 Namespace deltas and stable identity

The existing E records describe additions. Introduce a checked removal/tombstone
representation in the same maintained directory delta: **absent delta means
inherit; tombstone means absent from the effective namespace**. Lookup, listing,
emptiness checks, save lowering and reconciliation must all use that distinction.
Preserve stable directory-handle views and cookies across later changes.

Publish all entries, inode/reference facts, parent metadata, dirty counters and
revision changes for one operation through one candidate-root publication. Rename
must not become two separately visible unlink/link operations. Check the complete
request envelope and reserve needed backing before publishing anything. Refused
operations must leave the namespace and file state unchanged.

Inode identity must not depend on one mutable pathname. In particular, inspect
`runtime/state.rs::Node`, the current Handle comment that assumes names are only
added, `filesystem/namespace.rs::cache_lookup`, and
`filesystem/original.rs::serial_original`. A cached original path is not a valid
live locator after rename, alias removal, or unlink. Keep the minimum bounded
locator/version ownership necessary for handles and pinned directory views;
reuse saved content/metadata roots and existing backing. Do not add a whole-tree
resident path map or implement rename by recursively copying file contents.

Distinguish namespace link count from Local/Projection lookup references, open
handles and frozen/read owners. Updating one must not fabricate or drop another.

### 4.2 Aliases, removal and Commit

- Hard links are selected only for regular files. Fresh-file aliases created
  before the first Commit must work without an intermediate Commit. Reuse C1's
  derived reference counts; close a real shared semantic gap at its owning layer
  if necessary, rather than manufacturing a second inode.
- Unlink removes exactly one name. Surviving aliases and already open handles
  continue to read/write the same inode. Last-name removal does not free bytes
  while a handle, pinned read or frozen generation still owns them.
- Fresh create-then-unlink before Commit must not emit an invalid unbound fresh
  inode declaration. Open orphan state remains local as required; closing its
  last actual owner permits reclamation. A Commit must never resurrect its name.
- Rename replacement preserves the replaced inode for its existing handles.
  Rmdir checks the effective directory, including pending additions/removals;
  it never recursively deletes a nonempty directory.
- Lower each changed inode/version once across aliases. Retain G's names and
  contents during submission; a later G+1 rename/unlink/link/metadata update must
  survive known G completion. Earlier committed roots remain immutable.

### 4.3 Rename and mounted coherence

Implement ordinary rename and NOREPLACE; reject unsupported flags, root moves,
cross-Workspace operations and invalid type/emptiness combinations. Ordinary
same-inode aliases are a no-op; NOREPLACE must honor destination existence.
Directory moves must reject moving an ancestor beneath itself and preserve live
descendant lookup/open behavior. Validate against one selected namespace view and
revalidate its publication stamp. A bounded cycle-check refusal is distinct from
a detected cycle.

Extend the existing projection mutation permit and checked invalidator. Kernel
mutations retain the permit through the reply and must not send a reverse
notification while the kernel owns the relevant parent locks. SDK mutations
notify the affected entries/parents and inode attributes as needed. Rename has
a small fixed set of affected entries; it does not justify an unbounded queue.
Post-publication notification failure retains the mutation and Failed receipt;
it must not be reported as rollback or invite replay. Preserve reference cleanup
for pre-reply conversion failures and the existing checked remount recovery.

Never hold state locks over backing or remote I/O. Preserve typed backing/history
failures, cancellation, deadline behavior and no-replay rules. No third-party
patches, private C1/C2 calls from Workspace, new protocol, or implicit durability.

## 5. Ordered execution plan

Use the smallest complete vertical change per operation; share private primitives
once. Continue through the whole checklist rather than handing back after one
native-only operation. These are implementation steps, not separate benchmark
campaigns or mandatory long design reports.

| Step | Work and principal owners | Small correctness exit |
| --- | --- | --- |
| A | Resolve the Round49 post-rebase refusal contract; reproduce the suspected three-fresh-directory cycle through the public C1 API | One focused refusal check and a three-node cycle/valid-chain check; fix a demonstrated correctness defect only |
| B | Atomic portable attributes: Workspace filesystem/overlay/commit, existing metadata operations, FUSE SETATTR | Small file/directory mode+mtime; unchanged content root for metadata-only change; invalid mixed request leaves everything unchanged |
| C | Regular-file mknod: shared child creation plus FUSE callback | Empty file, mode/umask, no opened handle, duplicate/type refusal, one save |
| D | Regular-file link: stable inode/alias identity plus shared Commit reference derivation | Two names share serial/content/metadata; write through either; fresh aliases before first Commit |
| E | Unlink: tombstones, open-orphan ownership, lowering and completion | Remove one alias, then last name while FD open; read/write across Commit; close and reclaim with no resurrection |
| F | Rmdir: effective emptiness and retained directory-view behavior | Empty removal; nonempty/type/root refusals; pending child deletion counted correctly |
| G | Rename: atomic two-parent edits, replacement/orphan lifetime, directory relocation and coherent projection | Same/cross-parent move, replacement with destination FD held, NOREPLACE, same-inode case and tiny directory-cycle rejection |
| H | One tiny G/G+1 scenario covering new namespace and metadata deltas | Hold the existing deterministic save gate, apply later changes, finish G then Commit G+1; exact two versions |
| I | Final real-daemon integration and final core checks | Section 6; all required callbacks operational and all required correctness assertions passing |

Use the actual existing files under `layerfs-workspace/src/{filesystem,overlay,
runtime,commit}`, `layerfs-fuse/src/adapter.rs`, and the existing shared metadata/
prepared/history operations. Split new cohesive files only where responsibility
or the 999-line/200-line module ceilings require it. Keep one owner for shared
namespace state; parallel agents may handle genuinely independent, bounded work.
Heavy commands are always serialized.

### Existing failures: close correctness, defer load investigations

Round49's observed post-rebase Capacity refusal leaves state unchanged but issues
one read-only Inspect. The acceptance contract for this phase permits a necessary
read-only consultation before refusal; it forbids mutating RPCs, remote inode
reservation, remote content save, replay and visible-state change. Temporary
accounted local scratch admission is a separate ownership matter. Preserve
stronger local-progress guarantees
while G is retained. Trace the actual call against the documented contract, then
correct an overstrict zero-RPC oracle if that is the cause; do not add product
complexity solely to suppress a legitimate read. Preserve the original FAIL and
record the clarification and new focused result separately. A small public-API
case suffices; do not rerun DSH or stream megabytes to establish this ordering.

The three-directory C1 cycle suspicion is unconfirmed. Reproduce it with three
empty directories, not a large graph. If confirmed, fix the responsible shared
validator and use the same check as rename's prerequisite. Do not redesign C1's
input interface for this purpose.

Round43's unexplained near-limit capacity FAIL remains recorded and is explicitly
deferred from this small-workflow phase. If investigation of a required operation
exposes corruption, unsafe cleanup or a reproducible small-input correctness
failure, fix that shared cause. This deferral does not permit ignoring such a defect.

## 6. Verification: small, direct and fast

### During implementation

- Reuse the current Rust public-API tests, `support/native_workspace.rs`,
  `mkdir_route.py` and daemon control drivers. Do not build another harness,
  fault framework, benchmark runner, input generator or reporting system.
- Use deterministic literal content: a few files/directories, normally at most
  64 KiB per file and 24 changed names per scenario. These are new correctness
  inputs under the owner's new scope, not substitutes for old failed gate rows.
  A rare logical boundary assertion may use a sparse recipe without reading or
  saving the entire range. No DSH, package download or other external corpus.
- For each changed operation run the smallest public-API check and actual mounted
  check that establish its behavior, including one relevant refusal/ownership
  edge. Combine related assertions in an existing case. Prefer deterministic
  gates over sleeps, stress or timing thresholds.
- Record content/metadata/namespace equality, expected errors, unchanged state on
  refusal, reference/handle ownership and known Commit outcome. Wall time is
  diagnostic only; there is no speed pass/fail target.
- Run targeted package/target checks while iterating. Run the required whole-core
  checks once on the final integrated source, rather than after every small edit
  on both platforms. Re-run a passing check only for an affected change or concrete
  unresolved concern. Do not relabel old-source receipts as proof of new source.
- Keep existing operation deadlines and the live driver's 60-second hard cleanup
  budget. Tiny cases should finish in seconds; this expectation is not a timing
  assertion. Reuse builds, pinned images and closed prepared fixtures; byte-copy
  writable run state. Never reuse a mutated run or raise limits to get PASS.
- No obligatory DWARF, peak-memory, cache-residency or performance reports for each
  operation. Review actual new allocation ownership and bounds when changed.
- A concise command/result/failure record is sufficient. Preserve failed attempts
  and meaningful stdout/stderr; no per-operation dossier or repeated review cycle
  is required. Do not run retired preflight/CI or add an aggregate gate wrapper.

### Final integration: a real small project lifecycle

Extend the existing [daemon Commit driver](../../../../crates/layerfs-daemon/tests/control_commit.py)
with one small scenario, using its real Linux daemon, FUSE mount, authenticated
control and separate live Service/C5 producer. Embedded Workspace-only tests do
not replace this final wiring check. Start Service/daemon once for the scenario.

1. Attach and mount a writable Workspace; verify target identity/status. Use
   existing auth/readonly refusal helpers for a small negative check.
2. Through actual syscalls create two directories, several tiny files, a regular
   mknod file, a symlink and a hard-link alias. Write/append/truncate, chmod and set
   mtime. Rename between directories and replace an existing file while its old
   destination FD remains open. Check the old FD still addresses its own inode.
   For the selected mtime-only contract, use a syscall with atime omitted (for
   example UTIME_OMIT), rather than accidentally requiring an atime setter.
3. Unlink a last-name file with its FD held, then read/write that FD. Check a
   nonempty rmdir and a NOREPLACE collision fail without partial changes. Remove
   an empty directory successfully. Verify names, contents, inode identities,
   link counts and portable metadata with lstat/readlink/read/listing.
4. Request **explicit authenticated Commit A**. Verify the expected Branch head
   and saved namespace/content/metadata through public Service reads. Unlinked
   names remain absent and their still-open FDs remain usable.
5. Make a small second edit, rename/unlink an alias and change metadata. Request
   **explicit Commit B**. Verify the new head and exact final tree; the previously
   saved A root remains readable and unchanged. No hidden intermediate Commit.
6. Close all FDs, unmount/remount the same live Workspace and read the acknowledged
   state. Finish with checked Unmount/CloseClean and normal process/container
   cleanup. No writable process-restart or crash-recovery claim is required.

Keep the separate step H generation test tiny and deterministic using the existing
save gate. It must include a later rename/unlink or alias change and a metadata
change so G completion cannot erase them. Add one focused notification-failure
check for the new multi-entry SDK mutation; retain existing failure ownership.
No broad fault-schedule campaign is required.

### Final source checks

Run the repository-required core checks on the final source with Rust 1.85.1,
locked/offline dependencies, its own targets and root ARM AEAD flags:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --offline
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.85.1 build --manifest-path core/Cargo.toml --workspace --bins --examples --locked --offline
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

Use `CARGO_BUILD_JOBS=2`, `LAYERFS_CONSTRUCTION_WORKERS=1`, no overriding Rust flags,
and `CARGO_TARGET_DIR=$PWD/core/target` for host commands. Run the Linux build/test/
Clippy equivalents once in the existing pinned tool image with `--cpus 2` and
`CARGO_TARGET_DIR=/work/core/target-linux`. Reuse the exact image/command wiring in
the existing evidence. No concurrent builds or live tests in this worktree, and
no interruption of another owner's work. A matching final-source result need not
be run twice merely to repeat it in the final report.

## 7. Definition of implementation-phase completion

All of the following are required:

- [ ] Every required operation in section 3 works through native Workspace and
      its applicable actual FUSE path; no selected operation remains a stub.
- [ ] Fresh and existing objects, aliases, replaced/open-unlinked files, pinned
      directory views and subsequent generation completion preserve identity/data.
- [ ] Attribute/namespace mutations validate and publish atomically; bounds and
      unsupported behavior are explicit, with no false success or partial refusal.
- [ ] The small deterministic G/G+1 case and multi-entry notification-failure
      ownership check pass.
- [ ] The real-daemon small-project scenario passes two explicit Commits, public
      saved-state inspection, remount and native clean closure.
- [ ] Required final source checks pass; no observed small-workflow correctness
      failure is concealed by a test change, an external-only cleanup claim or
      a deferred performance item.
- [ ] Current architecture/operation matrix and exact reproduction commands are
      updated. Final report lists accepted bounds, optional unsupported features,
      deferred load work and historical open failures separately.
- [ ] Changes are committed with exact per-commit production LOC accounting.
      No push, merge, release claim or automatic issue closure without authorization.

The agent should finish all these steps autonomously. Do not stop after a plan,
one native API, or another general audit; do not restart completed foundations.
Routine implementation choices should be made from current code and these
requirements. Only a real external dependency blocker or incompatible owner
requirement warrants escalation.

## 8. Copyable prompt for the implementation agent

```text
Finish #179's bounded Linux implementation and successful integration.

Use /Users/yifanxu/.codex/worktrees/795c/layerfs on codex/pair1-remote-mount.
Inspect HEAD/status; preserve all existing work and historical receipts.
Read AGENTS.md, core/AGENTS.md, then:
core/docs/architecture/proposal/fuse-workspace-snapshot-overlay/51-implementation-completion-spec.md

That spec is the latest owner-directed plan. It supersedes the DSH-first handoff
and large-workload completion gate. Keep current numeric limits. Do not run DSH,
package installations, stress/maximum-size campaigns or R6/speed benchmarks.

Implement every required missing operation end to end: atomic mode/mtime SETATTR,
regular-file mknod, regular hard links, unlink/open-orphan lifetime, empty rmdir,
ordinary rename and NOREPLACE. Share the smallest namespace/identity/tombstone
primitives; preserve G/G+1, explicit Commit and failure ownership throughout.

Use small fast correctness checks while iterating, one tiny G/G+1 check, and the
real-daemon small-project integration with two explicit Commits, public saved-state
checks, remount and clean close. Run required whole-core checks once on final source.
Keep heavy work serial, Cargo jobs2, Docker CPUs2 and construction worker1.

Continue until section7's full implementation-phase checklist is satisfied.
Document accepted limits and deferred work honestly; do not make large-scale
support or benchmark perfection a prerequisite. Commit with exact production LOC.
Do not push, merge or close issues without authorization.
```
