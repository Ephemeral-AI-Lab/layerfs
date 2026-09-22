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
canonical identities, input finality and outcome distinctions. No managed-provider registry or cloud adapter is implemented. Pair 1 now has
separate Workspace/FUSE owners and the existing C5 history owner; their operation
and verification scopes are recorded in the linked round records. API and
canonical compatibility remain separate from persisted-schema compatibility.
The schema-7 checkpoint above retains its historical scope; the integrated tree
uses the [configured-concurrency/schema-8 contract](../../../docs/roadmap/0.1/0.1.7/concurrency-controls.md). Independent algorithm substitution remains #172.

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

R3b, implemented from `4629b8d62de1e0df8a7bd9808b59a86d7c6669f3`, adds one
explicit unmounted `LocalEdit` capability to Branch-backed Workspaces. The
[local range-edit record](proposal/fuse-workspace-snapshot-overlay/15-local-range-edit.md)
describes the maintained private page index, ownership ledger, generation
completion reserve and exact public operation. It retains the same logical
delivery boundary and makes no service mutation during a local edit. Daemon
read-only launch selects `ReadOnly` explicitly; the existing authenticated Status
wire remains its readable scope. LocalEdit projection reservation is refused
until the writable kernel coherence binding exists.


The next Pair 1 operation, implemented from
`788a63950500e6ba79c6a55dc07f7b84ca0fde89`, is
[Workspace Stage](proposal/fuse-workspace-snapshot-overlay/16-stage-capture.md).
It captures one immutable local generation with a live successor, streams only
its changed file spans, saves changed portable metadata and invokes the existing
StageChanges once. There is no added service opcode or service/storage/history
implementation dependency in Workspace. Disk completion associations use the
private index; the ordinary candidate reserves 137 pages and each dirty generation
reserves 208 pages for completion. The latter accounts for interleaved live slot
allocation and is an explicit disk-admission increase, with unchanged RAM/window/
FD limits. The real native save and failure proofs, narrower scope and pending
CommitStaged/reconciliation are recorded in 16. FUSE's error conversion accepts
the new Stage failure as EIO; no writable callback or daemon edit control is added.


The next public-operation round, based on
`0b2c729bdb3f026f12beb667ccdc15c52853280f`, adds
[Workspace CommitStaged and reconciliation](proposal/fuse-workspace-snapshot-overlay/17-commit-staged.md)
through the existing C5 command. It reserves 64 completion-fund pages, 26 metadata
slot credits and ledger replacement capacity before remote publication. A bounded
streaming builder compacts only the current D1 frontier and reuses piece trees;
the exact captured-version/result association governs rebasing. Local state owns
a changing canonical base/Branch context and baseline epoch, allowing lazy
canonical cache refresh while reads retain selected roots. A validated own result
survives later local failure; unknown results remain unknown. Exact staged replies,
local pre-admission, failure retention, repeated commits and current-source
regressions are recorded in 17. FUSE maps Commit errors to EIO; no new wire opcode,
service algorithm, writable callback or daemon Commit control is added here.


From parent `6702e31e629ada5e78981b6854e36721e619e65b`,
[ordinary Workspace Commit](proposal/fuse-workspace-snapshot-overlay/18-composite-commit.md)
shares Stage's file/metadata preparation and CommitStaged's known-result
reconciliation, using one existing HistoryCommand::Commit after preparation.
Clean Commit pre-reserves an empty capture descriptor and 208-page escrow and
submits empty PreparedChanges for actual C5 UpToDate. Pre-admission holds the
remote permit only through the first logical request. Composite outcome lacks a
token, so local Commit report/selector associations are optional and native stage
observations remain separate. The existing shared operation, wire profile and
one-construction-worker default are unchanged. Actual native scope and remaining
mounted/control/failure-disposition dependencies are recorded in 18.


From parent `573b4bbd35bdcd5d8fe55c8fc107f8c31cd791c5`,
[Workspace resize](proposal/fuse-workspace-snapshot-overlay/19-resize-zero-ranges.md)
adds set_len for existing cached regular inodes through the same mutation and
Commit owners. The64-byte private piece value gains strict tag2 Zero with no
payload/custody/offset; reads and Source synthesize only bounded requested spans.
Zero bytes count toward the existing8 MiB replacement envelope. In-memory Piece
is48 bytes; the conservative596,448-byte transient accounting remains below the
existing640 KiB reservation, with unchanged windows, FD and disk reserves.
No writable mount, shared service operation or reference change is introduced;
actual native results, retained failures and next dependencies are recorded in19.


