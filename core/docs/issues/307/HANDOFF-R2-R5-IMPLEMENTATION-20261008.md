# Next-agent prompt: implement R2–R5 on the verified R1 foundation

> **Status:** Dated implementation handoff; R1 verified, R2–R5 uncompleted.
> The text below is a self-contained prompt for the owner to dispatch to the next
> agent. Saving this prompt does not dispatch an agent or resume implementation.

You are the next implementation agent for LayerFS in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, on local `main`. Implement and verify
**full R2–R5**, in reviewable local checkpoints, using the existing `core/`
replacement product. R1 is complete at the exact scope and identities below.
Do not restart R1 or optional command administration. Inspect current state
before trusting this inherited snapshot, preserve unrelated work, and reconcile
any later owner direction before edits. Do not mistake a first mount demo for
complete filesystem acceptance.

The previous agent was explicitly told to stop after verified R1 and this prompt.
The subsequent architecture review changes only documentation and proposed homes;
the verified R1 product/source/build/proof identities below remain unchanged.
Your R2–R5 work begins only when the owner dispatches/resumes it. The later
reviewed source split below is part of that assignment, not permission for the
documentation agent to start implementation. The full R0–R9
objective remains unfinished; completing this batch does not complete R6–R9.
Do not create another chat, automation, remote publication or broader assignment.

## 1. Owner scope and architecture that remain binding

Only the optional privileged `backend/docker/admin` subsystem, custom per-Exec
ctr cancellation and command-client provenance were removed from active scope.
This was not permission to simplify filesystem correctness, ownership, resource
bounds, lifecycle or qualification. Preserve the full FUSE, Workspace and Commit
contracts. The archived admin experiment is deferred, not qualified or required
for R2–R5. Do not restore it as a prerequisite.

Commands behave like ordinary Bash or externally launched execution. Keep normal
stdin/stdout/stderr and root exit status; no daemon command supervisor, launcher,
per-Exec cgroup, filesystem command registration, custom Exec wire, automatic
command timeout, implicit Commit or automatic unmount. Any process with mount
visibility and permission must get the same filesystem behavior. Shell exit,
zero registered commands and transport EOF establish no filesystem drain.
Forced filesystem teardown never implicitly kills caller-owned processes.

A Workspace is the complete mutable filesystem, including `.git/index`, ignored
files, dependencies, symlinks, caches and outputs. Mount binds a complete prepared
root without whole-tree scan/copy/materialization or dependency restoration.
Demand reads still pay real I/O. Support fresh per-tool-call Workspaces and
long-lived Workspaces with sequential/concurrent calls and explicit incremental
Commits; command duration, Workspace lifetime and Commit cadence are independent.

Every daemon opens the same global Store directly from a shared named Linux VM
volume. The host performs Project Init, seals and installs one Store file, then
is control-only. No Store under the repository bind, no host data server, no
`layerfs-server` revival, no legacy adapter or fallback. Immutable data, Saves and
History remain in-process in the daemon through existing libraries.

Global Store execution is **explicit Disposable / WAL / synchronous=OFF only**.
Durable is `NOT_RUN — disabled by owner until explicit reauthorization`, including
tests, diagnostics and measurements. Do not rely on a default or fall back to it.
Each daemon owns one separate initialized Overlay MEMORY/OFF/EXCLUSIVE database;
Workspace rows share its tables with namespace isolation. Do not create a new
Overlay per mount. Do not add disposable backing fsync/fdatasync/sync_data/sync_all.

One original operation attempt; no automatic retry/busy handler, refresh/reprepare,
replay, alternate-backend substitution, guessed adoption/deletion or inferred
success after a lost outcome. Readiness waiting is not replay. Preserve original
known failure, unknown publication, pending inputs, replies, owners and cleanup
custody. A lost mutation reply does not undo publication.

## 2. Read these authorities before implementing

All paths here are relative to the repository root unless stated otherwise.
Read current files, not merely the historical pins quoted in them.

- `AGENTS.md`, `core/AGENTS.md`, `cluster_one_handbook.md`,
  `cas_cdc_deltaencoding_handbook.md`.
- `core/docs/issues/303/README.md`; all seven primary operation/engine contracts:
  `workspace-api/{mount,exec,commit,unmount,status}.md`, `daemon-sqlite.md`,
  `fuse.md`; also `06-cluster-one-integration.md` in that directory.
- `core/docs/issues/307/S8-SPECIFICATION-20261008.md` is S8's decision owner.
  Read `S8-IMPLEMENTATION-PLAN-20261008.md`,
  `S8-MECHANISM-EVIDENCE-20261008.md`, `S8-PROOF-PLAN-20261008.md`,
  `FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md`,
  `R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md`,
  `CLUSTER-TWO-LOC-AND-ROLLOUT-20261008.md` and
  `ROLLOUT-LEDGER-20261008.md` alongside it. Plans/research do not prove capability.
