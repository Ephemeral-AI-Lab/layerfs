# Service, bridge, daemon and hosted telemetry

> **Status:** Current general guide.

This describes the issue #192 implementation candidate for v0.1.7; it is not a
released contract or performance qualification.

Source basis: optimization work after documentation checkpoint
`0819f3f39833d477d9ed6d878a50691c3c046a83`, preserving the selected C1 production
checkpoint and dependency lock. The
[C2 schema-7 checkpoint](proposal/service-daemon-transport/implementation/evidence/optimization-c2-checkpoint-20260920.json)
binds the tested storage prerequisites; source inventories in each new command
receipt bind the subsequent service integration. The original implementation seal
and image do not identify this revision. The
[optimization decisions](proposal/service-daemon-transport/implementation/09-optimization-decisions.md)
and [acceptance record](proposal/service-daemon-transport/implementation/07-acceptance.md)
state exact selections and incomplete proofs. These changes do not rewrite Stage 6
results or qualify #193 performance work.

The optimization revision uses ordinary `TcpListener` and one
`TcpStream::connect_timeout` attempt, with TCP_NODELAY and explicit blocking mode
on accepted sockets. Socket option sizes are not admission criteria. The failed
preconnect experiment and original observations remain archived in the handoff.
The [C2 checkpoint](proposal/service-daemon-transport/implementation/evidence/optimization-c2-checkpoint-20260920.json)
qualifies the storage prerequisite on macOS; the wider native/Docker profile still
requires its own current-source evidence.

Pair 1 readable prerequisite, 2026-09-21: the additions below are based on
`0749180db34d1cdc57f905806a17e3f3f48ec2bc`. They add complete logical read
attributes, export the existing cooperative input contract without native
dependencies, and propagate the caller's deadline through connection setup.
They do not establish mounted verification or measured performance.

The R1-C Status extension is based on the R1 commit
`598d8405f1168f5f8409807c4f3b6e95d1a110c9`. Its shared bridge/service changes
define one authenticated daemon-targeted local observation. They do not add
network attach, edit, mount management or Commit controls.

The R2 portable metadata extension is based on
`4d6f5cd0fa4d21afe51fb1dda01db0d4a0095c88`, including the attribute hierarchy
prerequisite `d3d767393`. It adds one typed C1/C2 metadata save, with no Workspace
mutation, automatic inode attachment or Branch publication.

## Boundaries and public calls

`layerfs-bridge::contract` owns the closed content/history operation union and typed results.
Building bridge without its default `native` feature gives the portable contract
without Snow/nix or socket code. The native adapter authenticates and delivers
frames; it has no C1/C2, SQL or service dependency. `Client::call` consumes a
cooperative deadline-aware `Source` and bounded output. The real stdin source uses
poll and the same frame/input-state codec as the network endpoint. `Source` is
exported by the portable contract; `adapters::native::client::Source` remains a
compatible reexport. `Client::call` and `call_until` accept a dynamic Source,
so an embedding can bind logical delivery without importing socket ownership.
One scoped
upload thread permits concurrent early-response handling. It closes the upload
half on malformed input, allowing an authoritative service failure to arrive.
Only a returned terminal success validates provisional read bytes.

`layerfs-service::Service::handle` is the same direct and remote entry point. It
records around authorization/admission, validates the request, binds authenticated
public-key possession to configured Store/op grants, and then acquires one permit
without waiting: a logical mutation takes one writer permit of the Store it
targets, whose budget is that Store's persisted `max_concurrent_writes`, and a
read-only operation takes one of the process's `MAX_READ_OPERATIONS` read permits
and no writer permit at all. One operation takes exactly one permit, however many
saves or statements it performs. Native configuration selects one Store (logical ID 1);
the service facade admits at most four explicitly configured Store mappings.
No request carries a native Store path or independent construction capacities.
`VerifiedPeer` belongs to the portable bridge contract and only trusted native
entry code constructs it. Direct callers use `VerifiedPeer::from_private` using the same authorized
private key; a caller-chosen numeric principal is insufficient.

