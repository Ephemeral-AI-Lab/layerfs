# PostgreSQL Doctor observations

> **Status:** Implemented backend contract; PostgreSQL 17 qualification described below.

Store integrity performs an independent read-only connection and authority
preflight before opening its logical audit. Authentication, database/schema
access and wrong saved binding are identified as access or authority failures,
with dependent integrity coverage blocked. They do not establish content
corruption. The preflight and logical snapshot have explicitly different
observation intervals; the second open validates the saved identity again.

Without an explicit overall timeout, both admission sessions share one ten-second
absolute budget. Successful admission removes this temporary query deadline:
a full audit then has no hidden time limit. An explicit overall deadline remains
in force throughout admission, inventory, decoding and online physical checks.
Cancellation retains its at-most-one-second cleanup grace for server cancellation
and ROLLBACK acknowledgement. No connection or failed audit is replayed.

## Pinned schema and authority

The logical transaction rechecks its schema and catalog ID, the ID derived from
its binding key, and incarnation against the handle's admitted authority. An
identity replaced after admission fails authority verification before inventory.
Publication or project removal by an independent writer does not enter the
pinned REPEATABLE READ READ ONLY logical view.

Diagnostic admission compares the saved schema source/column definition with
shipped schema-2 constants on the server. The live column projection comes from
permission-independent PostgreSQL catalogs; a revoked table permission must not
masquerade as a disappeared table. Binding/catalog IDs are length-guarded before
projection. The source, saved definition and arbitrary catalog definition bodies
are never returned to the client just to compare them. Ordinary synchronous
writer admission retains its existing path.

A separate live-object contract checks required constraints, indexes, triggers
and trigger-function bodies independently of the saved source text. It inspects
only required named objects, allowing benign extra indexes and extension objects.
Definitions are compared on the server against 210 normalized signatures derived
from the shipped DDL on PostgreSQL 17.11, including applicable acquisition tables.
Missing/disabled/invalid required objects fail schema authority. On other server
majors a differing deparse representation is Unknown compatibility coverage,
rather than a content-corruption verdict. Matching signatures remain checkable.
Doctor creates no source or temporary PostgreSQL relation/function to do this.

## Online physical interval

Full mode follows the logical transaction with separately identified online
physical observations. Every checked relation has its OID, operation and duration;
the summary is not a substitute for per-relation coverage.

| Relation or prerequisite | Observation |
| --- | --- |
| Heap table, materialized view or sequence | Supported installed `verify_heapam`, with TOAST checking and `skip=none` |
| Ready/valid B-tree index | Supported installed `bt_index_check(heapallindexed=true)` |
| Hash/other index method, invalid index, partitioned or foreign relation | Explicit unavailable coverage; no unsupported function invocation |
| Missing supported amcheck signature/extension or denied execution | Unknown engine coverage; independent logical validation remains meaningful |
| amcheck XX001/XX002 or returned heap corruption rows | Confirmed physical finding, attributed to the relation |
| Replica | Use the same supported online operations; never request a stronger lock mode |

Heap output is capped at 100 rows per relation and truncation makes coverage
incomplete. Doctor never installs amcheck or invokes `bt_index_parent_check`.
It makes no offline server-file/checksum claim. PostgreSQL documents the supported
heap/sequence/materialized-view operations and B-tree lock/replica behavior in
[the PostgreSQL 17 amcheck manual](https://www.postgresql.org/docs/17/amcheck.html).

## Access and extraction boundaries

DNS admission preserves configured lazy host order, all resolved addresses,
Unix/TCP fallback and the hostname used for TLS identity. Randomized multi-host
admission remains explicitly unsupported. TLS uses the ordinary configured policy
and system trust, including rejection of an untrusted fixture certificate.
Actual stalled TLS and authentication sockets are bounded and closed. The
caller's cancellation flag is checked before admission and while polling the
owned async connect/handshake. A joined watcher carries the flag to preflight
SQL; cancellation performs the same bounded server-cancel/ROLLBACK cleanup.
Resolver callbacks check cancellation at their bounded entry/return boundaries;
this does not promise instant preemption inside a synchronous system resolver.
Scoped watchers stop and join even if an observer unwinds.

Pack bodies must match their bounded descriptor before crossing the wire;
descriptor digests, object IDs, signatures and ordinal bodies have format-derived
projection guards. Oversized authority/saved-DDL faults are compared on the
server, not materialized in the client. Qualification uses an independent TCP
byte counter as well as outcome checks for malformed one-megabyte rows.

Revoking access during a PostgreSQL transaction need not invalidate already
admitted/cached reads in that transaction. Those authenticated snapshot reads can
remain valid. A subsequent denied read/admission is incomplete; actual backend
loss during namespace qualification is a provider failure, never invented
namespace corruption. Reports omit passwords, credential-bearing URLs and raw
driver error parameters.

The crate-local PostgreSQL matrix exercises an owned Disposable PostgreSQL 17.11
primary and streaming standby, SCRAM authentication, a self-signed TLS server,
missing/denied amcheck, source row fingerprints, read/write counters and independent
backend release observation. Harness, fixture identities and final receipts live
in `layerfs-benchmark/doctor-postgresql` and its qualification report. Durable
execution and offline corruption tooling are outside this evidence.
