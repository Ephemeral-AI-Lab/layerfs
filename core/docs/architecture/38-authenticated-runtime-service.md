# Authenticated typed host service and receipt ownership

> **Status:** Historical implementation record. The host-mediated transport
> described here was retired by F12 after `52e1f2e18`; use
> [the current retirement/Init guide](66-host-transport-retirement.md).
> Original source links below refer to their recorded Git revisions, not the
> current tree. Historical measurements and verdicts are unchanged.

The active SDK runtime/service (`core/crates/layerfs-api/sdk/src/runtime/service/mod.rs` at local Git `52e1f2e18`)
borrows an initialized authenticated Sessions registry. It supplies real bounded
local host dispatch for policy, serials, object/length demands, Save and history
handlers. The application supplies native Bridge VerifiedPeer authentication;
connect rechecks exact bound authority and peer identity. Submission and execution
revalidate authority. Existing runtime/public entry points remain usable.

Service admission has fixed connection/job vectors and aggregate byte/job credits.
Workspace shares cover all its connections rather than allowing additional
connections to buy shares. Demand, policy, serial, accept, finish and history rotate
round robin across ready Workspaces/classes. Only dependency-ready Save heads enter
that rotation: requests for one Save execute in submission order across classes
and connections. Other Saves/Workspaces progress between bounded adapter units.
No provider checkout spans a Save or waits for transport delivery. Byte admission
reserves demand and control capacity from ordinary accepts; job/count admission
can refuse an unattempted request and returns its original owned body.

Canonical input charges actual Vec capacity. Object demand charges its ID/result
metadata and twice the maximal bounded canonical window, covering provider and
reply copies. Copy observations count actual canonical bytes delivered. Completion
credit remains held while queued, executing, inside the service or with the caller.
`Completion(SaveId)` and `History(SaveId)` identify exact registry-retained receipts;
they are not publication-success assertions. Access reads original typed knowledge.
Save release refuses while another queued/result owner references its receipt.
Local explicit acknowledgement is separate from delivery and publication.
Fixed per-class submission attempt/refusal counters include all pre-credit
failures, not only byte/job-credit refusals. Queued adapter errors remain counted
as dispatched work, keeping original refusal phase and body/error distinct.

Disconnect requires exclusive access to the synchronous dispatch owner, so invoked
adapter work has returned. It revokes only the attachment, converts only its queued
jobs to `Unattempted(original_request)`, preserves completed outcomes and owned Saves,
and promotes eligible queued jobs from surviving connections. Reconnection may
attach the original exact binding without refreshing a Branch or replaying Finish.
Fresh owner epochs and private serial capabilities reject stale connection/ticket
reuse. Caller-retained completions also prevent rewrapping the registry in a new
service until their ownership ends. These are local lifetime/dispatch fences, not
socket-close proof, process-restart custody or resolution of unknown publication.

For configured live jobs Q, connections C, ready Workspaces W and queued Save IDs R,
fixed registry memory is O(Q+C); admission/slot checks cost O(Q+C), and bounded
ownership/ready maps cost O(log W+log R). Dispatch chooses among6 classes, advances
one Save head and invokes exactly one adapter unit; its library work is separately
charged. Disconnect scans admitted jobs and blocked Save order once, O(Q+R log R)
plus Workspace queue operations, without a scan per cancelled ticket. Over M jobs,
queue/credit work accumulates O(M*(Q+C+log W+log R)) at fixed selected live windows;
it does not grow with historical Save/file/Workspace totals. The bounded maps hold
queue/capability ownership, never a duplicate mutable filesystem graph. Provider,
SQL, object cache, transport and OS residency are outside the credit gauge; opaque
allocator metadata is excluded. No sustained numerical latency/rate bound follows.

External real macOS provider tests cover fair Workspace/class rotation despite
multiple connections, same-Save Accept/read/Finish ordering, exact demand bytes,
retained-result release refusal, unattempted cancellation and successor promotion,
completed Finish surviving disconnect, original-binding reconnect, fresh authority
revocation and stale-epoch rejection. Original library refused/conflicted/unknown
carriers remain; no retry/re-stage/refresh/implicit abort or discard is added.

[S9 audit](../issues/307/S9-EXIT-AUDIT.md) keeps logical wire codecs, actual bounded
authenticated transport/client delivery, restart/unknown fences, faithful backed
initial acquisition and owning Sandbox/API-core assembly explicit. This service
checkpoint does not complete S9 or S10 incremental Commit prerequisites.

[Logical wire ownership](40-runtime-wire-ownership.md) now supplies real client and
handler codecs plus independent native input/output workers, header authorization
before body allocation, grants, exact typed receipts and socket/partial-input fences.
Each service job separately credits64KiB for control/error/header reply encoding.
The new Binding request inspects the original captured context without refreshing
its Branch; the response is boxed to keep ordinary fixed job entries small. The
existing provider/Save scope stays on its host service thread. Application/consumer
assembly, process restart and backed full-root acceptance remain incomplete.

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