A local `StoreProvider`, C1 call and `SaveHandoff` implement each operation. Complete
construction uses exact-length streaming; edits acquire at most 8 MiB of separate
replacement parts before the save, preserving current-result coordinates. Known
failures retain the available typed cause and checked cleanup disposition. Unknown
C2 outcomes are not aborted or replayed on a guess. A successful file or filesystem
root is returned only after validated input finality and successful C2 `finish`.

Prepared updates verify the original scope/root serial, existing identities and
retained references. They send final bindings only for changed names. Directory
content cannot be swapped through an inode value; it uses directory changes.
`FilesystemSaved` is a separate result from file `Saved`, with no fictitious file
length on a tree result. File saves followed by attachment remain separate saves;
no atomic composite, history publication or crash-durability promise exists.

Inspect supports File, combined Stat (identity, roots, count, mode and full mtime),
List with an explicit C1 continuation name, and Readlink. Paths use C1's canonical
relative UTF-8 grammar; the empty path is root. `/f` is invalid; `f` is valid.
Provider failures already collapsed by `StoreProvider` remain Provider failures;
no error-string parsing reconstructs unavailable native categories.

`Inspect::Attributes` supplies the complete Stat fields plus `size: u64` in one
logical response. The service obtains a regular file's exact length from C1's
FileView, a symlink's length from its checked stored target, and explicitly
projects directory size as zero. It uses the existing Inspect grant/read permit
and no input body. Subtag 4 and result tag 9 append to the existing wire unions;
older tags retain their exact encoding. The bridge validates supported kinds,
positive representable serials, namespace reference counts, mode masks,
nanoseconds and kind-specific size bounds. Native request matching additionally
checks that root attributes have directory kind/reference count zero and
descendants have positive references. Roots remain opaque 32-byte identities;
the service's C1 reader authenticates their bytes and logical role. Signed mtime
seconds remain exact, including values before the Unix epoch.

List still returns names/serials rather than complete child attributes. A consumer
requiring per-entry kinds issues bounded Attributes calls for its page under one
remaining callback deadline. This establishes no batching or request-count gain.

`WorkspaceStatus` is a separate daemon-control operation: profile 3, opcode 8,
positive request ID, zero Store/generation/result-body fields and at most 5,000 ms.
It supplies a 1–63-byte managed ID and nonzero 32-byte producer incarnation. Its
metadata is at most 124 bytes and it accepts no input or ResultData body. The
existing content/history service explicitly refuses it before Store lookup or
read/write admission; opcode 8 has no Store permission bit, even in an all-bits
grant. The daemon independently authorizes the authenticated peer and expiry for
the exact Workspace/incarnation. Service grants never confer that authority.

Response tag 10 encodes the echoed ID/incarnation, three state flags and five
u64 observations: active operations, nodes, handles, cookies and aggregate
`consumer_accounted_bytes`. Its maximum is 139 bytes including the tag. Reserved
flag bits and inconsistent closed-state counts are rejected. Native matching
requires the exact request ID/incarnation binding and no output body. This is
current local state, never an earlier edit/Commit receipt or recovery protocol.
The existing non-history three-byte failure representation applies; transport
loss is Io with no unknown-mutation claim. Existing framing, authentication,
server input completion and failure/session-closure algorithms are unchanged.

`UpdatePortableMetadata` uses content profile 1/opcode 9 and explicit Store grant
bit `0x80`. Legacy grants 31/127 do not confer it; opcode 8 remains reserved to
daemon control with no Store grant. The request is exactly 76 bytes, including
the common envelope and existing attribute-tree base root, inode kind, mode,
signed mtime seconds and nanoseconds. Input and ResultData budgets are zero.
Mode/kind/nanosecond checks reuse the complete-attributes validator's portable
rules: regular `0777`, directory `01777`, symlink exactly `0777`, and nanoseconds
less than one billion. The kind is the caller's typed construction context; an
attribute root does not by itself prove membership in a particular inode.

The existing mutation owner validates empty input, takes one writer permit and
opens one C2 save. `operation/metadata.rs` reads the existing typed fields through
C1, applies the two sorted portable patches and preserves every generic value
root through the existing streaming patch builder. No full attribute map, object
RPC, new storage implementation or arbitrary generic mutation operation is added.
The grouped read and patch cursor enforce the corrected hierarchy described in
[12](12-attributes.md). Metadata-only attachment later retains the original
content root and uses the separate prepared filesystem operation.

