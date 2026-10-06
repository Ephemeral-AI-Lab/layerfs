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
Finite schema startup is O(S + V) for supplied SQL bytes S and executed VM/row
work V, with fixed-schema resident state, plus the whole physical reservation.
It is initialized once per daemon rather than on each Workspace bind.

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