- `core/docs/issues/307/R1-COMPLETE-FUSE-HANDOFF-20261008.md` and final R1 receipts
  under `core/docs/issues/307/checks/r1-completion-20261008/`.
- `core/docs/issues/307/BRANCH-OVERWRITE-DECISION-20261007.md`,
  `PRE-S8-COMPLETION-20261007.md`, `PRE-S8-RESOURCE-GROWTH-RESULTS-20261007.md`,
  `PRE-S8-WAL-BASELINE-DECISION-20261007.md`,
  `PRE-S8-ACCEPTANCE-DECISION-20261007.md`, and the directly relevant S6/S7/Store
  architecture records. Retain P-1–P-7/overwrite/refill/profile rulings at their
  actual scope; do not resume closed Stage0–6 or E04 campaigns.
- Before performance-sensitive changes: `docs/general/optimization-guide.md`.
  Before each measurement/proof selection: `docs/general/agent-measurement-policy.md`,
  `docs/general/benchmark_rules.md`, `benchmark_agent_report.md`,
  `core/benchmark/fs-bench-pro/AGENTS.md` and the selected harness/family contract.
- Documentation/release rules: `docs/general/documentation-policy.md` and
  `docs/general/release-policy.md`. No release or remote action is requested.

Supersessions: the host-mediated runtime is retired; daemons directly open the
shared Store. Global WAL/Disposable policy supersedes older MEMORY-import/Durable
execution instructions. The Branch overwrite decision supersedes old head-CAS
assumptions. R0 withdrew proposed managed-daemon Exec selections prospectively.
The later owner removed only optional admin cancellation/client provenance.
Native mount Ready belongs to R2, not the completed R1 startup/control foundation.
Historical receipts and their original verdicts remain unchanged.

## 3. Exact starting state and reusable R1 evidence

R1 closure commit: `ed965f895dbe5f835a690180b6c76f93fe368d74`.
Its first parent and last production-changing commit:
`e5e95e76c16da65172d17b6a52bb9a25613547ec`.
R1 closure tree: `f8f2df8a94b69b46e2aab91f0aa6175d569c346f`.
The latest prompt-delivery/ownership-review commit is documentation/evidence only;
find its exact identity
with `git log -1 --format=fuller -- core/docs/issues/307/HANDOFF-R2-R5-IMPLEMENTATION-20261008.md`.
Do not assume HEAD equals the older R1 closure after that delivery commit.

The required unchanged product identities at handoff are:

| Input | Exact identity |
| --- | --- |
| `core/crates` Git tree | `e3a61dfd814a579b58f56b63754ec3dbce83d67e` |
| root reference `crates` Git tree | `498dd1917812ae90efb8841f57e22bfc284e96fb` |
| `core/Cargo.toml` SHA256 | `1e7491027ba16b0f7e6fc0d12e7e2b292369d652dbaac6d1be1b7e23a366a29c` |
| `core/Cargo.lock` SHA256 | `05d0c98c3fef6bdd46d33ed161ac913fad5a4406a50445fdb835dfe3a3eee159` |
| `.cargo/config.toml` SHA256 | `3a1863834c9fb76e20b1799459d1d90da5de7d347033171e025f3e2323dbe8c9` |
| Docker image | `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6` |
| `core/target/debug/examples/sandbox_lifecycle` SHA256 | `c5c57c8b94768cd24a235fc2ef7aaea8553951d45bee3bfeece04b0654ce82cf` |
| `core/target/debug/examples/engine_streams` SHA256 | `1054faaa5efcbf89a9c5fa27f3a6f9e807bc7b5fb3efbeee2805964bd3e38e73` |
| stripped `core/target/r1-completion/layerfs-daemon` SHA256 | `74e8bf3c055a3bdbcefd563648d840e8e2cbf7163900d5925c797c260195f6f6` (5736296bytes) |

At proof time: macOS ARM64/Rust1.85.1 controller; Docker Desktop4.76.0(228118),
Engine29.5.2/API1.54, Linux ARM64 kernel6.12.76-linuxkit; actual host SQLite3.51.0
and daemon SQLite3.53.2. These are historical runtime observations to inspect
again if relevant, not eternal platform assumptions or new command provenance work.

Let `R1` below mean `core/docs/issues/307/checks/r1-completion-20261008/`:

- `02-source-identity.json`: all selected product/test/example/manifest/config hashes.
- `03-host-build.txt`, `04-host-examples.txt`, `05-host-clippy.txt`: locked all-target
  SDK/Sandbox/Bridge/Daemon build and warning-denying Clippy PASS.
