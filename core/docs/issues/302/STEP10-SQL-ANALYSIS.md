# #302 step 10 SQL and protocol analysis

> **Status:** Current diagnostic analysis; no performance or admission claim.
> Source inspected at b2a6051cace1d273398545f176e69b4ae34cec71, 2026-10-04.
> The example timer correction is a separate harness change, not product tuning.

The candidate composes project Init, C2 Storage and C5 HistoryCatalog directly
with PostgreSQL/MinIO. Server and SDK code still coexist until step 12/cluster 2
M9. Avoiding the retained host Service does not establish lower elapsed time.
Q5 requires strictly lower measured time in every case while satisfying storage.

## Physical exchanges and SQL work

The metadata client sends query_typed on one current-thread I/O runtime. Each
ordinary client call sends an extended-protocol Parse/Bind/Describe/Execute/Sync
sequence and waits for the backend response; physical Sync and ReadyForQuery
counts, rather than Rust method names, bound acknowledged exchanges. No separate
prepare round trip is used. Schema bootstrap uses simple Query and can contain
many SQL statements in one exchange. TLS/socket bytes and plaintext query bytes
are separate cumulative counters. Client operations, SQL statements inside a
function, transactions and physical exchanges are different quantities.

C2's seven units remain unchanged:

| Unit | SQL request / transaction shape | Cadence and relevant work |
| --- | --- | --- |
| policy | one SELECT; implicit read transaction | policy validation at handle creation/open; open currently reads again when Storage::new reads policy |
| locate | one SELECT with BYTEA[] and primary-key object join to pack primary key | membership waves and dependency levels; missing entries omitted, <=4096 IDs |
| read_packs | one SELECT with pack-ID array | metadata bodies only; 4 MiB cap remains; repeated pooled acquisitions stay observable |
| value_groups | one SELECT for set or ordered page | ordinal reconstruction; metadata_value_group primary key and unique pack/group index |
| signatures | one ordered SELECT | per new save candidate ring load, not one query per candidate |
| reserve | one SELECT calling reserve_storage; one implicit writable transaction | row lock on store_policy; pack sequence block, optional ordinal highwater UPDATE |
| register | one SELECT calling register_storage; one implicit writable transaction | closed batches only; packs, first-wins object rows, pooled groups, signatures, window/release changes atomically acknowledged |

reserve_storage performs server-side SELECT ... FOR UPDATE, sequence allocation
and (only for positive ordinal count) highwater update. These server statements
do not introduce frontend round trips. register_storage executes several
server-side statements/loops. A count of one Sync must never be called one SQL
statement. Registration pays default synchronous_commit=on/fsync=on; it does not
hold its transaction across a MinIO upload. MinIO acknowledgements precede
registration and all closed references are checked before publication.

C5 retains one coherent transaction per operation. Reads send BEGIN REPEATABLE
READ READ ONLY, their validation/lookup queries, then COMMIT. Writes send BEGIN
READ COMMITTED, LOCK history_meta EXCLUSIVE NOWAIT, their validation/mutation
queries, then COMMIT. The lock grants immediate authority refusal and does not
block ordinary concurrent reads or lock C2 tables. Separate typed operations
remain separate transactions; the harness cannot coalesce stage/commit/add_layer
into an alternative publication path. Definite failures abort; uncertain handles
quarantine without a guessed rollback/reconnect/retry.

The retained step-7 small-fixture diagnostic measured the following exchanges,
not latencies. Successful writes include three control exchanges (BEGIN, lock,
COMMIT); the rest are validation and mutation exchanges.

| C5 operation | Operations / Sync / Ready | Control | Remaining |
| --- | ---: | ---: | ---: |
| initialize_layerstack | 8 / 8 / 8 | 3 | 5 |
| fork | 11 / 11 / 11 | 3 | 8 |
| stage_changes | 12 / 12 / 12 | 3 | 9 |
| commit_staged | 11 / 11 / 11 | 3 | 8 |
| add_layer | 11 / 11 / 11 | 3 | 8 |
| reserve_inodes, first allocation | 6 / 6 / 6 | 3 | 3 |
| branch_snapshot with Commit head | 6 / 6 / 6 | 2 | 4 |
| point reads in stated fixture | 3 / 3 / 3 | 2 | 1 |

These counts depend on source state (genesis vs Commit head, first allocation,
ancestry depth, pagination). They cannot be scaled to seven gate cases as though
all queries were constant. initialize/fork/stage/commit/add_layer enforce typed
identity, parent, profile, scope, immutable-row and conditional-head semantics.
Reducing exchanges requires a separately validated implementation preserving
those checks, NOWAIT refusal and uncertain-outcome behavior. No such optimization
has been accepted or measured by this analysis.

## Repeated acquisitions and index risks

The retained step-9 fresh-reader negative proofs made two metadata body calls
before an absent pooled ordinal and three before a corrupt value-base refusal,
with one catalogue call. This is reconstruction/acquisition demand, not a retry.
A second missing catalogue lookup is not present. Reader group/value caches and
Storage's locator/pack caches have different lifetimes. Cache-hit counters alone
do not prove fewer required storage reads or smaller frontend response bodies.

The object locator join is supported by object.object_id and pack.pack_id primary
keys; pooled set/page queries use the first_ordinal primary key. Registration's
foreign keys require referenced packs to precede locators, and signature slots
have fixed ring constraints. C5 point/ancestry reads use the source-defined
composite indexes and deferred keys. Query-plan performance remains UNMEASURED:
a plan observed on an empty or small functional fixture must not stand in for a
registered history case's cardinalities. Actual relation allocation includes
heap, index, TOAST and pack sequence; PostgreSQL catalogs and WAL are separate.

Array/body binding serializes owned vectors into the published client's binary
parameters. Metadata packs transfer as BYTEA; payloads transfer directly via S3.
The Init path opens four independent upload streams and waits before each closed
registration. All other saves retain one producer/upload. Network sent/received,
protocol sent/received, metadata write/read and S3 entity bytes are different
counters and must not be summed as unique logical content.

## Evidence status

Source tracing and earlier count receipts are diagnostic evidence only. There is
no eligible matched baseline yet. Earlier INELIGIBLE time rows remain unchanged.
New harness cases name all four Init tiers and all three retained-history strides.
Init's numeric storage ceiling remains absent; speed PASS alone cannot fill it.
All seven matched speed/storage acceptance results are currently NOT_RUN.
