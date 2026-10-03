# Bounded signature UPSERT treatment

Status: Mechanism count proof passes; new competitive speed unmeasured.
Based onf3913c845. Retained1000 diagnosis issued939 individual signature INSERTs;
source backend/sqlite/publish.rs loops one statement per signature. Public-API
512-row reproduction first failed:514 statements, expected one mutation plus
BEGIN/COMMIT. Failure log target/phase7-agent/signature-batch-before.log retained.

Signature UPSERT now uses ordered VALUES pages bounded by actual SQLite variable/
SQL-length limits and the existing512-object bound. Scalar/binding descriptors
are bounded;32-byte object IDs/signatures are borrowed, not cloned. The enclosing
atomic publication and>=stamp conflict predicate are unchanged. Empty signature
input does no work and imposes no unused statement capacity requirement.

Count proof512 signatures:3 statements/1 write commit.513:4 statements/1 write
commit. Stale stamps ignored; equal-stamp later publication replaces as before.
Second-page foreign-key failure rolls back preceding512 signatures and the earlier
body INSERT, with exactly one rollback and no acknowledged write commit. Full
publication readback and existing first-wins behavior pass. No format, row/byte,
cache, worker, durable profile or failure-handling bound changed.

Final checks10PASS:9 sqlite_publication tests and project init_sqlite full100/1000
namespace oracle. Scoped persistence/project all-target Clippy-Dwarnings, core
fmt, product boundary and23 self-testsPASS. Older unchanged workspace suites not
repeated. No CI/preflight claim. Test adjustments cover empty-signature and
cross-page behavior; intermediate logs remain, not speed samples.

Prospective measurement: one release/locked matched1000-file Init-v2 pair with
current sealed native cold helper, same15s complete/9.5s separate proof, fresh
DB creation/import/checkpoint/close product clock, exact roots and allocation
<=matched reference. Earlier failure and timeout receipts unchanged. No replay
at the same source/harness identity. Other tiers and histories remain required;
this count proof is not speed parity or all-seven completion.


## Matched release outcomes atdd19f21ea

| Files | Product baseline/current ns | Exact10*current /11*baseline | Final allocation baseline/current B | Complete command baseline/current ns | Separate proof baseline/current ns | Verdict |
|---|---|---|---|---|---|---|
|1000|140170375/250016458|2500164580/1541874125|23101440/20561920|209761541/1627944125|58894792/714420125|TimeFAIL; allocation/budgetsPASS|
|100000|5568755291/6851490875|68514908750/61256308201|518029312/521052160|11971168042/14125868875|1773591791/2493837959|Time/allocationFAIL; budgetsPASS|

Both exact roots match; independent sampled proof, cold-source-page attestation,
cleanupPASS. First completed current largest-tier result after the earlier
retained timeouts; it is still23.0345%slower and3022848B over final allocation.
1000ratio1.783661191. No controlled cross-window latency-delta claim.

| Actual candidate counts |1000|100000|
|---|---:|---:|
|Statements|320|9800|
|Executed VM|203940|14757575|
|Transactions|35|1006|
|Write commits|20|401|
|Commit ns|143981207|2690905661|
|Sealed bodies|97|2277|
|Body bytes|20126471|506418090|
|C2pack read B|0|1656|
|Individual payload reads|0|0|
|Reservations/publications|8/9|138/260|

Earlier completed1000 mechanism1284 statements/209982VM; this removes per-row
signature statements but write commits remain20. This window has higher commit
wall, not evidence of a latency improvement. Nested spans overlap; observed
sync system-call counts remain unavailable. Do not substitute lifetime counters
or EXPLAIN programs for these actual VM counts.

Raw issue302-signature{1000,100000}-{baseline,candidate}-treatment1; summary/
allocation diagnostic issue302-signature-comparison-treatment1. Reproduction
runner.py run --case phase7-sqlite-init-<size>-v2 --arm baseline --baseline-root
target/phase7-baseline/layerfs --out <fresh-owned-output>, then --arm candidate.
Read benchmark_agent_report.md before each invocation. No unchanged-identity
resampling. Other100/10000 Init cases and three histories NOT_RUN at this identity;
all seven still required. Source/body/harness/dependency seals in raw receipts.

## Allocation cause diagnosis, not a gate relabel

SystemSQLite3.51.0 read-only dbstat after performance; main hashes unchanged.
Current logical main515006464B vs reference515342336B (335872B smaller).
Allocated main521019392 vs517996544B; allocation beyond logical size6012928 vs
2654208B. Both add32768B companion/sidecar in the gate totals. Current freelist0
vs31reference pages; there is no evidence that vacuuming free pages fixes this.
These populated/warm diagnostics are not Stores reused in a timed arm.

A disposable4096B file with explicit16MiB preallocation retains16781312 allocatedB
after same-size ftruncate; bytes unchanged. A separate explicit F_TRANSFEREXTENTS
probe moves only extra extents to an empty same-volume scratch file, removes it,
and leaves4096 allocatedB with all4096 logical bytes unchanged. Dirty diagnostic
source/hash is recorded, zero product samples, no retained Store/input changed.
Raw issue302-preallocated-tail-diagnostic1 /issue302-preallocated-transfer-
diagnostic1. This qualifies a candidate mechanism on disposable APFS data, not
an implemented production fix or durable/crash proof. Installed fcntl documentation
and Apple XNU sources reviewed; no privileged active-file trim is selected.

Next: integrate a safe existing-library extent release with exact lifecycle,
cleanup, profile and error qualification; product publication/validation costs
and actual history driver/proof still remain. Active goal unchanged. Code commit
ProductionLOC137505->137544(+39), reference65417/core72088->72127,
active28044->28083/inactive44044; old191/new7634->7673/rest64263. Exact snapshot
counter/method in commit. Evidence/tooling-only followup+0. No push/PR/merge.