- `07-host-test-identities.json`:16 host tests PASS (protocol9/lifecycle5/SDK Init2).
- `08-linux-command.json`, `09-linux-checks.txt`, `10-linux-result.json`: source
  hashes checked before locked Linux build;26 tests PASS (same16 plus Bridge9 and
  application1), warning-denying Clippy PASS. Actual native application case9s;
  other test invocations100s. No timeout/hang in this final selection.
- `12-native-lifecycle-command.json`, `13-native-lifecycle-proof.txt`,
  `14-native-lifecycle-result.json`: PASS exit0,5431100500ns within9s. Actual macOS
  Init/seal/install, two direct-Store daemons sharing one VM volume with separate
  Overlays, authenticated InstallPending/ControlReady, ordinary UID501/GID20/groups20,
  CapEff0/CapAmb0/NNP1, denied private backing/proc/device control, logical bind/
  unmount/EndSession/stop/delete, then a third daemon reopening the same root after
  both deletions. Root `0d1d67c201e784764a8f53afed453fbe407882bcb562ee6bc11ba18fdce96371`.
- `17-stream-command.json`, `18-stream-proof.txt`, `19-stream-result.json`: PASS
  exit0,1530193000ns within9s; ordinary Bash delivered2097152bytes stdin/stdout,
 17 stderr bytes, pre-start null exit and expected root exit37. No registration.
- `20-active-scope-check.json`: no admin/CtrAdmin/cancel_with/helper/sha2 wiring;
  no active R2 native draft. Existing Engine-only cancel is Unsupported before effect.
- `21-format.txt`, `22-boundary.txt`, `23-tooling-tests.txt`: fmt,759-file product
  guard and47 tooling tests PASS. These scans do not prove native implementation.
- `24-owned-cleanup.json`, `25-preserved-state.json`: exact owned cleanup and
  protected resources/notes. `26-build-runtime-pins.json`: exact commands/binaries.
- `28-exact-production-loc.json`, `29-per-file-production-loc.json`,
  `35-commit-confirmation.json`: exact R1 parent/staged/committed accounting.

Reuse this evidence only where source, behavior, platform and treatment are
unaffected. These were functional tests with natural caches, not speed/storage,
cold eligibility or residency qualification. Whole-Sandbox stop is explicit crash
cleanup, not graceful FS/application drain. Native FUSE Ready, mounted read/write,
live namespace Commit and integrated acceptance are all NOT_RUN in R1.

Retain original failures, including R1d lifecycle59 (terminal ENOTCONN ordering),
its repaired proof94, and the macOS responder's genuine ENOTCONN fence outcome.
Do not suppress it or label clean responder shutdown from Linux-only evidence.
Historical WAL Init speed/storage FAIL remains the accepted exact baseline;
incomplete phase attribution, fractional signed-minimum native timestamp FAIL,
>4GiB execution waiver, original unknown custody and owner-deferred numerical
acceptance remain scoped as documented. R1 did not waive them. Full R1 closure
whitespace FAIL is from preserved raw transcripts/archived patch bytes; its scoped
source/docs check passes. Preserve that distinction rather than editing evidence.

## 4. Members, working tree, deferred drafts and resources

Inspect `git status --short`, `git log`, `core/Cargo.toml`, current public exports
and source hashes before editing or building. A directory/old test is not an
active replacement. At handoff the active members are:

`layerfs-content`, `layerfs-storage`, `layerfs-persistence`, `layerfs-project`,
`layerfs-history`, `layerfs-telemetry`, `layerfs-overlay`, `layerfs-workspace`,
`layerfs-daemon`, `layerfs-api/sdk` (package `layerfs-sdk`), `layerfs-bridge`,
`layerfs-sandbox` under `core/crates/`.

Excluded: `core/vendor/fuser-0.18.0` as an independent workspace member;
`layerfs-{sdk,sandbox,daemon,workspace,bridge}-legacy`, dormant `layerfs-fuse`, and
`layerfs-server` under `core/crates/`. Root `crates/` is v0.1.6 reference only,
never a dependency/source include/fallback. No early reference retirement.

No uncommitted product source changes remain. The R1 post-commit confirmation35
is included by the prompt-delivery documentation commit. The three preexisting
owner notes remain untracked and byte-preserved; do not stage, delete or rewrite:

