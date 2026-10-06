# Host runtime supervision and consumer attachment

> **Status:** Current general guide. R1 implementation checkpoint; S7/S9 qualification remains open.

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

Each [turn](../../crates/layerfs-api/sdk/src/runtime/supervisor/drive.rs) polls one
rotating attachment, then invokes at most one existing fair service job. Each
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
R3 work. Application/daemon/Sandbox integration remains R4.

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
and capability route lookup scan at most C slots. A turn polls one attachment in
O(1), plus the existing fair service scheduling/invocation cost; matching a dispatched
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
retains no namespace-sized or lifetime-call-sized collection. Held events, permits,
partial bodies and reports retain their owning credit; callers must release them
for further admission.

These source bounds and work counters are not process/kernel/socket/pager/cache
residency, physical I/O, latency, sustainable service or reclamation-debt gates.
External public tests cover the real initialized macOS provider/socket route,
same-Save objects/history/consumer ports, blocked header/body/output peers with
independent Policy progress, pre-body denial, pre-service fencing, retained Delivery
and AttachmentFence credits, and independent consumer shutdown during a blocked call.
R1's scoped validation receipts own actual check outcomes. Full contextual/root
acceptance, process restart, macOS-host/Docker integration and S7/E/Q qualification
remain open; those milestones are not completed by this source or these small cases.
