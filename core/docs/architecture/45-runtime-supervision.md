# Host runtime supervision and consumer attachment

> **Status:** Current general guide. Source and functional checkpoints; S7/S9 qualification remains open.

The [SDK Supervisor](../../crates/layerfs-api/sdk/src/runtime/supervisor/owner.rs)
composes an already initialized `Runtime::sessions()` serving scope, the existing
fair `Service`, shared `InputPool`/`OutputPool`, and authenticated native `Connection`
owners. The application supplies completed KK connections and drives the supervisor
on its provider-serving thread. Socket receive/send workers own no provider handles,
borrowed Saves or whole-Commit mutex. Runtime, Sessions and Supervisor use ordinary
stack borrows; no self-reference or lifetime extension is introduced. Runtime setup
and provider open stay outside individual calls/attachments.

`attach` obtains the initial authority-bound Branch/root through the owning service.
`attach_bound` accepts the exact existing Binding and checks it against its peer and
runtime without refreshing a Branch. Neither creates a global Store or imports a
filesystem. Pre-start refusal returns the original Connection. Input/output startup
errors retain their exact underlying cause; an output-start refusal returns the
already fenced input owner and original unattempted Sender for caller-owned join.
That partial startup is not a completed attachment.

## Bounded serving turns and independent I/O

Each [turn](../../crates/layerfs-api/sdk/src/runtime/supervisor/drive.rs) selects the
next occupied attachment by scanning at most the configured slots, polls that one
owner, then invokes at most one existing fair service job. Unused connection
admission slots do not consume separate caller turns. The cursor advances after
the selected slot; fenced owners remain occupied until their original worker joins
return, and removal or slot reuse preserves rotation. An empty attachment registry
still reaches the existing service step. Each
connection has one current exchange, matching `Calls`' bounded serialized exchanges.
Connections progress independently. Service retains its Workspace/class rotation,
protected demand/control slots and byte reserves, and same-Save ordering. Raw native
framing remains available separately; this Supervisor does not claim pipelined
application exchanges on one connection.

The first header is checked by `Service::authorize_header` before permitting body
receive. The host encodes an admission grant/refusal only after acquiring an
`OutputPermit`. It waits for that original local send receipt before answering the
input rendezvous. In particular, a refused header's original reply completes its
local write before `NativeInput::decide(false)` closes the channel. A refusal thus
cannot be erased by its own immediate local shutdown. A grant establishes input
admission only; later receive, service admission and provider invocation remain
distinct. The input collector still applies its shared count/byte windows before
allocating a body; body admission failure retains the original worker report.

One complete input is decoded with its original receive lease. Before service
submission, the supervisor reserves the maximum bounded original reply capacity.
An occupied output window is a readiness observation: no packet is submitted and
no provider operation has begun. Submission to the existing service happens once.
Success transfers body ownership/charge to that service and releases the transport
lease; refusal retains the exact decoded request, original error and receive lease.
No failed request is submitted again.

Reply reservation covers the owning library's existing windows: Objects reserves
`min(count * 16 MiB, 32 MiB) + count * 36 + 64 KiB`; Lengths reserves
`count * 40 + 64 KiB`; control/Accept results reserve 64 KiB. The allowance covers
bounded headers/errors. A larger error encoding refusal retains the typed result
and reservation rather than truncating it. These are one-message delivery windows,
not limits on total files, edits, Saves, Workspaces or accumulated flow.

After the original provider invocation returns, the typed `ServiceCompletion` is
retained while the host counts/encodes its reply and transfers the packet once to
the independently owned output worker. A transfer refusal returns original packet
and its terminal one-attempt permit, then fences the attachment. A second transfer
with that permit refuses before accounting or I/O. It does not retry the send or
invoke the provider again. A successful local final-send receipt yields `Delivery`, containing
the typed completion or refused body plus original packet/receipt. Holding Delivery
retains both service/Save-receipt credit and output credit. Local write completion
does not establish remote consumption, SaveFinish success or history publication.

The initial config supports 32 connections with 128 shared output slots. The
supervisor selects at least `3 * connections + 2` output slots as its conservative
composition configuration rule. Caller-held Delivery values can accumulate across
sequential exchanges; that rule does not guarantee progress under arbitrary retained
delivery pressure. Actual shared class reserves, permits, released owners and
explicit admission govern readiness and count/byte capacity.
Output observations distinguish packet Vec capacities, pre-encoding reserved bytes
and aggregate credited bytes. A transferred permit shrinks to its actual packet
capacity plus fixed ownership charge before entering its queue.

## Explicit local completion fences