| `core/docs/issues/307/` note | SHA256 |
| --- | --- |
| `HANDOFF-PRE-S8-SERVERLESS-20261007.md` | `a7fb0474c4ed154d654659c7b0d0cc40545297498ec30792d10d9f43780d50b7` |
| `HANDOFF-S7-S9-RESUME-20261006.md` | `39ab313e0b38b1e47f6620262feabb8d94770248c9edfe77ef9dca728c1234d7` |
| `S7-S9-SPEED-TEST-PLAN.md` | `52f09b72e4d3bb3a28dbbd0b07fc8692311a346baa5e03420dd8edea67cec027` |

A later post-commit prompt-delivery confirmation may be the only other untracked
receipt. Reinspect rather than assume nothing else has changed. No Cargo/test/
proof process or owned runtime resource was intentionally left running. Check
any newly inherited live handles before launching work; don't restart for a mere
tool polling timeout. All previous review agents stopped without edits/builds.

Deferred admin patch:
`core/docs/issues/307/checks/r1-runtime-cancel-20261008/48-deferred-command-capability.patch`,
SHA256 `8162672c8575ad9a6c39581b4dcfa2e59a900edd762939208a2a64612d1b8797`;
base/per-file hashes49, original receipts01–47, cleanup50 in that directory.
Narrower proof25 predates later artifact gates; final artifact-gated qualification
was not run. Keep it deferred. The unchanged ctr download/extraction remains in
ignored `core/target/runtime-tools/containerd-2.2.4`, outside the product.

Unbuilt/unexecuted R2 draft:
`core/docs/issues/307/checks/filesystem-poc-20261008/06-unqualified-r2-draft.patch`,
SHA256 `afcbf80ef96b5fa392140392d576c04753c4c062a26f547d6e37a7fadf6aa830`;
base/per-file hashes/blockers07. It is absent from the active tree. Do not apply it
as a qualified baseline. It has wrong Fuse/Daemon layering, a create callback
signature mismatch, pre-admission allocation, blocking per-mount service, nonatomic
lookup/count transitions, cookie reclamation gaps, lost failure/source/startup
custody, unchecked attributes and incomplete Ready/session-error/drain semantics.
No native runtime resource was created. The rejected broad offline lock update
and minimal temporary lock are retained diagnostics; neither changed the final lock.

Exactly owned R1 final resources, all removed by the recorded owners:

- Lifecycle containers `cb2a5e320f9858f7c1587a5f364d05a699a9830cc6954574d053d0c304fea8a7`,
  `ad2a5a6b7cad325771070d855933436019a6014e4b007e38d0238b6c3453d9ad`,
  `7532cc5058c72766149f197cbda3dcdf3e4562335a8abfe0d0b21ec19aea69d4`.
- Stream container `d4fc9eb564f809f6c4abd51b234eae052158f8506608b62dc369e41affae19e0`.
- Isolated test volume `layerfs-r1-final-store-0396a283a7b4457e95de2e59928a89a3`.

Admin cleanup50 removed exactly
`fc722b9adacaecd167ea454020eb89f33c1abce68c6318e1a7089a6e8382ea50`,
`5e3c98074b364e45e6bdf6a095d3f359826787041afa32e54d8930bd0835aba6`,
`884fb41ed6af1cd25444ad167014923507fb367a75a16b7c5e6ab190a5554e01`,
`cf5c7f4faf4890a779d3bd10043fee2bca08efb6a67091f027ce6ed6c6f8df55`, and local image
`sha256:32e4b9d89dd74111bb662c5e0e94be970534ae3e79f0c72aa9bc77765109232b`.
Do not treat those historical IDs as live resources to operate on again.

Protected/unrelated containers still existed and were never modified:
`9cf2fe345496b45d79ff272e6c698552948f1e92ea106926826a770592623b24`,
`ce75ac504df9d0e58cedcdd0df93fe7ef39c3b66ef3c13b0bb6b580def64516c`,
`d2433851ea5990e273b0673fdbd5c180917f1216f3c81bdfb4517da0207e2c2a`,
`d2550144998b4b71de98cb9ecf6448f87c7b99a3c2f69ea778a29413fffa5ffb`.
Leave them and historical E04 resources untouched. Cleanup only newly acknowledged,
exactly owned resources with retained receipts; never infer custody from names.

## 5. Existing source/API boundaries to reuse

