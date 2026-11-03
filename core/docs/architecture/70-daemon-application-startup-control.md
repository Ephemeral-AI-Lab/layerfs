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

DaemonSetup carries explicit connection/cache windows and namespace
admission, command identity and backing/mount placement. Command uid differs from
the daemon and root; it describes Sandbox policy, not an implemented command
launcher. Linux process/mount/capability access must still be proved through the
ordinary Sandbox/runtime route. Existing locked nix0.31.3 supplies safe process
and descriptor controls; provider-independent `store/` receives no path/engine
transport type. No daemon Exec registration or supervisor is introduced.

R8b update, 2026-10-10: `DaemonLimits` also carries `serial_low_water`, one
`u64` after the other limits, and the then-current record magic was `LFSD\x02`; a record of
the previous layout is refused, not read. Startup refuses a value at or above
the 1,024-serial refill window as invalid configuration. Each of the three
places that assemble the control Service passes the value to
`application/filesystem.rs`, which sets it on the Store before the Service
exists. Its use is described in
[native mutation](77-native-mutation-coherence.md).

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

An SDK-traced observation (request tag 21, #333) writes the same records. Its
diagnostic failure is not held as connection custody: the daemon reports its
kind and message as `diagnosticError` in the traced reply's telemetry and keeps
the connection serving. See
[SDK-traced observations](68-native-workspace-control.md#sdk-traced-observations-333).
`tools/decode_observations.py` names every section and value below from this
source order and joins records to SDK trace lines by scope.

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
and one request emits at most 64 plus twice the configured reader observation
capacity (Workspaces plus control and retired summary). The
earlier resource-coverage plan's 55 omitted the separate original observer-cost
record; that cost remains explicitly priced rather than hidden. Schema snapshots
have fixed size independent of base entries, mutation bytes, history or unrelated
Workspace populations. The declared reader population is visited one session at
a time. Any unavailable snapshot emits `available:false` and an empty value
array; it is never a successful zero-valued snapshot.

| Section | Index | Values, in order |
| --- | --- | --- |
| 0 | Namespace, or 0 when absent | Original operation tag, configured reader count, immutable cache allowance |
| 1 | 0 | Six OwnerWork scalar fields, then jobs[3], lock_wait_ns[3], hold_ns[3]; classes General, Mutation, Source; section 37 distinguishes retained six-class arrays |
| 2, 3 | StatementKind 0 to 11 (historical 0 to 13) | Foreground and maintenance StatementWork respectively, all 17 source fields |
| 4, 5 | 0 | Foreground and maintenance PayloadWork, all seven source fields |
| 6, 7 | 0 | Foreground and maintenance AllocationWork, six original encoded fields (attempts, requested_bytes, admitted_jobs, refusals, observations, freelist_queries) |
| 8 | 0 | StoreWork: object_batches, object_ids, length_batches, length_ids, serial_reservations |
| 9 | 0 | ReadServiceWork: stopping, poisoned, readers, quarantined, opening_failures, requests, lock_wait_ns; section 39 distinguishes historical 13-field scheduler arrays |
| 10 | 0 | ClientWork: cache_hits, cache_misses, upstream_batches, authenticated_bytes, evictions, charged_cache_bytes, cached_objects, then seeded_objects, seeded_bytes, seeded_lengths (appended for #320; earlier positions unchanged) |
| 11 | 0 | DispatchWork, all ten source fields |
| 12 | 0 | Native mount owner id, root serial, serving, detached, abort_bound |
| 13 | -1 | Writer SqlWork scalars in declaration order, then seven statement_phases (calls, wall_ns), then seven commit_phases (calls, wall_ns) |
| 14 | Reader identity; -2 for closed-reader sum | Reader SqlWork, same 55-field order as writer; unavailable final work invalidates the sum |
| 15 | 0 | Original Commit CapturedNamespaceWork, all 31 source fields |
| 16 | 0 | Original Commit Storage Diagnostics, all 42 source fields |
| 17 to 22 | 0 | Original Content filesystem objects, directories, inodes, references, release and validation work respectively, recursively flattened in declaration order |
| 23 | 0 | Original filesystem bindings_added, bindings_removed, directory_updates, base_records_read |
| 24 | 0 | Native loop phase, revision, configured, created, entered, exited, joined; phase declaration order 0 Prepared through 4 Joined |
| 25 | 0 | Native MountWork, all 13 source fields |
| 26 | 0 | All 39 OpcodeWork opcode counts in Opcode declaration order, then served, inline, refused, terminal, forget_units, store_units, then lookup_missing and served_ns (appended for #359) |
| 27 | 0 | Attempted abort disposition and value: (1,1) Written, (2,count) Short, (3,errno) Failed; no attempted abort is unavailable |
| 28 | 0 | Workspace Resources (section 38); historical global Resources: 17 StoredCounts fields in declaration order, five AllocationState scalar fields, six AllocationWork fields, then database_pages, free_pages, debt_upper_bytes |
| 29 | Reader index | That existing Store reader's 42 Storage Diagnostics fields, in the same order as section 16; unavailable after removal or while its reader lock is unavailable |
| 30 | 0 | Resource-observer admitted and completion_available bits; when completed, its actual StatementWork total (17 fields), PayloadWork (7), AllocationWork (6), parked_turns, queue_wait_ns and service_ns follow |
| 31 | 0 | Common overlay budget limit, aggregate pager allowance, fixed bytes, job bytes, peak job bytes and outstanding jobs; actual template SQLite ENABLE_MEMORY_MANAGEMENT readback bit |
| 99 | 0 | Number of preceding completed records, their values-array number count, their unavailable section count |
| 39 | 0 | Workspace reader schema: 1; absent in historical shared-pool arrays |
| 38 | 0 | Resource scope: 1 means the identified Workspace; absent in historical daemon-wide observations |
| 36 | 0 | SQL statement-family cardinality: 12 after Workspace/Lease removal; absent in historical 14-family receipts |
| 37 | 0 | Frontier class cardinality: 3; absent in historical six-class receipts |

The #320 reader implementation removes the fixed read_handles setup field and bumps
the private DaemonSetup layout to LFSD\x04. Old records are refused, never
misread. (#373 bumps it to LFSD\x05 for `idle_exit_s`, below; #535 D11 removes that field and bumps it to LFSD\x08.) Namespace admission now determines maximum mounted reader population;
each has a verified 2 MiB SQLite cache and short provider transactions. Reader
observations do not keep closed connections alive. Section 39 declares this
current demand/lock layout; historical pool records keep their own schema.

All sections use the same original authenticated call correlation. The normal
Mount facade issues separate bind and Attach calls. Store providers, owner,
immutable cache, native session and dispatcher retain separate observation
epochs. Snapshots are cumulative daemon/session diagnostics, not an atomic
whole-daemon snapshot or exclusive per-Workspace work under concurrency.
Lifetime peak values do not establish phase-local continuous peaks. Counter
sampling and stderr backpressure are explicit instrumentation cost.

An observed call with a live Workspace explicitly charges that Workspace's
Resources work before the owner snapshot. It never scans other Workspaces or
adds the closed template. A call with no live Workspace, including known
terminal removal, emits unavailable Resources with no observer attempt.
A failed live-Workspace selection or attempted observation retains its cause.
Resources computes counts on demand rather than reading the removed accounting
row. Section 30 reports the actual statement/payload/allocation/lock work and
section 34 the number of lock holds. Typed calls transfer owned values directly;
there is no Response, NativeReply, JobReceipt or heap-allocated SQL-family
receipt. Attempted failures retain exact cause, work and backing, independently
of the control outcome and diagnostic-output failure. Mutation and Source are
the only service classes that change frontier waits; other calls are General.
The parser accepts the declared three-class layout and retained older layouts.

An unavailable observation emits no resources or successful zero counters.
No observer failure authorizes a retry. Its original cause and any attempted
work/backing remain in diagnostic failure custody, separate from output errors.

The legitimate operator API `ResourceObservation::acquire(&OwnerClient)` executes
the same production acquisition used by Application. Its
`into_diagnostic_error(output_error)` transfers an original observer failure into
the existing `ConnectionFailure.diagnostic_error` I/O-error owner, preserving an
independent numeric-output failure alongside it. The public
`ResourceDiagnosticError` downcast owns `Mutex<ResourceFailure>` and the optional
output error. `ResourceFailure` distinguishes the original unattempted command
and admission cause, Workspace selection failure, and attempted typed error. Borrowed
inspection uses the mutex's ordinary poison outcome; it does not clone errors,
erase a cause, resolve uncertainty or authorize replay. The mutex owns only this
bounded original error payload, with no new service slots, fields, caches or
registries. Display/Debug stay credential-free and do not dump custody.

The operation outcome and its one original reply attempt remain independent of
observer and output failures. Both errors survive in the existing retained
connection slot; neither replaces the other. External owning proofs use a real
stopped Owner and a real backing-file identity mismatch, combined with an actual
`/dev/full` error, to inspect both original causes and retained failure backing.
An actual binary proof checks that such two failures still deliver the original
Status reply and leave the ordinary live state observable. No product fault
injection or test-only API is involved.

Reader Storage snapshots select an existing session under the reader registry
lock and try its short demand lock. A contended or closed session is unavailable;
observation performs no admission, SQL, cache fill or provider demand. The
registry scales with admitted Workspaces, not files or request tickets. Concrete
SQL observations use weak provider references and a fixed retired-work summary,
so healthy connections close after unmount even if diagnostics remain alive.

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
VmHWM, device I/O, external file allocations or EXPLAIN plans. Contended reader
Storage counters remain unavailable until a later separately requested
observation can inspect the owning session. Linux binary tests
cover ordinary-call silence, equal fixed record counts beside one/four unrelated
Workspace populations, exactly charged observer jobs/SQL, and original successful bind
after a failed diagnostic sink. Non-Linux application startup remains Unsupported;
host codec/SDK proofs do not claim Linux binary execution. These tests and source
descriptions do not constitute performance admission evidence.


## SDK refactor addition — 2026-10-12

The current [SDK surface](../../../docs/general/sdk.md) has Project init/fork/branches/commits,
Sandbox create/close and Workspace mount/exec/commit/status/unmount. Create folds
the original installation and requires its ready observation; close is explicit
container stop/delete and makes no application drain claim. Optional Workspace
exec delegates Sandbox execution; this daemon still supervises no commands.
The authorized Branch-page request is control tag 14, with reply tag 17 and one
existing HistoryCatalog::branches dispatch. Numeric section 0 reports operation
code 14 in its existing field. No other numeric section, column, persistence
schema, control operation or execution component is added by this refactor.
Earlier pinned statements and qualification gaps above retain their own scope.

The owner subsequently authorized reintroducing that reader architecture with
independent <=10% time/RSS allowances. Historical rejection/restoration records
remain unchanged; current runtime qualification is pending. See the
[reader design](../issues/320/WORKSPACE-READER-DESIGN-20261014.md).

## Idle exit (#373, removed by #535 D11)

> **Removed (#535 D11, 2026-11-03).** `application/idle.rs`, the per-call
> in-flight count and `DaemonLimits.idle_exit_s` are deleted, and the
> DaemonSetup magic is `LFSD\x08`. A daemon runs until its Sandbox is
> closed or its machine stopped; it never exits because it is unused. The
> paragraph below records the removed #373 behaviour.

`DaemonLimits.idle_exit_s` (a `u32` after `serial_low_water`; record magic
`LFSD\x05`) selects idle close. When it is not zero, `application/idle.rs`
reads the state once a second; the daemon is idle while it is ControlReady,
its control Service has no Workspace bound or binding, and no control call is
running (each call counts itself in from its decoded record to its answer).
After `idle_exit_s` seconds idle it writes any retained connection failures
and `LAYERFS_DAEMON_IDLE_EXIT` to stderr and ends the process with status 0,
under the state lock every call takes to begin, so no call starts after the
decision. Open connections do not keep it. The period counts from the last
check that saw it in use, so the exit comes between `idle_exit_s - 1` and
`idle_exit_s + 1` seconds after the last Workspace went. The SDK sets 120 s
for its Sandboxes (owner, 2026-10-21); tests and examples set 0.