The [fence owner](../../crates/layerfs-api/sdk/src/runtime/supervisor/fences.rs)
first disconnects the exact service connection after synchronous provider work has
returned. Its queued requests become `Unattempted`; already-dispatched results and
Saves remain unchanged. It then starts both independent socket fences and detaches
the bounded event/decision/receipt channels to wake blocked delivery. `try_join`
returns only after both worker exits are observed and their actual joins complete.

`AttachmentFence` contains service cancellation counts, joined input/output reports,
queued events/receipts, original incomplete/undelivered input, partial/unsent output,
original decoded/refused request, service completion, untransferred reply/permit and
exact orchestration/native/frame/close/panic causes. Reports and caller-held originals
retain their original leases. Neither shutdown success nor an empty partial list is
a publication fence. Explicit fence does not abort/release a Save, unmount a Workspace,
terminate Bash, delete global history or resolve P10's terminal unknown outcome.

Dropping a supervisor revokes remaining attachments and wakes workers through their
native owners. The caller uses the explicit fence/join route to retain completion
reports. This local composition supplies no process-crash receipt recovery and no
automatic unknown resolver; host/consumer restart and durable session custody remain
R3 work. Complete application/daemon/Sandbox integration remains R4 work.

## Observed turns and wait-only parking

`step_observed` executes the same one selected attachment turn and at most one
provider unit. Existing `step` is its thin event projection. `AttachmentTurn`
records the original attachment/correlation, existing stage before its turn,
actual progress and an observed unavailable prerequisite. Header acquisition can
progress while waiting for output credit. Partial actual worker joins count as
progress before the complete fence event exists. The separate `ProviderTurn`
identifies the exact dispatched attachment/correlation/ticket; it may belong to
an attachment other than the selected turn. No aggregate-counter comparison,
extra phase, draining loop or new provider route produces those observations.

InputHeader/InputBody and receipt waits mean the actual bounded receiver returned
Empty. They do not identify socket, crypto or scheduler latency. OutputCredit
means the existing class/count/byte reservation was unavailable before an attempt.
ServiceTurn requires the existing local dispatch step. Service admission refusal
still returns its original request/result; released credit never replays it.
WorkerJoin records which actual native joins remain pending. No park permission is
issued while such joins need observation, and no exit flag substitutes for
`JoinHandle::is_finished` or permits a blocking join/helper lane.

One shared coalescing boolean latch/condition variable is initialized before
attachments. Its fixed allocation, Arc ownership and per-worker references are
additional source work; existing registry-capacity gauges do not describe the
whole wake allocation or allocator overhead. Raw InputPool/OutputPool constructors
retain their standalone behavior. Supervisor wires the same latch into its
actual input-event/output-receipt publication and output credit release.

The latch is cleared before a complete occupied rotation. A turn with phase,
provider or join progress invalidates that idle rotation; attach/fence/explicit
join also invalidate it. After a complete no-progress rotation, and only when
no local service turn or actual join requires polling, an unnotified latch permits
`SupervisorPark`. Its fixed reason flags describe the prerequisite classes seen
in that rotation, not a physical latency cause. The permit is non-clonable and
borrows the owner, preventing a local step/attach/fence from making it stale.

Successful bounded sender publication notifies afterwards. Its owned sender is
dropped before the terminal notification, including during unwind and original
worker-start refusal. The queue, original values and SendError custody remain
unchanged. Output capacity release updates the ledger, unlocks it, then notifies.
The waiter checks the latch under that same latch mutex and uses a predicate
loop across spurious wakes. No event history or lifetime sequence cap is added.
Notification poison/unavailability is explicit in `SupervisorTurn::wake_error`
or the wait result, independently of original native/provider failures and queued
custody; it does not guess a channel fence or zero readiness.

`wake_handle` lets an application notify after publishing bounded listener/control
work. The application inspects at most its chosen bounded amount of control work
on each normal turn, including turns without a park permit. It must also inspect
its own queues after obtaining a permit and before waiting; Supervisor cannot
inspect those queues, and notification is only a hint to resume ordinary fair
inspection. The application discards the borrowed permit before attach/fence or
another owner mutation. Wakes do not promote an attachment
or change Workspace/class fairness. `wait_until` holds no provider/Workspace/SQL
lock and affects only the caller's wait: Notified resumes inspection, while
DeadlineReached closes no socket, terminates no Bash, releases no Save/Workspace
and decides no publication. No default RPC/Exec deadline or new budget is selected.

## Consumer attachment