From parent `a5bdc9f1e7e0fa4815ae356a7c5d783315fac649`,
[portable Workspace open](proposal/fuse-workspace-snapshot-overlay/20-portable-open.md)
adds FileAccess/FileOpenOptions and open_file, preserving legacy read-only open.
A charged pending handle pins its inode before truncate preparation; shared
mutation publication installs inode state and READY together. All consumers
reject pending IDs, and WriteOnly handles reject read even at EOF. The128-slot
24-byte table remains3,072 bytes, with56 bytes of pending control charged per
truncating open. No write API, FUSE option, daemon opcode or dependency is added;
actual native results and narrower compatibility claims are recorded in20.

From parent `1f9cceb73ba0ede11c86120b73b2015f900d1dd8`,
[routine healthy-owner reclamation](proposal/fuse-workspace-snapshot-overlay/21-routine-reclamation.md)
runs before input, mutation and submission admission. It retires registry-only
healthy metadata roots, then payloads released from their last custody, across
the same consumer. Existing writer/window/deadline bounds apply, without state
locks across I/O. Earlier failed/partial owners remain charged and are excluded;
a root marker preserves cleanup failure even before DFS starts. Explicit cleanup
keeps its deliberate repair semantics. The former post-install Commit sweep and
proposal CommitPhase::Cleanup are removed; the next pre-capture pass handles
healthy retirement between repeated Commits. No public operation, dependency,
kernel callback, worker or service opcode is added.

From exact parent `c4f965357381870b3784e3423e75783496c0b7c7`, the focused
[native stream-fragmentation correction](proposal/fuse-workspace-snapshot-overlay/23-native-stream-fragmentation.md)
coalesces short local Source/Write fragments at the existing 1,024-byte frame
budget quantum. Output owns a fixed 1 KiB pending tail and the original absolute
deadline; successful completion flushes before Success, while failure discards
unsent bytes and latches refusal without Drop flushing or replay. Upload uses its
existing 16 KiB array. Large-frame batching, wire bounds, receiver anti-abuse
checks and service algorithms remain unchanged. Output's selected fixed layout
grows 40 to 1,096 bytes; upload adds an eight-byte cursor. The source-pinned failure,
read-only discriminator and exact verification scope are recorded in 23.

From parent `1343b00accf2277085410c2fd84144b2432f6222`,
[native handle write](proposal/fuse-workspace-snapshot-overlay/22-handle-write.md)
adds write_file over borrowed OwnedPayload. The shared mutation body validates
READY writable Local handles, selects live EOF for native append, and uses one
two-piece splice for positional Zero gaps and Local bytes while preserving tails.
Handle release is checked again before publication; zero input is a validated
no-op. Existing capture/lowering/reconciliation and limits remain, with a
conservative 596,616-byte working allowance below 640 KiB. The FUSE adapter stays
read-only; kernel append positioning and reply/invalidation coherence are separate
required projection work. The original frontier failure and shared transport
prerequisite retain their source identities in 23.

From parent `cb9d5a8249602e77a454672de290f6358e04c23b`, the
[mounted SDK-coherence binding](proposal/fuse-workspace-snapshot-overlay/24-mounted-sdk-coherence.md)
admits at most two projection observations through their reply attempts and
excludes SDK publication while an older observation exists. One charged callback
and pending receipt carry checked inode invalidation outside all state/backing
locks. Applied failures retain the receipt and any READY truncating-open handle;
fresh observations and cleanup remain available. Mount finish drops completed
failed bindings only after checked detach/join. LocalEdit can use the existing RO
kernel projection, with WRITE/SETATTR and write access still explicitly refused.
Workspace has no fuser dependency. Six real mounted cases and two native completion
API subsets are recorded separately in 24; writable kernel and remote SDK controls
remain open. The internal per-Workspace control account grows 8,192 to 8,200 bytes;
mount control, both permit slots and the concrete callback Arc are separately charged.