| Task | Concrete source/API seam |
| --- | --- |
| Initial root and handoff | SDK `core/crates/layerfs-api/sdk/src/project/`: `ProjectApi::init`, SealedProject, install; existing Project/Content/Storage/Persistence/History route |
| Ordinary runtime and startup | SDK `sandbox/{api,lifecycle,types}.rs`; Sandbox `src/backend/docker/{container,container_types,archive,endpoint,streams}.rs` |
| Direct daemon startup/control | Daemon `src/application/{owner,serve,connection,config}.rs`, `bootstrap.rs`; initialize shared Store/fixed readers/cache and one Overlay once |
| Sole Workspace registry | Daemon `src/control/{registry,operations,status}.rs`; existing Binding retains `Arc<store::BoundWorkspace>` and control disposition |
| Root binding and fresh operation | Daemon `src/store/{bind,operation,ports}.rs`: bounded root validation; `Service::operation(token)`/`BoundWorkspace::operation()` returns StoreOperation with `workspace()`, `overlay()`, `ports()`, `client()` |
| Read semantics | Workspace `src/base/`, `src/workspace/view.rs`, `src/operations/namespace/list.rs`, `src/operations/file/read.rs`: BaseView/SourceView, stable source,64-key merge with whiteout continuation,128KiB read windows |
| Mutation semantics | Workspace `src/mutation/driver.rs`, `src/operations/`; `Workspace::mutate`, `Operation`, `Outcome::Applied { publication, stat }` |
| Typed SQL/lifetime jobs | Daemon `src/overlay/commands.rs`, `src/service/completion.rs`; Overlay `src/lifetime/` exact BaseSource/OpenFile/LookupOwner/FileRead/CapturedReader/OperationOwner and indexed records |
| Captured file construction | Workspace `src/construction/captured/owner.rs`: `CapturedFileEdits::prepare/construct`; `src/construction/records.rs`: indexed construction backing |
| Commit composition | Daemon `src/store/{commit,commit_types,settle}.rs`: Capture → begin Save → constructor → finish → History → prepared/known local install; exact failure/unknown custody |
| Native request service (new) | Fuse src/{ports,session,dispatch,operations,coherence}; daemon application/filesystem.rs and service/filesystem_port.rs supply assembly/engine ports only |
| Native wire addition | Bridge `src/control_types.rs`, request/reply codecs; SDK `src/workspace/` and `src/control/connection.rs` use the existing authenticated exchange/correlation owners |

`store::BoundWorkspace::commit` accepts a constructor of the form
`FnOnce(&Save<'_>, Capture, &BranchSnapshot) -> Result<FilesystemRootId, CommitError>`.
The ordinary control route currently refuses live Commit because that constructor
is absent. SDK bind returns `Bound`, not native Ready. Existing Store-half Commit
proofs do not establish live namespace assembly.

Fresh StoreOperation scopes retain independent original provider failure while
sharing the immutable cache/base. SQL stays in Overlay/Persistence, not Content
or the daemon's provider-independent `store/` adapter. Reuse public typed ports;
do not add a new encoder, SQLite engine, native-tree materialization, source
include or host Init route for Commit.

## 6. Dependency-aware implementation and proof plan

Prepare a deepest-file plan for each checkpoint from current source. R2 enables
native access and correct lifetime ownership. R3 composes ordinary mutations and
cache behavior onto it. R4's bounded captured namespace component work may proceed
alongside R2/R3, but retain independent component proofs and reviewable commits.
R5's mounted integrated proof depends on all three. Serialize Cargo/tests/native
proofs in this checkout; dependency overlap is not permission for competing builds
or measurements. Do not stop after a narrow read/write demo or count a proposal
as implementation.

### R2 — complete native read floor

Activate a real replacement `layerfs-fuse`, preserving/accounting the dormant
incompatible predecessor before relocation. The predecessor imports old Workspace
APIs and cannot activate unchanged. Only Fuse owns fuser types and the complete
kernel connection/request lifecycle:
mount/profile/session/readiness/drain, callbacks, bounded admission/queues/workers,
parked requests/completion wakeups, operation handlers, replies and cache coherence.
Daemon owns assembly/control, shared Overlay SQL fairness/direct Store and the
existing Commit driver. Workspace/Overlay retain semantics/backed state. The
dependency direction is daemon -> fuse -> workspace; Fuse imports no daemon.
Reuse current domain ports/types, adding only genuinely missing service interfaces.
No extra adapter crate, duplicated semantic engine or copied token hierarchy.

Use Fuse session/, dispatch/ and operations/ homes instead of planned daemon
native/ and kernel request/steps/. Daemon application/filesystem.rs assembles one
Fuse service shared across mounts; service/filesystem_port.rs implements narrow
engine ports, not a second session/request engine. One fixed K-worker Fuse pool
lives for the daemon-assembled service, distinct from per-mount receive/session
owners and the existing SQL scheduler serving filesystem/Commit/cleanup. Correct
existing engine modules remain in place. See the reviewed layout for conditional
splits; native_open.rs, read_service.rs, scratch and extra query modules are not
scaffolding requirements.

Extend the existing registry and authenticated protocol with Attach/Locate/Ready
and retained native disposition. SDK mount is two separately acknowledged original
attempts: bind then attach; a lost attach retains the original Bound token and
failure. Do not add another Workspace registry or command identity to Bridge.