[Attachment](../../crates/layerfs-api/sdk/src/client/attachment.rs) consumes one
authenticated Connection and a shared Reply-kind ReceiveBudget, returning original
native ownership on configuration/start refusal before any request. It assembles
existing `ClientSender`, `ClientReceiver`, `Calls` and independent CloseHandle, and
provides the ordinary `RemoteObjects`, `RemoteLengths` and `RemoteSerials` adapters.
It neither bootstraps a provider nor implicitly issues a Branch/binding refresh.

`Attachment::fence` closes independently of an in-flight call mutex. `try_join`
checks that the bounded original call released that mutex, then returns original
partial replies and send/receive/framing/copy/credit observations once. Original
`CallFailure` stays with its call/port caller; already returned Messages retain their
shared receive credits. Later calls are refused before a new header. There is no
implicit deadline, reconnect, resend, Save abort or Workspace teardown.

## Cost model and evidence limits

Let C be configured attached connections, J admitted Service jobs, B encoded or
decoded bounded body bytes, E encoded error fields and M original calls. Startup
allocates O(C) attachment slots and the existing O(C + J) service registries. Attach
and capability route lookup scan at most C slots. A turn selects the next occupied
attachment in O(C) worst-case bounded slot work and polls that owner in O(1), plus
the existing fair service scheduling/invocation cost; matching a dispatched
ticket to its attachment is O(C). Complete local fence scans existing bounded service
jobs/order state and drains owned queued/partial/result data. It does not walk a
filesystem namespace or reopen a Store.

Input Accept header removal copies B bytes once, recorded by `input_copied_bytes`.
Reply count/encode passes cost O(B + E); canonical encoding copies are recorded by
`reply_copied_bytes`. Existing Service delivery copies, native framing/crypto/I/O,
consumer Vec delivery and owning provider/SQL work remain separately accountable.
The composition adds no SQL/schema, immutable construction or filesystem algorithms.
Across M calls, its registry routing costs O(M*C), with C a declared simultaneous
admission window, plus actual cumulative bodies/errors/service/native work. It
retains no namespace-sized or lifetime-call-sized collection. Occupancy selection
adds no registry, allocation, provider attempt or extra phase to a turn. Observed
turns retain one selected phase and one possible provider unit; idle-round state
and coalesced wake state are fixed. `None` from step still means no completed event,
and an idle or parked occupied attachment still takes its fair share of turns. The R4
functional proof's existing caller sleep and budgets are unchanged; its retained
wall receipts do not establish a latency improvement or packet-level cause.
Held events, permits,
partial bodies and reports retain their owning credit; callers must release them
for further admission.

These source bounds and work counters are not process/kernel/socket/pager/cache
residency, physical I/O, latency, sustainable service or reclamation-debt gates.
External public tests cover the real initialized macOS provider/socket route,
same-Save objects/history/consumer ports, blocked header/body/output peers with
independent Policy progress, pre-body denial, pre-service fencing, retained Delivery
and AttachmentFence credits, and independent consumer shutdown during a blocked call.
R1's scoped validation receipts own their original check outcomes. The subsequent
[sparse serial/progress checkpoint](../issues/307/SPARSE-SERIAL-PROGRESS-20261007.md)
records occupied-slot fairness, observed phase/provider progress, native queue/
sender/credit wakes and two real application queue cases. The
[application proofs](../../crates/layerfs-api/sdk/tests/supervisor_application.rs)
keep one initialized Store/Sessions/provider owner on the main thread. The native
consumer publishes original authenticated Attach/Fence commands into a bounded
queue, then notifies. A command queued before the latch clear is still found by
the post-permit queue predicate. A publication after an Empty predicate wakes
the same owner; the following turn processes control even without a permit.
The consumer awaits actual host Delivery before asking for its own fence. Exact
correlation/provider/Delivery counts, original joined custody, retired identities
and final credits are checked. Native joins may complete in either order.

These macOS Store/native application bodies have no Linux executions.
Their bounded coordination/watchdogs change no runtime deadline or operation
lifetime. Notification poison with retained originals and some partial-join/
partial-round schedules remain source-reviewed without private test hooks.
Full contextual/root acceptance, process restart, complete native application/
daemon/Sandbox integration and S7/E/Q qualification
remain open; those milestones are not completed by this source or these small cases.

The following [R3 local custody checkpoint](46-runtime-custody.md) adds explicit
consuming Sessions completion after service ownership is released. It moves original
slot bindings/capabilities/receipts into application custody, distinguishing known
terminal results, retained cleanup failure and exact unknown outcomes. It supplies
no process-crash recovery or automatic resolver and does not replace the native
attachment fence/join described here.