The next operation is documented against parent
`d770f5d10bf160b57a90b212102e7960148dba18` in
[existing-file mounted WRITE](proposal/fuse-workspace-snapshot-overlay/25-mounted-write.md).
The public `mount_writable` explicitly chooses a LocalEdit direct-I/O projection;
`mount` and daemon `--mount-readonly` retain the cached RO profile. One
ProjectionWritePermit owns one of the existing two reply slots and excludes SDK
publication through WRITE's send attempt. It makes one write attempt using the
current kernel append flag and original deadline ceiling, shares the native
payload/splice pipeline, validates append at live EOF, and invalidates after
publication outside state/metadata locks. A second observation can progress during
notification; publication refuses outstanding old replies. Matching-inode flush
reports the existing retained coherence failure; release stays available. The
on-demand reservation now funds one 40-byte write permit plus one 16-byte observer
and the actual boxed-state layout, without another worker/window/FD/queue.
Kernel SETATTR/truncate and daemon writable controls remain open. The operation
record retains exact RWF append observability and concurrent SDK-size cached-read
limits; no universal writable or performance qualification is asserted.

The following size operation is implemented against parent
`4d5443c1239722c2ed57f0428ad2c32bdbb3d941` in
[size SETATTR and truncating OPEN](proposal/fuse-workspace-snapshot-overlay/26-mounted-resize.md).
ProjectionMutationPermit replaces the proposal's write-only token name and owns
one WRITE or size attempt. Its size method shares native SetLen/Zero publication,
validates an optional writable Projection handle and returns exact published
NodeAttributes. The explicit size origin skips userspace invalidation while the
kernel caller owns NOWRITE; kernel post-reply completion installs size and
invalidates pages. TTL stays zero, SDK publication remains excluded through reply
attempt, and unsupported non-size fields are refused. ATOMIC_O_TRUNC remains off,
so the existing OPEN refusal checks precede the separate truncate request.
Actual verification/accounting status is recorded in26; daemon writable controls
and the previous cached-size/RWF limitations remain separate.

The R1-C lifecycle extension is implemented against parent
`451a1f6bdda00482a528659489c8e4d053677e01` in
[authenticated Unmount](proposal/fuse-workspace-snapshot-overlay/27-control-unmount.md).
Bridge owns WorkspaceUnmount opcode10/profile3, a bounded100-byte tag12 result,
exact identity validation and lifecycle mutation/Unknown classification. The
result distinguishes Unmounted and entered-but-Retained; pre-admission refusal
uses the existing Failure terminal. Service explicitly rejects it before Store
lookup/admission and assigns it no Store permission bit. Daemon grants preserve
Status bit1 and add Unmount bit2; keys and expiry remain independent of service
authority. One Arc<Mutex<MountHandle>> shares native lifecycle ownership with
main; control uses immediate try_lock, keeps Status independent and reserves100ms
inside the original deadline for terminal delivery. No queue, worker, Source
protocol or Workspace algorithm changes. Signal cleanup joins control first;
incomplete cleanup retains the process/owners until another explicit signal.
The startup profile stays read-only; wider Attach/Close/edit/Commit controls are
not implemented by this extension. Actual results and qualifications are in27.

The next R1-C operation is based on parent
`dab1751312adecdc57d073145582a9a702449744`:
[CloseClean](proposal/fuse-workspace-snapshot-overlay/28-control-close-clean.md).
Bridge adds opcode11/tag13 with the same124/100-byte request/result bounds and
renames the proposal DTO to WorkspaceLifecycleWire/Outcome. Unmount opcode10/tag12
bytes are unchanged; operation-specific Response variants prevent cross-operation
completion. Daemon adds grant bit4 (valid mask0..7), calls existing native clean
closure under the same lifecycle try_lock and100ms completion headroom, and retains
checked failures. Service refuses all three controls before Store admission. The
closed target remains observable and a later signal skips an already-completed
close. No Workspace/FUSE algorithm, resource limit or dependency changes. Actual
verification status and remaining writable control prerequisites are in28.