Sandbox needs selected `/dev/fuse`, daemon SYS_ADMIN and a verified mount security
profile. Keep Privileged=false, NoNewPrivs, nonroot ordinary commands, root-only
container-local device access and protected Store/Overlay/config/proc aliases.
Capture connection-specific abort/unmount authority safely. Reprove access after
changing deployment powers; R1's non-FUSE protection proof does not cover them.
Record actual mount namespace/visibility, without assuming copied/private/slave
behavior. Use safe first-party nix mount/plain unmount ownership with an independent
retained device descriptor around `Session::from_fd`; no helper/Drop/lazy fallback.

Implement the selected S8 kernel profile, including ACLAll/allow_other plus
DefaultPermissions, admitted command UID/GID projection, nosuid/nodev/noatime,
128KiB windows, TTL60/KEEP_CACHE, background/congestion1, two shared-fd loops and
writeback off. Check the current spec for every negotiation bit and receipt.

Complete I-3 readiness: the kernel mount exists, handshake completed and every
dispatch loop is running. An outer thread spawn, INIT reply attempt or one stat
is insufficient. Pinned fuser starts internal loops later and exposes no separate
join handles; partial spawn/first join failure can leave loops unjoined. A finite
shared-fd callback probe need not reach both loops. Callback thread IDs show entry,
not continuous liveness; an open proc task-directory plus metadata is not an exit
witness. Resolve these mechanisms with source-supported evidence and exact retained
failure states; do not weaken Ready or extend the authorized third-party patch.

Implement callback-entry admission before copied input with R+N accounting, bounded
receive slots and terminal wakeups, plus one Fuse-owned fixed-K fair service
shared across daemon mounts with
parked owned continuations. Current Pending.wait and Workspace mutation drivers
provide semantics but are synchronous; calling them on receivers/service workers
does not satisfy I-8. The port must transfer the original pending handle and
race-safe completion/loss/credit wakeups; Fuse resumes and consumes that same
outcome once. Handle registration/completion races and terminal disposal; no
try_complete polling or thread per waiter. No admitted prerequisite wait on those
workers, no locks held
across parks, and no progress dependency on a later kernel request (I-9).

Add consistent read/lookup answers and atomic indexed lookup aggregate acquisition
in the same deciding owner transaction. FORGET uses checked `(mount incarnation,
namespace, serial, nlookup)` decrements; no resident visited-tree map. Keep root1
mapping, stable serial identity, checked attributes/time conversion, file-handle
route/serial validation and independent read/readlink/reply custody. Readdir needs
indexed handle/cookie associations with valid old offsets/name boundaries; advance
only accepted reply entries, continue through empty whiteout pages, and acquire
no lookup reference for READDIR alone. Release per-open cookie state after its last
owner during a live mount, not just on terminal teardown.

Normal unmount uses a reversible kernel busy probe while service remains usable.
EBUSY restores Ready. Terminal success requires detach, joined loops and complete
request/Owner/completion/Store/control drain, then native ownership revocation,
acknowledged logical Close and route removal; bounded physical deletion may follow.
Never let a second teardown attempt skip a consumed failed session/join result.
Retain each original error/input/reply owner, including startup and send failures.

Fuse reports connection serving/drained facts; daemon combines them with actual
Workspace/service admission and all namespace-bound engine/Store/control work for
overall Ready/terminal unmount. Do not duplicate those state machines or use a
connection receipt alone as the aggregate result. Device/capability Create wiring
belongs in existing Sandbox container.rs/container_types.rs; inspect through
endpoint.rs/topology.rs. request.rs remains the ordinary Exec request owner.

Required R2 proof: installed Store → complete Ready → ordinary external Bash
read/stat/readdir/permissions → normal unmount and full drain. Include an unregistered
process retaining cwd/FD, Busy service usability, lost Attach/Locate behavior,
stable identity and exact lookup/open/request disposal. A first mount/stat round
trip is intermediate evidence only, not R2 acceptance.

### R3 — ordinary mutations and kernel coherence

Wire existing Workspace operations through the native request/custody service:
create/mkdir/symlink, write/append, truncate/regrow, chmod/times, rename/link/unlink,
open-unlinked files, removed cwd/O_PATH and mapped access. Implement/refuse attribute
fields deliberately; don't silently ignore unsupported SETATTR fields. Preserve
opaque symlinks, permissions and namespace semantics.

Use exact OpenFile/FileRead/lookup/native owners, mutation publication and one
`ReplyAttempted(publication)` after the reply attempt. Lost replies retain published
changes. Keep kernel cached-I/O and writable-mapping flags correct with writeback
off; do not reject legitimate kernel-origin cache writes. No per-WRITE invalidation
that can deadlock on kernel-held folio locks. Follow the spec's notifications,
entry/attribute cache and retained-reference rules rather than switching profiles
to evade a failure.

