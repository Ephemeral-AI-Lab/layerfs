# Direct-Store daemon application startup and control

> **Status:** Implemented source during R1b after `932abe8089`; verification
> receipts are retained under [R1b checks](../issues/307/checks/r1-daemon-composition-20261008/).
> R1 ordinary Sandbox/runtime/access composition remains open. Native FUSE Ready,
> mounted live Commit and complete application drain are not established here.

The real `layerfs-daemon --config <protected-file>` binary delegates to
`application/`. It loads one explicitly encoded Bridge DaemonSetup, initializes
one Overlay owner, and listens for authenticated native installation/control.
The process disables Linux dumpability and sets umask077 before reading keys or
starting workers. Non-Linux production startup returns Unsupported. File and
backing-directory checks use actual opened descriptors, O_NOFOLLOW/O_CLOEXEC,
effective uid and restrictive modes; deployment must keep their ancestors stable
and daemon-controlled. The config and backing files are outside the mount root.
The controller's key and daemon key do not appear in environment/arguments.

DaemonSetup carries explicit connection/read/cache/job windows, namespace
admission, command identity and backing/mount placement. Command uid differs from
the daemon and root; it describes Sandbox policy, not an implemented command
launcher. Linux process/mount/capability access must still be proved through the
ordinary Sandbox/runtime route. Existing locked nix0.31.3 supplies safe process
and descriptor controls; provider-independent `store/` receives no path/engine
transport type. No daemon Exec registration or supervisor is introduced.

Startup is InstallPending, Installing, Retained or ControlReady. Explicit
existing-store configuration opens Disposable once with retained authority; an
installation failure never selects it as fallback. Otherwise one authenticated
StoreManifest enters the existing sole installer after the exact original record
has been decoded. Payload records stay in that installer. It refuses a profile
other than explicit Disposable before claim or file/database effects. Existing
retained Durable implementation can compile; execution remains
`NOT_RUN — disabled by owner until explicit reauthorization`.

The original InstalledStore transfers into Service without reopening the file.
The installer acknowledgment precedes application publication, so a correlated
Hello with an explicit startup wait observes ControlReady before control use.
Hello observes actual daemon incarnation and actual Overlay/Store SQLite versions;
it does not claim a mounted location or FUSE Ready. A stale supplied incarnation
receives Invalid. A lost final installer acknowledgment can leave ControlReady
only from the original Reply phase, published file, validated manifest/version
and opened owner together; the original InstallFailure remains retained. An
Open-phase partial owner alone cannot establish readiness. Startup failure
retains observed Overlay work and any already initialized Overlay owner through
later failures; application publication failure retains the entire original
install result.

The application admits a fixed number of ordinary connection workers and one
additional startup slot. Before Store readiness the additional slot accepts the
installer; an ordinary control gets a before-effect Capacity refusal and ends
that conversation. Authentication and first-record blocking I/O use explicit
configured waits, cleared before operations; these are I/O wait bounds, not a
whole-handshake wall bound or command runtime timeout. Subsequent controls and
installation payloads have no implicit runtime timer. Completed workers are
joined before their slot is reused. Fixed slots retain malformed request bytes,
unresolved product results, failed replies/fences and failed diagnostic transfer.
Their Debug representation redacts raw input and authority. Original
no-decoded-product Io/Channel termination diagnostics transfer once to the daemon
stderr owner before capacity is released; a failed transfer retains both errors.
An explicit operator custody-transfer method changes no filesystem state and
performs no replay/artifact removal; the current CLI does not expose that method.

EndSession/SessionEnded terminates a healthy authenticated control conversation.
SDK Control validates its original correlation and fences the underlying channel
once, so transferring that channel cannot reset the ended session. A failed local
fence retains the original SessionEnded answer at ControlPhase::Fence and does
not attempt another close. Session end is independent of terminal Workspace unmount;
Workspace ownership remains in Service. Unresolved control results retain the
actual Served value, without fabricating a transport/protocol failure. Delegated
installer failure ends its conversation without another receive, reply or fence.

The control-only Service path reuses ordinary bind/status/fork/history/terminal
logical unmount. Commit without a namespace producer validates token and original
custody read-only, then refuses before lifecycle admission, capture, Save or epoch
change. Original Unknown and known publication take precedence. Real construction
still enters the existing Commit driver; no placeholder producer or second driver
has been added. Native attachment is R2; captured namespace normalization is
implemented by R4 but not called by this driver's product callers, which is R5.

The current executable has no complete application stop/fence/join operation.
It does not claim a graceful daemon drain when the process/container is stopped.
Unexpected fatal accept/synchronization failure may leave worker owners live
until process termination. Complete lifecycle/force/drain custody remains R6;
retained malformed/product faults may exhaust the explicitly finite connection
capacity until an operator transfers custody or ends that daemon lifecycle.
Functional binary tests explicitly stop their owned process after proving logical
Workspace close and label that stop as process termination, not native drain.

