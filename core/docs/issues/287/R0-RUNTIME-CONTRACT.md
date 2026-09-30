# R0 runtime, native wire and provider contract

> **Status: Current planning checklist; no release candidate exists.**
> R0 implementation freeze, 2026-09-30. Product source inspected:
> `7edddbdb8e8512627aed0ed42533ef099d802384` in owned worktree
> `/Users/yifanxu/.codex/worktrees/issue287-implementation/layerfs`, branch
> `codex/issue287-implementation`. This document freezes implementation inputs;
> it does not advertise runtime support, execute a migration or qualify memory.

This is the runtime-owned supplement to the [packet](../../architecture/proposal/bounded-workspace-implementation-20260930/README.md),
[rollout](../../architecture/proposal/bounded-workspace-implementation-20260930/ROLLOUT.md),
[concurrency specification](../../architecture/proposal/bounded-workspace-implementation-20260930/CONCURRENCY.md)
and [acceptance specification](../../architecture/proposal/bounded-workspace-implementation-20260930/ACCEPTANCE.md).
Its scenario scope is SC-06/07/08 and the mounted route of SC-01–05.
The coordinating implementation owner owns all Bridge changes and integration.

## 1. Current source and reserved allocation

The audit examined the actual operation/response encoders and admission paths,
rather than treating older architecture descriptions as current registries.

| Actual baseline source | Current fact | Replacement obligation |
| --- | --- | --- |
| [Request and operations](../../../crates/layerfs-bridge/src/contract/request.rs), [control constants](../../../crates/layerfs-bridge/src/contract/control.rs) | Operation opcodes 1,2,6–29 are used; 3/4/5 were retired. File, history and daemon-control request profiles are 1,2,4. | Preserve existing encodings and retired allocations. |
| [Response codec](../../../crates/layerfs-bridge/src/adapters/native/protocol/response.rs) | Current response tags are 1–6 and 8–29. Tag 11 is MetadataSaved; it is occupied. | Preserve tag 11 and every historical tag; do not reuse retired tag 7. |
| [Frame codec](../../../crates/layerfs-bridge/src/adapters/native/protocol/frame.rs), [native client](../../../crates/layerfs-bridge/src/adapters/native/client.rs) | HELLO is u16=1; frame kinds are 1–7; data ≤16,384 B and metadata ≤32,768 B. Client creates one 2 MiB upload helper even for zero-input calls. | HELLO v2 and owned partial I/O replace helper ownership without increasing frame limits. |
| [Exec contract](../../../crates/layerfs-bridge/src/contract/execution.rs), [SDK Exec](../../../crates/layerfs-api/sdk/src/workspace.rs) | Opcode 19 and default SDK call use a 30,000 ms finite deadline, with 8,192 B captured per output stream. | Typed UntilOwnedExit is a new negotiated path; output limits stay fixed. |
| [Daemon control](../../../crates/layerfs-daemon/src/control.rs), [lifecycle](../../../crates/layerfs-daemon/src/lifecycle.rs) | One live/closing control session and one selected Slot; dispatch holds its lock through Exec/Commit. | Entry/command/submission leases replace the singleton authority. |
| [Execution](../../../crates/layerfs-daemon/src/execution.rs) | Direct child, process group, pipe draining and direct-child wait. | Exact domain custody, adopted-child attribution and independent cleanup predicates. |
| [FUSE mount](../../../crates/layerfs-fuse/src/mount.rs), [adapter](../../../crates/layerfs-fuse/src/adapter.rs) | Serving euid0, root.uid0 and SYS_ADMIN; Owner ACL; root-uid guard; 10 s callback observations; 128 KiB request envelope and max_background=1. | Separate serving/actor/backing authority and first-party cancellable Linux session. |
| [Sandbox launch](../../../crates/layerfs-sandbox/src/docker.rs), [retained session](../../../crates/layerfs-sandbox/src/session.rs) | Aggregate 2 CPU/512 MiB/64 PID Docker limits; no delegated per-command domain; one retained control session. | Explicit provider provisioning and admitted exclusive channels. |

The coordinated R0 allocations are:

