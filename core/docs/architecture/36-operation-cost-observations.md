# Exclusive owner operation cost observations

> **Status:** Current general guide.
> S7 checkpoint after `4ecea4198`; complete S7 acceptance remains open.

[Overlay statement observations](../../crates/layerfs-overlay/src/diagnostics/metrics.rs)
now distinguish host statement attempts, SQLite executions (including trigger
subprogram runs), all changed rows, direct changes reported by SQLite and returned
BLOB-column bytes and all logical SQL-column delivery bytes. Integer/real values
charge8 bytes, TEXT/BLOB charge their length and NULL charges0. Returned bytes
count values exposed before mapping; they are
not exclusive physical I/O, internal SQLite copies or a residency observation.
Attempted changes remain counted after rollback. BEGIN/COMMIT and read-only
statements do not inherit the preceding DML's direct-change count.

Each attempt also charges supplied SQL bytes, including cached-statement lookups.
Approximate prepared-statement `SQLITE_STMTSTATUS_MEMUSED` samples are recorded
with their sample count and summed bytes. SQLite defines this as a current
statement estimate, not a counter: summing samples does not measure resident
allocation, pager memory, RSS or a phase peak. The owning diagnostic is explicit.

[CreationWork](../../crates/layerfs-overlay/src/database/startup.rs) observes real
finite startup: exclusive artifact create, allocation, SQLite open, connection
configuration, profile PRAGMAs/readbacks, schema/accounting DDL and cache setup.
The former unobserved SQL batches now consume each actual prepared statement
once, including pragma rows and prepare/step failure. Batch input bytes are
charged once; final end-of-batch is not a phantom attempt. `create_observed`
retains original failure and an independent allocation observation; no failed
artifact is deleted or startup retried. Existing `create` forwards unchanged.
Daemon `start_observed` retains this receipt through worker readiness/failure;
`startup_work` remains separate from foreground and maintenance aggregates.
`OwnerStart::creation_reported` distinguishes received creation work from a
default placeholder on admission/spawn/pre-receipt worker failure. Unreported
worker creation cost is unavailable, not observed zero; original error remains.
Finite schema startup is O(S + V) for supplied SQL bytes S and executed VM/row
work V, with fixed-schema resident state, plus the whole physical reservation.
It is initialized once per daemon rather than on each Workspace bind.

The E04 native-backing continuation adds `filesystem_open_calls`,
`filesystem_identity_calls`, `filesystem_probe_calls` and `linux_filesystem_type`
to the original creation receipt. Linux makes one unconditional read-only,
O_NOFOLLOW reopen, two descriptor metadata attempts to establish the original
regular-file identity and link count, then one fstatfs attempt on the verified
reopened descriptor. Each counter increments before its own attempted call;
later calls remain zero after an earlier failure. The type is None unless that
observation succeeded. Other platforms and create-new refusals make zero such
calls. The original I/O error, identity refusal or UnsupportedFilesystem remains
in Creation.result; a field is not another outcome or positive qualification.
No new Arc owner or per-job receipt is introduced. The existing shared startup
receipt allocation grows by the added fields; startup stack, worker and other
allocations remain separate. This is not a phase-memory bound.
The versioned E04 v2 collector records these facts. Historical E01/E04 v1
serializations do not acquire this observation by inference.

[Payload observations](../../crates/layerfs-overlay/src/diagnostics/payload.rs)
record actual cell input, intersected/partial cells, codec/merge copies and zeroed
cell/composed-read windows. Whole-cell replacement copies input once without a
cell read or zeroed codec window. Partial updates expand at most one cell, merge
input and trim its bounded prefix. Read delivery records local byte assignments;
SQL-returned BLOB bytes remain separate. These scopes exclude driver-internal,
transport/kernel copies and unrelated namespace allocations. Counters saturate;
they never refuse a product operation or store a trace per input byte.

[Allocation](../../crates/layerfs-overlay/src/database/allocation.rs) separately
records physical high-water allocation, descriptor/path metadata observations,
pre-BEGIN freelist-cookie queries and the existing precise range-call counts.
Each successful state observation uses two metadata calls. A range call's requested
bytes can include already allocated storage; its cumulative sum is not newly
consumed disk. High-water storage includes the full 128 MiB mutation plus 128 MiB
cleanup reservation, SQL growth and allocation rounding/metadata. It is a lifetime
allocation gauge, never a phase-local memory peak. [Physical accounting](35-shared-physical-capacity.md)
retains its platform/exclusive-dense-file assumptions and S6 qualification limits.

[JobWork](../../crates/layerfs-daemon/src/service/observations.rs) travels with an
original owner's completion, including failed attempts and readiness checks which
park that job. The result/receipt retains the same aggregate byte credit until
all Pending/Completion owners release it. Admission charges the receipt's size
and includes it in lifecycle reservation validation. Foreground and automatic
maintenance keep separate SQL, payload and allocation aggregates. Parked turns
are counted; no failed operation is replayed. Queue admission-to-final-turn wall
includes parking; final service and inclusive SQL observations overlap, and must
not be added as exclusive time components. Cancelled unattempted jobs retain any
earlier readiness work and do not invent mutation work.

Cost shape is unchanged: a point seek is O(log N), a keyset window returns K rows
with O(log N + K) index work plus possible row fetches, and a W-byte write touches
at most ceil((W + 4095)/4096) cells. Partial stale cells add binary staircase seeks:
O(log S * log N) for staircase height S. Cumulative M writes cost actual bytes and
bounded cell work per request, not the sum of growing file prefixes. Retirement
has output-sized cumulative work through bounded turns; service/debt capacity and
physical I/O still need evidence. Counters are O(1) per observed statement/row/cell
and O(1) resident state per admitted job; the returned row/byte windows remain in
their owning APIs. This does not establish constant SQLite/journal/kernel residency.

[S7 audit](../issues/307/S7-EXIT-AUDIT.md) separates retained count/resource proofs
from missing page/journal/IO, complete request/transport accounting, residency and
sustained-rate evidence. No cold speed, RSS, native capability, milestone completion
or release claim follows from these observations.

[Native runtime wire observations](40-runtime-wire-ownership.md) now include actual
socket attempts/partial bytes, cryptographic input/output attempts, fixed scratch
requests/capacities/zero initialization, fragment copies, shared receive/result credits
and separate fair output ownership. Observed failed handshakes retain original costs.
A real Store-backed11-operation native path reports those scopes alongside typed
Service work. These are functional diagnostics with uncontrolled caches, not complete
SQLite-page/journal/kernel residency or cold/sustained acceptance. The whole256MiB
reservation and unchanged earlier allocation/SQL/EXPLAIN evidence retain their scope.
