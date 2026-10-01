# V4a transport work diagnostic and session integration

> Status: Research; informative and not a product contract.

Parent: `1a59e129f8a3d73f49efe9691b53ba4b377be612`. Tracking #294, parent #293.
This is the first submilestone of the full autonomous objective in CHECKLIST.md.

## Cause and single treatment

V3 opens/authenticates a new native metadata connection per lookup/register.
Recorded finalized-object counts imply at least 36 connections per Commit in
its consumer before additional canonical reads/publication. The exact attributed
cost is not yet measured. First instrument the unchanged request/construction/
physical-storage algorithm with fixed-size action counters and connect/call
nanoseconds. Run one explicitly labelled count-driven diagnostic on the exact V3
create/overwrite workload, preserving all bytes, MinIO/C5 steps, bounds and
unknown-cache disposition. It is not a repeat speed sample or admission arm.

Then change only the native metadata session lifetime. One admitted session is
shared across clones of a daemon's Remote handle, behind its request mutex.
Monotonic positive request IDs bind replies exactly. Before a new operation,
known local idle age over two seconds closes the old idle session and establishes
one fresh session; no submitted request is repeated. Failed/uncertain session
operations quarantine that handle. Request capacity is checked before network
side effects. Server accepts multiple successive Begin frames on the authenticated
connection, with strictly increasing IDs and exact payload EOF. The protocol
namespace becomes P6META4; actions/locator/storage/canonical grammars remain V3.
Connection count is fixed by active caller ownership, not object count. Five-second
native I/O/operation bounds and all existing provider deadlines remain unchanged.

## Ownership and evidence

Own `core/benchmark/phase6-live/src/metadata.rs`, focused session/statistics modules,
wire namespace and daemon Remote construction; no shipping product changes.
Instrumented source uses P6META3 until the treatment changes its grammar. No
command recognition, extra construction producer, byte oracle change, payload
cache, packing change, validation removal or quota/deadline increase.

Retain aggregate action counts, request count, connects/authentication time,
request/reply time and session reuses per Commit in daemon stderr, alongside
existing object/pack/callback counts. A statistics vector has five action cells;
no lifetime request registry. Timers are diagnostic and do not move work outside
Exec/Commit. Existing semantic proof and real provider/cleanup still execute;
independent canonical/physical-memory gaps remain open.

One baseline count diagnostic, then one source-pinned changed treatment invocation;
new output directory and frozen source/binaries/image each time. Complete child
15 seconds, separate proof 9.5 seconds. No unchanged-arm repeat. MinIO startup
and image/build preparation remain recorded outside the child, global genesis
inside its complete command as in V3. Cache unknown means INELIGIBLE; do not
claim a matched speedup from uncontrolled-cache numbers.

Exit: exact full-path bytes/mode/parent/head/cleanup preserved, request/reply
identity/refusal external tests pass, a short two-head run uses one metadata
connection for the daemon, calls remain visible and buffers bounded. Count-driven
cost evidence determines the following V4b/V4c work. No larger-family or locality
completion is inferred from V4a.
