# R1 completion and FUSE implementation handoff

> **Status:** Dated planning checkpoint; R1 complete at the verified scope below.
> Owner stop boundary2026-10-08: finish verified R1 and this actionable handoff,
> then pause the full R0–R9 objective. Do not automatically start R2.

## Scope and exact source

The owner removed only optional privileged command administration: per-Exec ctr
cancellation and command-client provenance. Ordinary commands retain Bash
semantics and standard streams/status. This does not relax any FUSE, Workspace,
Commit, ownership, lifecycle, permission, bounded-resource or acceptance contract.

The final active product is byte-identical to R1d commit
`e5e95e76c16da65172d17b6a52bb9a25613547ec`; core/crates tree
`e3a61dfd814a579b58f56b63754ec3dbce83d67e`, root reference tree
`498dd1917812ae90efb8841f57e22bfc284e96fb`. The closure commit adds documentation
and retained evidence, with no production source change or reference retirement.
[Source manifest](checks/r1-completion-20261008/02-source-identity.json) pins
all product, test/example, manifest/lock and ARM configuration bytes used below.
The exact closure commit, staged/committed tree and LOC confirmation are recorded
in the final accounting receipts in this campaign; no self-referential commit
hash is invented inside the committed document.

No active `backend/docker/admin`, `CtrAdmin`, `cancel_with`, signal helper,
privileged admin request path or Sandbox sha2 dependency remains. The original
Engine-only `ExecHandle::cancel` reports Unsupported before effects; it does not
implement cancellation. This existing explicit refusal is not an admin subsystem.
No daemon launcher, command registration, implicit command timeout, implicit
Commit/unmount or process-killing filesystem teardown was introduced.

## R1 evidence and interpretation