An operation-local provider/consumer delegation checks the absolute deadline
before and after each bounded read wave/object handoff. C1 has no deadline error,
so a typed local expiration flag distinguishes this cause without parsing text.
An in-progress storage call is not preempted. The existing save owner gives a
retained C2 failure precedence, aborts definite pre-finish failure once, and
checks the deadline before finish. A successfully acknowledged finish remains
success even if time expires immediately afterward; lost mutation delivery is
unknown and never automatically replayed.

Result tag 11 is exactly 98 bytes: base root, kind, mode, signed seconds,
nanoseconds, saved metadata root, inserted and reused counts. Native matching
requires every echoed input field and zero ResultData. This result means one
metadata tree has been saved, not that an inode, filesystem, stage, Commit or
Branch was updated. A no-op preserves the metadata root and still reports the
actual save result. The operation inherits current Store/C1 limits and introduces
no larger prepared-update or EditFile input envelope.

## Native protocol and ownership

The carrier is `Noise_KK_25519_ChaChaPoly_BLAKE2s` over ordered TCP, provided by
published Snow 0.9.6. Both public keys are pinned in operator configuration. The
four-byte principal selector is only a key lookup hint and is bound into the Noise
prologue. Every operation is authorized again, including credential expiry. Key
revocation changes configuration and restarts the process. No replay/resume exists.

Each encrypted record has a checked big-endian u32 ciphertext length. Nonces are
strictly increasing per direction and never supplied by a peer. The decrypted
frame is exactly `LFB1 | kind:u8 | flags:u8=0 | reserved:u16=0 | id:u64 | len:u32 |
bytes`, all multibyte numbers big endian. HELLO=1, BEGIN=2, BODY=3, END_INPUT=4,
RESULT_DATA=5, SUCCESS=6 and FAILURE=7. END_INPUT carries the exact total as u64.
Body/data frames contain 1..=16384 bytes; metadata is <=32768 bytes. No empty-body
frame, in-band cancellation, per-chunk ACK or object RPC is accepted. Local stdin
uses the same plaintext frame schema; credentials never come from those frames.

The transport admits `session_capacity(budget) = budget + MAX_READ_OPERATIONS`
persistent/handshaking/closing sessions, plus one synchronous accept/refusal
socket slot. At the default budget of two that is four sessions and **five
application connection resources in total**, as before; a raised writer budget
raises the session space with it, so the transport is never a lower ceiling than
the configured budget. The acceptor's extra slot is counted, not hidden behind
the session limit. At most that many 2 MiB service thread stacks exist, and at
most `budget` admitted handlers can construct, each with one producer. Client upload uses one explicit 2 MiB stack. Closing owners retain their
slots until their threads finish or the executable exits. A session retains a
socket shutdown clone with its worker. Shutdown stops admission, shuts down all
live sockets, and collects completed workers for at most two seconds. If core work
is still running, explicit process exit preserves an unresolved outcome; detached
workers are not labelled cleanup. Kernel socket state, backlog, stack mappings
and RSS are separate observed domains.

The [resource profile](proposal/service-daemon-transport/implementation/10-resource-profile.md)
records the aggregate byte/count ownership vector. At the default budget it
admits four session owners and two complete operation owners, preserves 16 KiB
body and 32 KiB metadata limits, and selects a 4 GiB construct/read-range limit.
Those counts are the default-budget instance of the configured one; see the
[concurrency controls](../../../docs/roadmap/0.1/0.1.7/concurrency-controls.md). Edit replay remains 8 MiB.
Frame work is bounded by `ceil(declared_bytes / 1024) + 257`, including boundary
slack and a terminal frame. File size grows total work and persisted bytes, not a
whole-file transport buffer.