R1d now qualifies actual macOS controller startup/installation against the Linux
application and corrects EndSession socket ordering: final reply send completes,
then the daemon waits once for the caller's EOF and fences once before releasing
its connection slot. Original Served and terminal byte/work/phase/fence-attempt
receipts survive failure; a failed connection retains receipts while its local
socket owner drops. This establishes no application force-close/join or native
filesystem drain. [Owned Sandbox lifecycle](72-owned-sandbox-lifecycle.md) records
the current source, actual failed/final proof identities and precise scope.

## Numeric observation schema

The R7 source amendment after planning identity `980c169e6` accepts the explicit
checked Observed control described in
[native control](68-native-workspace-control.md#explicit-operator-observations).
Only that request acquires diagnostic snapshots or formats numeric output. It
executes the ordinary operation once and streams bounded numeric records to the
existing stderr owner before sending the same original reply. A failed sink
does not alter the operation result: the original answer is attempted once and
the exact Served receipt and diagnostic I/O error remain separately held in
connection custody. No operation or diagnostic fragment is resent after failure.
The writer overrides the usual Interrupted/short-write continuation and retains
the first diagnostic error. Partial output has no completed footer and cannot
be interpreted as a complete observation.

Each line has this fixed envelope; values are unsigned numeric counters only:

```text
{"layerfs_observation":1,"call":CORRELATION,"slot":SLOT,"scope":[FOUR_WORDS],"daemon":[FOUR_WORDS],"section":SECTION,"index":INDEX,"available":BOOLEAN,"values":[NUMBERS]}
```

Scope and daemon are four unsigned big-endian 64-bit words from the existing
request scope and Application instance respectively. The formatter borrows those
bytes and the existing slot; it adds no owned nonce or registry. Complete groups
are identified by daemon, scope and call. The caller supplies a fresh nonzero
public scope per channel/run, and the consumer rejects duplicate complete
identities. Slot alone cannot distinguish a later connection reusing that slot.

There is no owned formatting buffer or retained opt-in flag, counter registry,
trace, additional file, thread, queue or cache. A line is at most 4096 bytes,
and one request emits at most 64 plus twice the configured reader count lines. The
current complete Linux schema emits 56 plus twice the reader count lines. The
earlier resource-coverage plan's 55 omitted the separate original observer-cost
record; that cost remains explicitly priced rather than hidden. Schema snapshots
have fixed size independent of base entries, mutation bytes, history or unrelated
Workspace populations. The declared reader population is visited one session at
a time. Any unavailable snapshot emits `available:false` and an empty value
array; it is never a successful zero-valued snapshot.

| Section | Index | Values, in order |
| --- | --- | --- |
| 0 | Namespace, or 0 when absent | Original operation tag, configured reader count, immutable cache allowance |
| 1 | 0 | OwnerWork scalar fields in source declaration order, then completed[6], queue_wait_ns[6], service_ns[6] |
| 2, 3 | StatementKind 0 to 13 | Foreground and maintenance StatementWork respectively, all 17 source fields |
| 4, 5 | 0 | Foreground and maintenance PayloadWork, all seven source fields |
| 6, 7 | 0 | Foreground and maintenance AllocationWork, all six source fields |
| 8 | 0 | StoreWork: object_batches, object_ids, length_batches, length_ids, serial_reservations |
| 9 | 0 | ReadServiceWork, all 13 source fields; booleans encoded as 0 or 1 |
| 10 | 0 | ClientWork, all seven source fields |
| 11 | 0 | DispatchWork, all ten source fields |
| 12 | 0 | Native mount owner id, root serial, serving, detached, abort_bound |
| 13 | -1 | Writer SqlWork scalars in declaration order, then seven statement_phases (calls, wall_ns), then seven commit_phases (calls, wall_ns) |
| 14 | Reader index | Reader SqlWork, same 55-field order as writer |
| 15 | 0 | Original Commit CapturedNamespaceWork, all 31 source fields |
| 16 | 0 | Original Commit Storage Diagnostics, all 42 source fields |
| 17 to 22 | 0 | Original Content filesystem objects, directories, inodes, references, release and validation work respectively, recursively flattened in declaration order |
| 23 | 0 | Original filesystem bindings_added, bindings_removed, directory_updates, base_records_read |
| 24 | 0 | Native loop phase, revision, configured, created, entered, exited, joined; phase declaration order 0 Prepared through 4 Joined |
| 25 | 0 | Native MountWork, all 13 source fields |
| 26 | 0 | All 39 OpcodeWork opcode counts in Opcode declaration order, then handoffs, inline, refused, terminal, unadmitted, forget_units, store_units |
| 27 | 0 | Attempted abort disposition and value: (1,1) Written, (2,count) Short, (3,errno) Failed; no attempted abort is unavailable |
| 28 | 0 | Global Resources: 17 StoredCounts fields in declaration order, five AllocationState scalar fields, six AllocationWork fields, then database_pages, free_pages, debt_upper_bytes |
| 29 | Reader index | That existing Store reader's 42 Storage Diagnostics fields, in the same order as section 16; unavailable while leased or when the pool observation lock is unavailable |
| 30 | 0 | Resource-observer admitted and completion_available bits; when completed, its actual StatementWork total (17 fields), PayloadWork (7), AllocationWork (6), parked_turns, queue_wait_ns and service_ns follow |
| 99 | 0 | Number of preceding completed records, their values-array number count, their unavailable section count |

All sections use the same original authenticated call correlation. The normal
Mount facade issues separate bind and Attach calls. Store providers, owner,
immutable cache, native session and dispatcher retain separate observation
epochs. Snapshots are cumulative daemon/session diagnostics, not an atomic
whole-daemon snapshot or exclusive per-Workspace work under concurrency.
Lifetime peak values do not establish phase-local continuous peaks. Counter
sampling and stderr backpressure are explicit instrumentation cost.

An observed call makes one additional route-free `Resources { global: true }`
owner Read job before the owner snapshot, using the existing `resources(None)`
path. It issues exactly three existing Startup statements: the namespace-zero
accounting point, page count and free-page count, with two allocation identity
metadata calls and no transaction or base demand. Section 30 contains this
original job's runtime receipt; those counts are observation work and must be
priced separately from the selected filesystem/control operation. The copied
Resource/cost schemas are fixed transient values, and a successful caller's Completion is
dropped before owner outstanding/credited-byte snapshots. The publisher can
still transiently hold its original Arc and that same credit after waking the
caller. These snapshots do not guarantee zero observer outstanding work or
credit and do not establish a phase-local peak. Existing stop/drain proofs own
eventual release; observation neither waits nor retries to force a zero.
Admission/completion failure stays explicit through missing Resources and the
completion-availability bit; no observation is retried. A failure also retains
its exact original command/cause, completion wait cause, or entire failed or
unexpected Completion. A failed Completion remains charged while held; its
work copy never substitutes for its original result/custody. Routed Resources calls retain their existing
state validation, and route-free non-global Resources is refused.

The legitimate operator API `ResourceObservation::acquire(&OwnerClient)` executes
the same production acquisition used by Application. Its
`into_diagnostic_error(output_error)` transfers an original observer failure into
the existing `ConnectionFailure.diagnostic_error` I/O-error owner, preserving an
independent numeric-output failure alongside it. The public
`ResourceDiagnosticError` downcast owns `Mutex<ResourceFailure>` and the optional
output error. `ResourceFailure` distinguishes the original unattempted command
and admission cause, admitted wait cause, and original Completion. Borrowed
inspection uses the mutex's ordinary poison outcome; it does not clone errors,
erase a cause, resolve uncertainty or authorize replay. The mutex owns only this
bounded original error payload, with no new service slots, fields, caches or
registries. Display/Debug stay credential-free and do not dump custody.

The operation outcome and its one original reply attempt remain independent of
observer and output failures. Both errors survive in the existing retained
connection slot; neither replaces the other. External owning proofs use a real
stopped Owner and a real backing-file identity mismatch, combined with an actual
`/dev/full` error, to inspect both original causes and held completion credit.
An actual binary proof checks that such two failures still deliver the original
Status reply and leave the ordinary live state observable. No product fault
injection or test-only API is involved.

Reader Storage snapshots use a nonblocking pool-state observation lock and copy
one existing Diagnostics value from idle, Ready or retired custody. The lock is
released before formatting; active leases and unavailable locks are unavailable,
without read admission, SQL, cache activity or demand. No whole-reader vector or
duplicate index is kept. Point observation traverses at most R reader entries
and S fixed admission slots; collecting R observations is worst-case O(R²+RS)
in configured service knobs, which remain fixed in this campaign. S is
(configured namespace capacity + 1) times (16 + configured control connections).
This work is independent of file/base/history and active Workspace populations.

Before an observed terminal operation the application clones the exact existing
SessionObserver while holding the registry briefly, then releases that lock.
That transient observer owns no lookup, open, request or Store reader. Sampling
after the operation preserves final native opcode and drain facts even after
registry removal. Final MountWork comes from the existing original typed Drained
receipt, because a successfully removed dispatcher lane cannot be observed
through its earlier queue token. Typing that existing receipt adds no copied
facts or retained field. Mount/Attach select only their acknowledged token. No observer
of a past mount is retained. Commit construction and storage observations come
from the original Success/Failure receipt, including failed attempted prefixes,
never from a later reconstruction.

A successful native drain followed by Revoke or Close refusal keeps the original
Drained receipt in generic failure custody. Section 25 is UNAVAILABLE at those
two failed-prefix boundaries after the lane was removed; no final work is
guessed from the stale queue token. Native opcode/join facts and the original
failure remain observable/retained with their actual scopes. This gap does not
remove custody or establish success of the failed terminal operation.

This output does not supply kernel syscall/wait counts, per-file residency,
VmHWM, device I/O, external file allocations or EXPLAIN plans. Leased reader
Storage counters remain unavailable until a later separately requested
observation finds them in pool custody. Linux binary tests
cover ordinary-call silence, equal fixed record counts beside one/four unrelated
Workspace populations, exactly charged observer jobs/SQL, and original successful bind
after a failed diagnostic sink. Non-Linux application startup remains Unsupported;
host codec/SDK proofs do not claim Linux binary execution. These tests and source
descriptions do not constitute performance admission evidence.
