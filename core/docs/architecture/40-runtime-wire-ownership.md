# Runtime wire records and independent socket ownership

> **Status:** Current general guide. Implemented S7/S9 checkpoint; neither milestone is complete.

The active SDK client, runtime/handlers and runtime/service use the Bridge contract,
codec and native groups. Existing local Session/Service and native entry points stay
usable. No excluded SDK/Bridge/FUSE source is a dependency or fallback. The host
application retains the initialized provider and borrowed Sessions independently of
socket attachments; native I/O workers never own those handles or a whole Save lock.

## Records, original identities and admission

Bridge plaintext records are at most65519 bytes. `LFR1` has a40-byte checked header:
kind, reserve class, start/end flags, zero reserved byte, direction-local message ID,
request correlation, total message bytes and fragment offset. Bytes follow that
header, so one fragment carries at most65479 bytes. Message IDs start in order;
completion may interleave. Reply correlations can arrive out of order or name an
original request again for its distinct admission grant and final result. Every
request correlation starts once in increasing order within its connection.

The SDK first fragment is exactly the104-byte `LRT1` admission header: magic,
operation/role/Save flag/reserved byte,48-byte Save identity,32-byte object identity,
u64 value and u64 generation. Unused fields must be zero. Accept's value is its
canonical length; object/length demand's value is ID count; ReserveInodes's value
is its count. Other values are zero. Object role exists only for Accept; candidate
root/generation exists only for Stage. The operations are:

| Code | Operation | Reserve class |
| --- | --- | --- |
|1/2/3|Policy/ReserveInodes/Begin|Control|
|4|Accept|Save|
|5/6|Objects/Lengths|Demand|
|7/8/9/10|Finish/Abort/Completion/Release|Control|
|11/12/13/14|Stage/Commit/Discard/History|Control|
|15|Original Binding snapshot|Control|

Canonical input remains an existing16MiB object window. Demand remains4096 IDs
and32MiB canonical output. These are library message windows; arbitrarily many
windows serve larger files/Saves/Workspaces. No total upload is computed or buffered.
SaveToken is runtime incarnation32bytes, slot u64 and serial u64. It is untrusted
wire data, with no public local SaveId constructor. The host checks current peer,
exact original binding, incarnation, slot and serial before body allocation and
again before invocation. Root expectations come from that binding rather than
untrusted Stage fields. Binding inspection returns its captured Branch snapshot,
root serial, peer/Workspace/catalog/runtime/continuity context; it does not refresh
or scan a Branch/namespace or open another provider.

A NativeInput worker receives/authenticates one record into fixed native scratch.
It validates opcode/size/class, delivers an admission event and parks on one checked
decision. The host authorizes it and emits `LRG1` grant before permitting body
receive; the client sends its original borrowed body only after that exact grant.
Shared ReceiveBudget count/byte reserves apply before body allocation, including
partial and caller-held messages across receivers. InputPool bounds actual worker/
report owners. Accepted data moves to the existing typed Service, with transport
credit held through downstream admission. Removing the Accept header performs one
explicit payload memmove, reported as copied_bytes; ID decoding allocates one bounded
ID vector. No body or adapter attempt follows a refused header.

## Replies and exact retained knowledge

`LRP1` distinguishes dispatched result/error, pre-dispatch admission refusal and
queued-unattempted cancellation. Result kinds preserve policy, serial range,
Begun identity, Accepted identity, ordered authenticated canonical objects/lengths,
Save completion, history, explicit release and original binding. Borrowed reply
views validate widths/windows, canonical hashes, Commit derivation, tokens,
coherent binding roots and complete consumption. Large object/error vectors are
not rebuilt by decoding; consumers borrow the original credited body.

A Completion response carries its original Save identity, receipt access status,
phase and original WriteOutcome or typed failure. History carries independent
optional Stage/Commit/Discard attempts, complete StageRecord/CommitRecord,
Committed/UpToDate/Removed/Absent outcomes and original failures. None means no
recorded attempt, not provider absence. Authority denial while inspecting a known
completion is a distinct receipt-access error; it cannot relabel Finish as refused.
Stage conflict carries the deciding transaction's exact disposition and record;
there is no reread, refresh, re-stage, automatic discard or transition replay.

Failure nodes contain domain/variant, zero reserved bytes and payload length.
Fields have tag, type and length; types1–6 are unsigned64, signed64, exact bytes,
UTF-8 text, nested original node and complete StageRecord. Runtime variant codes
are1Denied,2Invalid,3AdmissionUnavailable,4StaleCapability,5AlreadyAttempted,
6RetainedCustody,7Content,8Storage,9History,10Reply. Content codes1–35 correspond
to the exhaustive match in handlers/failure.rs; every owning variant/field is kept.
Storage codes1–15 are Content,Io,ObjectMissing,Unpublished,Collision,
MissingDependency,VisibilityCeiling,OwnershipUnavailable,UninspectedState,
UnsupportedPolicy,Integrity,CapacityExceeded,UnknownOutcome,CleanupFailed,Aborted.
History codes1–15 are WithStage,InvalidInput,Missing,Unsupported,Busy,
OwnershipUnavailable,Capacity,Integrity,HeadMoved,BaseMismatch,NotInHistory,
StackMoved,StageChanged,ContinuityUnavailable,UnknownOutcome.