Requests select an overall budget up to 600 seconds and I/O also observes a
five-second no-progress limit; handshake/idle is five seconds. Each process shares
one connection activity timestamp between upload and response consumption, so
productive upload traffic keeps its concurrent response wait alive. A blocked
socket read may continue waiting when the other direction made progress; this
does not reconnect, resend frames or replay a mutation. Complete silence remains
bounded by five seconds, and the overall deadline never extends.
Each process shares
one absolute operation deadline through its layers. Only a remaining duration
crosses the network, never an Instant. Synchronous
core calls cannot universally be preempted mid-hash or mid-commit. A late known
C2 success is not reclassified as an abort; a lost response remains unknown.
The daemon observes stdin/stdout deadlines with poll and closes unsynchronized
sessions rather than draining attacker-declared input. Native macOS accepted
sockets are explicitly switched to blocking mode because the nonblocking listener
otherwise passes O_NONBLOCK to accepted sockets.

`connect_until` shares a caller-local deadline across one TCP connect attempt,
authentication and Client HELLO; each connection phase still has at most five
seconds. The existing `connect` retains its separate five-second connect and
authentication/HELLO caps. A consumer using `connect_until` also passes the same
deadline to `Client::call_until`, so setup cannot grant a fresh operation budget.
Connection errors retain their existing typed mapping; socket timeouts can still
be reported as Io. Every failed Client operation closes that session, including
confirmed logical absence; consumers must not reuse a failed client or replay
the failed operation implicitly.

## Single-crate telemetry

Portable `Timing`, `RecordingLimits`, `OperationRecorder`, `Observation` and
`Window` remain std-only and forbid unsafe code. Timing's existing Result/panic,
child and attachment behavior remains. Owned labels are normalized to a new
bounded allocation on every public/import path, including short strings with
excess capacity. Clipping cannot be cleared with `with_incomplete(false)`.
`TimingNode::retained_bytes` reports Vec/String capacities; the conservative node
charge covers arena/tree conversion overlap. Caller-owned imported/cloned reports
remain caller-owned until transferred; arbitrary external clones are not a global
process-memory guarantee.

Native runtime reserves eight 256-node recordings under a 4 MiB pool; report owners
hold reservations until drop. Encoding preflights a valid whole or clipped timer
projection and accounts simultaneous prefix/suffix/timing/output buffers. It does
not truncate JSON. An independently recorded product-success bit survives timing
deselection. Off performs no telemetry allocation/probe/worker/file/export after
normal application configuration is supplied, verified by an external allocator.

One explicit monitor uses safe macOS nix `getrusage` for CPU and libproc 0.14.11
for RSS/start identity, or bounded Linux procfs plus safe nix sysconf. POSIX CPU
microseconds are checked and converted to nanoseconds; RSS is bytes. Raw macOS
libproc CPU fields are Mach ticks and are not used as nanoseconds. Reports include
the collection source and measured local probe duration. The 100 ms
selection keeps 600 recent slots and 32 fixed summaries. Live summaries survive
raw-ring wrap, use generation-checked nonblocking access, and release atomically
on return/error/unwind. Window opening/closing and observation times are local to
one monitor; reports include run, configured host/container namespace, PID and
native incarnation. CPU deltas need actual covered samples and are shared process
windows, never exclusive operation cost. RSS maxima are sampled maxima. Missing
capabilities or insufficient samples are unavailable. Cgroup fields are not yet
collected; process RSS must not be relabelled as cgroup/anonymous/file/socket memory.

`OutputConfig` bounds encoded bytes, queue count/capacity, aggregate rate/burst and
segments. `Output` handles share one owner whose final destruction stops its
writer exactly once, including concurrent final-clone drops; the writer retains
queue state but never its owner. Submissions recheck closure under the queue lock.
Current writes retain their queue charge. `Collector` has <=16 producer
slots; queued/in-flight records retain closing producer registrations. Fixed loss
counters saturate and expose overflow. Actual acceptance coordinators separately
bound their Python pipe decoders and retained diagnostic bytes.

Ordinary native startup/configuration/error diagnostics use one best-effort write
of at most 512 bytes and leave their dedicated stderr descriptor nonblocking.
Both CLIs return explicit exit codes so Rust's default `Result` termination cannot
block error cleanup behind an unread diagnostic pipe. Stderr must remain separate
from protocol stdout. Regular-file OS calls are not claimed preemptible.