Native Mount failure ownership is corrected against parent
`3c227266b6740cb9c50f63f0ba27a38d1b8b9a00` in
[the retained-mount prerequisite](proposal/fuse-workspace-snapshot-overlay/29-mount-failure-ownership.md).
FUSE mount functions return boxed MountFailure with exact phase/cause and a retained
MountHandle after every lease reservation failure. One temporary Session transfer
cell prevents failed worker creation from dropping the only owner. No LayerFS
constructor cleanup or renewed deadline occurs; explicit unmount owns cleanup.
Daemon startup keeps the original mount deadline, closes its unserved listener on
mount failure, and retains incomplete cleanup until a new explicit signal. Checked
cleanup preserves the original startup error exit. This does not add remote Mount,
Attach or writable management; exact route evidence and limits belong to29.

Authenticated Mount continues from documentation checkpoint
`3e5d6a66a9f4e4df4a8a19087e63909a18046c6d` in
[round32](proposal/fuse-workspace-snapshot-overlay/32-control-mount.md).
It adds WorkspaceMount opcode12/profile3/tag14 and independent daemon grant bit8;
valid masks are0..15. Exact target/incarnation, empty authenticated input and
124/100-byte request/result bounds reuse the lifecycle contract. The existing
native client requires the specific Mount response; service rejects it before
Store admission. Daemon shares one Arc<Mutex<Option<MountHandle>>> with main:
only checked successful Unmount clears it, and Mount refuses occupied, mounted,
closed or stopping state. It installs a returned partial owner before Retained;
ownerless admission refusal remains Failure. Original-deadline100ms headroom,
Unknown/no replay and the read-only projection remain. Round32 records exact
source identity, checks and limits; remote Attach/writable control remain separate.

The shared [pipe-cancellation prerequisite](proposal/fuse-workspace-snapshot-overlay/33-cancelled-pipe.md)
records an observed upload-worker busy loop caused by permanent cancellation being
reported as Interrupted to standard read_exact/write_all loops. Cancellation must
be terminal at the Pipe boundary; ordinary operating-system EINTR remains distinct.
This correction adds no queue, retry, worker or replay authority. Its source and
regression evidence are recorded with the operation round that exposed it.


From parent`0d220870175abb6e3f162cb164dba7c49bc98f9d`, the
[failed-Attach prerequisite](proposal/fuse-workspace-snapshot-overlay/35-failed-attachment-ownership.md)
keeps Attaching/Attached/Failed custody in the existing WorkspaceHost registry.
Public exact-identity observation returns the existing Workspace or bounded failed
resource progress; explicit cleanup retains cause/progress and removes only a fully
released failed entry. BranchContext allocation precedes resource acquisition,
and an acquired arena survives later failure. No daemon/Bridge control is added by
this prerequisite. Runtime locks end before backing I/O; short final custody locking
and syscalls have deadline observation points, not preemption. Native managed roots
are exclusively host-owned; static replacement checks do not claim atomic defense
against hostile concurrent root-level renames. Exact resource/count/verification
scope is recorded in35. The prepared DSH workload is pinned in[34](proposal/fuse-workspace-snapshot-overlay/34-preinstalled-dsh-workload.md).

From parent `3e92fe277a0379fd85f158b858085af3ecb2e2d5`,
[authenticated Attach](proposal/fuse-workspace-snapshot-overlay/36-control-attach.md)
adds identity-only opcode13/profile3 and grant16 (masks0..31). The startup RO profile
remains immutable. Daemon control and signal shutdown share one current selector,
Workspace capability and MountHandle; the native registry retains all failed
resources. An attempted selector is installed before Attach and restored to the
prior closed owner only on exact confirmed absence. Tags15/16 carry the distinct
Attach result and failed-attachment Status; existing healthy/lifecycle results
keep their validators. Failed Status observes native custody; CloseClean disposes
it explicitly. Startup attachment failure retains the same pending-selector
cleanup route. Round36 records source identity, bounds, checks and qualifications.