Nested original/cleanup fields remain separate. WithStage fields1/2/3 contain
its disposition code, complete original Workspace/stage and original cause.
Moved-state expected/actual heads and bases stay in independent typed fields.
I/O retains class, raw OS code, display diagnostic and source; known PersistenceError
sources retain their five exact variants/status. Erased application Error sources
remain explicitly opaque diagnostics. No Display string determines a semantic
error code, and decoded text is never leaked into a static-string Rust error.
Child views validate lazily over the credited body rather than allocating a recursive
mirror. Reply encoding counts once, allocates once, then copies canonical output
once. Encoding/admission failure leaves the original typed completion with its host.
Each Service job now also reserves64KiB for bounded control/error/header reply
encoding; output separately charges actual packet capacity through its receipt.

## Socket workers, fairness and fences

NativeOutput has its own socket worker and bounded shared owner/message/byte
admission. Every bounded fragment advances a round-robin control/demand/Save
class and rotates within that class. It preserves the rotation cursor across
calls; control does not have strict reads-first priority. One slow peer parks its
own worker/receipt queue, outside provider dispatch. Output receipts keep original
packet capacity/credit and complete local socket-byte progress. Successful socket
write is not remote consumption or filesystem/history publication knowledge.

Independent CloseHandle shutdown wakes blocked native I/O without borrowing its
worker. Input/output explicit fences detach bounded event/decision/receipt owners
and join only after observed thread exit. Original partial input, undelivered
complete input, partial/unsent output, native/frame/crypto/close errors and panic
payloads return to caller custody. No shutdown result alone is a complete fence.
A dropped transport owner revokes its socket/event delivery, not Bash, Workspace,
Save or history ownership; applications use explicit fence/join for receipts.
Worker start refusal returns original direction/connection ownership and original
OS/allocation cause. Private ownership leases remain until workers and retained
reports are released. No automatic socket deadline, reconnect or failed-call replay
is introduced. Process-restart custody and the owning application lifecycle remain
unfinished; terminal unknown history resolution remains P10, without guessed release.

## Costs and evidence limits

For body bytes B, error fields E, bounded active messages Q and admitted workers C:
framing/crypto/body encoding isO(B+E), reassembly indexingO(log Q) per fragment,
and output selects one of three classes withO(1) queue work per fragment. Error
encoding uses an iterative stack rather than recursive call depth. Count/encoding
passes are linear; canonical payload is copied only on the encoding pass. Fixed
native windows multiply by C; live owned body/packet capacities stay under shared
credit. Kernel sockets, allocator overhead, Store/pager/cache/journal and complete
whole-system residency are outside those gauges, explicitly unqualified.

ChannelWork now distinguishes API calls, record I/O attempts, all socket calls,
positive byte-transfer calls, partial bytes, exact first-party zero initialization,
Noise attempts/input/output bytes, scratch allocation requests and actual capacities.
Observed handshake APIs retain failed authentication/I/O work as well as success.
Crypto byte counts are not a claim about third-party internal copies or CPU time.

Real native/public-API tests cover header refusal before allocation, >record-size
accept/grant, original partial input after fencing, ready-class rotation, caller-held
credits and nested original/cleanup/persistence failures. Real macOS provider delivery
covers Binding/Policy/Serial/Begin/Accept/same-Save demand/Finish/length/Stage/UpToDate/
Release. Original conflict/stage and receipt-inspection denial are also encoded and
checked. Append-only receipts under issues/307/checks/s9-runtime-wire retain failures,
repair causes, exact source/build/binary/cache identities and120-second test ceilings.
These functional counts use uncontrolled caches and are not cold speed/RSS/rate proof.

S9 still requires owning Sandbox/API-core/daemon consumer wiring, reconnect/restart
custody and full contextual/root validation plus backed faithful native acquisition/
hard-link identity. S7 still requires complete page/journal/I/O/residency/debt and
sustained-service acceptance. S8 product integration remains open; the separately
accepted fuser patch/Docker evidence is not an external wait. S10–S13 and
P3/P6/P7/P13/P14 remain explicit later Commit work.

Service also protects the first live demand/control job slots from other groups;
its job window must contain at least3 slots. Caller-held completions keep those
class counts, so their credit cannot disappear during reply delivery. Default
control byte reserve is128KiB, covering the new64KiB reply allowance plus fixed
job/result ownership. Known terminal Accept/Finish/Abort headers refuse before
receive body allocation. Pending same-Save ordering is still checked at dispatch.
Framing counts encoding/copies at the actual encoding step, including a subsequent
native send failure; completed sends remain separate. Fixed frame allocation/
initialization and header bytes are explicitly observed. A dedicated regression
checks those costs after a quarantined-channel refusal without socket I/O.

[Native consumer ports](42-native-consumer-ports.md) now connect existing authenticated
calls to the public object/length/serial interfaces, preserving first failure and
independent close while a call holds its mutex. Original publication/custody knowledge
stays typed and fenced; S8 and complete S7/S9 acceptance remain separate/open.
