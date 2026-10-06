# Native runtime consumer ports and original exchange custody

> **Status:** Current general guide. S9 checkpoint; application/restart/context and backed-root exits remain open.

SDK client now owns `Calls`, a native exchange owner over existing authenticated
send/receive directions, VerifiedPeer and independent CloseHandle. Its mutex spans
one bounded policy/demand/serial/Save/history adapter unit, never a whole Save or
Commit. The application supplies the exact original attachment/binding. Native
framing directions remain independently usable for application multiplexing.
No provider open, reconnect, Branch refresh or automatic request replay occurs.

Each exchange checks the original grant correlation/class before body send, then
checks final kind/class/correlation, operation, object/Save identity, demand cardinality/
order, serial count and attempted receipt phase. A wrong result retains its original
credited body and exact codec/close cause. Transport/protocol failure makes this
owner terminal. A pre-body admission refusal remains distinct typed remote knowledge;
none of these outcomes proves filesystem/history publication absent or successful.

CloseHandle is outside the call mutex. Explicit close can wake a blocked receive
without first acquiring that mutex or applying a product timeout. Partial-reply drain
uses a nonblocking owner check: an in-progress call is unavailable, not falsely joined
or guessed complete. The consumer still owns its original CallFailure and any nested
malformed Message/partial collector state. Save/history knowledge remains in the host
registry; closing this link does not unmount a Workspace, terminate Bash or abort it.

RemoteObjects implements the actual AuthenticatedObjects port for saved/same-Save
windows. It preserves its first failed call or exact remote result, preventing later
reads in that operation from replaying it. Content provider absence is derived only
from typed owning MissingObject/ObjectMissing; permission, unpublished, integrity,
transport and uncertain errors never become absence. Canonical hash failure has its
own FrameError::IdentityMismatch and maps to the owning content identity class.
The public Vec-of-Vec content contract requires bounded canonical delivery copies;
original allocator causes and received message credit remain retained on refusal.

RemoteLengths implements owning Workspace FileLengths through one length message,
without full payload demand. RemoteSerials implements InodeSerials through the bound
host allocator; successful ranges are consumed and never recycled. Their original
call/remote/shape errors survive WorkspaceError::Service. The host checks authority
before buffering and invocation, with existing metadata/serial library semantics.

Real host Store tests exercise all three ports over actual authenticated sockets,
including missing-object preservation and refusal of another read after failure.
Portable native tests prove independent close while a call waits for a grant and
retention of an authenticated wrong response without another call. No product fault
hook, timeout or error-driven alternate route is added. Covering source/build/binary/
cache/failure receipts are append-only under issues/307/checks/s9-consumer-ports.

These adapters still need owning daemon/application supervision and process-restart
custody. Saved-root/context authority and fully backed native acquisition remain
S9 work; S8 is explicitly a separate batch. S7's independent complete engine cost/
resource evidence remains open, with integrated kernel request accounting depending
on S8. P3/P6/P7/P13/P14 and S10–S13 are outside this batch. Existing primitive/local
checks do not establish full milestone acceptance or cold speed/RSS/sustained rates.

R1 now provides [owning host supervision and consumer Attachment](45-runtime-supervision.md)
using these existing ports and Calls. Consumer configuration validates Reply-kind
shared credit before moving directions; original native ownership returns on startup
refusal. Explicit independent shutdown followed by a nonblocking call-completion
check returns original partial replies and work observations. The original CallFailure
remains with its caller. R3 process-restart custody, R4 daemon/Sandbox composition and
full contextual/root/service/resource acceptance remain open.