From parent `74e6fbd2d23cb7519ea63c275f42283ce3c79d17`,
[authenticated Workspace Commit](proposal/fuse-workspace-snapshot-overlay/37-control-commit.md)
adds opcode 14/profile 3 and independent grant 32 (masks 0..63). It invokes the
existing native capture/preparation/composite Commit/reconciliation operation.
Tag 17 preserves typed results and known/observed failure distinctions; tag 18
adds bounded writable Status while existing RO/failed-attachment forms remain.
Writable CLI entry requires Branch, explicit disk quota and current Commit grant.
Control starts under lifecycle exclusion before writable mount publication. Signal
cleanup retains control on dirty/refused closure, and only ends admission after
successful close while still holding the same owner slot. This is process assembly,
not a second Workspace or Commit algorithm; round37 records qualification.


The prepared-directory extension after source commit
`8e01d28a1c8b7708f5d319990c1440ae682f8b44` shares one typed directory metadata
record across fresh declarations and existing-directory patches in both prepared
filesystem routes. New declarations use the existing C1 directory builder under
the same C2 save; existing patches preserve generic attributes through the common
portable patcher. Their combined inode budget remains 128 and the complete metadata
envelope remains 32768 bytes. Empty extensions preserve legacy bytes.
[Round 38](proposal/fuse-workspace-snapshot-overlay/38-prepared-directories.md)
records the exact encoding, ownership, verification state and remaining namespace
work. This extends the earlier existing-identity-only prepared surface.


The native mkdir extension after source commit
`85582e1ac2fb75761897115ec9679c59439b1efe` adds backed generation-local directory
records and name deltas to Workspace's existing COW arena. Lookup consults that
state before immutable Service data; directory handles pin their view. Creation
uses one exact-scope C5 reservation, then atomically publishes child, binding,
parent metadata and generation accounting. Capture/lowering/reconciliation use
the existing prepared Service operation and retain D1 entry roots. Files and
directories share complete-request admission; namespace scratch remains charged
while remote admission is released between RPCs. [Round39](proposal/fuse-workspace-snapshot-overlay/39-native-mkdir.md)
records checked bounds and native proofs. That checkpoint refused mounted mutation.

The mounted mkdir extension after source commit
`e95c90d757d246d66ac96d2b4fa7e1d6fcbb11ca` adds a single-use projection mkdir permit
and a FUSE callback delegating to the same native publication. SDK creation shares
checked mutation completion with a borrowed parent/name notification target;
the adapter invalidates parent attributes then the entry outside Workspace locks.
Projected creation sends no reverse notification while the kernel parent lock is
held, relying on its normal entry reply. Notification failure retains the applied
namespace and receipt, releases the withheld Local return reference and refuses
further mutation until checked unmount/remount. No retained name queue or new
persistent layout is added. [Round40](proposal/fuse-workspace-snapshot-overlay/40-mounted-mkdir.md)
records real mounted proofs and the two corrected caller failures.

The portable metadata constructor after source commit
`3b34e3002b4a7abfd205b68bc94de9a4af22b572` adds operation15/profile1 under the
existing metadata grant128. It saves mode/mtime for a stated kind without a prior
metadata root, through the same deadline-aware metadata helper and C2 save owner.
The helper selects the existing portable patcher for update or the existing C1
builder for construction. Request44/result66 bytes and typed result tag19 carry
only the portable fields, saved root and save counts; no inode allocation,
filesystem publication or history authority is implied. [Round41](proposal/fuse-workspace-snapshot-overlay/41-construct-portable-metadata.md)
records the implementation and verification state. Fresh regular-file declarations
in a prepared filesystem remain a separate prerequisite.

The prepared fresh-file extension after source commit
`82c87d70a1f3e723b627bf1d0777305c9eb393ea` adds sorted fresh regular-file IDs as a
subset of the final inode rows in both direct and C5 prepared changes. Empty and
v1 directory trailers remain byte-identical; v2 adds the nonempty file-ID list.
Direct fresh rows and all C5 rows receive semantic content/metadata role checks.
The shared filesystem handler verifies base absence exactly for fresh declarations,
then lets C1 derive new file references from retained bindings. Unbound fresh
regular files fail; directory omission is a separate rule. The128 combined inode
and32768-byte request limits remain. [Round42](proposal/fuse-workspace-snapshot-overlay/42-prepared-files.md)
records the implementation, resource delta and verification state. Workspace still
supplies an empty fresh-file list until its native create operation is implemented.