Prove native behavior through ordinary filesystem calls from both the ordinary
runtime and an external unregistered executor. Include supported sequential and
concurrent operations, partial pages, append, truncate/regrow, metadata/links,
rename/removal, mappings, and lifetime races. Record exact original outcomes;
no early reclamation based on command exit or lost client output. Meet the full
owning R3 proof rows before claiming completion.

### R4 — complete captured namespace and incremental topology

Implement the missing namespace producer over one exact Capture/CapturedReader,
not live active state. Preserve stable input without bulk Overlay copy. Acquire
and retain original operation/reader ownership and publication uncertainty.

Reuse `CapturedFileEdits::prepare/construct`: existing files use the retained
FileView/backed incremental edit route, new files use construction from runs.
Use actual Store-derived construction policy and capacities, `Save::sink`, and
same-Save authenticated reads. Content owns canonical filesystem/attribute/value/
directory/inode algorithms; Storage owns encoding/dedup/packs; Persistence owns
writes. Metadata patches preserve untouched keys. Include complete final names,
links, metadata, changed file roots and fresh serial declarations.

Add provider-neutral captured namespace page ports over existing ReaderInodes,
ReaderInode and ReaderDirectoryEntries Owner jobs. Build a sealed indexed adapter
implementing every `StreamedFilesystemInput` ordered cursor and matching point
lookup: directory headers, parent-local names, inode updates, fresh serials and
fresh positions. Reuse `IndexedConstructionRecords`,64-row and64KiB job windows;
full names belong in values rather than truncated identity keys. Cursors and points
must agree across all passes. No artificial total edit/file/Workspace/Commit cap.

Use `update_filesystem_streamed_backed` and existing same-Save
`FilesystemObjects::new_with_accepted` boundaries. Fix remaining resident demanded-
serial/addition/parent maps, whole-base non-file alias scans and cycle/validation
state in their owning Content algorithms. A streamed front end around unbounded
validation is not the completed resource contract. Avoid quadratic work; tiny
incremental changes must not walk the whole base.

Prove full final namespace/byte/metadata/link results, wide/deep/sparse/alias/cycle
cases, new/deleted/changed entries, arbitrary captured fragmentation and bounded
windows. Count construction/SQL/I/O/copy/release work. Preserve first-original
file/namespace/backing/captured-reader failure custody; no root-only callback error
that drops retained owners. Release only after actual consumers end. Resident
FilesystemInput or existing-file-only demos cannot replace full R4 acceptance.

### R5 — mounted live Commit and known install

Connect the completed constructor to the existing real control Commit and Store
composition. Keep Capture → Save construction → `Save::finish()` → History
publication → known local install explicit. A live daemon keeps Store open; never
seal per file or per Commit. Use one construction producer:
`LAYERFS_CONSTRUCTION_WORKERS=1`.

Commit captures the shared published Workspace frontier, including changes from
unregistered processes and multiple calls, regardless of command exit status.
Reuse the owner Branch overwrite/captured-parent rule, not an obsolete head-CAS
interpretation. UpToDate may reuse the head/root without a new Commit. On known
install advance the base while preserving later active mutations and the effective
live view, retained descriptors/readers and their exact original roots/floors.

Required integrated proof: real mount → ordinary external Bash changes → explicit
Commit → known Save/History/install outcome → fresh mount of the resulting Branch
→ independent full namespace/bytes/metadata/hardlink/symlink oracle. Include
`.git`, ignored/cache/output membership. Do not reconstruct expected output from
constructor parameters or a host-materialized fallback.

Cover Committed/UpToDate, captured parent/overwrite publication, missing dependencies,
real Store Busy, definite failure, lost/unknown History outcome, known publication
with local install failure, later active writes, retained reads/descriptors and
same-mount as well as fresh-mount survival. No guessed refresh/replay/resolution.
Leave unknown custody retained with exact receipts. Distinguish semantic success,
cleanup, command budget and numerical eligibility. R5 completion must include all
owning proof rows, not only a successful happy-path Commit.

## 7. Build, proof, provenance and accounting discipline

Use the primary checkout; the main executor owns edits, Cargo, Docker and native
proofs/cleanup. If separately authorized read-only reviews are used, they must not
build, mutate, test or manipulate resources. Do not create a worktree or run parallel
build/measurement campaigns merely to bypass serialization/interference rules.

- Build from repository root with the actual core manifest and `--locked`.
  Preserve ARM inputs: `--cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon
  -C target-feature=+aes,+sha2`. Explicit RUSTFLAGS must repeat them.
