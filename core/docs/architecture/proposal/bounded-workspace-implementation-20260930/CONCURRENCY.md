# Concurrent Exec and multi-Workspace implementation specification

> **Status: Implementation design for review; research only, not an approved
> product contract or evidence of implementation.** Written 2026-09-30 against
> `7edddbdb8e8512627aed0ed42533ef099d802384`. This packet changes no product code
> and authorizes no builds, tests, measurements, profile changes or publication.

## 1. Scope, authority and selected design

Design [#249](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249) and
[#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219) together with bounded
streaming now. Enable and qualify concurrent delivery only after
[#248](https://github.com/Ephemeral-AI-Lab/layerfs/issues/248)'s exact-root,
large-final-run, bounded-memory and failure-custody prerequisites. Historical
finite proofs remain source-bound; this packet does not reactivate deferred
10,240 or Family 2 campaigns. Current issue bodies were read on this date.

Read the root/core AGENTS rules, [scenario catalog](../../../../../scenarios.md),
[integrated proposal](../bounded-memory-commit-20260930.md) and companion Workspace
and Server packets. SC-01–08 are the product workload envelope. SC-06–08 drive
the concurrency/custody proof; SC-01–05 continue through the same ordinary
`WorkspaceApi::exec(command)` → `/bin/sh -c` → FUSE → Workspace path. No command,
package, fixture or workload recognition chooses a product route.

Selected architecture:

1. An exact ID/incarnation registry owns each Workspace's mount/lifecycle and
   commands. The registry lock performs lookup/admission only.
2. One per-Workspace submission permit owns pending/retained Commit and its
   protected completion fund. Resource permits remain shared and charged.
3. Exclusive authenticated channels each carry one operation. A charged channel
   pool replaces singleton delivery; no wire multiplexing is required initially.
4. Owned incremental native I/O and a shared event supervisor replace borrowed
   long dispatch and per-command session/upload threads.
5. Exec has a typed UntilExit lifetime and an exact command capability. No elapsed
   whole-command kill timer; finite stalled-I/O, callback, Commit and cleanup
   bounds remain distinct.
6. Arbitrary descendant custody requires an explicitly supported execution-domain
   adapter. A Linux delegated-cgroup profile is specified below. Current launch
   does not prove that capability. Working directory/process groups alone are
   not its substitute. Hard sibling filesystem isolation is a separate profile.

The requested cleanup skill and playbook were read for responsibility/interface
design. Their generic suggestions to run tests, add CI, install dependencies or
apply code changes do not apply to this research task. Use concrete internal
types, narrow real I/O ports and existing locked dependencies; do not introduce
a strategy/factory/trait for every function or a service locator.

## 2. Before/after ownership and source observations

```text
CURRENT at baseline
 SDK owner -> one retained control session -> one native call
                                      |
 daemon one control-session thread + long lifecycle Slot lock
                                      |
                  selected Workspace + one mount
                   /                          \
        synchronous Exec loop              Commit preparation
        child/pipe polling                 host-wide frozen/remote
        30-second kill timer               singleton Service transport
                   \                          /
          implicit exclusion protects teardown

PROPOSED
 SDK calls / ExecHandles -> byte-admitted exclusive channel leases
                                      |
 authenticated owned I/O reactor: bounded records, one op/channel
             |                        |                       |
      exact registry lease       Exec supervisor       result/cancel delivery
             |
      +------ W-A ----------------+     +------ W-B ----------------+
      | mount + lifecycle owner   |     | separate mount + lifecycle |
      | commands A1, A2, ...      |     | commands B1, ...            |
      | inode/namespace leases    |     | inode/namespace leases     |
      | short current publisher   |     | short current publisher    |
      | G1 submission / live G2   |     | its G1 / live G2            |
      | protected completion      |     | protected completion       |
      +---------------------------+     +----------------------------+
               \                               /
             shared tagged byte/PID/FD/disk leases
             Store's unchanged writer admission / exact Branch CAS
```

High-confidence current-source facts:

| Source | Existing behavior | Consequence |
| --- | --- | --- |
| `daemon/lifecycle.rs:15` and `control.rs:310` | One Slot with selected/mount; dispatch retains its mutex through Exec/Commit | Opening more connections alone does not create concurrency |
| `daemon/control.rs:126` | Q=0; one control worker with a 2 MiB stack | An idle retained connection also occupies singleton admission |
| `daemon/execution.rs:55` | Direct child, process_group, bounded pipes, periodic polling; wall deadline kills group and waits direct child | No exact all-descendant or command-between-syscalls owner |
| `workspace/overlay/snapshot.rs:68` | Submission reserve acquires host-wide frozen | One retained Workspace submission excludes another Workspace |
| `workspace/runtime/state.rs:524` | Host-wide remote boolean; metadata exception has one additional slot | Distinct from the correct per-Workspace submission rule |
| `daemon/transport.rs` | Singleton Service session, one Client, global request IDs may arrive lower after scheduling | Pooling must allocate request IDs on the leased channel |
| `bridge/native/client.rs:45` | Source upload thread with 2 MiB stack, including zero-input control calls | Changing only daemon process polling leaves per-Exec helper growth |
| `bridge/native/connection.rs` | Blocking Read/Write, mandatory Instant, stateless Noise and checked nonces | Need owned partial I/O plus explicit lifetime/liveness |
| `sandbox/session.rs` | One retained idle channel, same-instance Hello validation, discard on failure | Retain identity checks; replace singular retention/retirement scope |
| `sdk/workspace.rs:75`, Bridge Exec contract | Exec uses 30,000 ms and 8 KiB capture per stream | Runtime policy must change end to end; output bound is independent |

These are source findings, not new measured failures. A shell's child processes
can already overlap on the mount; that is not multiple independent SDK Exec.

The concurrent target replaces the singleton ownership representation. It does
not add a second branch around the current long locked/threaded dispatcher.
Retire the old Slot-based lifecycle, synchronous Exec polling, singular transport
and per-call upload-worker bodies after their callers delegate to the owned
engine. Finite public call signatures and existing compatible wire grammars may
remain thin wrappers over that one engine; they are not retained fallback
implementations. A concurrent profile without required new capabilities refuses
before dependent work rather than selecting the old model.

| Old cause / counterexample | New authority/type that excludes it | Invariant / adversarial proof |
| --- | --- | --- |
| One selected Slot is replaced while another mount/command lives | Exact WorkspaceEntry plus count and lifecycle leases | A live/retained entry cannot be replaced by another ID; create count3 and close one while siblings run |
| Long dispatch lock implicitly protects all children | ExecLease + owned process-domain handle | Every spawn has an admitted owner through domain-empty/reap/drain; leader exit and pipe EOF alone cannot retire it |
| Host-wide frozen flag excludes all other submissions | Workspace-local SubmissionLease | Same-W second Commit refuses; distinct Ws retain independent known/Unknown outcomes |
| Short allocator/history operations acquire whole-Save slots | Protected C5CatalogPermit distinct from C2SavePermit | Refill/progress while both C2 Saves work; no extra Save arena/worker and no transaction-order deadlock |
| Global IDs arrive out of order on a reused socket | Exclusive ChannelLease owns next native ID | Allocate after lease; reverse scheduling cannot send an older ID |
| Borrowed Output/Source requires long worker stacks | Owned OperationTicket + bounded input/result continuations | One shared I/O engine; no thread/stack per admitted zero-input Exec |
| UntilExit represented by a very large Instant | Typed lifetime separate from liveness/admission | Quiet healthy command crosses30s; callback/Commit/stalled-I/O bounds remain finite |
| Heartbeats exhaust finite response frames | Typed Exec event contract with rate/byte credit | Unlimited healthy duration does not build a lifetime queue; finite file operations retain their length-derived frame accounting |
| View payload cast to u16 and placed in Success | ViewReadData contract plus bounded terminal trailer | 128KiB complete read needs data frames, never a metadata-frame allocation/cast |
| Cancellation names a PID or latest command | Principal-bound command capability and domain owner | Wrong W/incarnation/old capability cannot cancel a reused PID or sibling |
| A failure globally discards healthy sessions or assumes abort | Exact channel/operation disposition | Begun mutation is never resent; another channel's synchronized state is untouched |
| Admission/cancel/close scans all earlier commands | Live keyed capability/PID indices, reusable slots, removable timer/ready entries | N sequential commands do not retain N entries or perform1+2+...+N history work |

These are representation and authority requirements for the replacement, not
patches whose correctness depends on preserving singleton exclusion.

## 3. Concrete owners and interfaces

The following are proposed signatures and field contracts, not new public source.
Opaque keys own identity; do not pass loosely related ID/PID/socket integers.

| Owner/type | Required fields and ownership | Required operations |
| --- | --- | --- |
| `WorkspaceKey` | sandbox instance, validated Workspace ID, incarnation | Exact equality; no latest-by-ID resolution |
| `WorkspaceRegistry` | keyed live/retained entries plus one #219 count policy | `reserve_open`, `resolve(peer,key,operation)`, `finish_absent`; no long I/O |
| `WorkspaceEntry` | Workspace, mount owner, lifecycle state, tagged usage, active Exec index, submission/teardown custody | Short admission/closing transition; entry stays alive through all leases |
| `WorkspaceLease` | Arc entry + admitted registry/reference bytes | Direct release; no unmount on Drop |
| `ExecKey` | WorkspaceKey, authenticated principal, caller command nonce | Distinct from channel request ID and OS PID |
| `ExecLease` | entry lease, owned process domain, pipe FDs, capture buffers, terminal/failure slot, cancellation state | Spawn-to-terminal cleanup; never rollback Workspace writes |
| `CommandCapability` | tag + 32 random bytes bound to ExecKey/principal | Exact cancel/status authority; no authority by PID alone |
| `SubmissionLease` | Workspace-local permit, CapturedView, protected fund, sealed results/install/failure state | Only one retained submission per W; same-selector local completion |
| `ChannelLease` | exact peer/endpoint/daemon instance, channel epoch, next request ID, Noise state, windows | Exclusive operation; synchronized return or exact channel retirement |
| `OperationTicket` | owned input/output contracts, byte credits, finite deadline or Exec lifetime, result identity | InputDemand/result/terminal continuations; no borrowed Output stored |
| `ExecHandle` | validated command capability, owned result ticket and cancellation lease | `wait`, `cancel`; blocking `exec` wraps this primitive |

Registry lookup selects a charged BTreeMap by exact WorkspaceKey: O(log V)
lookup/insertion/removal under short registry ownership, with no scan of finished
entries. This is an explicit deterministic lookup bound rather than literal
worst-case O(1) registry growth. Command capabilities/PIDs use direct keyed live
indices; stable generational slots and a reusable free list avoid appending one
slot per lifetime command. Standard hash indices have expected O(1) lookup,
with their real capacity/rehash costs admitted and not claimed worst-case O(1).
Never retain tombstones/timer entries or a used-command set for all earlier Execs.
Resize from actual live admitted capacity; unknown/retained owners still count.

`max_workspaces_per_sandbox` is positive, immutable for a sandbox and proposed
default 2 per #219. Reserve before exposing ID/incarnation. Attaching, attached,
mounted, closing and failed/unknown-retained entries all consume the slot. G1/G2
is one entry; completed sequential creations consume no lifetime count. SDK
bindings may describe unresolved requests but cannot create a second count.
Release the slot only after exact mount/Workspace/backing owners are absent.

Suggested method boundaries:

```text
registry.reserve_open(key, creation_policy, owner_bytes) -> OpeningLease
registry.resolve(peer, key, rights) -> WorkspaceLease
exec.admit(WorkspaceLease, ExecSpec, ExecResources) -> ReservedExec
exec.spawn(ReservedExec, ProcessDomainPrepared) -> RunningExec
exec.cancel(peer, CommandCapability, reason) -> CancellationObservation
entry.begin_close(mode, finite_cleanup_deadline) -> ClosingLease | Busy
channels.lease(target, operation_class, bytes, finite_admission_deadline)
         -> ExclusiveChannel
channel.begin(OwnedOperation, input_contract, output_contract) -> OperationTicket
```

This packet owns daemon command/session admission and the I/O engine. Workspace
owns `OperationBudgetLease`, `MutationTicket`, `CapturedView`, `SubmissionLease`,
result/parent/exception ledgers and conflict-lease/current-publisher semantics.
Server owns Store permit
classes, serial-range authority and canonical construction. Use those interfaces
once; do not implement three competing byte budgets.

Server's selected new FileSetConstruct v1 results are a typed data sink. Every
ordinary Workspace non-directory construction, one or many files, uses the same
FileSetSource/assembler. Its natural unit has at most512 members and a4MiB
coalescing target; a larger single member still uses the same grammar under the
supported8GiB body envelope. Standalone SaveFile v2 stays at its independent API
compatibility boundary, never a file-count/size/error fallback for Workspace.

Result rows are96 bytes (token8, serial8, content32, metadata32, length8,
checked flags8), so a maximum unit has49,152 logical result bytes across
ResultData frames. This is not a Success-metadata payload or resident result Vec.
The Workspace writes provisional rows directly to its charged paged result table;
one UnitCompletion seal promotes the unit to KnownSaved after exact selection,
schema/count/result-byte total/digest and finished Save outcome. It does not scan
and flip all rows. Lost unit/full-set completion retains exact selection, received
prefix and unfinished Save ownership; no automatic file/whole-set resend. These
are new proposed format bounds, not current production constants.

## 4. Lock order and generation composition

Long operations do not hold registry, lifecycle, channel-pool or global allocator
locks. Acquire resource/channel leases before any Workspace publication lock.
Short shared allocation/accounting locks remain legitimate where they protect
global physical identities; logical isolation is not removal of shared integrity.

Order: validated registry entry lease → tagged byte/quota/source admission →
namespace-ordering lease if required → affected inode leases in stable serial
order → fixed current-catalog publisher → brief selected State/root install.
Acquire required remote/channel capacity before entering the publisher; never
hold State/catalog/root or registry locks while waiting for delivery. Bounded
remote preparation uses the admitted operation's continuation and callback
deadline. Avoid a callback that recursively requests the channel uploading it.

The Workspace packet selects a per-inode `MutationTicket`: pin the exact immutable
InodeVersion V/source owner, prepare a bounded per-file candidate off State/root
locks under only that inode's conflict lease, then enter a short publisher against
the CURRENT catalog. Namespace changes use their separate namespace-ordering
lease plus affected inode leases. A wide edit holds its inode while unrelated
same-W file writers, readers, capture and known-result install can progress. No
whole-Workspace mutation sequencer spans affected-range work.

At publication, validate incarnation/lifecycle and the same selected inode
version V, then apply a statically bounded number of inode/dirty/location key
changes to the current catalog and actual current dirty generation. The exclusive
inode lease excludes a conflicting inode change; a mismatch is coherence failure,
not a compare-and-retry or detached whole-root install. Path-copy publisher work
is O(H) page I/O for a fixed number of keys, not zero-time or a constant wall.
Only conflicting inode/namespace writers wait through the long candidate work,
within their syscall deadlines. Capture/install contend on the short publisher,
not on the candidate's inode lease.

A candidate that started before capture can finish afterward. G1 freezes the
last published V and excludes the candidate; its later publication belongs to G2.
Identify inherited shared subtrees against frozen V by exact selected root/range
identity without enumerating descendants. Preserve explicit terminal/orphan
origins for spans not selected from V. Known-result install changes immutable
parent/result binding while retaining the CURRENT G2 catalog; it does not cancel
the candidate, mutate V's bytes or discard its source pin. These cross-boundary
candidate/provenance cases require exact tests before the concurrency claim.

Apply WORKSPACE.md's first-touch rule: a new generation starts a changed file
from one opaque exact frozen/known parent whole-file span. An in-flight candidate
published after capture describes at most its retained parent boundary spans
plus current replacement/gap effects against V; it does not expand every old
private interval. Ordinary lowering stops at ParentSpan, so repeated one-new-edit
Commits cannot accumulate ancestral descriptors or recursive resolver chains.
Current-generation SourceCoverage and zero ordinary inherited-span expansion are
explicit count-proof obligations, distinct from correct bytes.

```text
Exec A1: syscall x published to G1 | syscall y published to G2 ---->
Exec A2: syscall z published to G1 | syscall w published to G2 ---->
                                 ^
                              capture
Commit:                     immutable G1 -> streams -> known R1 -> install
Live:                       separate G2 delta remains intact ------------>
```

The shell is not a transaction. A completed Exec may have writes in both heads;
exit 7, cancellation or disconnect retains every accepted private mutation and
does not undo another command. Concurrent rename/write conflicts follow exact
syscall publication order and POSIX errors, not whole-command serialization.

Local parent provenance and canonical construction policy remain explicit. The
selected concurrent end-state is **namespace profile v2 plus file-edit-policy v2:
immediate-selected-parent**, with FileState grammar v2 and a32-byte edit-policy
digest. G2 inherits the exact frozen G1 file's result coordinates; after known
completion its ordinary canonical construction Base is that predecessor's R1.
Captured dirty orphan/exception versions used by later ranges receive known
canonical roots through file-set construction and paged custody. No target live
recipe retains R0 solely to reproduce the old root, and no whole-file copy removes
lineage. Keep replacement-only frozen CDC and unchanged R1 subtree reuse.

This is a selected explicit identity contract, not an unresolved choice between
v1 equivalence and v2. V2 promises semantic bytes and its own independently
specified roots. Its changed partition/chunk/FileState/filesystem/Commit IDs may
differ from v1; do not claim cross-profile ID equality. Freeze precise layouts,
profile/policy digests and an independent v2 expected-root ledger before enabling
the profile. Legacy v1 callers retain their own exact grammar/roots at an explicit
compatibility surface, but that surface is not the concurrent target, cumulative
replay fallback or a way to satisfy #248/#249 by moving the old recipe to disk.

Namespace v2 binds an authenticated parent-tree root and the policy digest inside
the filesystem root. A parsed/hash-authenticated parent root alone is not proof
of forward/reverse semantic correspondence. The entry/attachment and construction
admission require the Server's opaque `VerifiedNamespace` authority for that exact
root/profile/scope, obtained through full initial/import certification or a checked
successor. Keep its byte-admitted validity lease alive for the operation; bounded
cache eviction cannot silently turn an unverified root into a valid Base. The
precise provenance/certification lease authority remains a design review gate in
SERVER.md, with no resident registry of every historical root.
The checked-successor verified owner is fixed authority, not a recursive parent-
proof Arc/DAG retained across every Commit. Its construction parent lease may
release after successor certification; actual old pins retain their own roots
and independent live leases. Restart/stale-epoch rejection remains before effects.

Admit v2 through explicit new-Stack creation/import: select a v1 snapshot,
allocate a new v2 Stack/scope and paged serial remap, certify complete namespace
correspondence, then publish only through the stated initialization outcome.
Original v1 Stack/history remains intact. Workspace registry bindings carry exact
Stack, namespace/policy digests and validity origin; opening a concurrent v2
Workspace on an old v1 Stack cannot silently upgrade it or assume current create/
attach already supports this capability. New creation/import failures retain their
known/unknown owners. These are prospective API/profile changes, not code executed.

READY and install fields are shared with the other packets:

```text
CapturedToken: incarnation, generation, revision, selected index/parent resolver,
               scope/root serial, exact selected-version/orphan facts
SubmissionInputSeal: captured identity, expected Branch head/base, profile,
                     exact namespace/file totals/digests and FileSetCompletion
SavedFileRow: serial + selected version + content/metadata roots
              + canonical construction base / grammar profile
InstallCapsule: exact submission selector + SubmissionInputSeal + fixed outcome slot
                + prebuilt parent/result/exception roots + baseline epoch
                + installed revision + separate cleanup disposition
```

Local READY precedes the one composite HistoryCommit request. Its final
filesystem root is an unfilled fixed outcome slot, not a presumed prepublished
candidate. Server constructs/validates the filesystem, finishes C2, internally
stages and checks Branch CAS inside that same operation. No added public Stage
RPC/Stage+CommitStaged sequence is introduced. Known file-set unit saves, complete
file-set completion, canonical filesystem root and final Branch outcome remain
distinct observations; none is inferred from an earlier partial result.

One Branch command per submission attempt; unknown delivery retains it without
replay. Known-result same-selector completion changes local state only. One
Workspace's retained submission cannot consume another entry's logical slot.
Shared Store/allocator quarantine remains a real broader refusal where integrity
is unknown. Two Workspaces using one Branch still conflict through expected-head
CAS; no automatic merge, rebase or retry.

## 5. Exclusive channel pool and owned native I/O

Choose exclusive channels before wire multiplexing. Each operation has its own
leased channel and request/response state; progress/cancel traffic uses separately
admitted control capacity. Pool capacity is derived from actual byte/FD admission,
not a new numeric Exec cap. Idle channels remain charged and can be retired by
their recorded idle policy. A failed operation retires only its exact channel;
it cannot discard another healthy entry/channel by a global `discard()`.

Allocate native request ID after channel lease acquisition. It must be greater
than that channel's previous ID. A host-global telemetry/correlation ID is a
separate field. Reconnecting establishes a fresh authenticated channel epoch;
it does not resend BEGIN or convert an Unknown mutation into a fresh attempt.
Hello/instance/peer validation remains mandatory before dependent operations.

```text
Channel state
  Admitted -> Connecting -> Authenticating -> IdleSynchronized
  -> BeginSealed -> Input/ResponseActive -> TerminalChecked -> IdleSynchronized
                                      \-> Broken/Unknown -> Retired or Quarantined

Owned receive state
  fixed record prefix -> checked ciphertext length -> partial ciphertext
  -> authenticated decrypt -> checked frame/type/id -> bounded dispatch

Owned send state
  byte credit -> encode once -> seal once / consume nonce once
  -> partial send cursor -> record complete -> release window
```

The reactor retains at most admitted frame/record windows per channel, including
the current 256 KiB coalescing window if preserved. Return a channel only after
complete input/terminal synchronization. Partial writes resume at their offset
without encrypting twice or reusing a nonce. Read lengths are checked before
allocation. Checked nonce increment remains mandatory; no wrapping/reset under
the same Noise key. Retire idle channels before ID/key exhaustion. Active
exhaustion is an explicit transport-capacity/disconnect failure with owned cleanup,
not an elapsed Exec timer or automatic reconnect/replay. Continuous rekey support,
if selected, needs authenticated epoch/ack ordering; do not assume Snow's rekey
method alone defines a protocol.

Keep one shared owned I/O reactor per owner assembly (or a declared fixed reactor
set); Linux uses readiness events. The command supervisor does not call blocking
construction, mount teardown or arbitrary Source callbacks. Those operations run
on their existing admitted operation producer and exchange byte-bounded owned
tickets. No extra construction fanout and no per-Exec helper thread/stack.

For legacy finite synchronous native calls, a caller-owned Source/output sink can
remain a wrapper: the caller/operation producer answers bounded `InputDemand`
and consumes bounded result events while the shared I/O engine owns frames.
It must not create another upload worker. Zero-input Exec has no producer pump.
Daemon dispatch returns an owned operation/reply ticket rather than retaining
borrowed `Output<'_>` or a stack `TimingScope` for an indefinite command. Telemetry
identity and admitted report state need owned bounded lifetimes; no growing
per-command timeline. Service producer threads/Store permits are composed in the
Server packet; this change is not a replacement for their resource envelope.

Use an explicit result contract per operation: FileReadData, SelectedViewData,
FileSetResultRows or ExecControlEvents. Data byte/count/sequence limits and
terminal predicates differ. The receiver cannot interpret arbitrary ResultData
as either a heartbeat or trusted saved rows. File-set rows retain the request,
unit and record sequence, match immutable submitted serial/token order, and feed
the paged sink directly. Only UntilExit Exec removes a finite lifetime
heartbeat-frame allowance; finite Save/file-set/read transfers retain exact declared
data/frame bounds and their operation deadline. No partial data prefix implies
known canonical publication.

Every turn has a maximum I/O/drain/crypto work quantum. Maintain only one pending
heartbeat per channel; partial terminal/output frames are bounded. Events waiting
for a byte credit have charged queue entries; reject admission or wait within
the declared finite admission bound. Never turn channel saturation into an
unbounded queue or a mutation retry.

## 6. Typed lifetime, protocol and SDK surface

#249's owner ruling removes the 30-second whole-shell deadline. Select:

```text
OperationLifetime = Finite(total_ms) | UntilOwnedExit
ExecSpec = exact WorkspaceKey + command bytes + caller command nonce
           + UntilOwnedExit + declared execution-domain profile
Liveness = finite handshake / silence / stalled-frame / terminal-delivery bounds
```

The new profile must cover SDK control_call, Request validation, native client/
server, daemon dispatch and execution supervision together. Do not encode infinity
as u32::MAX, a large Instant, or deadline_ms=0 accepted globally. Finite operations
retain finite validated durations. Begin/Exec admission has a finite budget;
UntilOwnedExit starts only after admitted execution ownership. Negotiation must
be read-only and succeed before spawn or dependent mutation.

Reserve fresh operation tags after an explicit opcode/response registry audit;
this packet does not assign guessed numeric opcodes. Keep existing v1 operations
and SaveFile/prepared grammar intact. New authenticated capability bits cover
UntilExit Exec, exact Exec control and chunked selected-view results. A peer
without a required capability returns Unsupported before BEGIN/spawn; it must
not silently run the legacy 30-second Exec or try another backend on failure.

Exec event grammar, specific to the new negotiated operation:

| Event | Bounded fields | Semantics |
| --- | --- | --- |
| Admitted | command capability, exact Workspace/daemon identity | Exact command lease exists; absence/loss is not permission to resend |
| Alive | event tag + checked sequence / execution phase | Liveness, not evidence of mutation progress |
| Terminal | exact command identity, exit/signal/cancel disposition, bounded stdout/stderr + truncation flags, cleanup disposition | Valid only after required owned-scope and pipe cleanup, or explicit retained cleanup failure |
| CancelResult | exact capability + accepted/already-terminating/already-terminal observation | Cancellation never starts another command or rolls back files |

Keep 8 KiB captured stdout and 8 KiB stderr unless a separately selected contract
changes them. Result metadata is checked against its actual bounded encoded
size; captures do not use an arbitrary file-sized spool. Draining extra output
sets truncation while continuing finite byte quanta. These caps do not bound
child heap, mmap, files, kernel pipe capacity or application threads.

A proposed heartbeat cadence is 2 seconds, inside the current 5-second silence
bound, with at most one unsent Alive event. It is a protocol design value to
freeze/verify, not measured performance. Heartbeats do not consume the old fixed
`frame_budget(0)` lifetime allowance. Validate rate and frame/byte sizes, keep
checked counters/nonces, and close a stalled/broken channel under finite liveness.
Silence caused by reactor starvation is still a failure to diagnose, not a reason
to enlarge a timer. Wall-clock passage alone does not kill a healthy command.

Public SDK default stays `WorkspaceApi::exec(id, command)` with generic command
bytes and blocking result. Implement it over a cancellable owned primitive:
`start_exec(id, command) -> ExecHandle`, `wait(handle)`, `cancel(handle)`. Whether
these additional public methods retain these exact names is an API review item;
the underlying exact handle/cancellation ownership is required. A handle dropped
before terminal completion is caller cancellation; enqueue its pre-admitted
control action, or close its owned operation channel to trigger scoped disconnect
cleanup if control delivery is broken. No detached run is implied by Drop.

Control cancellation uses a separately admitted channel/capacity class, binding
principal + daemon instance + Workspace ID/incarnation + command capability. A
PID/PGID, newest command or another channel's request ID is not authority. Repeated
explicit cancellation observes the existing state; this is idempotent control,
not automatic replay of Exec. A lost terminal reply can leave the caller's result
unknown while the daemon already knows resources are reaped; do not retain a
complete historical command record indefinitely for a new query guarantee.

## 7. Exec state machine and descendant custody

```text
Requested -> ResourcesReserved -> DomainPrepared -> SpawnStarted
    |                                  |                |
    +-> definite refusal               +-> exact abort   +-> Bootstrap / Running

Running -> leader exited ------+                        |
        -> stdout/stderr EOF --+--> domain no-live ----> Reaping/Draining
        -> explicit cancel/disconnect/close/shutdown --> Terminating
                                                      |
                           finite cleanup boundary ---+
                                                      v
                                 TerminalKnown -> Delivered/Disconnected -> Retired
                                 CleanupRetained -> charged exact owner / closing entry
```

The predicates leader-exited, execution-domain-empty, required descendants-reaped,
pipe-EOF/closed, output-finalized and terminal-delivery are separate. Only their
required conjunction releases the command/entry/process resources. Unsolicited
background descendants belong to the command domain; UntilOwnedExit waits for
them even if the shell leader returns zero. Persistent services should remain
foreground or retain an explicit handle; no daemonization-specific route is added.

Cancellation/connection failure marks the existing lease terminating, applies the
selected domain kill operation, continues reaping/draining, then reports exact
exit/cancel/cleanup facts. A finite cleanup failure retains its exact owner and
does not free a reused PID or silently drop a mount. Linux uninterruptible I/O can
delay actual death; SIGKILL acknowledgement alone is not completed cleanup.
Actual interrupted writes preserve their accepted Workspace publications.

Process-group-only ownership cannot prove this for arbitrary commands that call
setsid, fork twice or close inherited pipes. No command-text parser may reject or
special-case those programs to hide the limitation. Choose a real execution-domain
profile or report Unsupported when the requested guarantee cannot be provided.

## 8. Supported adapter research and runtime profile

Pinned dependencies are nix=0.31.3 and snow=0.9.6. Read-only inspection of their
published local sources confirms safe Epoll/SignalFd/TimerFd wrappers,
`set_child_subreaper`, `set_pdeathsig`, no-new-privileges support and stateless
record/rekey methods. Current daemon nix features include process/signal/fs/poll;
epoll/eventfd and timerfd require selecting existing event/time features later.
No Tokio/Mio/new package is needed. Source availability is not runtime capability
proof. [nix feature/re-export documentation](https://docs.rs/nix/0.31.3/nix/),
[SignalFd](https://docs.rs/nix/0.31.3/nix/sys/signalfd/struct.SignalFd.html).

Daemon and Bridge forbid unsafe_code. Do not weaken that guard or put allocating
Rust setup in an unsafe pre_exec hook. Rust documents that hook's post-fork
async-signal constraints. [Rust CommandExt safety](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html).

Choose a trusted same-binary bootstrap process for Linux domain setup, then safe
`CommandExt::exec` into the exact `/bin/sh -c` request. It is the already-admitted
child PID, not a persistent helper thread per Exec. Its internal mode is selected
before telemetry/reactor startup, validates a bounded parent-issued setup record,
enters the exact precreated domain, configures supported credential/environment/
namespace policy, then executes. It never interprets the user command to choose
an algorithm. A child bootstrap cannot manufacture another Workspace/command
capability. Its failure phases remain explicit; after spawn begins, an uncertain
start is not retried.

Selected Linux contained-execution profile requires a delegated cgroup v2
subtree owned by the trusted runtime. Descendants inherit membership; events
expose live-population changes; cgroup.kill covers the domain. Delegation and
controller availability are mandatory, and child credentials must not permit
escape/migration. memory.max is not an exact instantaneous RSS equality and can
temporarily overrun; PID accounting includes tasks. These qualifications follow
the kernel interface. [Kernel cgroup v2 authority](https://docs.kernel.org/admin-guide/cgroup-v2.html).

```text
delegated sandbox domain (existing aggregate CPU/memory/PID policy)
  +-- daemon/observer runtime leaf (protected progress resources)
  +-- Workspace A resource parent
  |      +-- Exec A1 domain: admitted child + descendants
  |      +-- Exec A2 domain: admitted child + descendants
  +-- Workspace B resource parent
         +-- Exec B1 domain
```

Set up controller topology before accepting Exec. Respect the no-internal-process
constraint: runtime processes live in a leaf, not a controller-distributing
parent. Protect domain-control paths from children. The bootstrap joins before
executing user code, avoiding the spawn-user-code-then-migrate race. If this
cannot be done with the selected safe runtime capabilities, fail the contained
profile; do not substitute raw clone3, unsafe hooks or a process-group fallback.

The shared supervisor blocks/owns SIGCHLD setup before other threads start,
uses signalfd notifications and nonblocking waitpid; a daemon subreaper adopts
otherwise orphaned descendants. Preserve known command/PID mappings and domain
identity when reaping. The domain live-empty event alone excludes zombies and
does not establish successful reaping. A global subreaper is not per-Workspace
filesystem confinement. Orphan attribution/notification races, bootstrap failure,
and adopted descendants require the prospective proof below.

For adopted children absent from the live PID index, the safe pinned nix waitid
API supports WNOWAIT. Observe a waitable child without reaping/PID reuse, resolve
its exact owned cgroup identity through the selected runtime port, then reap and
account it to that command. Keep the domain authority until this attribution and
required waitable-event drain finish; do not remove a live-empty cgroup before
reaping because zombies are absent from cgroup.procs. Missing/ambiguous attribution
is retained cleanup failure, not latest-by-PGID guessing. Process exit processing
is event-driven and proportional to actual exits, with admitted event quanta;
it does not periodically scan every past Exec. A production proof must cover the
empty-event/reparent/wait notification ordering before this is called exact.

Current Docker launch supplies 2 CPUs, 512 MiB memory/swap, 64 PIDs, read-only
root, /dev/fuse, SYS_ADMIN and no-new-privileges. It does not configure delegated
per-Exec cgroups, mount/user confinement or child credential separation. It also
overrides library Workspace defaults with 16 MiB accounted memory and 1 GiB
private disk. Preserve those declared deployment values in their evidence;
library default 8 MiB is a different selection. No runtime probe was run here.

| Guarantee/profile | Selected implementation requirement | Current verdict |
| --- | --- | --- |
| Logical Workspace/API isolation | Exact authenticated selectors, entry/root/output ownership | Architecture target; singleton currently blocks concurrent delivery |
| Exact arbitrary descendant cleanup | Delegated owned domains, pre-exec membership, non-escapable control, reaping/draining | Required contained-execution profile; runtime support NOT_PROBED |
| Aggregate child resource policy | Existing sandbox ceiling plus supported nested accounting/limits | Container aggregate exists; per-W/Exec composition unqualified |
| Hard sibling filesystem/security isolation | Supported child mount/credential/FD/environment/capability confinement | Separate strict profile; current cwd does not provide it |
| Crash durability / resurrection after daemon death | New durable recovery contract | Outside this proposal; no fsync/WAL added |

A trusted shared sandbox may declare weaker filesystem isolation while using
contained process domains. Same-UID commands can otherwise name sibling mounts,
signal siblings or read reachable control/private paths. A strict profile must
hide unrelated mount/backing/control trees, sanitize inherited FDs/environment,
drop effective privileges/capabilities and enforce inaccessible domain controls;
do not pass daemon service/control private-key environment to a confined shell.
No current deployment is relabeled strict. Other platforms implement the same
ProcessDomain/Readiness/Confinement ports or reject required capabilities; canonical
algorithms and ID/Commit semantics remain platform-independent.

### 8.1 Selected RuntimeActor and mount-access contract

Choose a root serving daemon and a distinct nonzero RuntimeActor UID/GID per live
Workspace in the concurrent contained profile. All that W's admitted commands
use its actor; W-A and W-B have different actors. Allocate/release this identity
with the exact Workspace incarnation and retained count slot, never reuse it
while mounts, command domains, kernel handles or failed owners remain. Actor
allocation is selected resource policy, not another low hard-coded W limit.

Separate three identities in the representation:

```text
CanonicalPortableFacts: serial/kind/mode/mtime/content; actor identity not encoded
                       FS/FileState identities follow selected v2 profile/policy
RuntimeActor: W key + nonzero host UID/GID + actor epoch + admitted domain policy
BackingOwner: privileged daemon UID/GID + exact physical incarnation/identities
```

Reported POSIX uid/gid and owner permission checks use RuntimeActor for every
selected live/canonical inode. Portable mode/mtime remain exact. Private backing,
domain-control, mount-session FD and service/control credentials remain owned by
BackingOwner. Today's owner_uid field cannot keep serving both identities;
split that authority rather than chowning control paths to the arbitrary shell.

Baseline mount is not compatible with this design unchanged: capability checking
requires daemon euid0 **and** Workspace root.uid0 **and** SYS_ADMIN; Config uses
SessionACL::Owner; Adapter::guard compares req.uid to root.uid. Selected target
introduces typed `MountAccess::ExactActor(RuntimeActor)` and passes it to mount/
adapter assembly. The mount capability checks privileged serving identity and
requested actor/profile separately. It does not require logical inode uid0.

Select kernel allow_other ingress with DefaultPermissions, NoSuid and NoDev in
the ExactActor mount contract. Read-only pinned fuser0.18.0 source calls this
SessionACL::All and maps it to allow_other, while Owner filters to the mounting
uid; that is semantic evidence, not the target session implementation. Ingress
is **not** authorization of every UID. Every user-origin operation
requires the mount's exact RuntimeActor uid; presented inode uid/gid match that
actor so DefaultPermissions preserves portable modes. Kernel-owned cleanup such
as Forget/Release must use a typed issued-handle/mount owner context when request
credentials are unavailable; it cannot free another actor's handle or treat
arbitrary uid0 requests as authority. Direct-I/O/non-writeback paths remain;
supported cached reads need their own credential/handle proof. No current mount
flag or root guard is patched by this document.

The end-state projection is root's [owned cancellable kernel-session contract](ACCEPTANCE.md),
not a fuser fallback: a first-party Linux UAPI adapter owns byte-admitted
RequestLease identity `(kernel unique, W, mount epoch, actor)` and prioritizes
INTERRUPT through an independent session reader. Cooperative cancellation checks
and the publisher race forbid publication after EINTR was answered before
publication; accepted publication retains its actual result/custody. Keep
max_background1, the128KiB envelope and exact actor/allow_other semantics.
Fuser All's mapping above is source/legacy evidence for those mount semantics,
not proof of the target interruption capability. This adapter adds no construction
worker and involves no third-party patch. Its implementation LOC remains outside
this packet's estimate, in root's FUSE scope.

The trusted bootstrap joins the selected domain before executing user code,
clears inherited service/control-key environment and unneeded descriptors,
disables keepcaps, selects no_new_privs, and executes with the actor UID/GID and
explicit supplementary-group policy. Verify effective/permitted/ambient
capabilities are empty. Domain controls and backing paths are privileged/private
and inaccessible through permissions/selected namespace policy; child actors
cannot move tasks out, join another command domain or regain root. Nonzero UID
without those control-path/capability checks is insufficient. This uses safe
existing nix/std setup in the fresh bootstrap, not unsafe pre_exec work.

The contained profile promises exact command-domain custody and actor admission,
not complete metadata confidentiality from every cached kernel path. A strict
filesystem profile additionally hides sibling mounts/control/backing via the
supported mount/confinement port and proves no inherited FD/proc escape. Keep the
weaker trusted-shared versus strict profile distinction explicit.

Required runtime proof before concurrent enablement: two nonzero actor UIDs on
two mounts; ordinary create/read/write/chmod/rename/readdir under each exact
portable mode; denied sibling-UID access to both mounts and domain/backing/control
paths; no command capability/sys_admin inheritance; SessionACL::All actually
produces the expected kernel mount options under the locked fuser/provider;
kernel handle cleanup works without granting another actor access; independent
retained/close paths preserve actor identities. Credential separation must not
silently make generic tools fail on their own mount. Unsupported delegation,
actor/access/confinement capability refuses before spawn/visibility; no root
same-UID, Owner-ACL or process-group fallback is accepted in this target.

## 9. Hierarchical admission, progress and fairness

Admit actual live LayerFS bytes, channels/FDs, process-domain state and requested
child resource envelopes. Exec count is a consequence of capacity, not a new
constant. Child heap can grow after spawn; the supported runtime policy handles
that growth and reports resource termination/refusal. An 8 KiB output limit or a
LayerFS accounting charge cannot bound arbitrary application memory.

```text
Global envelope
  fixed reactor/runtime + protected control reserve
  + sum W(idle owners + pins/cleanup debt + protected completion)
  + sum admitted Exec(lease + bounded capture + domain/FDs + child envelope)
  + sum admitted operations(stream windows + channel crypto/coalescing)
  + admitted Server/Store caches/codecs/waves/connections
```

Tagged per-Workspace usage composes into the global byte/disk ceiling. A
WorkspaceStatus host-shared accounted_bytes cannot be summed over Workspaces.
Per-W protected completion resources cannot be borrowed by another W; idle W
does not reserve full Commit scratch. Kernel/file cache, child RSS/mmap, mount
threads and supervisor stacks are separate physical-domain terms. Protected
progress memory must remain reachable under child pressure in the selected
runtime; an admission formula is not proof of OOM isolation.

Use fair rotation of ready Workspace operation queues with byte-work quanta;
control/cancel/liveness and terminal cleanup have their protected class. Queue
nodes are charged, linked directly by live operation and removed on completion;
no history scan. Large waiting operations retain only admitted continuation state
and release expendable windows. A request holding a submission/failure fund may
not block control admission. No throughput guarantee independent of CPU/device
contention or absolute starvation bound follows before scheduling proof.

Select distinct **C2SavePermit** and **C5CatalogPermit** in the new explicit
admission profile. C2Save keeps the existing two construction/Save slots and
encoder/working arena leases. C5Catalog covers only short history/allocator
transactions, with protected catalog engine/control bytes, FD/channel capacity
and the catalog's short transaction gate. It allocates no C2 Save arena and adds
no construction producer, helper thread or third Save slot. This structurally
removes the baseline use of a whole-Save permit for history-only ReserveInodes.
One producer per admitted construction unit remains; no global long Commit mutex.

Pre-admit a live catalog/control channel or the selected provider's proven
socket/FD acquisition capacity and its bounded native windows. A logical FD
counter by itself does not guarantee an OS FD under pressure. Bulk channels,
codec arenas and child requests cannot consume this protected class. Server's
global SQLite heap guard composes all Store/scratch/catalog connections; actual
short-transaction serialization follows each database's scope. Catalog admission
is not unlimited SQL memory or a bypass of physical transaction integrity.

SERVER.md selects one active transaction per separate C5 catalog,64KiB protected
pending-control ring,1MiB first-party catalog work/terminal capacity, a protected
channel/FD owner and4MiB engine headroom inside the one32MiB global SQLite guard.
Its default engine model is26MiB C2/read/scratch plus4MiB catalog, leaving2MiB
margin; this is a proposed envelope requiring qualification, not reserved SQLite
heap created by a Rust counter. Count the Server-side ring/work/channel windows
once within its176MiB first-party pool, not again in this packet's control sum
or each72MiB Save lease. Daemon/client endpoint buffers are distinct physical
allocations charged to their own assembly. A transferred ticket moves the same
charge; it does not duplicate first-party capacity. Per-connection proof/row/cache
shapes must establish the engine protection before this strict profile enables.

Composite operations use phase-scoped leases: construct/finish C2 and seal exact
known/unknown output custody, release its C2SavePermit, then acquire C5Catalog for
internal stage/Branch publication. Never hold a C5 transaction while waiting for
C2 work, retain a C2 unit permit while requesting final C5 publication, or acquire
channels while State/current-catalog publisher locks are held. Ready short
catalog work must be scheduled by the existing owned executor between bounded
construction/I/O quanta; protected sockets alone do not prove progress if a
synchronous whole-file producer monopolizes every execution context. No new
helper worker or automatic SQLITE_BUSY retry is the remedy.

Finite allocator ranges remain amortization: initial4,096 serials and low-water
trigger1,024 are proposed Server policy. Consume credits in the ordered namespace
transaction/current-catalog publisher for G2 creates. Refill uses C5Catalog,
outside Workspace state/publication locks, and must not wait for a complete4GiB
C2 Save merely because both Save permits are busy. It may still meet real short
catalog/SQL transaction contention, resource/identifier exhaustion or its callback
deadline; report that precise scope. Ranges are scope/incarnation-bound and
possibly exposed serials burn once. Unknown allocation retains exact custody,
without a guessed replacement request. Range size is neither a file-count cap nor
an unlimited-progress guarantee. The named catalog profile above is shared with
SERVER.md and must be qualified before implementation admission; this packet
does not add another heap guard, Save slot or catalog budget copy.

## 10. Close, shutdown and known/unknown isolation

Entry lifecycle:

```text
Opening -> Attached -> Mounted <-> Unmounted
    \-> FailedRetained             |
                            ClosingRequested
                              |  refuse new Exec/capture
                              |  consult command/FUSE/submission/pin owners
                              +-> exact resources absent -> Closed -> slot released
                              +-> finite cleanup failure -> ClosingRetained
```

Unmount detaches a mount; it is not dirty discard. Clean close remains an explicit
operation that refuses dirty/submission/handle/pin owners unless the selected
supported recovery/teardown operation first resolves them. Default unmount/close
during active Exec may return Busy; an explicit cancel-and-close mode cancels the
exact W command leases and performs bounded cleanup. Do not turn ordinary close
into an implicit Commit or canonical replay. The API needs distinct refused,
closing, installed-with-cleanup-error and retained observations.

Daemon orderly shutdown marks all entries closing under short registry state,
stops new admission, then signals/drains owners via the shared supervisor and
bounded resource continuations. A finite failure reports exact unresolved domain,
mount and submission custody; it does not claim clean shutdown. Abrupt daemon
death/forced container deletion is a separate profile/failure case. A parent-death
signal covers only its selected process relationship, not every descendant or
Workspace durability. Do not claim restoration of Unknown results after a crash.

Per-entry failure containment has limits: corruption of shared allocator/Store
accounting may correctly quarantine a broader scope. Document that scope/reason;
do not continue using uncertain credits or call every shared stop a bug. Known
canonical results survive local cleanup failure without rollback fiction.

## 11. Targeted correctness packets

### C-READ: pinned selected-view response framing

Confirmed source mismatch at baseline: Workspace view read admits 128 KiB;
put_read encodes byte length as u16 and take_read decodes u16; native Success
metadata is capped at 32,768 bytes including identity/flags. A full 32 KiB payload
already exceeds that frame, and 64/128 KiB lengths truncate. Smaller retained
successful reads cannot prove the advertised full request. Read queries and
view-release uncertainty are different custody events.

Chosen fix design: negotiate chunked selected-view result capability. Keep the
public bounded read size; emit <=16 KiB authenticated ResultData payload frames,
then a bounded Success trailer with exact Workspace/incarnation/view/serial,
returned u32 count, EOF and file size. Match every frame to the active request,
cap total data at the requested <=128 KiB and validate trailer completeness.
SDK admits its bounded result allocation and returns bytes only after terminal
validation. On a partial/broken read, retire the channel; preserve the view lease
and reject partial data as a successful read. No guessed ReleaseView occurs.
Do not merely raise Success metadata/cipher limits or lower the advertised size.

Prospective reproduction: one pinned file spanning 128 KiB with deterministic
bytes; separate exact 16 KiB, 31 KiB, 32 KiB, 65,535, 65,536 and 128 KiB requests,
EOF-adjacent range and malformed count/ID/trailer; compare complete requested
bytes and original view identity while later writes change the live file. Retain
old-source failures; final source proof is separate. No test ran for this study.

Adjacent inferred shape: VIEW_LIST_RESULT_BYTES is 65,536 while Success metadata
is 32,768. A 128-entry page with long allowed component names can also exceed
the frame despite valid entry count. A shared negotiated bounded-data result
encoder can cover this grammar; first freeze its exact maximal-page oracle.

### C-LIFE: cancellation, output and descendant lifetime

Root cause: direct-child status + pipe EOF + process_group/killpg does not prove
domain completion, and active_operations is not a shell lease. The current
elapsed deadline hides some indefinite-owner shapes by killing after 30 seconds.
Opening control concurrency without explicit owners exposes close/use-after-
teardown and cross-request output risks.

Prospective separate cases: silent >30-second command; explicit cancel; disconnect
while stdout is blocked; leader exit with background child closing both pipes;
setsid/double-fork descendant; descendant holding a pipe without file writes;
output beyond each 8 KiB cap; child resource refusal; close of W-A while W-B
continues; wrong incarnation/capability; known and Unknown Commit in another W.
Use deterministic barriers and sanctioned runtime fault routes, not injected
production test hooks. A kill request or observed direct-child reap is not the
all-descendant cleanup verdict. Preserve accepted partial Workspace bytes.

### C-CHANNEL: identity, synchronization and liveness

Root cause: host-global request IDs can reach an exclusive reused channel out of
order; active heartbeats count against finite lifetime frames; partial encrypted
I/O and global session discard require exact ownership.

Prospective cases: two calls schedule in reverse channel-acquisition order;
malformed old/cross-channel result ID; auth/instance change before BEGIN; broken
channel after BEGIN; partial ciphertext/prefix/terminal; long silent Exec with
bounded unsent heartbeat; synthetic nonce/ID boundary through external protocol
proof without changing production arithmetic. Record no mutation replay and no
healthy sibling-channel closure. Key exhaustion remains explicit capacity.

## 12. SRP module plan and dependency direction

The [consolidated target layout](LAYOUT.md) groups the tentative flat daemon
names below into workspaces/, commands/, control/ and transport/ folders. Use
that canonical path map for implementation; responsibility and interfaces stay
the same. This tree records the original per-responsibility size budgets.

Keep existing packages; add focused production modules, not a new framework.
Suggested responsibility ceilings below are physical-line planning limits,
not production LOC estimates or permission to fill files to 999.

```text
layerfs-daemon/src/
  lib.rs, main.rs                  declarations/delegation only, <=200
  run.rs                          assembly/capability setup, <=300
  workspace_registry.rs           keyed count/entry leases, <=350
  workspace_lifecycle.rs           mount/close transitions, <=350
  control_auth.rs                 exact selector/rights, <=250
  control_dispatch.rs             owned operation routing, <=300
  command_types.rs                Exec keys/lease/terminal states, <=300
  command_admission.rs            tagged resources, <=250
  command_supervisor.rs           ready events/state transitions, <=500
  command_output.rs               bounded capture/drain, <=200
  command_cancel.rs               exact cancellation/cleanup, <=250
  process_domain.rs               narrow ProcessDomain port, <=200
  platform/linux/readiness.rs     nix readiness adapters, <=300
  platform/linux/domain.rs        checked cgroup authority, <=450
  platform/linux/bootstrap.rs     fresh process setup then exec, <=400
  transport_pool.rs               exclusive service leases, <=300

layerfs-bridge/src/
  contract/exec_lifetime.rs        UntilExit/control/result grammar, <=300
  contract/view_stream.rs          negotiated data/trailer bounds, <=250
  adapters/native/io_state.rs      owned prefix/record cursors, <=350
  adapters/native/crypto_record.rs nonce/seal/auth, <=300
  adapters/native/operation.rs     channel request transitions, <=400
  adapters/native/reactor.rs       readiness/byte-credit dispatch, <=450
  adapters/native/liveness.rs      cadence/stall state, <=200
  adapters/native/result_stream.rs typed bounded results, <=300
  adapters/native/client.rs        finite caller-owned wrapper, <=300
  adapters/native/server.rs        owned request/reply wrapper, <=250

layerfs-sandbox/src/
  channel_pool.rs                  instance-bound SDK session leases, <=350
  control_operation.rs             owned call/wait/cancel, <=300
  execution_profile.rs            runtime capability/config port, <=300
  docker.rs                       selected delegation/count plumbing, <=500
  owner.rs                        registry/API coordination only, <=500

layerfs-api/{core,sdk}/src/
  exec_handle.rs, workspace_exec.rs exact public handle/default wrapper, <=300 each
```

Thin mod/lib files declare/reexport/delegate only. Split any approaching 999-line
file by responsibility. Protocol parsing does not spawn children; supervisor
does not allocate C1/C2 objects; registry does not perform wait/reap; domain
adapter does not interpret shell commands; transport pool does not select a
canonical builder. Resource interfaces are shared with the other packets rather
than duplicated. Real ports are Readiness, ProcessDomain, ExecutionProfile and
operation delivery; concrete state machines stay ordinary structs/functions.

Dependency direction: SDK → Sandbox owner → Bridge contract/native I/O;
Daemon assembly → Bridge + Workspace + FUSE + Linux adapters. Workspace does not
depend on Docker/daemon process policy; canonical C1/C2 do not depend on child
execution or namespace setup. Native I/O protocol contract does not import Store
internals. First-party platform adapters remain product source with external
tests; no third-party forks, patches, registry edits or vendored implementation.

## 13. Honest cost model and LOC budget

Let V be admitted live/retained Workspaces, Q active commands, C leased/idle
channels, Z completed-but-undelivered bounded results, X event notifications,
B_pipe all produced stdout/stderr bytes and W_io admitted window bytes.

| Term | Architecture target | Limits/remaining cost |
| --- | --- | --- |
| Registry/command owners | O(V+Q+Z) charged live records; W registry O(log V), command/PID keyed expected O(1) | Allocation/rehash overlap; no historical Exec population |
| Captured command output | <=Q*16 KiB plus bounded terminal/result state | All B_pipe bytes still drain; truncation is not CPU-free |
| Native I/O | O(C*W_io) + fixed reactors + charged events | Handshake/crypto/FD/coalescing costs; too many channels consume real memory |
| Event work | O(X + actual I/O bytes), plus bounded timer-index operations | Readiness is not zero-cost; no per-command 5 ms polling |
| Heartbeats | O(Q/runtime cadence) control traffic | Quiet commands still consume actual crypto/nonces and scheduling |
| Child resources | actual application/descendant envelope | Not bounded by LayerFS windows/output; kernel/runtime capability required |
| Mutation/Commit | Companion packet's bounded memory, real changed/proof work | Conflicting inode/namespace waits, O(H) publisher I/O and shared CPU/device/Store contention remain |
| Temporary disk | Actual Workspace/replay/pin/cleanup debt | Concurrency multiplies simultaneous owners; process supervision adds bounded config/metadata, not an output spool |

### Lifetime quadratic exclusion and latest #276 scope

The retained [latest #276 comment5905916098](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5905916098)
and its preceding deep270/custody comments were read from the root-owned
ISSUE276-LATEST-COMMENTS.json. They identify whole-frontier memory, repeated graph
work and distinct custody/framing/measurement limits. Their historical count
improvement and source formulas do not establish latency or a current merged
candidate. This section adds the command-owner counterpart without promoting
#174 residual descriptions or #235's historical telemetry loss into new product
failures.

For N sequential Execs with bounded active Q, admission and terminal retirement
must depend on current owners, not N. Exclude `retain`/search over an append-only
command vector, a set of every spent token, and close loops visiting all lifetime
requests. Stable generational slots reuse freed positions. A command capability
is checked against its exact live lease/epoch/principal; old capabilities cannot
authorize a new slot after reuse. Terminal history is not an implicit query API.

Cancellation performs exact capability lookup, changes only that command's state,
and signals its owned domain. Reaping uses keyed live PID/domain attribution and
removes each completed child directly; retain only required counters/leader
status, not all earlier descendant exit records. Work is proportional to actual
exit events and bytes drained. A command that forks many short-lived children
still pays those real events but does not retain their lifetime population.

Use an indexed timer heap (or equivalent removable bounded timer index) with one
current timer record per admitted lease/channel and O(log(Q+C+V)) insert/update/
remove. Do not use accumulating lazy-deleted timers whose far-future entries
survive every completed command. Ready queues have direct removable links and a
coalesced-ready bit; repeated notifications cannot append unbounded duplicates.
Fairness visits ready owners within a fixed work quantum, not every idle/past
command on each heartbeat or I/O notification. PID/capability and timer index
capacity/resize overlaps remain admitted; no worst-case constant hash promise.

Closing one W first changes its entry state through O(log V) registry lookup,
then visits only that W's Q_W live/retained command owners plus actual FUSE/pin/
submission/cleanup owners. O(Q_W+actual descendant exits/required cleanup) teardown
is real necessary work; it does not scan another W or all historical commands.
Orderly whole-daemon shutdown can legitimately visit O(V+Q+actual owners) once.
Finite cleanup failure retains exact owners and no later pass restarts a complete
finished-history scan. The benchmark cost must include the registered cleanup;
moving it to a backlog does not erase it.

Prospective count-driven proof varies N at fixed active Q and varies Q separately:
record keyed lookups, live slot/index capacity, timer/ready records, exact cancel
targets, owner visits per W close and actual ready/exit events. Require no
1+2+...+N lookup/cleanup pattern and no retained N-sized index. Use source-bound
counter diagnostics/independent process ownership, not repeated speed samples.
Telemetry/result completeness must be checked by operation identities and exact
terminal/cleanup counts; a missing #235-style record is unavailable evidence,
never filled from a partial log or a lifetime aggregate. None of these proofs ran.

Removing one 2 MiB reserved thread stack per operation removes a proportional
virtual reservation mechanism, not a measured 2 MiB RSS improvement per command.
Current daemon/native wrappers allocate those helpers; the new engine must
replace both. Kernel socket/pipe buffers and application threads remain counted.
No throughput, wall-time, constant-total-RSS or arbitrary-scale PASS is claimed.

Root independently reproduced baseline reference65,417/Core70,279/combined135,696.
Scoped Core baseline: daemon2,151; Bridge6,834; API796; Sandbox1,106. These package
totals include more than this packet's changed modules and are not deletion
budgets. The same `tools/production_loc.py` classifier/version counts nonblank,
non-comment implementation and runtime SQL, excluding tests/examples/tools/docs
and inline tests. Counter Git blob: `c7dd2b9c6aa9db63393a4ff3ebca9327529d146e`.

A read-only per-file count in the unchanged product checkout reproduced:

| Selected baseline file | Production LOC |
| --- | ---: |
| daemon control.rs / lifecycle.rs / execution.rs | 491 / 297 / 162 |
| Bridge native client.rs / server.rs / connection.rs / payload.rs | 650 / 102 / 449 / 154 |
| Sandbox session.rs / owner.rs | 122 / 588 |
| SDK workspace.rs | 175 |
| Selected total, not the whole affected scope | 3,190 |

Reproduction: load `tools/production_loc.py` with Python `-B` and call
`counted_lines(Path(file))` on those exact paths; whole-tree/package confirmation
uses `python3 -B tools/production_loc.py --root . --json`. No production file was
edited for this count. Exact staged/parent archives must replace these design
estimates before any future commit.

**ESTIMATED future production changes, not measured or staged:**

| Exclusive packet scope | Additions | Deletions/replacements | Net range |
| --- | ---: | ---: | ---: |
| Daemon registry/supervision/domain/control | 2,100–3,400 | 850–1,100 | +1,000–2,550 |
| Bridge owned I/O/lifetime/view-result grammar | 1,300–2,200 | 700–1,000 | +300–1,500 |
| Sandbox/SDK channels/handles/runtime profile and explicit v2 new-Stack/import routing | 650–1,250 | 150–250 | +400–1,100 |
| Packet combined | 4,050–6,850 | 1,700–2,350 | +1,700–5,150 |

Minimum/maximum arithmetic is independent-bound estimation, not a promise that
all extremes occur together. If this were the only production change, Core would
be71,979–75,429 and combined137,396–140,846. The updated Sandbox/SDK range includes
ESTIMATED150–350 lines for the explicit v2 creation/import/attachment routing,
excluded from Server's import/remap/certification estimate. Workspace/Server/
shared-resource and FUSE changes are excluded to avoid double-counting their
packets. Strict sibling
filesystem/user-namespace confinement is excluded; a separate reviewed adapter
could add ESTIMATED900–1,800 lines, with no claim that existing runtime privileges
make it available. Relocation into modules does not reduce production LOC.

## 14. Ordered implementation and prospective proof

No commands in this section are being run. External public-API/protocol fixtures
must cover real product behavior without inline tests, fake counters or alternate
test routes. Freeze source/profile/identities before proof; reuse unaffected
earlier family evidence, especially Family 2, and retain all failures.

1. Freeze joint interfaces and capabilities: registry entry lease, per-W submission,
   protected completion, exact Exec domain/lifetime, owned operation tickets and
   channel IDs, selected namespace/file-policy v2 layouts/digests/independent roots
   and new-Stack import/validity authority. Resolve required runtime capability
   selection before enablement; retain v1 expected roots only for legacy scope.
2. Compose #248 bounded generation/stream/canonical custody first. Native all-phase
   writer proof remains narrower than independent SDK control overlap.
3. Replace native borrowed/helper-thread I/O behind finite-operation wrappers;
   preserve exact input/result grammar and failure identities. Qualify partial
   I/O/nonce/ID/byte-credit state before using it for indefinite Exec.
4. Add negotiated UntilExit, selected-view chunk results and exact cancellation.
   Keep default ordinary SDK exec generic; no v1 fallback after capability refusal.
5. Add exact registry/count/profile assembly and event process-domain supervision.
   Requested count3 must not hit old root/arena/session magic caps; failure/closing
   entries retain count until absence is known.
6. Post-#248 only, prove same-W two-Exec overlap, more resource-admitted commands,
   unrelated W progress, W-A/W-B Commit overlap under unchanged Store slots,
   same-W second Commit refusal and explicit shared-Branch conflict. Include
   accepted syscalls on each side of capture and exact old/new/pinned oracles.
7. Prove quiet lifetime >30 seconds plus separate cancel/disconnect/domain/pipe/
   close/shutdown cases, exact output and no cumulative owner scan/stack growth.
   Root's registered benchmark wall gates are separate; a >30-second lifetime
   correctness case is not a performance selection with an enlarged timeout.
8. Prove hierarchical child/runtime resource and filesystem profiles actually
   advertised. Keep physical phase memory/cache qualification separate (#283);
   a source-based bounded state machine or successful cgroup write is not that
   measurement. Unsupported required capability remains an explicit limit.

## 15. Open decisions and stop conditions

Before product implementation, settle exact public handle names/cancel-result
grammar, allocated capability/opcode values, runtime cgroup delegation/bootstrap
authority, adopted-descendant attribution and terminal delivery/disconnect policy.
Confinement capabilities must state whether they cover logical selection, process
custody, child resource limits or strict sibling filesystem security.

Stop a proposed implementation path that requires third-party changes, unsafe
guard removal, a command-specific route, a hidden fixed Exec cap, a global long
Commit lock, per-Exec helper stacks, automatic replay of begun operations, guessed
refund, unproved canonical Base swap, relaxed quota/worker/deadline or an added
durability service. Report the concrete blocker and retain its source/evidence.
The protocol/runtime design is reviewable here; current product support remains
the baseline's finite singleton scope until the separate implementation/proofs.

## Source references

Baseline-relative implementation links:

- [daemon control](../../../../crates/layerfs-daemon/src/control.rs),
  [lifecycle](../../../../crates/layerfs-daemon/src/lifecycle.rs),
  [execution](../../../../crates/layerfs-daemon/src/execution.rs),
  [transport](../../../../crates/layerfs-daemon/src/transport.rs).
- [Workspace host](../../../../crates/layerfs-workspace/src/runtime/host.rs),
  [operation gates](../../../../crates/layerfs-workspace/src/runtime/state.rs),
  [capture/frozen permit](../../../../crates/layerfs-workspace/src/overlay/snapshot.rs),
  [completion](../../../../crates/layerfs-workspace/src/commit/completion.rs).
- [native client](../../../../crates/layerfs-bridge/src/adapters/native/client.rs),
  [server](../../../../crates/layerfs-bridge/src/adapters/native/server.rs),
  [connection](../../../../crates/layerfs-bridge/src/adapters/native/connection.rs),
  [view codec](../../../../crates/layerfs-bridge/src/adapters/native/protocol/workspace_view.rs),
  [frame limits](../../../../crates/layerfs-bridge/src/adapters/native/protocol/frame.rs),
  [view contract](../../../../crates/layerfs-bridge/src/contract/workspace_view.rs).
- [SDK Workspace](../../../../crates/layerfs-api/sdk/src/workspace.rs),
  [Sandbox session](../../../../crates/layerfs-sandbox/src/session.rs),
  [owner](../../../../crates/layerfs-sandbox/src/owner.rs),
  [Docker profile](../../../../crates/layerfs-sandbox/src/docker.rs).