Forward is structured `LFT1 ` JSON lines on stderr, independent of framed stdout.
The Docker host coordinator must attach product stdout and diagnostics through
separate Engine connections: the ordinary Docker CLI attachment multiplexes them
and can block stdout when its stderr consumer stalls. The external acceptance
coordinator uses stdout/stdin-only `docker run` plus a bounded stderr-only Engine
attachment. It discards excess diagnostics and never writes a retry spool.
Resource records include `selected` bits (CPU=1, RSS=2), so deliberately disabled
fields differ from unavailable selected fields. All process samples name their
local clock/source; run summaries carry namespace and counter-overflow status.
Local/both use an exclusively locked, marked operational namespace; permissions
are 0700 for new directories and 0600 for files. Fixed segment names, retained
length validation and profile markers allow bounded restart. Active segments count
against capacity. Expiry is checked at least once per second while the writer runs;
local wall-clock reversal delays age expiry. Unknown directories, links, conflicting
writers and changed retention profiles fail closed. Only owned segment paths can
be retired; benchmark receipts beside the namespace are untouched. There are no
sync calls or durable-output claims. Local filesystem syscalls are not preemptible;
shutdown attempts a bounded drain, then releases rather than indefinitely joining
a stuck writer. Expected telemetry loss cannot replace a known product result.

Native environment configuration: service uses `LAYERFS_LISTEN`, `LAYERFS_STORE`,
`LAYERFS_PRIVATE_KEY`, and `LAYERFS_PEERS` (`selector,public-key,expiry,op-mask` entries
separated by semicolons). Daemon uses `LAYERFS_ENDPOINT`, `LAYERFS_SELECTOR`,
`LAYERFS_PRIVATE_KEY` and `LAYERFS_SERVER_KEY`. Keys are exactly 32 bytes represented
by 64 hex digits and are never printed. Telemetry uses `LAYERFS_TELEMETRY` (off,
forward, local, both); enabled collection requires `LAYERFS_RUN_ID` and
`LAYERFS_NAMESPACE`. Local/both also use `LAYERFS_TELEMETRY_DIRECTORY`. Numeric
runtime/monitor/output configuration is exposed through the public Rust structs;
these executables select the documented initial profile. Invalid optional native
setup emits a separate bounded initialization diagnostic and remains disabled.

## Portability and remaining qualification

Service keeps native Store paths, `rusqlite::Error`, connection/session lifetimes,
SQLite transaction/ownership, codecs, packs and mutable caches local. A managed
provider would replace C2's connection and grouped read/write execution and qualify
lifetime/transaction/resource semantics. It would preserve logical requests,
canonical identities, input finality and outcome distinctions. No provider registry,
cloud adapter, filesystem mount, Workspace, history or allocator is implemented.
API compatibility, canonical compatibility and schema-7 persisted compatibility
are separate checks. Independent algorithm substitution remains #172.

The acceptance record must distinguish deterministic tests, real macOS/Linux
processes, OS faults, optional local/both output and unrun platform/resource cases.
Functional evidence does not qualify overhead, cold storage, bandwidth or #193.

Native early-refusal teardown correction, 2026-09-21, based on
`4d6f5cd0fa4d21afe51fb1dda01db0d4a0095c88`: after sending a decoded request's
handler failure, the server half-closes output and discards remaining input
through the existing bounded adapter before closing both directions. The
original length/frame/progress/absolute deadline remains in force. This avoids
resetting a terminal frame while upload bytes are unread. The handler is not
reentered and the original failure is preserved; malformed BEGIN metadata keeps
its immediate best-effort refusal. See the [observed failure and correction](proposal/fuse-workspace-snapshot-overlay/12-early-refusal.md).

The R3a Workspace input primitive, based on `d555c8bef`, is described in
[the owned-payload implementation](proposal/fuse-workspace-snapshot-overlay/14-owned-payload.md).
It uses the existing portable Source contract and introduces no service operation
or daemon input control. Read-only daemon startup keeps disk backing disabled;
its shutdown and failed-control cleanup now pass the existing absolute deadline
through `close_clean_until`. FUSE preserves typed local backing error causes in
its errno mapping, without adding a writable callback.