- Use existing dependency capabilities. When activating a real replacement,
  review the exact required graph/lock change; do not perform a broad update or
  edit registry packages. Keep fuser exactly0.18.0, default-features=false for the
  selected native route, with only the authorized signed-timestamp correction.
  Before an actual fuser build run:
  `python3 -B core/tools/check_fuser_integrity.py`.
  Provenance lives at `core/patches/fuser-0.18.0/provenance.json`;
  base archive SHA256 `b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd`,
  patched time.rs SHA256 `a333dba1c186c022eb55b950ec8895fb767c63e8ca156150b605d31d6876287d`.
  Preserve both owning path/version patch declarations. Docker qualification is
  accepted for this patch; its historical fractional signed-minimum FAIL remains.
  No other third-party patch/fork/vendor change is authorized.
- Build selected tests/examples with `--no-run` before executing. Give every test
  invocation an explicit wall stop, normally100s and never over120s. Treat the
  ceiling as a failed hang: retain output and diagnose before changed rerun.
  Native independent proof defaults remain under10s (the R1 proofs used9s).
  Performance commands normally≤15s; only declared allowed exceptions≤25s or
  existing frozen-family exceptions apply. Do not alter timeouts/workloads/cache
  treatment to turn a failure into PASS. Product Bash gets no such automatic timer.
- One sample per selected case/arm at its declared identity. Preserve append-only
  failures/ineligible/unrun rows. Reuse closed prepared inputs and qualifying
  unaffected evidence; use setup clone when supported, never a mutated sample.
  Natural/warm state proves no cold performance. No earlier-phase/own-write cache
  credit, lifetime-peak substitution or hidden file-sized backing growth. No new
  numerical gate. For SQLite performance claims require both EXPLAIN and correlated
  runtime database profiling. Report speed/storage together at frozen scopes.
- New production files≤999 physical lines; lib.rs/mod.rs≤200 and declaration/
  delegation only. Product source has no inline tests/test-only paths/hooks/mocks.
  External tests use public APIs. Update source architecture/API docs with changes.
- Run scoped tests/examples, warning-denying Clippy, fmt, product-boundary guard and
  its self-tests for affected product changes. No CI/aggregate pre-push gate or
  retired `tools/preflight.sh` replacement. Passing empty scans do not prove code.
- Every local commit records exact first-parent → final staged → committed
  production LOC, signed delta, counting method and migration subtotals. Use the
  pinned `tools/production_loc.py`, SHA256
  `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, with the same scope
  on both snapshots: first-party Rust plus shipped SQL, excluding inline/transitive
  tests, fixtures, examples, tools, docs, archives and third-party/generated files.
  Classify by exact active Cargo-member paths. Starting totals: combined170673,
  core105256, active62382, reference65417, excluded predecessors37431, excluded
  integration5443. Relocation/duplication/retirement are separate, honest categories.
  Never delete validation or move product code out of scope to improve LOC.
  Commit messages use `Production LOC: <before> -> <after> (delta <signed>)` and
  end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Keep local work reviewable; no remote push/PR/issue/release. Root reference
  retirement belongs only after R8 acceptance/R9 dependency coverage, not this batch.
  Reconcile any covered predecessor relocation explicitly; don't delete early.

## 8. Start and completion behavior

On dispatch, inspect repository status/history, relevant source/public APIs, active
members, reusable binary/image identities and actual runtime state. Read the
mandatory authorities and current rulings. Check any live inherited tool handles;
collect original outcomes before launching work. Do not adopt arbitrary resources.
Record the R2 deepest-file plan, unresolved native mechanism requirements and
initial proof selection, then implement. Required engineering difficulty is not
an owner-approval gate or permission to lower the contract.

Progress through R2, R3, R4 and R5 with separate source/receipt/commit checkpoints.
Update the rollout ledger and affected architecture docs honestly after each.
Continue authorized work rather than stopping at plans or a narrow demo. When a
required external prerequisite is genuinely unavailable, report its exact evidence
and retained state; do not silently substitute a partial implementation.

At R5 completion report each checkpoint's actual implementation, all independent
and integrated proof outcomes/limits, source/build/runtime pins, per-commit LOC,
resource cleanup/custody and remaining R6–R9 work. Keep the full R0–R9 objective
uncompleted unless those later requirements have actually been separately fulfilled.
This prompt itself requests no R6–R9 execution, early retirement or remote action.

The previous thread's pause tool returned `cannot update goal because this thread
has no goal`; no app paused status was established. It nevertheless stopped all
work at verified R1. Inspect the goal state on your own dispatched thread; do not
invent an active/paused Goal or create a replacement merely to repair a receipt.