| Registry | Allocation | Meaning |
| --- | --- | --- |
| Request opcode | 30 | FileSetConstruct, Server-owned, profile 1 with explicit member canonical digests. |
| Request opcode | 31 | RuntimeCapabilities, authenticated read-only daemon query. |
| Request opcode | 32 | RuntimeControl, grouped versioned daemon operations. |
| Request profile | 5 | RuntimeCapabilities/RuntimeControl only; payload version 1. |
| Response tag | 34 | RuntimeCapabilities. |
| Response tag | 35 | ExecTerminal. |
| Response tag | 36 | ExecCancelObservation. |
| Response tag | 37 | RuntimeLifecycleObservation. |
| Response tag | 38 | SelectedViewStreamCompletion. |
| Response tag | 39 | FileSetCompletion, Server-owned. |
| Capability bitmap | version 1, u64 | Runtime features below; separate from permissions. |
| Command capability | tag 0x51 + 32 random bytes | Exact live command authority, separate from view-token tag 1. |

The coordinated [Service allocation](R0-FROZEN-INTERFACES.md#6-service-capability-certification-and-final-publication-allocation)
owns operations33–37/profile6 and responses30–33/40. This runtime note assigns
none of them independently. No response is
inferred from a request opcode with the same numeric value. Unknown tags,
versions or required capability bits refuse before dependent effects.

Native HELLO v2 is exactly `version:u16=2 | purpose:u8 | reserved:u8=0`.
Purpose General=1, Catalog=2 and Control=3 is authenticated by the established
Noise record and peer. HELLO v1 retains its exact two bytes and gets no protected
resource privileges. General owns ExecStart, construction, bodies and streamed
reads. Catalog admits only history queries and metadata_mutation() operations.
Control admits finite capability/status/cancel/cleanup operations; it never
admits ExecStart, construction, input bodies or bulk reads. One independently
reserved Catalog dispatch/FD/window owner cannot be consumed by normal sessions.
Control endpoint windows are separately charged; the Server's catalog ring and
work reserve are counted once at the Server assembly.

## 2. Exact identities, versions and capability refusal

All multibyte integers below are big endian. `blob` is u16 length plus exact
bytes. Existing managed Workspace IDs retain their 1–63-byte ASCII grammar.
The runtime `Selector` is `sandbox:[16] | daemon_instance:[32] |
workspace:blob | incarnation:[32]`; both instance and incarnation are nonzero.
Its maximum encoding is 145 B. Authenticated principal comes from VerifiedPeer,
not a caller-supplied field. An endpoint, PID, Workspace ID alone or a current
registry observation cannot substitute for this selector.

RuntimeCapabilities has zero input, Store=0, generation=0, response_bytes=0,
positive request ID, profile 5 and a finite ≤5,000 ms request. Its payload is
`version:u8=1 | sandbox:[16] | expected_instance:[32]`. Tag 34 returns those
exact identities, bitmap_version:u8=1, available:u64, control_payload_version:u8=1,
execution_domain:u16, mount_actor:u16, kernel_session:u16,
private_version:u8, namespace_version:u8, file_policy_version:u8,
runtime_profile_digest:[32] and max_workspaces:u64. It is ≤128 B including tag.
Zero version/profile fields mean unavailable; they never select legacy support.
Query consumes only its protected read/control owner and creates no Workspace,
actor, command, mount or migration owner. A capability response is not capacity
reservation or a receipt for an earlier operation.

| Bit | Frozen feature | Enablement condition |
| --- | --- | --- |
| 0 | OwnedNativeV2 | HELLO v2, exclusive ownership and partial authenticated I/O gates. |
| 1 | UntilOwnedExit | Entire SDK/Bridge/daemon typed lifetime and contained domain composition. |
| 2 | ExactExecCancel | Principal/selector/capability lookup plus protected finite control. |
| 3 | ExactActorMount | Split actor/backing owner and real actor permission proof. |
| 4 | CancellableKernelSession | First-party Linux UAPI request/interrupt/publication gates. |
| 5 | SelectedViewStreams | Exact bounded read/list data and terminal seals. |
| 6 | DiscardPrivate | Explicit same-selection discard, pin and cleanup custody. |
| 7 | KeyedWorkspaceRegistry | Count/incarnation/retained-entry ownership. |
| 8 | ConcurrentCommands | R4/R5 prerequisites and admitted same-W/cross-W proof. |
| 9 | ConcurrentCommits | #248 gates, per-W submissions and actual shared Store capacity. |
| 10 | ContainedExecDomains | Verified delegated Linux cgroup v2 authority and exact reap/drain. |
| 11 | TruthfulQuota | Semantic allocated/reserved/uncertain/shared/protected snapshot. |
| 12 | SupervisorMountRecovery | Owned stale mount lease and explicit bounded detach proof. |
| 13 | StrictSiblingConfinement | Separate mount/FD/proc/environment confinement proof; absent from the initial contained profile. |
| 14 | PrivateV3 | Qualified private v3 attachment/owner authority. |
| 15 | CanonicalV2Attachment | Server-certified namespace v2/file-policy v2 binding. |

Higher bits are reserved. Every requested operation revalidates actual provider,
rights, exact selector and required profile before effects; query does not waive
that admission. Registry/command concurrency bits remain disabled until their
composition gates pass. Darwin mounted/contained execution is Unsupported.

Runtime domain profile 1 is LinuxDelegatedCgroupV2; actor profile 1 is ExactActor;
kernel session profile 1 is FirstPartyLinuxUapi. Qualified attachment requires
private v3, namespace v2 and immediate-parent file policy v2, with the separately
frozen canonical digests and verified namespace authority. Count policy is
immutable positive max_workspaces, default 2; configured 1/2/3 are separate proofs.

The runtime profile digest is SHA-256 over exactly the following 216 ASCII bytes,
including every newline and the final newline. It identifies the chosen runtime
grammar/bounds, not a measured resource result or dynamically available capacity.

```text
layerfs-runtime-profile-v1
native_hello=2
runtime_control=5/1
execution_domain=1
mount_actor=1
kernel_session=1
stdout_bytes=8192
stderr_bytes=8192
callback_ms=10000
silence_ms=5000
heartbeat_ms=2000
cleanup_ms=5000
```

Digest: `bbf90c79c541baf176dc3ac7673ac3a5eef579d79e93c39709116dfb2bd9f57f`.
Runtime rights use an explicit version-1 u16 grant: Inspect=0x0001,
ExecStart=0x0002, ExecCancel=0x0004, Detach=0x0008, Discard=0x0010,
View=0x0020, AttachMount=0x0040, CloseClean=0x0080, CancelAndClose=0x0100.
Remaining bits refuse as reserved. Legacy u8 grant masks retain their meaning;
old mask 255 never confers new discard/domain/control rights by conversion.
Capabilities and grants are distinct. ExecCancel additionally requires the exact
command capability issued to that authenticated principal.
Grant expiry is checked at operation/control admission. An admitted execution
lease lasts until its owned terminal/cleanup disposition; expiry does not become
a whole-command timer. Later control requests still require current grants.
Closing a broken/dropped operation channel triggers scoped cleanup independently
of permission to open another cancel request.

## 3. RuntimeControl grammar and SDK handle ownership

Common native Request metadata remains generation:u64, store:u32, profile:u16,
deadline_ms:u32, response_bytes:u64 and opcode:u8, with the ID in the frame header.
Profile 5 appends `version:u8=1 | variant:u8 | Selector | variant_fields`.
All variants have zero input/zero response_bytes; typed result events/data do
not masquerade as ReadFile. The ordinary zero-input END_INPUT(total=0) is required.
The largest ExecStart request is 4,341 B, below the current metadata frame.

| Variant | Fields after Selector | Exact semantics / finite bound |
| --- | --- | --- |
| 1 ExecStart | caller_nonce:[32], lifetime:u8=1, domain:u16=1, actor:u16=1, runtime_profile_digest:[32], command:blob≤4096 | Nonzero caller nonce and existing UTF-8/non-NUL command grammar. ≤5 s through admitted execution ownership; then UntilOwnedExit. Requires General channel. |
| 2 ExecCancel | caller_nonce:[32], capability:[33], reason:u8 | User=1, HandleDrop=2. Exact live capability; ≤5 s observation on protected Control. This never starts a command. |
| 3 DetachMount | mode:u8 | IdleOnly=1; CancelCommands=2 requires CancelAndClose right as well as Detach. Stop new mounted admissions, resolve exact commands/callbacks, detach once; ≤5 s or exact retained cleanup. Dirty state stays selected. |
| 4 DiscardPrivate | generation:u64, revision:u64, baseline_epoch:u64, known_baseline:[32], mode:u8 | IdleOnly=1 or CancelCommands=2. Exact current selection, known baseline and workflow barrier; ≤5 s or exact retained cleanup. Refuse pending/Unknown submission. |
| 5 ViewReadStream | view:[33], serial:u64, offset:u64, bytes:u32≤131072 | Exact held view; ≤5 s, General channel. |
| 6 ViewListStream | view:[33], directory:u64, after_present:u8, optional after:blob≤255, entries:u16 in 1..128 | Exact held view and name cursor; ≤5 s, General channel. |
| 7 CloseClean | mode:u8 | IdleOnly=1, CancelCommands=2 with separate right. No dirty/pending/pin/handle/callback owner may disappear implicitly; ≤5 s or retained cleanup. |
| 8 OpenQualified | project:[17], branch:[17], commit_present:u8, optional commit:[33], private:u8=3, namespace:u8=2, file_policy:u8=2, canonical_profile:[32], runtime_profile:[32] | Explicit qualified selection and independent Server validity authority; ≤15 s. No old Stack/owner promotion. |
| 9 MountExisting | runtime_profile:[32], actor_profile:u16=1, session_profile:u16=1 | Mount exact already qualified attachment; ≤5 s or entered mount custody. |

Bool/option fields accept only 0/1; no trailing bytes are allowed. Mode values
are shared and require their explicit rights. No wire ExecWait/query/reattach
operation is introduced. `start_exec(id, command) -> ExecHandle` returns after
the Admitted event on the same operation's leased channel. `wait(&mut handle)`
consumes that channel's bounded continuation until its terminal; it sends no
second command and polls no historical registry. `cancel(&handle)` uses a
separately admitted Control channel; blocking `exec` is start followed by wait.
The handle is an owned, non-Clone SDK capability. An independent cancellation
reference may share only its fixed exact identity/control owner.

Handle Drop enqueues one pre-admitted cancellation intent. If its control path
is broken, closing its exact operation channel triggers the same command's
disconnect cleanup. Drop grants no detached-run mode and waits on no unbounded
cleanup. A lost Admitted/terminal makes the caller outcome uncertain once BEGIN
was attempted; the SDK never replays ExecStart. Caller_nonce rejects duplicates
among active/retained commands, but is not an all-lifetime replay database.

Exec ResultData uses `event_version:u8=1 | event:u8 | sequence:u64`.
Admitted(event=1, sequence=0) contains Selector, caller_nonce, command capability,
owned_domain:[32], actor_uid:u32 and actor_gid:u32; payload ≤272 B (actual
maximum 260 B before native framing).
Alive(event=2) adds phase:u8 (Running=1, Terminating=2, Draining=3) and is 11 B.
Alive cadence is 2 s with one unsent event; assigning sequence at sealing avoids
gaps from coalescing. Sequence increments are checked. Alive acknowledges
supervisor liveness, independently of Workspace revision or mutation progress.

Tag 35 terminal encodes version1, Selector, nonce, capability, domain, final
sequence:u64, leader_kind:u8 and leader_value:i32 (Exit=1, Signal=2,
BootstrapFailure=3), disposition:u8 (Exited=1, Cancelled=2, Disconnected=3,
ResourceTerminated=4), cleanup:u8 (Complete=1, Retained=2), cleanup_code:u8
(0 only for Complete), output_flags:u8 (stdout_truncated bit0,
stderr_truncated bit1), stdout:blob≤8192 and stderr:blob≤8192. Reserved flags
refuse. The maximum is <17 KiB, within unchanged 32 KiB Success metadata.
Caller returns success only after exact terminal and output validation.

Tag 36 echoes version1, Selector, nonce and capability, plus observation:u8:
Accepted=1, AlreadyTerminating=2 or AlreadyTerminal=3. AlreadyTerminal exists
only while that exact live lease still owns an undelivered terminal. After
retirement, old capabilities get NotFound; no new query guarantee retains every
past terminal. Repeated explicit cancel observes current state without another
spawn or Workspace rollback. Unknown cancel delivery never implies cleanup.

Tag 37 echoes exact variant and selection; it distinguishes Completed,
SelectionInstalledWithCleanupRetained and EnteredUnresolved with the actual
cause/cleanup codes and an operation selector. A definite pre-entry refusal is
Failure. Discard advances only private selection to the requested known
baseline, retains old pin roots and retires exact unreachable private owners.
It never burns new canonical work, refunds exposed serials, resolves Unknown
publication or deletes selected bytes based on a status query.

Selected-view data is raw typed read/list bytes across ≤16 KiB ResultData frames.
Tag 38 includes version, Selector, view, query kind, requested serial/directory,
returned_bytes:u32, record_count:u16, EOF:u8, size:u64 and checked continuation.
Read data totals ≤requested bytes≤128 KiB; listing is the existing exact entry
grammar bounded to 128 rows and 65,536 encoded bytes. Read has record_count=0
and no continuation. Listing validates entry boundaries/order and the exact
last-name continuation. Partial data is provisional; terminal mismatch retires
the channel while preserving the selected view lease. Neither path raises the
Success limit nor casts read bytes through u16.

## 4. Native I/O, allocation and finite liveness

One admitted operation exclusively owns a channel. Allocate its increasing ID
after lease acquisition; endpoint/peer/instance/channel epoch stay fixed through
terminal synchronization. A reconnect creates an independent future channel,
never a second attempt of begun work. Active ambiguity quarantines/retire only
that channel and preserves the operation's exact effects/custody.

The reactor retains prefix/ciphertext/read offsets and sealed-send/write offsets.
It validates length before growth, authenticates before frame dispatch, seals
once and consumes each nonce once. EAGAIN means continuation, not re-encoding.
Nonce/ID exhaustion is explicit capacity with owned cleanup; no rekey/replay
assumption. In-place checked dispatch owns the plaintext window until consumed;
there is no extra full decoded Frame Vec. Existing finite Source callers answer
bounded InputDemand on their producer; zero-input Exec has no upload helper.

The selected maximum full-duplex window arithmetic is:

```text
plaintext per direction = 32,768 + 20 = 32,788 B
ciphertext per direction = 32,788 + 16 = 32,804 B
both directions + strict 256 KiB send batch
  = 2 * (32,788 + 32,804) + 262,144 = 393,328 B
```

The batch checks remaining capacity before appending another record. Prefix
arrays, channel/crypto owner, handshake scratch, queue/index capacity, command
state, 16 KiB output captures, source demands, provisional read buffers, thread
stacks and kernel socket/pipe state are additional actual terms. No transfer
clones a charge. Terminal encoding uses the existing send plaintext window;
retained output is charged independently until delivery/cleanup disposition.
An admitted Exec needs its future terminal, cancellation and retained-owner
capacity before spawn; bulk traffic cannot consume that protected class.

Connect/authentication/HELLO and admission stay ≤5 s; stalled I/O/silence ≤5 s.
Alive is one coalesced 2 s event, not a lifetime frame-budget queue. General
finite operations retain their declared byte/frame/overall limits. UntilOwnedExit
has no elapsed command timer after Admitted. Cancellation/disconnect starts
an independent ≤5 s cleanup observation; domain emptiness, required reaping and
pipe drain must all be known, otherwise cleanup is retained. Normal exit waits
for the entire owned domain; it does not start a timer merely because the leader
or pipes exited first. Once the domain is empty, final drain/delivery each has
its finite 5 s liveness bound. Mounted callbacks retain 10 s observation points
and cooperative bounded-step checks; kernel/device waits are not preempted.

One removable timer record and one coalesced ready entry per live owner/channel,
keyed live capability/PID indices and charged reusable slots exclude a lifetime
command population. Close W visits W's actual owners; whole shutdown visits
all current owners once. Actual descendant exit events, produced output bytes,
index resize overlap and physical child resources remain real charged work.

## 5. Linux execution, actor and cancellable mount authority

Serving daemon and backing/control owners remain privileged. RuntimeActor is
`Selector + nonzero uid/gid + actor_epoch:[32]`; different live Workspaces have
different actors. Projected POSIX uid/gid uses the actor, while canonical mode,
mtime, serial and content stay portable. Actor assignment remains charged until
commands, mounts, kernel handles and retained cleanup owners are absent.

The trusted same-binary bootstrap starts before normal daemon/reactor assembly,
validates one bounded parent-issued setup record, joins the exact pre-created
cgroup before user code and configures the actor. It clears inherited credentials
from environment/FDs, supplementary groups, keepcaps and ambient/effective/
permitted capabilities, selects no_new_privs and exact uid/gid, verifies the
result, then safely execs `/bin/sh -c` with opaque command bytes. No unsafe
pre_exec hook, command recognizer, root same-UID or process-group fallback is
selected. Existing pinned nix has safe readiness/prctl/wait/user APIs; event,
time,user,mount feature selection, where required, is an explicit locked build
input. Daemon, Bridge and FUSE unsafe_code guards remain intact.

Delegation must supply a supervisor-owned domain subtree, inaccessible control
paths for child actors and protected runtime progress resources. Runtime lives
in a leaf; Workspace parents distribute existing aggregate controllers and each
Exec has a leaf. Active-domain authority holds exact open files and directory
identity. SIGCHLD ownership/subreaper setup precedes other threads; the shared
supervisor handles notifications and bounded nonblocking wait operations.

For an unknown adopted PID, `waitid(WNOWAIT)` observes without releasing PID
identity; then `/proc/PID/cgroup` identifies its exact retained owned domain,
then the supervisor reaps/account it. Linux documents retention of membership
until reaping even for zombies; empty-domain notification excludes zombies.
Keep the domain until all waitable attribution/drain is complete. Missing or
ambiguous membership retains cleanup instead of choosing a current command.
[Linux 6.12 process/cgroup semantics](https://docs.kernel.org/6.12/admin-guide/cgroup-v2.html#processes)
supports this method; the notification/reparent ordering still needs real proof.
`cgroup.kill` covers the selected domain while actual empty/reap/drain remains
the terminal predicate. Strong child physical bounds and healthy progress are
qualified separately from this logical custody.

ExactActor ingress uses allow_other with DefaultPermissions, NoSuid and NoDev,
followed by exact actor checks for every user operation. Kernel Forget/Release
uses a typed issued handle/node reference bound to mount epoch/Selector/actor;
missing credentials authorize only that kernel cleanup, never new user I/O.
Root UID is not a wildcard. Cached-read/confinement semantics remain explicit.
The initial contained profile does not advertise StrictSiblingConfinement.
Authorized external actor processes use the same mounted syscall authority
without an Exec capability; their supervision remains externally owned unless
explicitly integrated with the domain owner.

FirstPartyLinuxUapi owns checked wire fields, mount FD, byte-admitted request
windows, independent interrupt ingress and the semantic Workspace projection.
RequestLease binds `(kernel unique, Selector, mount epoch, actor/issued handle)`
to deadline/cancellation/publication/single-use reply state. It retains only live
keyed requests. The short publication race arbitrates cancellation before
irreversible selection; answered pre-publication EINTR cannot be followed by
candidate visibility. Publication entered/accepted or uncertain returns the
actual result/custody, never an EINTR rollback fiction. The kernel permits
interrupt/original races; unmatched interrupts use bounded pending state then
EAGAIN, following the [FUSE interrupt contract](https://docs.kernel.org/filesystems/fuse/fuse.html#interrupting-filesystem-operations).
Max_background=1, TTL zero, direct I/O and 128 KiB remain unchanged. The session
reader adds no construction producer. No patched/vendored fuser implementation
or silently selected legacy interrupt provider is permitted.

Orderly mount close drains accepted commands/callbacks/handles/submissions,
detaches once, confirms absence and only then releases entry/actor count. A
supervisor can recover its exact stale mount path/epoch after daemon death by
explicit bounded detach. That restores mount-path availability, not canonical
outcome knowledge, Unknown cleanup or crash data recovery.

## 6. Migration and real-provider evidence boundary

Store schema migration uses the [Server maintenance contract](../../architecture/proposal/bounded-workspace-implementation-20260930/SERVER.md#primary-c2-v10---v11-migration-quiesced-bounded-metadata-copy).
The baseline [Store arbitration](../../../crates/layerfs-storage/src/sqlite/ownership.rs)
is process-local and cannot exclude a noncooperating old process between SQL
transactions. Deployment ownership must close admission and stop/drain every
configured old Store user before issuing exact-file maintenance authority.
New cooperating users hold shared lifetime Flock<File>; the migration owns
its exclusive nonblocking counterpart and selected file identity. A manifest,
PID list, absence of an active SQL transaction or local mutex alone is insufficient.
Uncontrolled external Store access means Unsupported before schema effects.

Private-v2 owners/old runtime entries stay drained and charged while retained
pins/submissions/Unknown owners exist; qualified v3 attach cannot reuse their
actor, mount path or incarnation prematurely. V1 snapshot import into a new v2
Stack has distinct canonical/profile scope; it does not revive an old mutable
runtime path. Every migration's unknown phase keeps ingress closed and exact
custody. There is no new sync/WAL/crash recovery contract.

Read-only host/provider inventory at 2026-09-30 20:08 Asia/Shanghai:

```text
uname -srm: Darwin 25.4.0 arm64
id: uid=501(yifanxu), gid=20(staff)
docker context show: desktop-linux
docker info selected fields:
  KernelVersion=6.12.76-linuxkit, CgroupDriver=cgroupfs, CgroupVersion=2
  SecurityOptions=[seccomp builtin,cgroupns]
  MemTotal=4108828672, NCPU=8
docker ps: no running containers
```

Commands were `uname -srm`, `id`, `docker context show`,
`docker info --format '{{json .KernelVersion}} {{json .CgroupDriver}}
{{json .CgroupVersion}} {{json .SecurityOptions}} {{json .MemTotal}}
{{json .NCPU}}'` and `docker ps --format '{{.Names}} {{.Status}}'`.
All exited 0. No container was launched/entered, cgroup control written, mount
created, foreign target built or benchmark invoked. Existing image inventory
was read without modifying it; its tags are historical source, not qualified
current binaries. Provider version and cgroup support do not establish delegated
controller, actor ingress, interruption, containment, phase peak or resident-window
capability. Those proofs remain NOT_RUN. The same-open-FD memory.peak method in
ACCEPTANCE remains a separate supported-provider proof; no lifetime peak, heap
estimate or Docker containment is substituted for host Server observation.

## 7. Smallest coherent runtime deliveries and gates

| Boundary | Owned delivery and dependent interface | Exit proof |
| --- | --- | --- |
| R5a owned native I/O | Bridge owner replaces upload helper/blocking partial state; finite wrappers use same engine and root-owned HELLO/capacity contracts. | Partial prefix/cipher/send, nonce/ID order, authoritative early refusal, exact terminal, sibling channel integrity and no begun replay. |
| R5b command domain and UntilOwnedExit | Daemon commands/platform + Sandbox provider + SDK handle under one complete selector/grant/event grammar. | Real quiet >30 s; explicit cancel/drop/disconnect; setsid/double fork; leader/pipe/domain/reap ordering; truncation; bootstrap/resource/retained cleanup; accepted exit7 writes. |
| R5c exact mounted authority | FUSE projection/kernel session + Workspace RequestLease/cancellation publisher + runtime actors. | Two actor mounts and ordinary supported syscalls; sibling denial; real INTERRUPT publication race; issued cleanup; truthful quota; external actor route; exact mount-death/detach custody. |
| R5d explicit private workflow | Runtime control + Workspace exact discard selection and bounded retirement. | Dirty detach preserves bytes; explicit selected discard retains pins; Unknown submission refusal; cleanup failure remains charged. |
| R6a registry/channel ownership | Daemon keyed entries/count + Sandbox pools; no source recognition and no legacy singleton fallback. | Operator count1/2/3; retained entries count; incarnation-safe close; live capacity/refund; no lifetime commands/timer/ready scans. |
| R6b concurrent enablement | R4 qualified #248 composition + R5 owning gates + per-W submission leases/shared C2/C5 resources. | Same-W Exec overlap, cross-W progress, one pending submission perW, explicit shared-Branch conflict, independent exact outputs/roots/pins/cleanup and actual aggregate admission. |

Submilestones do not license incomplete enablement. Protocol checks can land
before the required Linux provider exists, but the corresponding capability
stays absent. Every real-provider proof is an implementation correctness/resource
proof, source-pinned and independent; no seven-family, Family8/9 or Family2
campaign is run under this runtime ownership. The final #288 handoff names
changed native/Exec/FUSE/channel/admission mechanisms without a speed PASS.

No product source, test, build, process-domain or mount implementation was
changed during this R0 audit. Exact runtime/provider proofs and future owning
checks remain unrun. The coordinating owner integrates this freeze with the
Workspace/Server notes before product implementation and records actual R0
publication separately.
