# Operations and bounded transport

> **Status: proposed design, not implemented or qualified.** Pair 3 (#181)
> establishes service/daemon/bridge foundations; pair 1 (#179) later consumes
> these operations for FUSE and Workspace state. Pair 2 (#180) owns history.
> No protocol ABI, transport choice, resource default or performance result is
> approved merely by appearing here.
>
> Source inspection pin: `795fb1a2f` plus frozen working-tree snapshot manifest
> SHA-256 `f916d0015473dcbb1ba893f6327c47a1a821e5419d785017ab62f090b55ff44e`.
> The file identities are retained in [source-snapshot.json](source-snapshot.json).
> Concurrent Stage 6 changes are outside this pin. All deployment, protocol and
> transport acceptance checks described below are **NOT_RUN**.

This document defines a small proposed operation protocol with one implementation
of the logical handlers. [Architecture and portability](01-architecture-and-portability.md)
owns the process/dependency model; [file layout and boundaries](02-file-layout-and-boundaries.md)
owns implementation placement; [resources and verification](04-resource-and-verification-plan.md)
owns numeric profiles, accounting and evidence. The governing scope remains
[portable service, bridge and Docker daemon](../04-boundary-and-trust.md).

## 1. One operation, local content/storage calls

```text
 external driver        Linux daemon           host service
       |                     |                       |
       | stable logical op   |                       |
       +-------------------->|                       |
       |                     | bounded logical op    |
       |                     +---------------------->|
       |                     |                       | authorize + route Store
       |                     |                       | C1 logical operation
       |                     |                       |   <-> StoreProvider (local)
       |                     |                       |   --> SaveHandoff (local)
       |                     |                       | C2 -> native SQLite
       |                     | typed terminal result |
       |                     |<----------------------+
       |<--------------------+                       |
```

The daemon initiates one configured connection to the host endpoint. An ordered
TCP byte stream is the concrete transport candidate; the endpoint route,
authentication mechanism and remote confidentiality/integrity requirements must
be selected and recorded before implementation. Docker localhost is not the host
address. A bridge is endpoint code, not another process or an SQL proxy.

The main acceptance route is network delivery between the actual Linux container
daemon and native macOS service processes. One bridge implementation provides both
endpoints; native placements do not select separate Docker/local/cloud protocols. A
service can accept multiple independently authenticated daemon connections under
its aggregate bounds. Request correlation is connection-scoped and associated
with the verified caller; it is not authorization or an idempotency guarantee.

A secondary in-process parity invocation calls the same authorized Service
handler with typed requests, source and sink capabilities. It is not another
bridge carrier. It skips framing and network copies while retaining input limits,
authorization, admission, stable-source ownership, core work and completion
semantics. It cannot replace the main Docker/Linux-daemon to macOS-host-service
network acceptance path.

The service handler need not know whether its caller uses FUSE, a headless driver
or a particular container layout. It must know the authorized caller/Store context
and logical operation. The bridge delivers the request to that handler and encodes
its result; only the handler can establish the operation's storage outcome.

C1 and C2 stay together in the service. The daemon neither predicts canonical IDs
nor asks `contains` before uploading. Canonical object emission, dependency/base
lookups, membership checks, delta selection and SQL stay inside the service's
local C1/C2 composition. This avoids a network exchange for each object, tree
page or delta dependency, while preserving the grouped local calls.

## 2. Initial operation set

The [public operation catalog](07-public-operations.md) defines the five proposed
caller operations, their inputs/results, public entry points and C1/C2 mappings:
`ReadFile`, `Inspect`, `ConstructFile`, `EditFile`, and
`UpdatePreparedFilesystem`. This document defines their shared transport,
completion and failure behavior. Names remain proposed, not existing SDK methods
or frozen wire discriminants. Requests name an authorized logical Store, not a
filesystem path or connection string; routing belongs to the service.

The first filesystem slice changes existing identities and prepared bindings.
Creation of new inode identities requires the later explicit allocator/nonreuse
contract; a test driver cannot make that obligation disappear by supplying random
serials. Empty construction and empty/no-op edit/update requests are valid only
where the corresponding core operation permits them; empty bytes still use the
canonical C1 empty representation, not a transport-invented null root.

The wire request carries an operation correlation ID and caller input-generation
label. They associate input/results with one attempt; they are not an idempotency
key, a server-side Workspace, or permission to replay. An explicit immutable root
is the base authority for this initial surface. Future logical head checks are a
history contract; a generation label alone does not implement a branch CAS.

### Core constraints that the bridge must preserve

**Construction can be sequential.** `construct_stream` performs a bounded cutoff
probe and then scans the source; it need not retain the whole file. The service
supplies a length-checked `Read` adapter over bounded incoming body chunks. The
adapter exposes EOF only after a valid `END_INPUT`, never merely because a socket
read temporarily produced no bytes. Zero-length input still requires explicit
input completion before publication.

**Known edits require replayable input.** `EditSource::read_at(index, offset, dst)`
is not a one-pass socket interface. The comparison and edit construction paths
may read replacement bytes again. The initial proposed implementation receives
replacement parts into explicitly reserved, capped service memory before calling
`apply_edits`; it does not claim file-size-independent memory for arbitrary edits.
The per-op replay cap `R_replay` and aggregate admission charge are specified in
the resource profile; complete-file total byte allowance `R_request` may exceed both
`F` and `W` because that route is sequential. A request above that cap fails before mutation. A separately designed,
bounded file-backed replay source could later serve larger replacements; there
is no automatic spool fallback, and its disk/page-cache/cleanup costs would have
to be charged and verified. Do not flatten known edits into a complete new file
just to fit a sequential transport abstraction. Before implementation, decide how
every required edit workload fits the supported replay route. A required case that
does not fit is `NOT_RUN` with its reason until the design is resolved; do not
shrink its input or move acquisition/replay work outside its measurement timer.

Edit coordinates are **current-result coordinates**, and the current C1 stream
rejects an edit reaching back into bytes an earlier replacement introduced. Keep
the declared record order and boundaries; sorting or coalescing them can change
CDC segmentation and exact roots. Decode untrusted offsets with checked length
arithmetic, construct `Edit::new(start, end, replacement_len)`, then validate with
`EditStream::new(base_len, edits)`. Do not call `Edit::overwrite` on an unchecked
inverted range: it computes `end - start`. This is the current core operation
contract, not a promise to support every future SDK/Workspace edit sequence.

**Filesystem input is a bounded complete request, not an infinite record feed.**
The C1 API borrows slices of directory/inode changes and validates them. Materialize
those records within declared byte/count limits before invocation. Directory
records state final bindings for the names changed, not every unchanged directory
entry. Check aggregate names/attribute bytes, list cardinalities and arithmetic
before allocation, as well as C1's own grammar checks.

**New content must already be readable by filesystem validation.** The first
`UpdatePreparedFilesystem` route uses retained roots accessible through its
ordinary `StoreProvider`. It does not assume that objects accepted through a
mutable `SaveHandoff` can simultaneously be read through an unrelated provider.
Although `SaveOperation::read_batch(&mut self, ...)` exists, it is not an already
composed shared reader-plus-consumer API. A construct/edit followed by a filesystem
update therefore costs two logical exchanges in this initial surface; the first
save can remain unreferenced if the second fails. A future atomic combined save
needs a reviewed same-save authenticated-read and ownership contract. Do not add
an unbounded object bag, unsafe aliasing, or premature save publication to pretend
that this operation is already supported in one exchange.

For N independently constructed/edited files followed by one prepared-tree
attachment, the initial surface costs **N + 1 logical exchanges**. The caller
retains O(N) result-root/record metadata under explicit count and byte caps. N
must fit one bounded update request; splitting into more updates adds exchanges
and intermediate published roots. This is not an atomic multi-file save or a
constant-exchange batch. A future combined operation is a separate capability.

Network bundling is a separate decision from storage atomicity. A proposed future
`SubmitChanges` handler could accept one bounded stream and perform those N + 1
saves locally, using already retained roots for the tree update. Earlier saves
would remain if a later step failed. It needs an explicit partial/unknown outcome,
input-ID-to-root mapping and reconciliation contract, not a same-save provider
merely to reduce network exchanges. See the [legacy comparison](05-v0.1.6-comparison.md).
This recommendation does not change the initial public operation catalog.

Source anchors: C1 [sequential construction](../../../../crates/layerfs-content/src/file/content.rs),
[edit source](../../../../crates/layerfs-content/src/file/edit/input.rs),
[edit dispatch](../../../../crates/layerfs-content/src/file/edit/apply.rs),
[filesystem input](../../../../crates/layerfs-content/src/filesystem/input.rs),
[validation](../../../../crates/layerfs-content/src/filesystem/validate.rs),
and C2 [save/read ownership](../../../../crates/layerfs-storage/src/cas/store.rs).
These links describe source seams, not new implementation.

## 3. Normal exchanges and round-trip cost

For the initial native-stream mapping, authenticate and negotiate protocol/limits
once per established connection.
The service supplies the selected protocol version, supported operations and
resource/profile limits in the handshake. Operation authorization still runs for
every `BEGIN`; successful connection authentication does not grant every Store.
Store/profile expectations can accompany `BEGIN` and be checked in that same
operation, without an application preflight query.

```text
 Warm connection: read                    Warm connection: construct / edit
 daemon                  service          daemon                  service
   | BEGIN(Read, root,range) |               | BEGIN(op,limits)        |
   | END_INPUT               |               | BODY(first chunk)       |
   +------------------------>|               | BODY(next chunks)       |
   |                         | C1/C2 reads   | END_INPUT                |
   |<---- RESULT_DATA* -------+               +------------------------>|
   |<---- SUCCESS(length) ---+               |                         | C1/C2
   |                         |               |<---- SUCCESS(root) -----+
```

The daemon may pipeline body frames immediately after `BEGIN`, within its bounded
application buffers. It does not wait for `READY`, and the service does not send
per-frame acknowledgements. `END_INPUT` is an input terminator, not a second
request. Reads with no payload still use the explicit terminator. Upload and
response relay run in full duplex so an early refusal can stop a slow sender.

An accepted warm operation therefore has one logical request/response exchange;
it is not a constant-latency claim. Transfer time, TCP flow control, core work,
resource waiting, scheduling and final publication still cost time. The first
connection additionally pays handshake/authentication exchanges. Listing another
page, constructing a file then updating a prepared tree, or a separately requested
authoritative inspection are additional operations with additional exchanges.
Report each, rather than hiding them in a one-RTT headline.

Future HTTP/WebSocket delivery must preserve these operation meanings and prove
its own upload/refusal/buffering behavior; it need not copy the native handshake
or frames verbatim. See [later integration](06-future-fuse-and-cloud.md).

Large values are chunked, not converted into one giant frame. Small control frames
may use ordinary buffered/vectored writes where the selected runtime provides
those without another allocation. Do not add a custom batching runtime to save
one syscall before the actual path is measured. The sender must flush bounded
buffers when needed to make progress; waiting to fill a whole window must not
stall a small request or terminal response.

## 4. Frame grammar and validation

Use a fixed-size binary header followed by bounded bytes. Proposed header fields
are magic, protocol version, frame kind, closed-set flags, operation ID and
unsigned payload length, with one declared byte order. Final widths and opcode
numbers are protocol-freeze decisions; no native Rust layout is serialized.
Unknown mandatory versions/kinds/flags are explicit refusals, not guessed parsing
or fallback to another protocol. Data frames must contain at least one byte; an
empty operation uses its terminator directly. Together with finite total byte
limits and the closed state machine, this prevents unlimited zero-length frames
or repeated control frames from creating unbounded work.

Proposed frame kinds:

| Frame | Direction | Meaning |
| --- | --- | --- |
| `HELLO` / negotiated response | Both at connection start | Authenticated protocol and resource selection; bounded separately from operation data |
| `BEGIN` | Daemon → service | Bounded opcode, Store selector, roots/generation, record counts, exact body lengths and output allowance |
| `BODY` | Daemon → service | Next ordered bytes of the current declared input; no per-object interpretation |
| `END_INPUT` | Daemon → service | All declared input bytes/records supplied; permits input finality validation |
| `RESULT_DATA` | Service → daemon | Next ordered result bytes or typed bounded record page |
| `SUCCESS` | Service → daemon | Exactly one terminal success; includes result identities/length and bounded outcome data |
| `FAILURE` | Service → daemon | Exactly one terminal failure with typed class and known/unknown disposition |

Control metadata has its own aggregate cap `P`; `BODY`/`RESULT_DATA` payloads have
maximum `F`. Decoded operation records must also fit the applicable metadata and
per-op record budgets; putting records in body frames does not evade those budgets. Neither a peer-supplied length nor record count may cause an allocation
before validation. The decoder:

1. Reads exactly the fixed header into fixed storage; truncated header is a
   protocol failure. It does not scan arbitrary bytes looking for a resync magic.
2. Validates version, legal frame kind in current state, active operation ID,
   reserved flags, the per-kind cap and checked remaining operation totals.
3. Charges/reserves destination capacity before reading the payload. For byte
   data, fills/reuses the bounded relay buffer instead of allocating `payload_len`
   afresh. Metadata uses count/length-checked decoding with checked arithmetic.
4. Accounts consumed bytes and records once, rejects duplicate/out-of-order
   semantic parts, and accepts `END_INPUT` only at the declared totals.
5. Rejects another `BEGIN` before the active operation's terminal result; there
   is no multiplexing, interleaving or cross-operation body reuse in this version.

An ordered reliable stream already provides byte ordering. Per-chunk offsets,
hashes and acknowledgements are unnecessary for construction unless a concrete
operation semantics requires them. Edit replacement boundaries come from declared
part lengths and edit records. Correlation IDs are connection-scoped and checked;
reconnecting does not resume an operation under the same ID.

No base64, JSON byte arrays, per-byte objects, per-canonical-object envelopes, or
second payload compression layer. Core owns canonical hashing and storage codecs.
Content IDs authenticate bytes against a trusted expected identity; they do not
authenticate a peer, authorize a mutation or protect an attacker-chosen request.
The selected authenticated transport and request checks must supply those
properties. Credentials/token expiry behavior must be specified before protocol
freeze: reject an operation that cannot be authorized for its allowed execution
window, or define explicit cancellation on expiry/revocation. Never reconnect,
refresh credentials and replay a mutation silently.

## 5. Admission and backpressure: one story

[The resource plan](04-resource-and-verification-plan.md) owns the illustrative
numbers and final configuration. Symbols used here are:

- `F`: maximum data-frame payload; `P`: maximum control metadata payload.
- `W`: per-direction, per-connection **application buffering window**.
- `C`: process connection cap; `A`: globally active service-handler cap.
- `Q = 0`: no waiting request descriptors in the initial design; `D`: operation deadline.
- Per-op total input, response, replay and record limits: finite values frozen by
  opcode in the resource plan, not unbounded streaming promises.

`W` limits queued/held application bytes, not all bytes transmitted but
unacknowledged by the peer. Kernel socket buffers, TLS buffers and the daemon's
upstream driver buffers are separate owners in the memory ledger. There is no
application credit counter that secretly requires a per-frame ACK.

On `BEGIN`, check the authenticated caller, authorize, validate declared work,
and reserve active capacity. The initial design has **Q = 0**: if capacity is
unavailable, send a small typed refusal if possible and close without draining
the upload. There is no waiting-request queue, scheduler, automatic retry or
configurable nonzero Q in this scope. A later queuing requirement needs its own
bounded lifetime/fairness contract.

When an admitted handler stops consuming input, its bounded receive buffers fill;
the daemon fills at most its outgoing `W`, then stops pulling from upstream.
Ordinary transport flow control stops further transfer; separately bounded
socket/TLS storage accounts for bytes already beyond the application buffers.
This propagates pressure to the external driver and later Workspace. A sender
must read responses while writing so early refusal does not deadlock behind an
upload. Bounded I/O buffers are not a waiting-operation scheduler.

An active handler consumes one bounded input window at a time. Construction
passes those bytes through C1/C2 without accumulating the entire file. Edits
first reserve their complete permitted replay storage; those bytes remain charged
until the last replacement read ends. Filesystem metadata and its ordering backing
are likewise reserved under their own owner. Active count `A` does not authorize
additional C1 construction workers or concurrent writers. C2 ownership refusal
is returned once, not hidden by retries, backoff or an alternate backend.

For responses the service's bounded sink stops producing when outgoing `W` fills;
the daemon's bounded relay stops consuming when its downstream reader is slow.
Charge the downstream sink, driver buffers and daemon stdout/stderr pipes too; an
implementation that buffers all output before writing stdout violates this model.
If output closes or its deadline expires around mutation completion, the daemon
may have an unknown outcome and must not replay merely to print a result.
Do not launch an unbounded task per frame or collect a whole read in a `Vec` before
sending. Concurrent socket I/O for backpressure is not another construction lane.

The wire admits finite total work, not merely finite live memory. Cap request and
response bytes, edit count and replacement totals, directory/inode record counts,
aggregate path/name/attribute bytes, listing page cardinality, roots/references
and operation fanout. Existing C1/C2 grammar/work limits remain additional limits;
a larger bridge allowance cannot override them. Exact values must cover the
registered intended workload, and must not be shrunk to manufacture a performance
pass. Until those values and combined memory accounting are frozen, resource
acceptance is incomplete.

## 6. Operation state and completion

```text
 CONNECTING -> AUTHENTICATING -> IDLE
                                |
                              BEGIN
                                v
                    VALIDATE / AUTHORIZE / RESERVE
                      | refused             | admitted
                      v                     v
                FAILURE + CLOSE        RECEIVE INPUT
                         streaming construct| replay edit / metadata
                                            v
                                       EXECUTE C1/C2
                                            |
                   valid complete END_INPUT + required input checks
                                            |
                                            v
                                 FINISH / PUBLISH (mutations)
                                            |
                                            v
                                  TERMINAL SUCCESS -> IDLE

 Any active state -- definite failure --> single abort/cleanup disposition
 Any active state -- lost certainty ---> UNKNOWN; no replay or guessed cleanup
 Protocol truncation/desync -----------> CLOSE; never reinterpret trailing bytes
```

Input reception and execution overlap only for the sequential construction path
and other operations whose real API permits it. Edits cannot start their replay
against incomplete/mutable replacement data. A protocol `END_INPUT` alone is not
success: C1 and C2 must complete and their required cleanup must be acknowledged.

For a mutation, begin one C2 save, retain the constructed root privately, validate
all declared input completion, then call `SaveOperation::finish` once. The returned
root is released to the daemon as successful only after C2 returns success.
Before that, accepted objects may be buffered or private early writes; they are
not an ordinary published root. If input fails after early private work, abort
once through the existing failure boundary. Preserve original storage errors
retained by `SaveHandoff`; do not flatten every error into `OutputRejected`.

A C2 success means storage completion under its no-sync/no-WAL profile. It does
not create a Commit, stage, layer or branch move and does not promise crash
durability. Client correlation/generation fields do not change that meaning.

Read `RESULT_DATA` is **provisional until terminal success**. A later provider,
capacity or transport failure makes the overall read incomplete; consumers must
support discarding/invalidating partial output or explicitly accept partial-data
semantics. Bytes handed to an irreversible consumer cannot be recalled by the
bridge. Empty successful reads carry a zero length and terminal success rather
than relying on EOF to mean success.

### Deadlines and cancellation

The service validates the requested deadline and derives one local monotonic
operation budget `D` under its configured maximum. It does not trust a remote
wall-clock timestamp. Admission, upload, execution, output and completion are
inside the declared budget, with any cleanup disposition reported separately
according to the resource/verification contract. Progress frames do not reset it.
No timeout inflation or hidden reattempt turns a missed budget into success.

Check cancellation/deadline at adapter I/O and operation boundaries. Existing core
calls have no universal asynchronous cancellation token; do not promise immediate
preemption in the middle of hashing, encoding or a SQLite commit. Finite admitted
work limits the uninterruptible span, and its actual bound needs verification.
Do not forcibly terminate the owner mid-commit and label that a definite abort.
If completion certainty is lost, report unknown. A hard wall-budget guarantee
cannot be claimed until these spans and cleanup are measured.

The initial protocol has no in-band `CANCEL` frame. Local cancellation closes
that sole-operation connection; deadline and disconnect checks trigger the known
safe disposition when execution can observe them. This is not acknowledged
cancellation, immediate preemption or proof of rollback. A completed operation is
not undone, and a mutation may have an unknown outcome. The canceled connection
is not reused. A later acknowledged-cancellation feature needs a real requirement
and a separately defined race/outcome contract.

After early refusal, close unless all input was consumed and framing is known
synchronized. Do not drain an attacker-declared upload to recycle a socket. Apply
the same rule to daemon stdin: unread local BODY frames must never become a new
BEGIN after host refusal or local cancellation. Terminate that submission session
unless its input boundary is already known synchronized; do not drain unbounded
stdin merely to keep the daemon running. Deliver the bounded known/unknown
outcome when possible before closing output.

## 7. Failures and stream reuse

| Situation known by this endpoint | Outcome | Required action |
| --- | --- | --- |
| Invalid version/frame, denied Store/op or admission refusal before execution | Definite rejection if its terminal status is delivered | No mutation; bounded error; close if input is unread/desynchronized |
| Invalid complete input or missing root, with established definite abort and cleanup | Typed known failure | Return original class and cleanup status; reuse only a synchronized connection |
| C2 ownership unavailable | Definite failure of this attempt | Return once; no automatic queue retry or worker increase |
| Cleanup also fails | Failure with explicit cleanup failure | Preserve ownership evidence and original cause; do not claim clean abort |
| Failure with unproven persistence outcome | Unknown outcome | Quarantine/retain uncertain state under C2 contract; no resend, polling or guessed deletion |
| Server completed save but terminal response was lost | Server may know success; daemon knows only unknown | Daemon must not return root as successful or repeat the mutation |
| Daemon disconnected mid-upload after `BEGIN` | Client cannot infer exact server execution point | Server performs its known safe disposition; daemon reports unknown unless it has an authoritative definite rejection |
| Read response ends before terminal success | Incomplete/failed read | Invalidate partial result; do not translate EOF into success |
| Deadline/cancel races with final publication | Known failure or unknown according to actual evidence | Never infer rollback from cancellation request or socket closure |

Keep logical `PathNotFound`, missing canonical `MissingObject`, invalid input,
unauthorized, resource/capacity refusal,
ownership failure, provider/integrity failure and unknown outcome distinguishable.
Do not expose raw SQL, host paths or credentials in wire errors. A bounded typed
code plus bounded context and cleanup disposition is enough; diagnostic detail
belongs in a bounded authorized service report.

Preserve only distinctions actually available from the invoked API. Current
[StoreProvider](../../../../crates/layerfs-storage/src/cas/provider.rs) maps several
C2 read failures, including capacity and engine errors, to C1 `ProviderFailure`.
That path cannot supply their original structured categories. Report the preserved
provider-failure class; never reconstruct types by parsing diagnostic strings.
If a finer read taxonomy is required, review an explicit error-preservation
correction before promising it. Admission, direct C1 and retained save errors
still carry the typed distinctions those boundaries actually provide.

A fresh connection may perform a genuinely new authorized operation. It cannot
silently resume or reissue this one. A separately requested authoritative
inspection can establish what persisted where the defined API supports it; there
is no background retry/polling/status-recovery service implicit in correlation IDs.
Content-addressed reuse alone does not make a complete operation replay-safe.

## 8. CPU/copy accounting and verification

Optional completed diagnostics must fit the actual response envelope and remain
independent of the product outcome. The [telemetry proposal](08-telemetry-and-retention.md)
separates per-operation reports from independent resource observation/export;
it introduces no sixth product operation or unsolicited metric frame. The default
periodic daemon report route uses a bounded diagnostic channel collected by the
host, with stdout reserved for the operation protocol.


Frame parsing, checked length/count accounting and byte relay should cost
`O(payload bytes + control records + frame count)`, with bounded per-frame state.
This excludes actual C1 traversal/hash/CDC/edit work, C2 membership/base lookups,
codec work, SQL and authentication; those are separately reported, not declared
linear merely because transport is. Repeated edit reads are real work. Avoid
reparsing or copying completed payloads into another serialization format.

The copy inventory must include driver → daemon, daemon → stream, service receive
buffer → stable edit source when required, C1 canonical construction and C2
ownership, plus the reverse read path. “Direct” removes wire work, not core copies;
“streaming” bounds buffers, not total bytes moved or page cache. Do not claim zero
copy or a resource/speed improvement before measured evidence.

External tests must launch the real host service and Docker daemon and drive this
same operation/client path. No test-only core cfg, alternate algorithm or private
source inclusion. Compare semantic results from direct and stream endpoints,
then verify the actual Docker route independently. Cover exact-length/empty input,
all frame split points, bounded oversized declarations, partial bodies, wrong IDs,
unauthorized routes, stable edit replay, prepared filesystem reads, slow peers,
admission exhaustion, terminal loss and cleanup failures. Verify old immutable
roots after successful updates and failure attempts.

The [resource and verification plan](04-resource-and-verification-plan.md) owns
exact deployment identities, runnable commands, numeric acceptance and measurement
locks. Per-operation network exchanges must be counted separately from frames,
local C1/C2 calls and SQL statements. No test has been run for this proposed
protocol, and no Stage 6 result proves its Docker/host behavior.