The owning [rollout R1 row](CLUSTER-TWO-LOC-AND-ROLLOUT-20261008.md#4-combined-implementation-rollout)
requires real SDK/Sandbox startup, ordinary execution and access protection.
Native mount `Ready` belongs to R2. Older R1c/R1d prose requiring native Ready
before R1 closure is superseded prospectively; original receipts remain intact.

Final verification is selected in [01](checks/r1-completion-20261008/01-selection.md).
The campaign records locked macOS and Linux all-target builds of SDK, Sandbox,
Bridge and Daemon; separate actual example binaries; warning-denying Clippy;
public protocol/lifecycle/SDK Init tests; native Bridge/application tests; and
one final real lifecycle proof plus one real ordinary stream proof. Tests are
built first and run with explicit100s stops (native application and independent
functional proofs9s). No test or proof budget is relaxed to obtain success.

The lifecycle proof exercises macOS Project Init/seal and installation into a
named Linux VM volume, then two simultaneous direct-Store daemons with separate
private Overlays. It verifies authenticated InstallPending/ControlReady,
UID501/GID20/groups20, zero effective/ambient capabilities, NoNewPrivs1, denied
Store/Overlay/config/proc aliases/device-control access, logical bind/unmount,
EndSession and exact owned stop/delete. A third daemon reopens the same root
after both earlier containers are deleted. The borrowed Store survives product
cleanup. Host Init is the last host data operation; subsequent host work is control.

The stream proof uses ordinary Bash with no filesystem registration:2MiB stdin,
2MiB exact stdout,17 stderr bytes, pre-start null status and known root exit37.
Transport EOF is not treated as process/descendant exit or filesystem drain.
Existing public tests retain partial Create/Start/upload/output/status/fence
failures and original ownership without replay or guessed adoption/deletion.

Global Store runs explicitly select Disposable/WAL/OFF. Durable is
`NOT_RUN — disabled by owner until explicit reauthorization`. Overlay is the
separate MEMORY/OFF/EXCLUSIVE database. Construction workers1 and root ARM64
flags are preserved. These are functional proofs with natural caches, not
speed/storage, cold-cache, resident-memory, S8 or R8 qualification. Explicit
whole-Sandbox stop is crash teardown; graceful filesystem/application drain is
not inferred from it.

## Final results and executable pins

| Verification | Final outcome |
| --- | --- |
| macOS locked builds/examples/Clippy | PASS [03](checks/r1-completion-20261008/03-host-build.txt), [04](checks/r1-completion-20261008/04-host-examples.txt), [05](checks/r1-completion-20261008/05-host-clippy.txt) |
| macOS public tests | PASS16: protocol9, lifecycle5, SDK Init2; [exact executables/commands](checks/r1-completion-20261008/07-host-test-identities.json) |
| Linux locked build/Clippy/public/native tests | PASS26: protocol9, lifecycle5, SDK Init2, native Bridge9, application1; [raw09](checks/r1-completion-20261008/09-linux-checks.txt) |
| Real host Init/shared Store/owned lifecycle/access/reopen | PASS exit0,5431100500ns within9s; [command12](checks/r1-completion-20261008/12-native-lifecycle-command.json), [proof13](checks/r1-completion-20261008/13-native-lifecycle-proof.txt), [result14](checks/r1-completion-20261008/14-native-lifecycle-result.json) |
| Real ordinary Bash streams/status | PASS exit0,1530193000ns within9s; command root exit37 is the expected oracle; [command17](checks/r1-completion-20261008/17-stream-command.json), [proof18](checks/r1-completion-20261008/18-stream-proof.txt), [result19](checks/r1-completion-20261008/19-stream-result.json) |
| Active no-admin/no-R2 source | PASS [scope20](checks/r1-completion-20261008/20-active-scope-check.json); product/lock/config equal R1d |
| fmt / boundary / tooling tests | PASS [21](checks/r1-completion-20261008/21-format.txt), [22](checks/r1-completion-20261008/22-boundary.txt), [23](checks/r1-completion-20261008/23-tooling-tests.txt):47 tests |
| Exact owned cleanup / preservation | PASS [24](checks/r1-completion-20261008/24-owned-cleanup.json), [25](checks/r1-completion-20261008/25-preserved-state.json) |

All proof/build/config/image identities and exact build commands are in
[26](checks/r1-completion-20261008/26-build-runtime-pins.json). The Linux build
validated every selected source hash before compiling. The stripped daemon SHA256
is `74e8bf3c055a3bdbcefd563648d840e8e2cbf7163900d5925c797c260195f6f6`,
5736296bytes; it is identical to the prior verified R1d daemon. The final proof
root is `0d1d67c201e784764a8f53afed453fbe407882bcb562ee6bc11ba18fdce96371`.
Host SQLite3.51.0 and Linux SQLite3.53.2 are observed by their actual providers.
The expected unused fuser patch warning confirms FUSE remains outside this graph.

## Deferred material and preserved resources

The optional admin implementation is preserved as an exact reversible
[patch](checks/r1-runtime-cancel-20261008/48-deferred-command-capability.patch)
with [per-file hashes and base](checks/r1-runtime-cancel-20261008/49-deferral.json).
Patch SHA256:
`8162672c8575ad9a6c39581b4dcfa2e59a900edd762939208a2a64612d1b8797`.
The original narrower signal proof25 passed before later provenance repairs;
final artifact-gated qualification was not run. Later reviews found remaining
strict archive-version/known-size receipt gaps. Deferral is not qualification.
Its four exact owned temporary containers and local sealed client image were
removed with [cleanup50](checks/r1-runtime-cancel-20261008/50-owned-cleanup.json).
The unchanged downloaded ctr archive/binary remains ignored under
`core/target/runtime-tools/containerd-2.2.4`; it is not part of the product/build.

The R2 draft begun before the stop boundary is preserved as an
[unqualified patch](checks/filesystem-poc-20261008/06-unqualified-r2-draft.patch)
and [hashes/blockers](checks/filesystem-poc-20261008/07-unqualified-r2-draft.json).
Patch SHA256:
`afcbf80ef96b5fa392140392d576c04753c4c062a26f547d6e37a7fadf6aa830`.
It was never built or executed. No native filesystem was mounted. Its temporary
manifest/lock/fuser activation was restored to the exact R1 product. The broad
offline lock update was rejected and retained as a diagnostic; no dependency
updates from it remain. Do not apply this patch as a qualified R2 starting point.

Known draft defects include wrong fuser/daemon boundary, incorrect create callback
signature, pre-admission allocation, per-mount blocking worker scheduling,
non-atomic lookup/count transitions, incomplete live cookie reclamation, lost
startup/source/subsequent-error custody, unchecked attributes, and incomplete
Ready/session-error/drain behavior. Preserve useful integration ideas while
implementing the real contract afresh. No source-size reduction or algorithmic
simplification is credited for removing an uncommitted draft from active source.

The three owner notes remain unmodified/untracked. Protected and E04 containers
remain outside this task's cleanup scope. Final campaign resource receipts
record every newly owned container/volume and its disposition. No remote push,
PR, issue update, release, automation or root/reference retirement is authorized.

## Concrete R2 entry points

These are implemented, reusable APIs in the pinned R1 source:

| Boundary | Entry point and ownership |
| --- | --- |
| Host provisioning | SDK `project/` exposes `ProjectApi::init`, SealedProject and install; canonical Project/Content/Storage/Persistence/History route, one sealed file |
| Owned runtime | SDK `sandbox/api.rs`, `sandbox/lifecycle.rs`; Sandbox `backend/docker/{container,container_types,archive,endpoint,streams}.rs` supply real ordinary Engine lifecycle/stdio/status |
| Daemon process | `layerfs-daemon/src/application/{owner,serve,connection,config}.rs` opens the same shared Store and one Overlay, authenticates Control and retains original failures |
| Workspace control | `control/registry.rs` has the sole bounded Binding registry; `control/operations.rs` owns bind/Commit/status/unmount admission. `Service::operation(token)` returns an authorized operation scope |
| Direct data ports | `store/bind.rs` validates only bounded root metadata; `BoundWorkspace::operation()` in `store/operation.rs` returns fresh StorePorts/client failure custody over the shared Workspace/base/cache and OwnerClient |
| Immutable/effective reads | Workspace `BaseView`, `SourceView::{lookup,stat,list,readlink,read_file}`; list advances through64 distinct keys including whiteouts, reads use128KiB windows |
| Mutation and file ownership | Workspace `mutate`, `Operation::{WriteOpen,SetOpenAttributes,Create,...}`; Owner `OpenFile`, `AcquireFileRead`, `AcquireLookupRead`, exact releases and `ReplyAttempted(publication)` |
| Actual lifetime/SQL owner | Daemon `overlay/commands.rs`, `service/completion.rs`; Overlay source/open/lookup/captured/operation owners and indexed records, with exact lost-outcome custody |
| Native control extension | Bridge `control_types.rs` and request/reply codecs; SDK `workspace/` and `control/connection.rs`; add bounded Attach/Locate/Ready with separate original bind/attach attempts |

R2 can start from this foundation without recreating Init, Store install, ordinary
command execution, the SQL owner, direct Store ports or a host data service.
Readiness for R2 does not mean its device profile, attachment protocol or request
service already exists. Current SDK `bind` returns `Bound`; it is never `Ready`.
`execute_control(Commit)` still refuses the absent live namespace constructor.

## Full FUSE implementation obligations

Read root/core AGENTS, both root handbooks, the [303 index](../303/README.md),
[primary FUSE contract](../303/fuse.md), [integration contract](../303/06-cluster-one-integration.md),
and the owning [S8 specification](S8-SPECIFICATION-20261008.md),
[implementation plan](S8-IMPLEMENTATION-PLAN-20261008.md),
[mechanism ledger](S8-MECHANISM-EVIDENCE-20261008.md),
[proof plan](S8-PROOF-PLAN-20261008.md) and
[file layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md). Proposal/research/source/
proof distinctions and all existing numerical/failure/unrun dispositions remain.

1. Preserve dormant `layerfs-fuse` as an explicitly accounted predecessor before
   activating a real replacement. Its old Workspace imports are incompatible;
   it is not a fallback. Only the replacement Fuse adapter names fuser types.
   Daemon receives owned first-party request/reply values and owns semantic service.
2. Keep fuser exactly0.18.0 with only the authorized timestamp patch. Before each
   actual native fuser build run `python3 -B core/tools/check_fuser_integrity.py`.
   Base archive SHA256 `b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd`;
   patched time.rs `a333dba1c186c022eb55b950ec8895fb767c63e8ca156150b605d31d6876287d`.
   Both owning path/version patches and ARM cfg/features stay intact. No other
   third-party patch, package substitution or shared registry edit is authorized.
3. Add narrowly selected `/dev/fuse` and daemon SYS_ADMIN plus an actually permitted
   mount security profile to Sandbox deployment and exact inspection. Keep
   Privileged=false, NNP and ordinary nonroot Exec. Restrict the container-local
   device to root before exposure; protect Store, Overlay, credentials, proc aliases,
   unmount authority and connection-specific abort control. Reprove permissions
   after this changed deployment; old R1 access proof does not qualify new powers.
4. Use first-party safe nix mount/plain unmount ownership and a retained independent
   device descriptor around `Session::from_fd`. Preserve original attach effects,
   errors and exact custody. Do not rely on automatic Drop/helper/lazy unmount.
   Target ACLAll/allow_other/default_permissions, nosuid/nodev/noatime,128KiB windows,
   TTL60/KEEP_CACHE, background/congestion1, two shared-fd loops and writeback off.
5. Establish I-3: real kernel mount, completed handshake and every dispatch loop
   running. INIT construction and one stat only show narrower progress. Unmodified
   fuser starts internal loops later and exposes no per-loop join handles. Callback
   thread-ID probes do not guarantee both loops receive a fixed batch or prove
   continuous liveness. Open proc task-directory metadata is not an exit witness.
   These are unresolved engineering prerequisites, not permission to weaken Ready.
6. Implement callback-entry admission before copying with R+N accounting, finite
   receive slots and terminal wakeups; one daemon-wide K-worker fair service with
   parked owned continuations. Existing synchronous Pending.wait and Workspace
   mutate loops are reusable semantics, not the required resumable scheduling.
   Never wait for a prerequisite on a receiver or admitted service worker, and
   never depend on a later kernel request for progress (I-8/I-9).
7. Add atomic indexed native lookup aggregates and consistent lookup/attributes,
   exact open/read/reply custody, stable serial/root1 mapping and checked FORGET.
   Readdir needs indexed old-cookie/name-boundary semantics and live last-owner
   reclamation; READDIR alone acquires no lookup count. Preserve publication after
   a lost reply and independent processing readers after file/lookup release.
8. Normal unmount probes the kernel while service remains usable; EBUSY restores
   Ready. Known detach, all loops joined and all request/Owner/completion/Store/
   control consumers disposed precede native revocation, logical Close and route
   removal. fuser run errors can leave loops unjoined; retain them, never infer
   drain from an outer handle or permit a second attempt to skip a lost result.
   Forced FS teardown does not kill caller-owned commands.

The first resumed implementation/proof slice should add the actual Fuse adapter,
request service and Attach/Locate state to the existing registry, then demonstrate
installed Store → complete native Ready → externally launched ordinary Bash
read/stat/permissions → busy-preserving normal unmount → complete drain. Include
lost Attach custody and an unregistered process retaining cwd/FD. A narrow kernel
round trip is useful intermediate evidence only. Continue R3–R9 under their full
owning contracts after R2; no automatic continuation occurs from this handoff.

## Later Commit integration prerequisites

`BoundWorkspace::commit` already sequences Capture → Save → constructor → finish →
History → known local install. Reuse `CapturedFileEdits::prepare/construct`, actual
Store policy, same-Save authenticated reads, Content metadata constructors and
`update_filesystem_streamed_backed`. Do not route live Commit through Project Init,
materialize a native tree or add another encoder.

Missing work includes provider-neutral captured namespace pages, a sealed backed
row adapter implementing every StreamedFilesystemInput cursor/point, captured
names/links/metadata/fresh serials, bounded validation/topology and exact failure
custody. Existing ReaderInodes/ReaderDirectoryEntries and IndexedConstructionRecords
supply64-row/64KiB processing windows. Resident demanded-serial/addition/parent maps,
whole-base non-file alias walks and cycle state still require their owning fixes.
A resident FilesystemInput demonstration would not complete these requirements.
R4/R5 mounted Bash mutation → explicit Commit → fresh mount/read remains unproven.

## Closure accounting and stop disposition

Production LOC:170673 →170673 (delta +0). Core105256, active core62382,
root reference65417, excluded predecessors37431 and excluded integration5443
are unchanged. The [exact parent/staged comparison](checks/r1-completion-20261008/28-exact-production-loc.json)
and [per-file classifications](checks/r1-completion-20261008/29-per-file-production-loc.json)
use pinned counter SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
Archived uncommitted drafts are evidence, not active product or credited retirement.
The local closure commit must retain these counted product trees; post-commit
confirmation recomputes the committed production count and records the exact commit.

Changed Markdown local links pass. Full staged whitespace is FAIL exit2 on
byte-preserved transcript/archive contents; scoped source/docs whitespace is PASS.
[Exact exclusions and outcomes](checks/r1-completion-20261008/33-whitespace-scope.json)
keep those verdicts distinct. No CI/aggregate pre-push claim is made.

R1 is complete under the current owner scope and R2 has an actionable verified
foundation. Work stops here at the owner's boundary; the full R0–R9 objective
remains unfinished. Request the app Goal pause after the local commit and its
confirmation. Report the tool's actual returned state; if it has no active Goal
to pause, report that limitation and remain stopped without creating a new Goal
or any automatic/background continuation. Resume R2 only on owner direction.
